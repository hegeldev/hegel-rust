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
use crate::native::core::{ChoiceNode, ChoiceValue, NativeTestCase, Span, Status};
use crate::native::data_source::NativeDataSource;
use crate::native::det_tree::{ChoiceValueKey, DetTreeNode, record_into};
use crate::test_case::{ASSUME_FAIL_STRING, LOOP_DONE_STRING, STOP_TEST_STRING, TestCase};

use super::runner::panic_message;

/// Wraps the user's test function with a data tree (non-determinism detection)
/// and a result cache (avoiding redundant calls during shrinking).
///
/// All test case execution flows through this struct, so recording and
/// caching happen automatically.
pub struct CachedTestFunction<F: FnMut(TestCase)> {
    test_fn: F,
    /// Root of the data tree trie.
    tree_root: DetTreeNode,
    /// Cache of test results keyed on complete choice sequences.
    cache: HashMap<Vec<ChoiceValueKey>, (bool, Vec<ChoiceNode>)>,
    /// Execution mode forwarded to each TestCase. Defaults to `Mode::TestRun`;
    /// `native_run` overrides via `set_mode` to propagate `Settings::mode`.
    mode: crate::runner::Mode,
}

/// One run's worth of results. Mirrors what Hypothesis exposes via the
/// per-run `ConjectureData` after the test body returns: status, the
/// realised choice nodes/spans, any `tc.target()` observations the body
/// recorded, and (for `Status::Interesting`) the panic message that
/// triggered the failure plus an opaque origin string identifying *where*
/// the panic happened. The origin is supplied by
/// [`crate::run_lifecycle::run_test_case`] from the captured panic
/// `file:line:col`; per-origin shrinking and database storage key on it.
#[derive(Clone)]
pub struct RunResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub spans: Vec<Span>,
    pub target_observations: HashMap<String, f64>,
    pub panic_message: Option<String>,
    pub origin: Option<String>,
}

/// Object-safe surface used by the native targeting / hill-climber code:
/// "run a [`NativeTestCase`] and tell me what happened." Both the legacy
/// [`CachedTestFunction`] (which owns the user's test_fn directly) and the
/// production [`EngineCtx`](super::test_runner::EngineCtx) (which wraps the
/// cross-backend `run_case` callback) implement it, so targeting doesn't
/// need to care which lifecycle drove the test.
pub trait NativeRunner {
    fn run(&mut self, ntc: NativeTestCase) -> RunResult;
}

impl<F: FnMut(TestCase)> CachedTestFunction<F> {
    pub fn new(test_fn: F) -> Self {
        CachedTestFunction {
            test_fn,
            tree_root: DetTreeNode::new(),
            cache: HashMap::new(),
            mode: crate::runner::Mode::TestRun,
        }
    }

    /// Override the mode forwarded to each executed [`TestCase`].
    pub fn set_mode(&mut self, mode: crate::runner::Mode) {
        self.mode = mode;
    }

    /// Run a test case during the generation or database-replay phase.
    ///
    /// Records the resulting nodes in the data tree (checking for
    /// non-determinism) but does not use the cache (random generation
    /// produces unique sequences, so caching would just waste memory).
    pub fn run(&mut self, ntc: NativeTestCase) -> RunResult {
        let result = self.execute(ntc);
        self.record(&result.nodes);
        result
    }

    /// Run a test case during shrinking.
    ///
    /// Checks the cache first; on a miss, runs the test, records in the
    /// data tree, and stores the result in the cache.
    /// Returns `(is_interesting, actual_nodes)` where `actual_nodes` is
    /// the sequence of nodes the test actually produced (which may differ
    /// from `candidate_nodes` if the test exited early or if values were
    /// punned for a changed kind).
    pub fn run_shrink(&mut self, candidate_nodes: &[ChoiceNode]) -> (bool, Vec<ChoiceNode>) {
        if let Some(cached) = self.cache_lookup(candidate_nodes) {
            return cached;
        }

        let choices: Vec<ChoiceValue> = candidate_nodes.iter().map(|n| n.value.clone()).collect();
        let ntc = NativeTestCase::for_choices(&choices, Some(candidate_nodes), None);
        let RunResult { status, nodes, .. } = self.execute(ntc);
        self.record(&nodes);

        let result = (status == Status::Interesting, nodes);
        self.cache_store(candidate_nodes, result.clone());
        result
    }

    /// Run a probe test case: replay `prefix` then draw randomly beyond it,
    /// up to `max_size` total choices.
    ///
    /// Used by `mutate_and_shrink`. Results are cached on the actual
    /// produced node sequence (not the prefix), so a probe that happens to
    /// reproduce a previously-seen trace hits the cache. Records in the
    /// data tree like any other run.
    pub fn run_probe(
        &mut self,
        prefix: &[ChoiceValue],
        seed: u64,
        max_size: usize,
    ) -> (bool, Vec<ChoiceNode>) {
        use rand::SeedableRng;
        use rand::rngs::SmallRng;
        let rng = SmallRng::seed_from_u64(seed);
        let ntc = NativeTestCase::for_probe(prefix, rng, max_size);
        let RunResult { status, nodes, .. } = self.execute(ntc);
        self.record(&nodes);
        let result = (status == Status::Interesting, nodes);
        self.cache_store(&result.1, result.clone());
        result
    }

    /// Core test execution: run one test case and return results.
    ///
    /// Final-replay output (the `is_last_run = true` path on `TestCase::new`)
    /// is owned by [`crate::run_lifecycle::drive`], which is what every
    /// `Hegel::run` invocation goes through. `CachedTestFunction` is only
    /// used by embedded tests that drive the engine directly without that
    /// lifecycle, so it never needs to mark a run as final.
    fn execute(&mut self, ntc: NativeTestCase) -> RunResult {
        let (data_source, ntc_handle) = NativeDataSource::new(ntc);
        let tc = TestCase::new(Box::new(data_source), false, self.mode);
        let result =
            with_test_context(|| catch_unwind(AssertUnwindSafe(|| (self.test_fn)(tc.clone()))));

        let (status, panic_message) = match result {
            Ok(()) => (Status::Valid, None),
            Err(e) => {
                let msg = panic_message(&e);
                if msg == ASSUME_FAIL_STRING || msg == STOP_TEST_STRING {
                    (Status::Invalid, None)
                } else if msg == LOOP_DONE_STRING {
                    (Status::Valid, None)
                } else {
                    (Status::Interesting, Some(msg))
                }
            }
        };

        RunResult {
            status,
            nodes: NativeDataSource::take_nodes(&ntc_handle),
            spans: NativeDataSource::take_spans(&ntc_handle),
            target_observations: NativeDataSource::take_target_observations(&ntc_handle),
            panic_message,
            // `EngineCtx` (the live `Hegel::run` path) receives an origin
            // from `crate::run_lifecycle::run_test_case`. The embedded-test
            // path through `CachedTestFunction` doesn't drive through the
            // cross-backend panic hook, so it has no origin to attach.
            origin: None,
        }
    }

    /// Record nodes in the data tree, checking for non-determinism.
    ///
    /// Delegates to [`crate::native::det_tree::record_into`] so the trie
    /// structure and divergent-kind diagnostic are shared with
    /// [`crate::native::test_runner::EngineCtx`].
    fn record(&mut self, nodes: &[ChoiceNode]) {
        record_into(&mut self.tree_root, nodes);
    }

    fn cache_lookup(&self, nodes: &[ChoiceNode]) -> Option<(bool, Vec<ChoiceNode>)> {
        let key: Vec<ChoiceValueKey> = nodes
            .iter()
            .map(|n| ChoiceValueKey::from(&n.value))
            .collect();
        self.cache.get(&key).cloned()
    }

    fn cache_store(&mut self, nodes: &[ChoiceNode], result: (bool, Vec<ChoiceNode>)) {
        let key: Vec<ChoiceValueKey> = nodes
            .iter()
            .map(|n| ChoiceValueKey::from(&n.value))
            .collect();
        self.cache.insert(key, result);
    }
}

impl<F: FnMut(TestCase)> NativeRunner for CachedTestFunction<F> {
    fn run(&mut self, ntc: NativeTestCase) -> RunResult {
        CachedTestFunction::run(self, ntc)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/tree_tests.rs"]
mod tests;
