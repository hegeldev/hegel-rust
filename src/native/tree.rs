// Data tree and test function cache for the native backend.
//
// The data tree records the ChoiceKind (schema parameters) at each position in
// the choice sequence, keyed by the prefix of choice values. When a test is
// replayed with the same choice values, the tree verifies that the schema
// parameters haven't changed — a change indicates non-deterministic data
// generation (e.g. a generator that depends on global mutable state).
//
// The cache maps complete choice sequences to their test results, avoiding
// redundant test function calls during shrinking.
//
// CachedTestFunction owns the test function and is the sole way to execute
// test cases. This ensures every run is automatically recorded in the data
// tree and (during shrinking) checked against the cache.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::control::with_test_context;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Span, Status};
use crate::native::data_source::NativeDataSource;
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING, TestCase};

use super::runner::{panic_message, store_final_panic_info};

/// Hashable version of `ChoiceValue`, for use as tree/cache keys.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ChoiceValueKey {
    Integer(i128),
    Boolean(bool),
    Float(u64), // f64::to_bits()
    Bytes(Vec<u8>),
}

impl From<&ChoiceValue> for ChoiceValueKey {
    fn from(v: &ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueKey::Integer(*n),
            ChoiceValue::Boolean(b) => ChoiceValueKey::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueKey::Float(f.to_bits()),
            ChoiceValue::Bytes(b) => ChoiceValueKey::Bytes(b.clone()),
        }
    }
}

/// A node in the data tree trie.
///
/// Each node stores the expected `ChoiceKind` for this position and branches
/// to children keyed by the choice value at this position.
struct TreeNode {
    /// The expected ChoiceKind at this position (set on first visit).
    kind: Option<ChoiceKind>,
    /// Children keyed by the choice value at this position.
    children: HashMap<ChoiceValueKey, TreeNode>,
}

impl TreeNode {
    fn new() -> Self {
        TreeNode {
            kind: None,
            children: HashMap::new(),
        }
    }
}

/// Wraps the user's test function with a data tree (non-determinism detection)
/// and a result cache (avoiding redundant calls during shrinking).
///
/// All test case execution flows through this struct, so recording and
/// caching happen automatically.
pub struct CachedTestFunction<F: FnMut(TestCase)> {
    test_fn: F,
    /// Root of the data tree trie.
    tree_root: TreeNode,
    /// Cache of test results keyed on complete choice sequences.
    cache: HashMap<Vec<ChoiceValueKey>, (bool, usize)>,
}

impl<F: FnMut(TestCase)> CachedTestFunction<F> {
    pub fn new(test_fn: F) -> Self {
        CachedTestFunction {
            test_fn,
            tree_root: TreeNode::new(),
            cache: HashMap::new(),
        }
    }

    /// Run a test case during the generation or database-replay phase.
    ///
    /// Records the resulting nodes in the data tree (checking for
    /// non-determinism) but does not use the cache (random generation
    /// produces unique sequences, so caching would just waste memory).
    pub fn run(&mut self, ntc: NativeTestCase) -> (Status, Vec<ChoiceNode>, Vec<Span>) {
        let (status, nodes, spans, _) = self.execute(ntc, false);
        self.record(&nodes);
        (status, nodes, spans)
    }

    /// Run a test case during shrinking.
    ///
    /// Checks the cache first; on a miss, runs the test, records in the
    /// data tree, and stores the result in the cache.
    /// Returns `(is_interesting, nodes_consumed)`.
    pub fn run_shrink(&mut self, candidate_nodes: &[ChoiceNode]) -> (bool, usize) {
        if let Some(cached) = self.cache_lookup(candidate_nodes) {
            return cached;
        }

        let choices: Vec<ChoiceValue> = candidate_nodes.iter().map(|n| n.value.clone()).collect();
        let ntc = NativeTestCase::for_choices(&choices, Some(candidate_nodes));
        let (status, new_nodes, _, _) = self.execute(ntc, false);
        self.record(&new_nodes);

        let result = (status == Status::Interesting, new_nodes.len());
        self.cache_store(candidate_nodes, result);
        result
    }

    /// Run the final replay of a failing test case (with output enabled).
    ///
    /// Does not use the cache or record in the tree — the test is about
    /// to fail and we need the actual panic payload for re-raising.
    pub fn run_final(&mut self, ntc: NativeTestCase) -> (Status, Vec<ChoiceNode>, Vec<Span>) {
        let (status, nodes, spans, _) = self.execute(ntc, true);
        (status, nodes, spans)
    }

    /// Core test execution: run one test case and return results.
    fn execute(
        &mut self,
        ntc: NativeTestCase,
        is_final: bool,
    ) -> (Status, Vec<ChoiceNode>, Vec<Span>, Option<String>) {
        let (data_source, ntc_handle) = NativeDataSource::new(ntc);
        let tc = TestCase::new(Box::new(data_source), is_final);
        let result =
            with_test_context(|| catch_unwind(AssertUnwindSafe(|| (self.test_fn)(tc.clone()))));

        let (status, panic_msg) = match result {
            Ok(()) => (Status::Valid, None),
            Err(e) => {
                let msg = panic_message(&e);
                if msg == ASSUME_FAIL_STRING || msg == STOP_TEST_STRING {
                    (Status::Invalid, None)
                } else {
                    if is_final {
                        store_final_panic_info(&msg);
                    }
                    (Status::Interesting, Some(msg))
                }
            }
        };

        let nodes = NativeDataSource::take_nodes(&ntc_handle);
        let spans = NativeDataSource::take_spans(&ntc_handle);
        (status, nodes, spans, panic_msg)
    }

    /// Record nodes in the data tree, checking for non-determinism.
    fn record(&mut self, nodes: &[ChoiceNode]) {
        let mut current = &mut self.tree_root;
        for node in nodes {
            let key = ChoiceValueKey::from(&node.value);
            let child = current.children.entry(key).or_insert_with(TreeNode::new);
            if let Some(ref expected_kind) = child.kind {
                if *expected_kind != node.kind {
                    panic!(
                        "Your data generation is non-deterministic: at the same choice \
                         position with the same prefix, the schema changed from {:?} to {:?}. \
                         This usually means a generator depends on global mutable state.",
                        expected_kind, node.kind
                    );
                }
            } else {
                child.kind = Some(node.kind.clone());
            }
            current = child;
        }
    }

    fn cache_lookup(&self, nodes: &[ChoiceNode]) -> Option<(bool, usize)> {
        let key: Vec<ChoiceValueKey> = nodes
            .iter()
            .map(|n| ChoiceValueKey::from(&n.value))
            .collect();
        self.cache.get(&key).copied()
    }

    fn cache_store(&mut self, nodes: &[ChoiceNode], result: (bool, usize)) {
        let key: Vec<ChoiceValueKey> = nodes
            .iter()
            .map(|n| ChoiceValueKey::from(&n.value))
            .collect();
        self.cache.insert(key, result);
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/tree_tests.rs"]
mod tests;
