// `NativeConjectureRunner` — the native-engine wrapper that
// `tests/hypothesis/conjecture_engine.rs` (and its sibling Conjecture
// test ports) exercise directly.
//
// This type mirrors the subset of Hypothesis's
// `internal/conjecture/engine.py::ConjectureRunner` public surface
// that the ported Conjecture tests assert on:
// `interesting_examples`, `exit_reason`, `shrinks`, `call_count`,
// `valid_examples`, `save_choices`, `secondary_key`, `pareto_key`,
// `reuse_existing_examples`, `clear_secondary_key`, `new_shrinker` /
// `fixate_shrink_passes`, `pareto_front` / `dominance`,
// `tree.is_exhausted`, `generate_novel_prefix`, `ignore_limits`,
// `statistics`, `cached_test_function`, `shrink_interesting_examples`,
// plus the `run_to_nodes(f)` conftest fixture and the
// `fails_health_check(label)` decorator.
//
// Most attributes start as `todo!()` stubs. Each subsequent port-loop
// cycle that lands a native-gated test from
// `conjecture/test_engine.py` fills in the specific attribute(s) that
// test exercises, as per the design captured in
// `.claude/skills/porting-tests/SKILL.md` under "`test_engine.py`-shape".

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

use crate::native::bignum::BigUint;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Status};
use crate::native::database::ExampleDatabase;
use crate::native::datatree::compute_max_children;
use crate::native::shrinker::Shrinker;

/// Re-export of [`crate::native::database::serialize_choices`] under
/// Hypothesis's public name.  Mirrors
/// `hypothesis.database.choices_to_bytes`.
pub use crate::native::database::deserialize_choices as choices_from_bytes;
pub use crate::native::database::serialize_choices as choices_to_bytes;

/// Why a `NativeConjectureRunner::run()` call terminated.  Port of
/// Hypothesis's `internal/conjecture/engine.py::ExitReason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExitReason {
    /// `max_examples` budget exhausted by the generation phase.
    MaxExamples,
    /// `max_examples * INVALID_PER_VALID` iterations exhausted with
    /// too few valid examples.
    MaxIterations,
    /// Shrinker exceeded the `MAX_SHRINKS` per-example limit.
    MaxShrinks,
    /// Run completed normally with no pending work.
    Finished,
    /// A replayed counterexample no longer reproduced — the test is
    /// non-deterministic.
    Flaky,
    /// Shrinking exceeded the `very_slow_shrinking` wall-clock budget.
    VerySlowShrinking,
}

/// Hypothesis's `InterestingOrigin` reduced to what the ported tests
/// observe: an opaque lineno-like tag used as a dict key so distinct
/// failure points don't collide.  Mirrors
/// `internal/escalation.py::InterestingOrigin`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterestingOrigin {
    /// Stable id assigned by `interesting_origin(n)`.  `None` means
    /// "the default origin" (call site of `mark_interesting()` with
    /// no argument).
    pub id: Option<i64>,
    /// Label derived from a panic payload that escaped the test
    /// function without a `mark_interesting` call.  Two panic-derived
    /// origins compare equal iff their labels match, mirroring
    /// Hypothesis's "distinct traceback ⇒ distinct interesting
    /// example" behaviour for arbitrary user exceptions.  `None` for
    /// the explicit-call path.
    pub panic_label: Option<String>,
}

/// Construct an `InterestingOrigin` with the given stable id, so
/// `interesting_origin(n) == interesting_origin(m) iff n == m`.
/// Mirrors the `tests/conjecture/common.py::interesting_origin`
/// fixture.
pub fn interesting_origin(n: Option<i64>) -> InterestingOrigin {
    InterestingOrigin {
        id: n,
        panic_label: None,
    }
}

impl InterestingOrigin {
    /// Synthesise an origin from a panic payload that escaped the test
    /// function.  Used by [`run_test_fn`] to map non-mark / non-stop
    /// panics to a [`Status::Interesting`] result, mirroring the way
    /// Hypothesis records each distinct user-thrown traceback as its
    /// own interesting example.  Two payloads with the same downcast
    /// string (or, failing that, the same concrete type) hash to the
    /// same origin.
    fn from_panic_payload(payload: &(dyn Any + Send)) -> Self {
        let label = if let Some(s) = payload.downcast_ref::<&'static str>() {
            format!("&str:{s}")
        } else if let Some(s) = payload.downcast_ref::<String>() {
            format!("String:{s}")
        } else {
            format!("type-id:{:?}", payload.type_id())
        };
        InterestingOrigin {
            id: None,
            panic_label: Some(label),
        }
    }
}

/// A single interesting (failing) test case observed by the runner.
/// Mirrors the `ConjectureResult` value stored in
/// `runner.interesting_examples[origin]`.
#[derive(Clone, Debug)]
pub struct InterestingExample {
    pub nodes: Vec<ChoiceNode>,
    pub choices: Vec<ChoiceValue>,
    pub origin: InterestingOrigin,
}

/// Health-check labels raised by `FailedHealthCheck` panics.  Port of
/// Hypothesis's `HealthCheck` enum values referenced in
/// `test_engine.py::fails_health_check` assertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HealthCheckLabel {
    FilterTooMuch,
    TooSlow,
    LargeBaseExample,
    DataTooLarge,
}

/// Three-way dominance relation between two test cases' target
/// observations.  Port of
/// `internal/conjecture/pareto.py::DominanceRelation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DominanceRelation {
    NoDominance,
    LeftDominates,
    RightDominates,
    Equal,
}

/// Stub for `pareto.dominance(a, b)` — compares two test cases'
/// target-observation vectors.
pub fn dominance(_left: &InterestingExample, _right: &InterestingExample) -> DominanceRelation {
    todo!("NativeConjectureRunner: implement dominance (pareto.py)")
}

/// Settings snapshot for a `NativeConjectureRunner`.  The fields
/// listed here are the ones `test_engine.py` tests pass to
/// `ConjectureRunner(settings=...)`; anything not set defaults to the
/// engine's normal behaviour.
pub struct NativeRunnerSettings {
    pub max_examples: usize,
    pub database: Option<Arc<dyn ExampleDatabase>>,
    pub derandomize: bool,
    /// Subset of `Phase` values to enable.  `None` = default
    /// (generate + shrink).
    pub phases: Option<Vec<Phase>>,
    pub suppress_health_check: Vec<HealthCheckLabel>,
    /// Override for `engine_module.MAX_SHRINKS`; `None` = default.
    pub max_shrinks: Option<usize>,
    /// Whether the runner shrinks every distinct interesting origin or
    /// only the first one found.  Mirrors Hypothesis's
    /// `settings(report_multiple_bugs=...)`.  Defaults to `true`.
    pub report_multiple_bugs: bool,
    /// Per-test-case byte budget for `draw_bytes`.  `None` = use the
    /// default `CONJECTURE_BUFFER_SIZE`.  Mirrors Hypothesis's
    /// `buffer_size_limit(n)` context manager which monkeypatches
    /// `engine.BUFFER_SIZE` for the lifetime of a single
    /// `runner.run()` call.
    pub buffer_size_limit: Option<usize>,
}

impl NativeRunnerSettings {
    pub fn new() -> Self {
        NativeRunnerSettings {
            max_examples: 100,
            database: None,
            derandomize: false,
            phases: None,
            suppress_health_check: Vec::new(),
            max_shrinks: None,
            report_multiple_bugs: true,
            buffer_size_limit: None,
        }
    }

    pub fn max_examples(mut self, n: usize) -> Self {
        self.max_examples = n;
        self
    }

    pub fn database(mut self, db: Option<Arc<dyn ExampleDatabase>>) -> Self {
        self.database = db;
        self
    }

    pub fn derandomize(mut self, d: bool) -> Self {
        self.derandomize = d;
        self
    }

    pub fn phases(mut self, p: Vec<Phase>) -> Self {
        self.phases = Some(p);
        self
    }

    pub fn suppress_health_check(mut self, v: Vec<HealthCheckLabel>) -> Self {
        self.suppress_health_check = v;
        self
    }

    pub fn max_shrinks(mut self, n: usize) -> Self {
        self.max_shrinks = Some(n);
        self
    }

    pub fn report_multiple_bugs(mut self, b: bool) -> Self {
        self.report_multiple_bugs = b;
        self
    }

    pub fn buffer_size_limit(mut self, n: usize) -> Self {
        self.buffer_size_limit = Some(n);
        self
    }
}

impl Default for NativeRunnerSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Port of Hypothesis's `Phase` enum.  Subset listed covers what the
/// ported tests toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Generate,
    Shrink,
    Reuse,
    Explain,
}

/// Unique-per-`NativeConjectureData` id used as the panic payload for
/// `mark_interesting` / `mark_invalid`.  When runners are nested (the
/// `test_interleaving_engines` shape), the inner runner's `catch_unwind`
/// inspects the captured id; a mismatch means some outer data raised
/// the mark and the panic resumes unwinding.
static NEXT_DATA_ID: AtomicU64 = AtomicU64::new(1);

/// Sentinel panic raised by a `draw_*` call whose underlying
/// `NativeTestCase` draw returned `StopTest` (buffer exhausted).
const STOP_TEST_PANIC: &str = "__hegel_conjecture_stop_test__";

/// Byte-size limit for a single test's accumulated `draw_bytes` calls.
/// Mirrors Hypothesis's `BUFFER_SIZE` in `conjecture/engine.py`:
/// when a `draw_bytes(n, n)` call would push the running count past
/// this limit, the draw triggers `StopTest` / Overrun instead of
/// returning a value.  The native `NativeTestCase::max_size` only
/// caps *choice count*, not bytes, so without this check the
/// `test_draw_to_overrun` shape would wrongly accept a
/// `first_byte = 0 → d = 248 → draw_bytes(31744, 31744)` shrink
/// candidate that in Hypothesis would Overrun.
const CONJECTURE_BUFFER_SIZE: usize = 8 * 1024;

/// Minimum number of test calls before the generation phase is
/// allowed to stop after finding an interesting example.  Mirrors
/// `engine.py::MIN_TEST_CALLS`.
const MIN_TEST_CALLS: usize = 10;

/// Base invalid-call budget before the generation phase exits with
/// `MaxIterations`.  Derived in `engine.py` from
/// `_invalid_thresholds(r=0.01, c=0.99)` — stop once we're 99%
/// confident the true valid rate is below 1%.  Hard-coded here to
/// match the Python value exactly (the `test_max_iterations_with_*`
/// tests assert on the exact call count).
const INVALID_THRESHOLD_BASE: usize = 458;

/// Per-valid-example increment to the invalid-call budget.  From the
/// same `_invalid_thresholds(r=0.01, c=0.99)` formula in `engine.py`.
const INVALID_PER_VALID: usize = 100;

/// Wall-clock budget for the shrink phase, in seconds.  Mirrors
/// `engine.py::MAX_SHRINKING_SECONDS` (5 minutes).
const MAX_SHRINKING_SECONDS: f64 = 5.0 * 60.0;

/// Kind of mark recorded on a `NativeConjectureData`.  Either
/// `Interesting` (the test function called `mark_interesting`) or
/// `Invalid` (the test function called `mark_invalid`, signalling that
/// this draw sequence should not be counted as a valid example).
#[derive(Clone, Debug, PartialEq, Eq)]
enum MarkKind {
    Interesting,
    Invalid,
}

/// Panic payload raised by [`NativeConjectureData::mark_interesting`] and
/// [`NativeConjectureData::mark_invalid`].  Carries the `data_id` of the
/// originating data so nested runners can tell "mine" from "someone
/// else's" and propagate the latter.
#[derive(Debug)]
struct MarkPanic {
    data_id: u64,
}

/// Test-case surface passed to the user's runner callback.  Mirrors the
/// subset of Hypothesis's `ConjectureData` used by `test_engine.py`
/// ports.
#[non_exhaustive]
pub struct NativeConjectureData {
    ntc: NativeTestCase,
    data_id: u64,
    mark: Option<(MarkKind, Option<InterestingOrigin>)>,
    bytes_drawn: usize,
    /// Per-test-case byte budget enforced by [`Self::draw_bytes`] /
    /// [`Self::draw_bytes_forced`].  Pulled from
    /// [`NativeRunnerSettings::buffer_size_limit`] for runner-driven
    /// invocations; defaults to [`CONJECTURE_BUFFER_SIZE`] otherwise.
    buffer_size_limit: usize,
}

impl NativeConjectureData {
    fn new(ntc: NativeTestCase, buffer_size_limit: usize) -> Self {
        NativeConjectureData {
            ntc,
            data_id: NEXT_DATA_ID.fetch_add(1, Ordering::Relaxed),
            mark: None,
            bytes_drawn: 0,
            buffer_size_limit,
        }
    }

    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Vec<u8> {
        if self.bytes_drawn.saturating_add(min_size) > self.buffer_size_limit {
            std::panic::panic_any(STOP_TEST_PANIC);
        }
        match self.ntc.draw_bytes(min_size, max_size) {
            Ok(v) => {
                self.bytes_drawn += v.len();
                v
            }
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    /// Forced variant of [`draw_bytes`]: the draw returns `forced`
    /// verbatim and records a `was_forced` choice node.  Mirrors
    /// Hypothesis's `data.draw_bytes(..., forced=value)`.
    pub fn draw_bytes_forced(
        &mut self,
        min_size: usize,
        max_size: usize,
        forced: Vec<u8>,
    ) -> Vec<u8> {
        if self.bytes_drawn.saturating_add(forced.len()) > self.buffer_size_limit {
            std::panic::panic_any(STOP_TEST_PANIC);
        }
        match self.ntc.draw_bytes_forced(min_size, max_size, forced) {
            Ok(v) => {
                self.bytes_drawn += v.len();
                v
            }
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn draw_integer(&mut self, min_value: i128, max_value: i128) -> i128 {
        match self.ntc.draw_integer(min_value, max_value) {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn draw_boolean(&mut self, _p: f64) -> bool {
        todo!("NativeConjectureData::draw_boolean")
    }

    pub fn draw_float(
        &mut self,
        _min_value: f64,
        _max_value: f64,
        _allow_nan: bool,
        _allow_infinity: bool,
    ) -> f64 {
        todo!("NativeConjectureData::draw_float")
    }

    pub fn mark_interesting(&mut self, origin: InterestingOrigin) -> ! {
        self.mark = Some((MarkKind::Interesting, Some(origin)));
        let data_id = self.data_id;
        std::panic::panic_any(MarkPanic { data_id })
    }

    pub fn mark_invalid(&mut self) -> ! {
        self.mark = Some((MarkKind::Invalid, None));
        let data_id = self.data_id;
        std::panic::panic_any(MarkPanic { data_id })
    }

    pub fn start_span(&mut self, label: u64) {
        self.ntc
            .span_stack
            .push((self.ntc.nodes.len(), label.to_string()));
    }

    pub fn stop_span(&mut self) {
        if let Some((start, label)) = self.ntc.span_stack.pop() {
            self.ntc.record_span(start, self.ntc.nodes.len(), label);
        }
    }

    pub fn nodes(&self) -> &[ChoiceNode] {
        &self.ntc.nodes
    }

    pub fn choices(&self) -> Vec<ChoiceValue> {
        todo!("NativeConjectureData::choices")
    }

    /// Accessor for the status recorded on the underlying test case.
    /// Used by `new_shrinker` predicates (`|d| d.status() ==
    /// Status::Interesting`).
    pub fn status(&self) -> Status {
        todo!("NativeConjectureData::status")
    }
}

/// Data-tree accessor for `runner.tree.is_exhausted`.
#[non_exhaustive]
pub struct NativeDataTreeView<'a> {
    _runner: std::marker::PhantomData<&'a NativeConjectureRunner>,
}

impl<'a> NativeDataTreeView<'a> {
    pub fn is_exhausted(&self) -> bool {
        todo!("NativeConjectureRunner::tree::is_exhausted")
    }
}

/// Shrinker handle returned by `runner.new_shrinker(data,
/// predicate)`.  Opaque stub; each port-loop cycle that lands a test
/// calling `fixate_shrink_passes` wires this to the concrete
/// `src/native/shrinker/Shrinker` internals.
#[non_exhaustive]
pub struct NativeShrinker {
    _private: (),
}

impl NativeShrinker {
    /// Run the full shrink loop.  Mirrors `Shrinker.shrink()`.
    pub fn shrink(&mut self) {
        todo!("NativeShrinker::shrink")
    }

    /// Run a named subset of shrink passes to fixation.  Mirrors
    /// `Shrinker.fixate_shrink_passes(passes)`.
    pub fn fixate_shrink_passes(&mut self, _passes: &[&str]) {
        todo!("NativeShrinker::fixate_shrink_passes")
    }

    /// Accessor for the current shrink result.
    pub fn current_nodes(&self) -> &[ChoiceNode] {
        todo!("NativeShrinker::current_nodes")
    }
}

type RunnerTestFn = Box<dyn FnMut(&mut NativeConjectureData)>;

/// Hashable choice-value key, mirroring [`crate::native::tree`]'s
/// internal tree.  Kept local so we don't force the private tree node
/// type to be `pub`.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ChoiceValueKey {
    Integer(i128),
    Boolean(bool),
    Float(u64),
    Bytes(Vec<u8>),
    String(Vec<u32>),
}

impl From<&ChoiceValue> for ChoiceValueKey {
    fn from(v: &ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueKey::Integer(*n),
            ChoiceValue::Boolean(b) => ChoiceValueKey::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueKey::Float(f.to_bits()),
            ChoiceValue::Bytes(b) => ChoiceValueKey::Bytes(b.clone()),
            ChoiceValue::String(s) => ChoiceValueKey::String(s.clone()),
        }
    }
}

/// Minimal data tree used for non-determinism detection and
/// novel-prefix generation — a port of the subset of Hypothesis's
/// `internal/conjecture/datatree.py::DataTree` that's needed to avoid
/// re-sampling dead branches.  Each node stores the observed
/// [`ChoiceKind`] at its position (fixed on first visit), child
/// subtrees keyed by the choice value drawn, an optional terminal
/// `Status` if the test concluded at this position, and a cached
/// `is_exhausted` flag.
#[derive(Default)]
struct DataTreeNode {
    kind: Option<ChoiceKind>,
    children: HashMap<ChoiceValueKey, Box<DataTreeNode>>,
    /// Terminal status if the test case ended at this node.  Only set
    /// when the recording run concluded with `Status >= Invalid`
    /// (an EarlyStop / overrun is not treated as exhausting a path).
    conclusion: Option<Status>,
    /// Cached: true iff the subtree rooted here has been fully
    /// explored — either because this is a terminal (conclusion is
    /// set) or because every possible child has been observed and is
    /// itself exhausted.
    is_exhausted: bool,
}

/// Iterative drop so a thousands-deep single-path tree (built when the
/// all-simplest probe runs an infinite-loop test fn) doesn't blow the
/// thread's stack via the default recursive `Box<DataTreeNode>` drop.
impl Drop for DataTreeNode {
    fn drop(&mut self) {
        let mut stack: Vec<Box<DataTreeNode>> =
            self.children.drain().map(|(_, child)| child).collect();
        while let Some(mut node) = stack.pop() {
            stack.extend(node.children.drain().map(|(_, child)| child));
        }
    }
}

impl DataTreeNode {
    /// Recompute `is_exhausted` based on current state.  Mirrors
    /// Hypothesis's `TreeNode.check_exhausted`.
    fn check_exhausted(&mut self) -> bool {
        if self.is_exhausted {
            return true;
        }
        if self.conclusion.is_some() {
            self.is_exhausted = true;
            return true;
        }
        if let Some(ref kind) = self.kind {
            let max_c = compute_max_children(kind);
            if BigUint::from(self.children.len() as u64) >= max_c {
                let all_exhausted = self.children.values_mut().all(|c| c.check_exhausted());
                if all_exhausted {
                    self.is_exhausted = true;
                    return true;
                }
            }
        }
        false
    }
}

/// Walk `nodes` through `tree_root`, asserting that the schema at every
/// position matches what was observed on previous runs.  A mismatch
/// panics with the same "non-deterministic" wording as the rest of the
/// native engine so `test_erratic_draws`-shape tests can `expect_panic`
/// on it.  Records the terminal `status` at the leaf (if the test
/// concluded cleanly) and propagates exhaustion up the path so the
/// runner's `generate_novel_prefix` walk can avoid dead branches.
fn record_tree(tree_root: &mut DataTreeNode, nodes: &[ChoiceNode], status: Status) {
    // Iterative descent: a single-path walk can be thousands of nodes
    // deep (e.g. an infinite-loop test under the all-simplest probe),
    // and a recursive walk would blow the thread's stack.  We track
    // the descent as a chain of raw mutable pointers; only one is
    // dereferenced at a time, so no two `&mut DataTreeNode` references
    // overlap.
    let mut path: Vec<*mut DataTreeNode> = Vec::with_capacity(nodes.len() + 1);
    path.push(tree_root as *mut _);

    for first in nodes {
        let parent_ptr = *path.last().unwrap();
        // SAFETY: `parent_ptr` was either the original `tree_root`
        // pointer (whose backing `&mut` outlives this function) or a
        // pointer derived in the previous iteration from a unique
        // `entry().or_insert_with(...)` borrow.  No other live `&mut`
        // aliases this node.
        let node = unsafe { &mut *parent_ptr };
        match &node.kind {
            Some(expected_kind) if *expected_kind != first.kind => {
                panic!(
                    "Your data generation is non-deterministic: at the same choice \
                     position with the same prefix, the schema changed from {:?} to {:?}. \
                     This usually means a generator depends on global mutable state.",
                    expected_kind, first.kind
                );
            }
            None => {
                node.kind = Some(first.kind.clone());
            }
            _ => {}
        }
        let key = ChoiceValueKey::from(&first.value);
        let child = node
            .children
            .entry(key)
            .or_insert_with(|| Box::new(DataTreeNode::default()));
        path.push(child.as_mut() as *mut _);
    }

    if status >= Status::Invalid {
        // SAFETY: same as above — leaf pointer is the only live
        // reference into this subtree.
        let leaf = unsafe { &mut **path.last().unwrap() };
        leaf.conclusion = Some(status);
    }

    // Ascend, calling `check_exhausted` on each node bottom-up so an
    // exhausted leaf can propagate up the chain.  We can pop one
    // pointer at a time because each node has a unique parent and we
    // only touch one node at each step.
    while let Some(p) = path.pop() {
        // SAFETY: `p` is the pointer we just popped, no other live
        // reference exists to the same node at this point.
        let node = unsafe { &mut *p };
        node.check_exhausted();
    }
}

/// Small-domain cap for enumeration fallback in
/// `pick_non_exhausted_value`.  Only kinds with at most this many total
/// children can be enumerated directly.
const ENUMERATION_CAP: u64 = 1024;

/// Draw a single random value of `kind`.  Deliberately simple — uniform
/// where possible; the runner only needs this for novel-prefix walks,
/// where hitting a boundary special isn't important.  Returns `None` for
/// kinds the novel-prefix walker has no bespoke sampler for (strings,
/// floats): the caller then truncates the prefix at that position and
/// falls back to fresh-RNG sampling in the actual test run.
fn random_choice_value(kind: &ChoiceKind, rng: &mut SmallRng) -> Option<ChoiceValue> {
    match kind {
        ChoiceKind::Integer(ic) => Some(ChoiceValue::Integer(
            rng.random_range(ic.min_value..=ic.max_value),
        )),
        ChoiceKind::Boolean(_) => Some(ChoiceValue::Boolean(rng.random::<bool>())),
        ChoiceKind::Bytes(bc) => {
            let len = if bc.min_size == bc.max_size {
                bc.min_size
            } else {
                rng.random_range(bc.min_size..=bc.max_size)
            };
            let bytes: Vec<u8> = (0..len).map(|_| rng.random::<u8>()).collect();
            Some(ChoiceValue::Bytes(bytes))
        }
        ChoiceKind::String(_) | ChoiceKind::Float(_) => None,
    }
}

/// Enumerate every possible value of `kind`, provided the total count
/// fits under [`ENUMERATION_CAP`].  Returns `None` for large or
/// unsupported kinds, signalling the caller should fall back to random
/// sampling.
fn enumerate_choice_values(kind: &ChoiceKind) -> Option<Vec<ChoiceValue>> {
    let max_c = compute_max_children(kind);
    if max_c > BigUint::from(ENUMERATION_CAP) {
        return None;
    }
    match kind {
        ChoiceKind::Integer(ic) => {
            let mut v = Vec::new();
            let mut n = ic.min_value;
            loop {
                v.push(ChoiceValue::Integer(n));
                if n == ic.max_value {
                    break;
                }
                n += 1;
            }
            Some(v)
        }
        ChoiceKind::Boolean(_) => Some(vec![
            ChoiceValue::Boolean(false),
            ChoiceValue::Boolean(true),
        ]),
        ChoiceKind::Bytes(bc) => {
            let max_idx: u64 = max_c.try_into().ok()?;
            let mut v = Vec::with_capacity(max_idx as usize);
            for i in 0..max_idx {
                let bytes = bc.from_index(BigUint::from(i))?;
                v.push(ChoiceValue::Bytes(bytes));
            }
            Some(v)
        }
        _ => None,
    }
}

/// Pick a choice value whose subtree is either absent from `children`
/// or present but not marked exhausted.  Returns `None` only when the
/// parent's children set is already complete and all marked exhausted,
/// which the caller should treat as an exhausted-subtree signal.
fn pick_non_exhausted_value(
    kind: &ChoiceKind,
    children: &HashMap<ChoiceValueKey, Box<DataTreeNode>>,
    rng: &mut SmallRng,
) -> Option<ChoiceValue> {
    for _ in 0..10 {
        let value = random_choice_value(kind, rng)?;
        let key = ChoiceValueKey::from(&value);
        match children.get(&key) {
            Some(child) if child.is_exhausted => continue,
            _ => return Some(value),
        }
    }
    let candidates = enumerate_choice_values(kind)?;
    let mut untried: Vec<ChoiceValue> = candidates
        .into_iter()
        .filter(|v| {
            let key = ChoiceValueKey::from(v);
            children.get(&key).is_none_or(|c| !c.is_exhausted)
        })
        .collect();
    if untried.is_empty() {
        return None;
    }
    untried.shuffle(rng);
    untried.into_iter().next()
}

/// Walk the data tree and return a prefix of choice values that stops
/// at the first novel (never-before-seen) position.  Port of the
/// `DataTree.generate_novel_prefix` walk in Hypothesis's
/// `internal/conjecture/datatree.py`, simplified to hegel's tree shape
/// (no radix-node compaction, no float-bit hashing, no children cache).
///
/// The caller feeds the returned prefix to `NativeTestCase::for_probe`
/// so early draws replay the deterministic walk and later draws pick up
/// fresh RNG sampling.  Returning an empty prefix means "just draw
/// everything at random" — correct for the first call in a run, when
/// the tree is still empty.
fn generate_novel_prefix(tree_root: &DataTreeNode, rng: &mut SmallRng) -> Vec<ChoiceValue> {
    if tree_root.is_exhausted {
        return Vec::new();
    }
    let mut prefix = Vec::new();
    let mut current = tree_root;
    while let Some(ref kind) = current.kind {
        let Some(value) = pick_non_exhausted_value(kind, &current.children, rng) else {
            break;
        };
        let key = ChoiceValueKey::from(&value);
        let next = current.children.get(&key);
        prefix.push(value);
        match next {
            Some(child) if !child.is_exhausted => current = child,
            _ => break,
        }
    }
    prefix
}

/// Run the caller-supplied test function on a freshly-constructed
/// [`NativeConjectureData`] wrapping `ntc`, unwrap the panic taxonomy
/// into a [`Status`], and surface the recorded
/// `mark_interesting(origin)` if any.  Pulled out of the runner struct
/// so the generation and shrink paths can both invoke it without
/// running into overlapping-self-borrow issues.
fn run_test_fn(
    test_fn: &mut RunnerTestFn,
    ntc: NativeTestCase,
    buffer_size_limit: usize,
) -> (Status, Vec<ChoiceNode>, Option<InterestingOrigin>) {
    let mut data = NativeConjectureData::new(ntc, buffer_size_limit);
    let my_id = data.data_id;

    let result = catch_unwind(AssertUnwindSafe(|| {
        test_fn(&mut data);
    }));

    let status = match result {
        Ok(()) => Status::Valid,
        Err(payload) => {
            if let Some(mp) = payload.downcast_ref::<MarkPanic>() {
                if mp.data_id == my_id {
                    match &data.mark {
                        Some((MarkKind::Interesting, _)) => Status::Interesting,
                        Some((MarkKind::Invalid, _)) => Status::Invalid,
                        None => unreachable!("MarkPanic matched but data.mark is None"),
                    }
                } else {
                    std::panic::resume_unwind(payload)
                }
            } else if payload.downcast_ref::<&'static str>().copied() == Some(STOP_TEST_PANIC) {
                Status::EarlyStop
            } else {
                // Arbitrary panic from user test code.  Mirror Hypothesis's
                // behaviour of treating each distinct user exception as an
                // interesting example with a per-traceback origin so the
                // runner records the bug rather than aborting the whole run.
                let origin = InterestingOrigin::from_panic_payload(payload.as_ref());
                data.mark = Some((MarkKind::Interesting, Some(origin)));
                Status::Interesting
            }
        }
    };

    let origin = match data.mark {
        Some((MarkKind::Interesting, o)) => o,
        _ => None,
    };
    let nodes = std::mem::take(&mut data.ntc.nodes);
    (status, nodes, origin)
}

/// Concatenate `database_key + b"." + sub` to derive a sub-corpus key.
/// Mirrors `ConjectureRunner.sub_key` (`b".".join((database_key, sub))`).
fn sub_key(database_key: &[u8], sub: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(database_key.len() + 1 + sub.len());
    out.extend_from_slice(database_key);
    out.push(b'.');
    out.extend_from_slice(sub);
    out
}

/// Order two byte slices by shortlex: length first, then lexicographically.
/// Mirrors Hypothesis's `shortlex(s) -> (len(s), s)` sort key.
fn shortlex_cmp(a: &Vec<u8>, b: &Vec<u8>) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Port of Hypothesis's `ConjectureRunner` for the subset of
/// `test_engine.py` that doesn't already live under the
/// targeting/optimiser surface.
///
/// Most methods are `todo!()` stubs.  Subsequent port-loop cycles
/// land tests that fill in the attributes they exercise.
pub struct NativeConjectureRunner {
    #[allow(dead_code)]
    test_fn: RunnerTestFn,
    #[allow(dead_code)]
    settings: NativeRunnerSettings,
    #[allow(dead_code)]
    rng: SmallRng,
    database_key: Option<Vec<u8>>,
    /// Monotonic clock used for the shrink-phase wall-clock budget,
    /// in seconds.  Defaults to `Instant::now()`-derived elapsed time;
    /// tests override via [`NativeConjectureRunner::with_time_source`]
    /// to simulate a mocked clock (mirrors Python's
    /// `monkeypatch.setattr(time, "perf_counter", ...)` pattern).
    time_source: Box<dyn FnMut() -> f64>,
    /// Data tree shared between `run()`'s generation phase and
    /// [`Self::cached_test_function`] so a seeded replay marks the
    /// reused choice sequence as exhausted before the novel-prefix
    /// walker picks a fresh prefix.
    tree_root: DataTreeNode,
    /// `call_count` snapshot of the first / most-recent interesting
    /// example.  Mirrors `engine.py`'s `first_bug_found_at` /
    /// `last_bug_found_at`; together they bound the post-bug
    /// continuation window in [`Self::should_generate_more`].
    first_bug_found_at: Option<usize>,
    last_bug_found_at: Option<usize>,
    /// Set when `reuse_existing_examples` replays the entire primary
    /// corpus and every interesting entry's choices come back identical.
    /// Mirrors `runner.reused_previously_shrunk_test_case`; if set,
    /// `run()` skips the shrink phase entirely.
    reused_previously_shrunk_test_case: bool,

    /// Externally-visible bookkeeping.  `run()` populates these; tests
    /// read them back.  All `todo!()` accessors lift from here once the
    /// backing state is wired up.
    pub interesting_examples: HashMap<InterestingOrigin, InterestingExample>,
    pub exit_reason: Option<ExitReason>,
    pub shrinks: usize,
    pub call_count: usize,
    pub valid_examples: usize,
    pub invalid_examples: usize,
    pub overrun_examples: usize,
    pub statistics: HashMap<String, String>,
    /// Number of times [`Self::shrink_interesting_examples`] has been
    /// invoked.  `test_shrink_after_max_examples` /
    /// `test_shrink_after_max_iterations` assert on this counter (their
    /// upstream form `Mock`s the method and inspects `Mock.call_count`).
    pub shrink_interesting_examples_call_count: usize,
    /// When true, `run()` keeps generating past `max_examples` /
    /// `max_iterations`.  Mirrors `runner.ignore_limits`; flipped by
    /// the `test_can_be_set_to_ignore_limits` cluster.
    pub ignore_limits: bool,
}

impl NativeConjectureRunner {
    pub fn new<F>(test_fn: F, settings: NativeRunnerSettings, rng: SmallRng) -> Self
    where
        F: FnMut(&mut NativeConjectureData) + 'static,
    {
        let start = std::time::Instant::now();
        NativeConjectureRunner {
            test_fn: Box::new(test_fn),
            settings,
            rng,
            database_key: None,
            time_source: Box::new(move || start.elapsed().as_secs_f64()),
            tree_root: DataTreeNode::default(),
            first_bug_found_at: None,
            last_bug_found_at: None,
            reused_previously_shrunk_test_case: false,
            interesting_examples: HashMap::new(),
            exit_reason: None,
            shrinks: 0,
            call_count: 0,
            valid_examples: 0,
            invalid_examples: 0,
            overrun_examples: 0,
            statistics: HashMap::new(),
            shrink_interesting_examples_call_count: 0,
            ignore_limits: false,
        }
    }

    pub fn with_database_key(mut self, key: Vec<u8>) -> Self {
        self.database_key = Some(key);
        self
    }

    /// Replace the runner's clock.  The callback returns the elapsed
    /// time in seconds; it is called at the start of the shrink phase
    /// to set the deadline, then once per re-validated interesting
    /// example and once per origin-shrink iteration.  Mirrors the
    /// `monkeypatch.setattr(time, "perf_counter", ...)` pattern used
    /// by `test_exit_because_shrink_phase_timeout`.
    pub fn with_time_source<F>(mut self, f: F) -> Self
    where
        F: FnMut() -> f64 + 'static,
    {
        self.time_source = Box::new(f);
        self
    }

    /// Mirror of `engine.py::should_generate_more`.  Pre-bug, the
    /// in-loop termination check at the bottom of `run()` handles
    /// max-examples / max-iterations exits and sets the matching
    /// [`ExitReason`] — this helper just keeps the loop alive.  Post-bug,
    /// the helper enforces both the budget limits and the flakiness
    /// continuation heuristic that mirrors Python's
    /// `call_count < min(first_bug_found_at + 1000, last_bug_found_at * 2)`.
    fn should_generate_more(&self, do_shrink: bool) -> bool {
        if self.interesting_examples.is_empty() {
            return true;
        }

        let invalid_threshold = INVALID_THRESHOLD_BASE + INVALID_PER_VALID * self.valid_examples;
        if self.valid_examples >= self.settings.max_examples
            || self.invalid_examples + self.overrun_examples > invalid_threshold
        {
            return false;
        }

        if !do_shrink || !self.settings.report_multiple_bugs {
            return false;
        }

        let first_bug = self.first_bug_found_at.unwrap_or(0);
        let last_bug = self.last_bug_found_at.unwrap_or(0);
        let heuristic = (first_bug.saturating_add(1000)).min(last_bug.saturating_mul(2));
        self.call_count < MIN_TEST_CALLS || self.call_count < heuristic
    }

    /// Main entry point.  Runs the generation + shrink phases to
    /// completion and populates `interesting_examples` / `exit_reason`
    /// / `shrinks` / `call_count` / `valid_examples` / `invalid_examples`
    /// / `overrun_examples` / `statistics`.
    pub fn run(&mut self) {
        let phases = self
            .settings
            .phases
            .clone()
            .unwrap_or_else(|| vec![Phase::Reuse, Phase::Generate, Phase::Shrink]);
        let do_reuse = phases.contains(&Phase::Reuse);
        let do_generate = phases.contains(&Phase::Generate);
        let do_shrink = phases.contains(&Phase::Shrink);
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);

        // --- Reuse phase ---
        if do_reuse {
            self.reuse_existing_examples();
        }

        // Fast path: every primary-corpus replay was an exact-match
        // interesting example, so re-shrinking is unlikely to yield
        // anything new.  Mirrors `engine.py::_run` lines 1535-1536.
        if self.reused_previously_shrunk_test_case && self.exit_reason.is_none() {
            self.exit_reason = Some(ExitReason::Finished);
        }

        // --- Generation phase ---
        if self.exit_reason.is_none() && do_generate {
            // One-shot "all simplest" probe.  Mirrors Hypothesis's
            // `cached_test_function((ChoiceTemplate("simplest", count=None),))`
            // call at the head of `generate_new_examples`: every draw
            // resolves to its kind's simplest value, so the runner gets
            // a fast-path attempt at the all-zero leaf before random
            // exploration starts.  Without this, `buffer_size_limit`
            // tests like `test_can_navigate_to_a_valid_example` rely
            // on hitting the boundary-probability path within the
            // invalid-threshold budget — too unreliable.
            if self.should_generate_more(do_shrink) && !self.tree_root.is_exhausted {
                let ntc = NativeTestCase::for_simplest(CONJECTURE_BUFFER_SIZE);
                let (status, nodes, origin) =
                    run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
                self.call_count += 1;
                record_tree(&mut self.tree_root, &nodes, status);
                self.record_test_result(status, nodes, origin);
            }

            loop {
                if self.set_exit_reason_if_done() {
                    break;
                }
                // Mirrors engine.py line 744: `exit_with(Finished)` when
                // the tree has no novel prefixes left.  Handled here as a
                // break so the shrink phase is skipped entirely, matching
                // Python's `RunIsComplete` unwind.
                if self.tree_root.is_exhausted {
                    self.exit_reason = Some(ExitReason::Finished);
                    break;
                }
                if !self.should_generate_more(do_shrink) {
                    break;
                }

                let mut batch_rng = SmallRng::from_rng(&mut self.rng);
                let prefix = generate_novel_prefix(&self.tree_root, &mut batch_rng);
                let ntc = NativeTestCase::for_probe(&prefix, batch_rng, CONJECTURE_BUFFER_SIZE);
                let (status, nodes, origin) =
                    run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
                self.call_count += 1;

                // Non-determinism detection (schema mismatch panics) plus
                // exhaustion bookkeeping for the next novel-prefix walk.
                record_tree(&mut self.tree_root, &nodes, status);

                self.record_test_result(status, nodes, origin);

                if self.set_exit_reason_if_done() {
                    break;
                }
            }
        }

        // --- Shrink phase ---
        if do_shrink && self.exit_reason.is_none() && !self.interesting_examples.is_empty() {
            self.shrink_interesting_examples();
        }

        if self.exit_reason.is_none() {
            self.exit_reason = Some(ExitReason::Finished);
        }
    }

    /// Pre-iteration termination check for the generation loop.
    /// Mirrors `engine.py` lines 732-742: when no interesting example
    /// has been observed yet, exhausting `max_examples` exits with
    /// `MaxExamples` and exhausting the `invalid_examples +
    /// overrun_examples` budget exits with `MaxIterations`.  Returns
    /// `true` if the loop should break.
    fn set_exit_reason_if_done(&mut self) -> bool {
        if !self.interesting_examples.is_empty() {
            return false;
        }
        let max_examples = self.settings.max_examples;
        if self.valid_examples >= max_examples {
            self.exit_reason = Some(ExitReason::MaxExamples);
            self.statistics.insert(
                "stopped-because".into(),
                format!("settings.max_examples={max_examples}"),
            );
            return true;
        }
        let invalid_threshold = INVALID_THRESHOLD_BASE + INVALID_PER_VALID * self.valid_examples;
        if self.invalid_examples + self.overrun_examples > invalid_threshold {
            self.exit_reason = Some(ExitReason::MaxIterations);
            self.statistics.insert(
                "stopped-because".into(),
                format!(
                    "settings.max_examples={max_examples}, \
                     but < 1% of examples satisfied assumptions"
                ),
            );
            return true;
        }
        false
    }

    /// Update the runner's call-count / status counters and bug-tracking
    /// fields from a single test invocation's outcome.  Shared by the
    /// generation loop and [`Self::cached_test_function`].
    fn record_test_result(
        &mut self,
        status: Status,
        nodes: Vec<ChoiceNode>,
        origin: Option<InterestingOrigin>,
    ) {
        match status {
            Status::Valid => self.valid_examples += 1,
            Status::Invalid => self.invalid_examples += 1,
            Status::EarlyStop => self.overrun_examples += 1,
            Status::Interesting => {
                let origin = origin.expect("Interesting status carries an origin");
                let new_origin = !self.interesting_examples.contains_key(&origin);
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                self.interesting_examples
                    .entry(origin.clone())
                    .or_insert(InterestingExample {
                        nodes,
                        choices,
                        origin,
                    });
                // Mirrors `engine.py` lines 690-697: `first_bug_found_at`
                // / `last_bug_found_at` only advance on a *new* origin so
                // the post-bug continuation heuristic doesn't reset the
                // budget every time we re-discover the same bug.
                if new_origin {
                    if self.first_bug_found_at.is_none() {
                        self.first_bug_found_at = Some(self.call_count);
                    }
                    self.last_bug_found_at = Some(self.call_count);
                }
            }
        }
    }

    /// Run only the shrink phase against an already-populated
    /// `interesting_examples`.  Used by `test_shrink_after_max_examples`
    /// / `test_shrink_after_max_iterations`, and by [`Self::run`] once
    /// the generation phase finishes.
    pub fn shrink_interesting_examples(&mut self) {
        self.shrink_interesting_examples_call_count += 1;
        let phases = self
            .settings
            .phases
            .clone()
            .unwrap_or_else(|| vec![Phase::Reuse, Phase::Generate, Phase::Shrink]);
        if !phases.contains(&Phase::Shrink) || self.interesting_examples.is_empty() {
            return;
        }
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);

        let deadline = (self.time_source)() + MAX_SHRINKING_SECONDS;
        let origins: Vec<InterestingOrigin> = self.interesting_examples.keys().cloned().collect();

        // Re-validation pass: mirrors `shrink_interesting_examples`
        // lines 1588-1595.  Each re-run checks the deadline at the
        // bottom (engine.py's test_function postscript, lines 716-730)
        // and then the Flaky-when-not-interesting check
        // (line 1594-1595).  Deadline takes priority over flakiness,
        // matching Python's call order.
        for origin in &origins {
            let initial = self.interesting_examples[origin].nodes.clone();
            let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value.clone()).collect();
            let ntc = NativeTestCase::for_choices(&choices, Some(&initial));
            let (status, _, _) = run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
            self.call_count += 1;

            if (self.time_source)() > deadline {
                self.exit_reason = Some(ExitReason::VerySlowShrinking);
                self.statistics
                    .insert("stopped-because".into(), "shrinking was very slow".into());
                return;
            }

            if status != Status::Interesting {
                self.exit_reason = Some(ExitReason::Flaky);
                self.statistics
                    .insert("stopped-because".into(), "test was flaky".into());
                return;
            }
        }

        for origin in origins {
            let initial = self.interesting_examples[&origin].nodes.clone();
            // Nothing to shrink if no choices were recorded (e.g.
            // `test_no_read_no_shrink`).
            if initial.is_empty() {
                continue;
            }

            let test_fn = &mut self.test_fn;
            let call_count = &mut self.call_count;
            let shrunk = {
                let mut shrinker = Shrinker::new(
                    Box::new(|candidate: &[ChoiceNode]| {
                        *call_count += 1;
                        let choices: Vec<ChoiceValue> =
                            candidate.iter().map(|n| n.value.clone()).collect();
                        let ntc = NativeTestCase::for_choices(&choices, Some(candidate));
                        let (status, actual_nodes, _) =
                            run_test_fn(test_fn, ntc, buffer_size_limit);
                        (status == Status::Interesting, actual_nodes)
                    }),
                    initial,
                );
                shrinker.shrink();
                shrinker.current_nodes
            };

            let choices: Vec<ChoiceValue> = shrunk.iter().map(|n| n.value.clone()).collect();
            self.interesting_examples.insert(
                origin.clone(),
                InterestingExample {
                    nodes: shrunk,
                    choices,
                    origin,
                },
            );
        }
    }

    /// Seeded replay entry point.  Mirrors
    /// `ConjectureRunner.cached_test_function` for the subset that the
    /// ported tests exercise: run the test function with `choices` as a
    /// forced prefix, update the runner's call / status / bug counters,
    /// and feed the resulting `nodes` into the data tree so the
    /// novel-prefix walker won't re-pick the same prefix later.
    pub fn cached_test_function(&mut self, choices: &[ChoiceValue]) {
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);
        let ntc = NativeTestCase::for_choices(choices, None);
        let (status, nodes, origin) = run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
        self.call_count += 1;
        record_tree(&mut self.tree_root, &nodes, status);
        self.record_test_result(status, nodes, origin);
    }

    /// Hill-climb from the current best interesting example and return
    /// a `Shrinker`-like handle the test can drive with
    /// `fixate_shrink_passes`.  Mirrors
    /// `ConjectureRunner.new_shrinker`.
    pub fn new_shrinker<P>(&mut self, _data: NativeConjectureData, _predicate: P) -> NativeShrinker
    where
        P: FnMut(&NativeConjectureData) -> bool + 'static,
    {
        todo!("NativeConjectureRunner::new_shrinker")
    }

    /// View of the internal data tree for `runner.tree.is_exhausted`
    /// assertions.
    pub fn tree(&self) -> NativeDataTreeView<'_> {
        NativeDataTreeView {
            _runner: std::marker::PhantomData,
        }
    }

    /// Produce a novel choice-sequence prefix.  Mirrors
    /// `ConjectureRunner.generate_novel_prefix`.
    pub fn generate_novel_prefix(&mut self) -> Vec<ChoiceValue> {
        todo!("NativeConjectureRunner::generate_novel_prefix")
    }

    /// Key under which the runner stores not-yet-shrunk candidates.
    /// Mirrors `ConjectureRunner.secondary_key`.
    pub fn secondary_key(&self) -> Vec<u8> {
        sub_key(
            self.database_key
                .as_deref()
                .expect("secondary_key requires database_key"),
            b"secondary",
        )
    }

    /// Key under which the runner stores the pareto front.  Mirrors
    /// `ConjectureRunner.pareto_key`.
    pub fn pareto_key(&self) -> Vec<u8> {
        sub_key(
            self.database_key
                .as_deref()
                .expect("pareto_key requires database_key"),
            b"pareto",
        )
    }

    /// Primary database key (as passed to `with_database_key`).
    pub fn database_key(&self) -> Option<&[u8]> {
        self.database_key.as_deref()
    }

    /// Save a choice sequence under the primary database key.  Mirrors
    /// `ConjectureRunner.save_choices`.
    pub fn save_choices(&mut self, choices: &[ChoiceValue]) {
        if let (Some(db), Some(key)) = (
            self.settings.database.as_ref(),
            self.database_key.as_deref(),
        ) {
            let bytes = crate::native::database::serialize_choices(choices);
            db.save(key, &bytes);
        }
    }

    /// Load existing examples from the database and replay them as the
    /// first phase of generation.  Mirrors
    /// `engine.py::reuse_existing_examples`: the primary corpus
    /// (`database_key`) is replayed in full; if it falls short of the
    /// target size, a sample of the secondary corpus is appended;
    /// once both are processed and no interesting example was found,
    /// a sample of the pareto corpus is replayed too.
    ///
    /// Bookkeeping mirrors upstream: `choices_from_bytes`-failures get
    /// deleted from the corpus they were drawn from; a non-interesting
    /// replay is also deleted from both primary and secondary; an
    /// interesting replay saves itself into the primary and (if it came
    /// from primary and matched the stored choices exactly) lights up
    /// `reused_previously_shrunk_test_case`.
    pub fn reuse_existing_examples(&mut self) {
        let (db, db_key) = match (self.settings.database.clone(), self.database_key.clone()) {
            (Some(d), Some(k)) => (d, k),
            _ => return,
        };
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);

        let phases = self
            .settings
            .phases
            .clone()
            .unwrap_or_else(|| vec![Phase::Reuse, Phase::Generate, Phase::Shrink]);
        let factor: f64 = if phases.contains(&Phase::Generate) {
            0.1
        } else {
            1.0
        };
        let desired_size = std::cmp::max(
            2,
            (factor * self.settings.max_examples as f64).ceil() as usize,
        );

        let mut corpus = db.fetch(&db_key);
        corpus.sort_by(shortlex_cmp);
        let primary_corpus_size = corpus.len();

        let secondary_key = sub_key(&db_key, b"secondary");

        if corpus.len() < desired_size {
            let mut extra_corpus = db.fetch(&secondary_key);
            let shortfall = desired_size - corpus.len();
            if extra_corpus.len() > shortfall {
                extra_corpus.shuffle(&mut self.rng);
                extra_corpus.truncate(shortfall);
            }
            extra_corpus.sort_by(shortlex_cmp);
            corpus.extend(extra_corpus);
        }

        let mut found_interesting_in_primary = false;
        let mut all_interesting_in_primary_were_exact = true;

        for (i, existing) in corpus.iter().enumerate() {
            // Once we've found a bug in the primary corpus we don't keep
            // re-running secondary entries — they're a fallback.
            if i >= primary_corpus_size && found_interesting_in_primary {
                break;
            }
            let Some(choices) = choices_from_bytes(existing) else {
                // `choices_from_bytes`-failures are only purged from the
                // corpus the entry came from — secondary deletes happen in
                // `clear_secondary_key`, not here.
                db.delete(&db_key, existing);
                continue;
            };
            let ntc = NativeTestCase::for_choices(&choices, None);
            let (status, nodes, origin) = run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
            self.call_count += 1;

            if matches!(status, Status::Valid) {
                self.valid_examples += 1;
            }
            if matches!(status, Status::Interesting) {
                let origin = origin.expect("Interesting status carries an origin");
                let replay_choices: Vec<ChoiceValue> =
                    nodes.iter().map(|n| n.value.clone()).collect();
                if !self.interesting_examples.contains_key(&origin) {
                    self.save_choices(&replay_choices);
                    self.interesting_examples.insert(
                        origin.clone(),
                        InterestingExample {
                            nodes,
                            choices: replay_choices.clone(),
                            origin,
                        },
                    );
                }
                if i < primary_corpus_size {
                    found_interesting_in_primary = true;
                    if replay_choices != choices {
                        all_interesting_in_primary_were_exact = false;
                    }
                }
                if !self.settings.report_multiple_bugs {
                    break;
                }
            } else {
                db.delete(&db_key, existing);
                db.delete(&secondary_key, existing);
            }

            if self.interesting_examples.is_empty()
                && self.valid_examples >= self.settings.max_examples
            {
                let max_examples = self.settings.max_examples;
                self.exit_reason = Some(ExitReason::MaxExamples);
                self.statistics.insert(
                    "stopped-because".into(),
                    format!("settings.max_examples={max_examples}"),
                );
                return;
            }
        }

        if found_interesting_in_primary && all_interesting_in_primary_were_exact {
            self.reused_previously_shrunk_test_case = true;
        }

        // Pareto corpus: only consulted when we still have budget left
        // and no interesting example has been found.  Mirrors
        // `engine.py::reuse_existing_examples` lines 1066-1082.
        if corpus.len() < desired_size && self.interesting_examples.is_empty() {
            let pareto_key = sub_key(&db_key, b"pareto");
            let mut pareto_corpus = db.fetch(&pareto_key);
            let desired_extra = desired_size - corpus.len();
            if pareto_corpus.len() > desired_extra {
                pareto_corpus.shuffle(&mut self.rng);
                pareto_corpus.truncate(desired_extra);
            }
            pareto_corpus.sort_by(shortlex_cmp);

            for existing in &pareto_corpus {
                let Some(choices) = choices_from_bytes(existing) else {
                    db.delete(&pareto_key, existing);
                    continue;
                };
                let ntc = NativeTestCase::for_choices(&choices, None);
                let (status, nodes, origin) =
                    run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
                self.call_count += 1;
                // The native runner doesn't yet track a pareto front, so
                // every replayed pareto entry is treated as "no longer in
                // the front" and deleted.  Matches upstream's behaviour
                // for any entry whose `data not in self.pareto_front`
                // branch fires.
                db.delete(&pareto_key, existing);
                self.record_test_result(status, nodes, origin);
                if matches!(status, Status::Interesting) {
                    break;
                }
            }
        }
    }

    /// Delete every stored value under `secondary_key`.  Mirrors
    /// `ConjectureRunner.clear_secondary_key`: replays each secondary
    /// entry through `cached_test_function` (skipped here when the entry
    /// matches a known interesting example, mimicking upstream's LRU-cache
    /// hit) and then deletes it.  Stops at the first entry whose
    /// shortlex order exceeds every interesting-example bytestring.
    pub fn clear_secondary_key(&mut self) {
        let (db, db_key) = match (self.settings.database.clone(), self.database_key.clone()) {
            (Some(d), Some(k)) => (d, k),
            _ => return,
        };
        let secondary = sub_key(&db_key, b"secondary");

        let mut corpus = db.fetch(&secondary);
        corpus.sort_by(shortlex_cmp);

        let primary_set: std::collections::HashSet<Vec<u8>> = self
            .interesting_examples
            .values()
            .map(|e| choices_to_bytes(&e.choices))
            .collect();
        // `max_primary` is the shortlex-largest primary entry; entries
        // worse than it can't be useful as shrinks.
        let max_primary = primary_set
            .iter()
            .max_by(|a, b| shortlex_cmp(a, b))
            .cloned();

        for c in &corpus {
            let Some(choices) = choices_from_bytes(c) else {
                db.delete(&secondary, c);
                continue;
            };
            if let Some(ref m) = max_primary {
                if shortlex_cmp(c, m).is_gt() {
                    break;
                }
            }
            // Skip the replay if we've already seen these exact choices
            // as an interesting example — upstream's LRU cache returns the
            // stored result without bumping `call_count`, and our minimal
            // port mimics that for the common "primary entry already
            // matches" case the test_discards_invalid_db_entries cluster
            // hits.
            if !primary_set.contains(c) {
                self.cached_test_function(&choices);
            }
            db.delete(&secondary, c);
        }
    }

    /// Pareto front snapshot.  Mirrors `ConjectureRunner.pareto_front`
    /// (the `ParetoFront` object).
    pub fn pareto_front(&self) -> Vec<InterestingExample> {
        todo!("NativeConjectureRunner::pareto_front")
    }
}

/// Conftest helper: run `f` through a `NativeConjectureRunner` to
/// completion and return the shrunk `nodes` of the sole interesting
/// example.  Port of `tests/conjecture/common.py::run_to_nodes`.
pub fn run_to_nodes<F>(f: F) -> Vec<ChoiceNode>
where
    F: FnMut(&mut NativeConjectureData) + 'static,
{
    use rand::SeedableRng;
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new()
        .max_examples(300)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ]);
    let mut runner = NativeConjectureRunner::new(f, settings, rng);
    runner.run();
    assert!(
        !runner.interesting_examples.is_empty(),
        "run_to_nodes: no interesting example observed"
    );
    let (_, example) = runner
        .interesting_examples
        .into_iter()
        .next()
        .expect("run_to_nodes: interesting_examples is non-empty");
    example.nodes
}

/// Assert that constructing the runner from `build` and calling
/// `.run()` raises a `FailedHealthCheck` whose message carries
/// `label`.  Port of `test_engine.py::fails_health_check`.
pub fn fails_health_check<B>(_label: HealthCheckLabel, _build: B)
where
    B: FnOnce() -> NativeConjectureRunner,
{
    todo!("fails_health_check: assert FailedHealthCheck panic with label")
}
