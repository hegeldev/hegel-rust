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
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

use crate::native::bignum::BigUint;
use crate::native::cache::LRUCache;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Span, Status};
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

pub use crate::native::core::{InterestingOrigin, interesting_origin};

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

/// Full result of running a single test case through the runner.
/// Mirrors `ConjectureResult` from `internal/conjecture/data.py`.
/// Used by `dominance` and `ParetoFront`.
#[derive(Clone, Debug)]
pub struct ConjectureRunResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub choices: Vec<ChoiceValue>,
    pub target_observations: HashMap<String, f64>,
    pub origin: Option<InterestingOrigin>,
    /// Structural-coverage tags from non-discarded spans.  Mirrors
    /// `ConjectureResult.tags` from `internal/conjecture/data.py`.
    /// Used by `dominance` to determine whether one result covers
    /// at least the structural paths of another.
    pub tags: HashSet<u64>,
}

impl PartialEq for ConjectureRunResult {
    fn eq(&self, other: &Self) -> bool {
        self.nodes == other.nodes
    }
}

impl Eq for ConjectureRunResult {}

/// Compare two test results' target observations to determine which
/// dominates the other.  Mirrors `internal/conjecture/pareto.py::dominance`.
pub fn dominance(left: &ConjectureRunResult, right: &ConjectureRunResult) -> DominanceRelation {
    let left_key = crate::native::core::sort_key(&left.nodes);
    let right_key = crate::native::core::sort_key(&right.nodes);

    if left_key == right_key {
        return DominanceRelation::Equal;
    }

    // Ensure we process left_key < right_key (left is simpler).
    // If right is actually simpler, recurse with swapped args and flip the result.
    if right_key < left_key {
        let result = dominance(right, left);
        return match result {
            DominanceRelation::LeftDominates => DominanceRelation::RightDominates,
            other => other,
        };
    }

    // left_key < right_key: left is simpler.  Check if left dominates right.

    // right has higher status → left cannot dominate
    if left.status < right.status {
        return DominanceRelation::NoDominance;
    }

    // right is interesting for a different origin → no dominance
    if left.status == Status::Interesting && right.origin.is_some() && left.origin != right.origin {
        return DominanceRelation::NoDominance;
    }

    // left must cover at least all structural paths that right covers.
    // Mirrors `right.tags.issubset(left.tags)` in pareto.py::dominance.
    if !right.tags.is_subset(&left.tags) {
        return DominanceRelation::NoDominance;
    }

    // For each target, right must not score strictly higher than left
    let all_targets: std::collections::HashSet<&String> = left
        .target_observations
        .keys()
        .chain(right.target_observations.keys())
        .collect();
    for target in all_targets {
        let left_score = left
            .target_observations
            .get(target)
            .copied()
            .unwrap_or(f64::NEG_INFINITY);
        let right_score = right
            .target_observations
            .get(target)
            .copied()
            .unwrap_or(f64::NEG_INFINITY);
        if right_score > left_score {
            return DominanceRelation::NoDominance;
        }
    }

    DominanceRelation::LeftDominates
}

/// Approximate Pareto front of test results.  Mirrors
/// `internal/conjecture/pareto.py::ParetoFront`.
pub struct ParetoFront {
    /// Results kept sorted by `sort_key(nodes)` (ascending).
    front: Vec<ConjectureRunResult>,
    rng: SmallRng,
}

impl ParetoFront {
    pub fn new(rng: SmallRng) -> Self {
        ParetoFront {
            front: Vec::new(),
            rng,
        }
    }

    /// Add `data` to the front.  Returns `(in_front, evicted)` where
    /// `in_front` is true when `data` is now in the front and `evicted`
    /// lists any entries that were removed because `data` dominates them.
    /// Mirrors `ParetoFront.add` + the eviction-listener mechanism.
    pub fn add(&mut self, data: ConjectureRunResult) -> (bool, Vec<ConjectureRunResult>) {
        if data.status < Status::Valid {
            return (false, vec![]);
        }
        if self.front.is_empty() {
            self.front.push(data);
            return (true, vec![]);
        }
        // Already present (by node equality)?
        if self.front.contains(&data) {
            return (true, vec![]);
        }

        let data_key = crate::native::core::sort_key(&data.nodes);

        // Find insertion position (sorted by sort_key ascending).
        let insert_pos = self
            .front
            .partition_point(|e| crate::native::core::sort_key(&e.nodes) < data_key);
        self.front.insert(insert_pos, data.clone());

        let mut to_remove: Vec<usize> = Vec::new();
        let n = self.front.len();

        // Randomised cleanup to the right (larger sort_key entries).
        // Mirror Python's LazySequenceCopy.pop: sample without replacement from
        // the pool of right-side indices.
        let mut available: Vec<usize> = (insert_pos + 1..n).collect();
        let mut failures = 0;
        while !available.is_empty() && failures < 10 {
            let pick = self.rng.random_range(0..available.len());
            let j = available.swap_remove(pick);
            let dom = dominance(&data, &self.front[j]);
            debug_assert_ne!(dom, DominanceRelation::RightDominates);
            if dom == DominanceRelation::LeftDominates {
                to_remove.push(j);
                failures = 0;
            } else {
                failures += 1;
            }
        }

        // Check elements to the left (smaller sort_key) for dominance
        // of `data`.
        let mut dominators: Vec<usize> = vec![insert_pos];
        let mut done = insert_pos == 0;
        let mut i = if insert_pos > 0 { insert_pos - 1 } else { 0 };
        while !done && dominators.len() < 10 {
            let candidate_idx = i;
            let mut dominated_by_some = false;
            let mut j = 0;
            while j < dominators.len() {
                let v_idx = dominators[j];
                // We need temporary immutable borrows to call dominance().
                // Clone the slice indices to avoid borrow conflicts.
                let (candidate_clone, v_clone) = {
                    let c = self.front[candidate_idx].clone();
                    let v = self.front[v_idx].clone();
                    (c, v)
                };
                let dom = dominance(&candidate_clone, &v_clone);
                match dom {
                    DominanceRelation::LeftDominates => {
                        to_remove.push(v_idx);
                        dominators[j] = candidate_idx;
                        dominated_by_some = false;
                        j += 1;
                    }
                    DominanceRelation::RightDominates => {
                        to_remove.push(candidate_idx);
                        dominated_by_some = true;
                        break;
                    }
                    DominanceRelation::Equal => {
                        dominated_by_some = true;
                        break;
                    }
                    DominanceRelation::NoDominance => {
                        j += 1;
                    }
                }
            }
            if !dominated_by_some {
                dominators.push(candidate_idx);
            }
            if i == 0 {
                done = true;
            } else {
                i -= 1;
            }
        }

        // Remove dominated entries (in reverse index order to preserve indices).
        to_remove.sort_unstable();
        to_remove.dedup();
        let evicted: Vec<ConjectureRunResult> = to_remove
            .iter()
            .rev()
            .map(|&idx| self.front.remove(idx))
            .collect();

        // Return whether `data` survived the purge plus the evicted entries.
        let in_front = self.front.contains(&data);
        (in_front, evicted)
    }

    /// Check whether `data` is currently in the pareto front.
    pub fn contains(&self, data: &ConjectureRunResult) -> bool {
        self.front.contains(data)
    }

    /// Iterate over the entries in the pareto front (sorted by sort_key).
    pub fn iter(&self) -> std::slice::Iter<'_, ConjectureRunResult> {
        self.front.iter()
    }

    pub fn len(&self) -> usize {
        self.front.len()
    }

    pub fn is_empty(&self) -> bool {
        self.front.is_empty()
    }
}

impl std::ops::Index<usize> for ParetoFront {
    type Output = ConjectureRunResult;
    fn index(&self, i: usize) -> &ConjectureRunResult {
        &self.front[i]
    }
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
    /// Override for `engine_module.CACHE_SIZE` — the maximum number of
    /// entries kept in the runner's `cached_test_function` LRU before
    /// the oldest is evicted.  `None` falls back to the default
    /// [`CACHE_SIZE`].
    pub cache_size: Option<usize>,
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
            cache_size: None,
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

    pub fn cache_size(mut self, n: usize) -> Self {
        self.cache_size = Some(n);
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
    /// Targeting / hill-climbing phase.  Mirrors `Phase.target` in
    /// Hypothesis's `phases.py`.
    Target,
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

/// Default capacity for the runner's `cached_test_function` LRU.
/// Mirrors `engine.py::CACHE_SIZE`; per-runner overrides flow through
/// [`NativeRunnerSettings::cache_size`].
const CACHE_SIZE: usize = 10_000;

/// Wall-clock budget for the shrink phase, in seconds.  Mirrors
/// `engine.py::MAX_SHRINKING_SECONDS` (5 minutes).
const MAX_SHRINKING_SECONDS: f64 = 5.0 * 60.0;

/// Default cap on the number of successful shrinks per interesting example.
/// Mirrors `engine.py::MAX_SHRINKS`.
const MAX_SHRINKS: usize = 500;

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
    events: HashMap<String, String>,
    /// Per-test-case targeting observations: maps target label to score.
    /// Mirrors `ConjectureData.target_observations`.
    pub target_observations: HashMap<String, f64>,
}

impl NativeConjectureData {
    fn new(ntc: NativeTestCase, buffer_size_limit: usize) -> Self {
        NativeConjectureData {
            ntc,
            data_id: NEXT_DATA_ID.fetch_add(1, Ordering::Relaxed),
            mark: None,
            bytes_drawn: 0,
            buffer_size_limit,
            events: HashMap::new(),
            target_observations: HashMap::new(),
        }
    }

    /// Construct a `NativeConjectureData` from a fixed choice prefix, using
    /// the default `CONJECTURE_BUFFER_SIZE`.  Mirrors Hypothesis's
    /// `ConjectureData.for_choices(choices)`.
    pub fn for_choices(choices: &[ChoiceValue]) -> Self {
        let ntc = NativeTestCase::for_choices(choices, None, None);
        Self::new(ntc, CONJECTURE_BUFFER_SIZE)
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

    pub fn draw_boolean(&mut self, p: f64) -> bool {
        // Each boolean choice contributes one byte to Hypothesis's
        // `data.length` (its `choices_to_bytes` encoding is a single
        // tag-and-payload byte).  Mirror that so a per-test-case
        // `buffer_size_limit` bound on choice-byte cost — the upstream
        // `engine_module.BUFFER_SIZE` knob — caps boolean-only paths
        // too, not just `draw_bytes` accumulation.
        if self.bytes_drawn.saturating_add(1) > self.buffer_size_limit {
            std::panic::panic_any(STOP_TEST_PANIC);
        }
        match self.ntc.weighted(p, None) {
            Ok(v) => {
                self.bytes_drawn += 1;
                v
            }
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
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

    pub fn mark_invalid(&mut self, why: Option<String>) -> ! {
        if let Some(reason) = why {
            self.events.insert("invalid because".to_string(), reason);
        }
        self.mark = Some((MarkKind::Invalid, None));
        let data_id = self.data_id;
        std::panic::panic_any(MarkPanic { data_id })
    }

    pub fn events(&self) -> &HashMap<String, String> {
        &self.events
    }

    pub fn start_span(&mut self, label: u64) {
        self.ntc.start_span(label);
    }

    pub fn stop_span(&mut self) {
        self.stop_span_with_discard(false);
    }

    /// Variant of `stop_span` that flags the span as discarded.  Mirrors
    /// `data.stop_span(discard=True)` in Hypothesis: filter-style retry
    /// loops use it to mark unsuccessful attempts so the shrinker's
    /// `remove_discarded` pass can prune them.
    pub fn stop_span_with_discard(&mut self, discard: bool) {
        self.ntc.stop_span(discard);
    }

    pub fn nodes(&self) -> &[ChoiceNode] {
        &self.ntc.nodes
    }

    pub fn choices(&self) -> Vec<ChoiceValue> {
        self.ntc.nodes.iter().map(|n| n.value.clone()).collect()
    }

    /// Accessor for the status recorded on the underlying test case.
    /// Used by `new_shrinker` predicates (`|d| d.status() ==
    /// Status::Interesting`).
    pub fn status(&self) -> Status {
        match &self.mark {
            Some((MarkKind::Interesting, _)) => Status::Interesting,
            Some((MarkKind::Invalid, _)) => Status::Invalid,
            None => Status::Valid,
        }
    }
}

/// Data-tree accessor for `runner.tree.is_exhausted`.
#[non_exhaustive]
pub struct NativeDataTreeView<'a> {
    runner: &'a NativeConjectureRunner,
}

impl<'a> NativeDataTreeView<'a> {
    pub fn is_exhausted(&self) -> bool {
        self.runner.tree_root.is_exhausted
    }

    /// Walk the data tree along `choices` and return `true` when the
    /// path terminates at a recorded leaf.  Mirrors
    /// `DataTree.simulate_test_function(data)`: a `true` return is the
    /// "no `PreviouslyUnseenBehaviour`" path; `false` is upstream's
    /// raise.  Simulation, like upstream, does not bump `call_count`
    /// or repopulate the runner's `test_cache`, so a successful
    /// simulation followed by `cached_test_function(choices)` against
    /// an evicted cache entry will still re-execute the test
    /// function.
    pub fn simulate_test_function(&self, choices: &[ChoiceValue]) -> bool {
        let mut current = &self.runner.tree_root;
        for value in choices {
            let key = ChoiceValueKey::from(value);
            match current.children.get(&key) {
                Some(child) => current = child,
                None => return false,
            }
        }
        current.conclusion.is_some()
    }
}

/// Run a shrinker user function (one that uses the `NativeConjectureData`
/// API) on a `NativeTestCase`, catching panic-based marks.  Returns
/// `(is_interesting, nodes, spans, has_discards)`.  Used by
/// `NativeShrinker::from_choices` to avoid duplicating the run-and-catch
/// logic.
fn run_shrinker_user_fn(
    user_fn: &mut dyn FnMut(&mut NativeConjectureData),
    ntc: NativeTestCase,
) -> (bool, Vec<ChoiceNode>, Vec<Span>, bool) {
    let mut data = NativeConjectureData::new(ntc, CONJECTURE_BUFFER_SIZE);
    let my_id = data.data_id;
    let res = catch_unwind(AssertUnwindSafe(|| user_fn(&mut data)));
    let interesting = match res {
        Ok(()) => false,
        Err(p) => {
            if let Some(mp) = p.downcast_ref::<MarkPanic>() {
                if mp.data_id == my_id {
                    matches!(&data.mark, Some((MarkKind::Interesting, _)))
                } else {
                    std::panic::resume_unwind(p)
                }
            } else if p.downcast_ref::<&'static str>().copied() == Some(STOP_TEST_PANIC) {
                false
            } else {
                // Arbitrary user panic → treat as interesting.
                true
            }
        }
    };
    let spans: Vec<Span> = data.ntc.spans.iter().cloned().collect();
    let has_discards = data.ntc.has_discards;
    let nodes = std::mem::take(&mut data.ntc.nodes);
    (interesting, nodes, spans, has_discards)
}

/// Span snapshot shared between `NativeShrinker` and the closure inside
/// its `Shrinker`.  Updated on every interesting test run so
/// `remove_discarded` can inspect the latest span structure.
#[derive(Default, Clone)]
struct SpanSnapshot {
    spans: Vec<Span>,
    has_discards: bool,
}

/// Shrinker handle returned by `runner.new_shrinker(data, predicate)` or
/// by the local `shrinking_from` helper in conjecture engine tests.  Wraps
/// a concrete [`Shrinker`] plus span bookkeeping needed by
/// `remove_discarded`.
pub struct NativeShrinker {
    inner: Shrinker<'static>,
    /// Spans from the last interesting test run; updated by the
    /// closure baked into `inner.test_fn`.
    span_snapshot: Rc<RefCell<SpanSnapshot>>,
}

/// Span view for `NativeShrinker::shrink_target()`.
pub struct NativeShrinkTarget {
    pub has_discards: bool,
    pub spans: Vec<NativeShrinkSpan>,
}

/// A span entry as visible to tests: just the fields they assert on.
pub struct NativeShrinkSpan {
    pub discarded: bool,
    pub choice_count: usize,
}

impl NativeShrinker {
    /// Build a `NativeShrinker` from initial choices and a user test function.
    /// The `user_fn` uses the same `NativeConjectureData` API as a runner
    /// callback: call `data.mark_interesting(...)` to flag an interesting case.
    /// Used by the local `shrinking_from` helper inside engine tests.
    pub fn from_choices<F>(initial: Vec<ChoiceValue>, mut user_fn: F) -> Self
    where
        F: FnMut(&mut NativeConjectureData) + 'static,
    {
        let snapshot = Rc::new(RefCell::new(SpanSnapshot::default()));
        let snapshot_clone = Rc::clone(&snapshot);

        // Seed: run initial choices to get initial nodes + span data.
        let ntc = NativeTestCase::for_choices(&initial, None, None);
        let (ok, initial_nodes, spans, has_discards) = run_shrinker_user_fn(&mut user_fn, ntc);
        assert!(ok, "initial choices did not trigger mark_interesting");
        {
            let mut s = snapshot.borrow_mut();
            s.spans = spans;
            s.has_discards = has_discards;
        }

        let test_fn = Box::new(move |candidate: &[ChoiceNode]| {
            let values: Vec<ChoiceValue> = candidate.iter().map(|n| n.value.clone()).collect();
            let ntc = NativeTestCase::for_choices(&values, Some(candidate), None);
            let (is_interesting, nodes, spans, has_discards) =
                run_shrinker_user_fn(&mut user_fn, ntc);
            if is_interesting {
                let mut s = snapshot_clone.borrow_mut();
                s.spans = spans;
                s.has_discards = has_discards;
            }
            (is_interesting, nodes)
        });

        NativeShrinker {
            inner: Shrinker::new(test_fn, initial_nodes),
            span_snapshot: snapshot,
        }
    }

    /// Run the full shrink loop.  Mirrors `Shrinker.shrink()`.
    pub fn shrink(&mut self) {
        self.inner.shrink();
    }

    /// Run named shrink passes to fixation (loop until stable).
    /// Mirrors `Shrinker.fixate_shrink_passes(passes)`.
    ///
    /// Pass names: `"minimize_individual_choices"`, `"lower_common_node_offset"`,
    /// `"remove_discarded"`.
    pub fn fixate_shrink_passes(&mut self, passes: &[&str]) {
        use crate::native::core::sort_key;
        loop {
            let prev = sort_key(&self.inner.current_nodes);
            for &name in passes {
                match name {
                    "remove_discarded" => {
                        self.remove_discarded();
                    }
                    "lower_common_node_offset" => {
                        self.inner.lower_common_node_offset();
                    }
                    _ => {
                        self.inner.run_named_pass(name);
                    }
                }
            }
            let curr = sort_key(&self.inner.current_nodes);
            if curr == prev {
                break;
            }
        }
    }

    /// Accessor for the current shrink result nodes.
    pub fn current_nodes(&self) -> &[ChoiceNode] {
        &self.inner.current_nodes
    }

    /// Current choice values (values from current_nodes).
    pub fn choices(&self) -> Vec<ChoiceValue> {
        self.inner
            .current_nodes
            .iter()
            .map(|n| n.value.clone())
            .collect()
    }

    /// Mark node index `i` as changed.  Mirrors `Shrinker.mark_changed(i)`.
    pub fn mark_changed(&mut self, i: usize) {
        self.inner.mark_changed(i);
    }

    /// Joint-lower changed integer nodes by their common offset.
    /// Mirrors `Shrinker.lower_common_node_offset()`.
    pub fn lower_common_node_offset(&mut self) {
        self.inner.lower_common_node_offset();
    }

    /// Snapshot of the current shrink target's discard metadata.
    pub fn shrink_target(&self) -> NativeShrinkTarget {
        let s = self.span_snapshot.borrow();
        NativeShrinkTarget {
            has_discards: s.has_discards,
            spans: s
                .spans
                .iter()
                .map(|sp| NativeShrinkSpan {
                    discarded: sp.discarded,
                    choice_count: sp.choice_count(),
                })
                .collect(),
        }
    }

    /// Remove discarded spans from `current_nodes`.  Returns `true` if no
    /// discards remain after the pass, `false` if removing a discarded span
    /// produced a non-interesting result (the span can't be dropped).
    /// Mirrors `Shrinker.remove_discarded()`.
    pub fn remove_discarded(&mut self) -> bool {
        loop {
            let (spans, has_discards) = {
                let s = self.span_snapshot.borrow();
                (s.spans.clone(), s.has_discards)
            };
            if !has_discards {
                return true;
            }
            // Collect non-overlapping discarded spans with at least one choice.
            let mut discarded: Vec<(usize, usize)> = Vec::new();
            for span in &spans {
                if span.choice_count() > 0
                    && span.discarded
                    && discarded.last().is_none_or(|(_, end)| span.start >= *end)
                {
                    discarded.push((span.start, span.end));
                }
            }
            if discarded.is_empty() {
                // All discards are zero-length — can't remove anything.
                return true;
            }
            let mut attempt: Vec<ChoiceNode> = self.inner.current_nodes.clone();
            for (start, end) in discarded.iter().rev() {
                attempt.drain(*start..*end);
            }
            if !self.inner.consider(&attempt) {
                return false;
            }
            // After consider(), span_snapshot is updated by the test fn closure.
        }
    }
}

type RunnerTestFn = Box<dyn FnMut(&mut NativeConjectureData)>;
type TestFnResult = (
    Status,
    Vec<ChoiceNode>,
    Option<InterestingOrigin>,
    HashMap<String, f64>,
    HashSet<u64>,
    Vec<usize>,
);

/// Cached outcome of a [`NativeConjectureRunner::cached_test_function`]
/// invocation.  Mirrors `engine.py`'s `__data_cache` entries: the
/// terminal status plus enough state to refuse to re-run the test
/// function on a repeat call with the same choice prefix.
#[derive(Clone)]
struct CachedRun {
    status: Status,
    nodes: Vec<ChoiceNode>,
    origin: Option<InterestingOrigin>,
    target_observations: HashMap<String, f64>,
}

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
/// Walk `nodes` through `tree_root` ... (see full doc below)
fn record_tree(
    tree_root: &mut DataTreeNode,
    nodes: &[ChoiceNode],
    status: Status,
    kill_depths: &[usize],
) {
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

    // Mark kill depths as exhausted.  Mirrors Python's kill_branch():
    // when a span is closed with discard=True, the tree node at that
    // depth is marked exhausted so novel-prefix generation won't
    // re-explore it.
    for &depth in kill_depths {
        if depth < path.len() {
            // SAFETY: path[depth] is the only live reference to that node.
            let node = unsafe { &mut *path[depth] };
            node.is_exhausted = true;
        }
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
) -> TestFnResult {
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
    let target_observations = std::mem::take(&mut data.target_observations);
    let tags: HashSet<u64> = data.ntc.tags.iter().map(|t| t.label).collect();
    // Collect kill depths from discarded spans (each discarded span kills the
    // tree node at its end position, mirroring Python's kill_branch()).
    let kill_depths: Vec<usize> = data
        .ntc
        .spans
        .iter()
        .filter_map(|s| if s.discarded { Some(s.end) } else { None })
        .collect();
    let nodes = std::mem::take(&mut data.ntc.nodes);
    (
        status,
        nodes,
        origin,
        target_observations,
        tags,
        kill_depths,
    )
}

/// Return `true` if `choices` is a strict prefix of some path already
/// recorded in `tree_root`.  Mirrors Python's `simulate_best_attempt`
/// logic: if the choices walk into the tree but end at a non-terminal
/// node (one with known continuations), the call would result in an
/// EarlyStop without re-running the test function.
fn is_prefix_of_known_path(tree_root: &DataTreeNode, choices: &[ChoiceValue]) -> bool {
    let mut current = tree_root;
    for choice in choices {
        let key = ChoiceValueKey::from(choice);
        match current.children.get(&key) {
            Some(child) => current = child.as_ref(),
            None => return false, // novel choice value, not in tree
        }
        if current.conclusion.is_some() {
            // The path terminates here; `choices` extends beyond a known path.
            return false;
        }
    }
    // All choices consumed at a non-terminal node with known children.
    !current.children.is_empty()
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
    test_fn: RunnerTestFn,
    settings: NativeRunnerSettings,
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
    /// LRU cache for `cached_test_function` keyed by `choices_to_bytes`
    /// of the input choices.  Mirrors `engine.py`'s `__data_cache`: a
    /// repeat call with the same choice prefix returns the previously
    /// recorded outcome without re-executing the user's test function
    /// or bumping `call_count`.  Capacity is
    /// `settings.cache_size.unwrap_or(CACHE_SIZE)`.
    test_cache: LRUCache<Vec<u8>, CachedRun>,
    /// Approximate Pareto front of valid (and interesting) test results.
    pareto_front: ParetoFront,
    /// Best score seen per target label.
    pub best_observed_targets: HashMap<String, f64>,
    /// Best choice sequence seen per target label (for hill-climbing).
    best_choices_for_target: HashMap<String, Vec<ChoiceValue>>,
    /// How many times `optimise_targets()` has been called during
    /// `generate_new_examples()`.
    pub optimise_targets_call_count: usize,
}

impl NativeConjectureRunner {
    pub fn new<F>(test_fn: F, settings: NativeRunnerSettings, mut rng: SmallRng) -> Self
    where
        F: FnMut(&mut NativeConjectureData) + 'static,
    {
        let start = std::time::Instant::now();
        let cache_capacity = settings.cache_size.unwrap_or(CACHE_SIZE);
        let pareto_rng = SmallRng::seed_from_u64(rng.random::<u64>());
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
            test_cache: LRUCache::new(cache_capacity),
            pareto_front: ParetoFront::new(pareto_rng),
            best_observed_targets: HashMap::new(),
            best_choices_for_target: HashMap::new(),
            optimise_targets_call_count: 0,
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
            self.generate_new_examples();
        }

        // --- Target phase (when generate is skipped) ---
        // Mirrors `engine.py::_run` lines 1543-1546: if Phase.generate is
        // not active but Phase.target is, call optimise_targets() directly.
        let do_target = phases.contains(&Phase::Target);
        if self.exit_reason.is_none() && do_target && !do_generate {
            self.optimise_targets();
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
        target_observations: HashMap<String, f64>,
        tags: HashSet<u64>,
    ) {
        match status {
            Status::Valid => {
                self.valid_examples += 1;
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                // Update best observed targets.
                for (k, &v) in &target_observations {
                    let entry = self
                        .best_observed_targets
                        .entry(k.clone())
                        .or_insert(f64::NEG_INFINITY);
                    if v > *entry {
                        *entry = v;
                        self.best_choices_for_target
                            .insert(k.clone(), choices.clone());
                    }
                }
                // Only add to pareto front when target_observations is
                // non-empty.  Mirrors Python engine.py which only calls
                // pareto_front.add(data) when data.target_observations.
                let has_targets = !target_observations.is_empty();
                if has_targets {
                    let result = ConjectureRunResult {
                        status: Status::Valid,
                        nodes,
                        choices: choices.clone(),
                        target_observations,
                        origin: None,
                        tags,
                    };
                    let (added, evicted) = self.pareto_front.add(result);
                    if added {
                        self.save_to_pareto_key(&choices);
                    }
                    for e in evicted {
                        self.delete_from_pareto_key(&e.choices);
                    }
                }
            }
            Status::Invalid => self.invalid_examples += 1,
            Status::EarlyStop => self.overrun_examples += 1,
            Status::Interesting => {
                let origin = origin.expect("Interesting status carries an origin");
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                // Mirrors `engine.py::test_function` lines 685-712:
                //   * a fresh origin saves to primary and inserts;
                //   * an existing origin compares `sort_key(nodes)` and,
                //     when the new candidate is strictly smaller,
                //     downgrades the old primary entry to secondary,
                //     saves the new choices, replaces the stored example,
                //     and increments `shrinks`.
                let changed = match self.interesting_examples.get(&origin) {
                    None => true,
                    Some(existing) => {
                        if crate::native::core::sort_key(&nodes)
                            < crate::native::core::sort_key(&existing.nodes)
                        {
                            self.shrinks += 1;
                            self.downgrade_choices(&existing.choices);
                            true
                        } else {
                            false
                        }
                    }
                };
                if changed {
                    let new_origin = !self.interesting_examples.contains_key(&origin);
                    self.save_choices(&choices);
                    // Add to pareto front (interesting status >= valid);
                    // persist to db and evict dominated entries.
                    let has_targets = !target_observations.is_empty();
                    let pareto_result = ConjectureRunResult {
                        status: Status::Interesting,
                        nodes: nodes.clone(),
                        choices: choices.clone(),
                        target_observations,
                        origin: Some(origin.clone()),
                        tags,
                    };
                    let (added, evicted) = self.pareto_front.add(pareto_result);
                    if added && has_targets {
                        self.save_to_pareto_key(&choices);
                    }
                    for e in evicted {
                        self.delete_from_pareto_key(&e.choices);
                    }
                    self.interesting_examples.insert(
                        origin.clone(),
                        InterestingExample {
                            nodes,
                            choices,
                            origin,
                        },
                    );
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
    }

    /// Save `choices` under the pareto sub-key.  Mirrors the
    /// `engine.py::test_function` path that calls
    /// `save_choices(data.choices, sub_key=b"pareto")` when a result is
    /// newly admitted to the pareto front.
    fn save_to_pareto_key(&self, choices: &[ChoiceValue]) {
        if let (Some(db), Some(key)) = (
            self.settings.database.as_ref(),
            self.database_key.as_deref(),
        ) {
            let pk = sub_key(key, b"pareto");
            db.save(&pk, &choices_to_bytes(choices));
        }
    }

    /// Delete `choices` from under the pareto sub-key.  Mirrors
    /// `engine.py::on_pareto_evict`: called when a pareto-front entry
    /// is dominated and removed.
    fn delete_from_pareto_key(&self, choices: &[ChoiceValue]) {
        if let (Some(db), Some(key)) = (
            self.settings.database.as_ref(),
            self.database_key.as_deref(),
        ) {
            let pk = sub_key(key, b"pareto");
            db.delete(&pk, &choices_to_bytes(choices));
        }
    }

    /// Mirrors `engine.py::downgrade_choices`: move the stored bytes for
    /// `choices` from the primary key to the secondary key.  Used when a
    /// smaller interesting example arrives for an origin already in
    /// `interesting_examples` — the previous best is no longer minimal
    /// but is still worth keeping as a fallback shrink target.
    fn downgrade_choices(&self, choices: &[ChoiceValue]) {
        if let (Some(db), Some(key)) = (
            self.settings.database.as_ref(),
            self.database_key.as_deref(),
        ) {
            let bytes = crate::native::database::serialize_choices(choices);
            let secondary = sub_key(key, b"secondary");
            db.move_value(key, &secondary, &bytes);
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
            let ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
            let (status, _, _, _, _, _) = run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
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

        // Mirrors `engine.py::shrink_interesting_examples` line 1597:
        // before the per-origin shrink loop, replay any leftover
        // secondary-key entries through `cached_test_function` and
        // delete them.  This both narrows the saved corpus to entries
        // that are still useful and exercises the
        // `record_test_result` replace+downgrade path on entries
        // whose `sort_key` is smaller than the current best.
        self.clear_secondary_key();

        for origin in origins {
            let initial = self.interesting_examples[&origin].nodes.clone();
            // Nothing to shrink if no choices were recorded (e.g.
            // `test_no_read_no_shrink`).
            if initial.is_empty() {
                continue;
            }

            let max_shrinks = self.settings.max_shrinks.unwrap_or(MAX_SHRINKS);
            let remaining = max_shrinks.saturating_sub(self.shrinks);

            let test_fn = &mut self.test_fn;
            let call_count = &mut self.call_count;
            let report_multiple_bugs = self.settings.report_multiple_bugs;
            let target = origin.clone();
            let (shrunk, improvements, downgraded) = {
                let mut shrinker = Shrinker::new(
                    Box::new(|candidate: &[ChoiceNode]| {
                        *call_count += 1;
                        let choices: Vec<ChoiceValue> =
                            candidate.iter().map(|n| n.value.clone()).collect();
                        let ntc = NativeTestCase::for_choices(&choices, Some(candidate), None);
                        let (status, actual_nodes, found_origin, _target_obs, _tags, _kill_depths) =
                            run_test_fn(test_fn, ntc, buffer_size_limit);
                        // Mirrors `engine.py`'s per-target predicate
                        // (`d.interesting_origin == target`): when
                        // `report_multiple_bugs` is on, slipping to a
                        // different origin's minimum is rejected.
                        // Slips are only allowed when the user has
                        // opted in via `report_multiple_bugs=false`.
                        let matches_target = match (&found_origin, report_multiple_bugs) {
                            (Some(o), true) => *o == target,
                            (_, false) => true,
                            (None, true) => false,
                        };
                        (
                            status == Status::Interesting && matches_target,
                            actual_nodes,
                        )
                    }),
                    initial,
                );
                shrinker.max_improvements = Some(remaining);
                shrinker.shrink();
                (
                    shrinker.current_nodes,
                    shrinker.improvements,
                    shrinker.downgraded,
                )
            };

            // Mirrors `engine.py::test_function` lines 698-714:
            // each improvement the shrinker found increments `shrinks`,
            // downgrades the displaced best to the secondary corpus, and
            // saves the new best to the primary corpus.
            self.shrinks += improvements;
            for old_choices in &downgraded {
                self.downgrade_choices(old_choices);
            }

            let choices: Vec<ChoiceValue> = shrunk.iter().map(|n| n.value.clone()).collect();
            // Save the final minimum to primary.  Mirrors the
            // `save_choices(data.choices)` call in `engine.py::test_function`
            // line 703 that follows each `downgrade_choices`.
            if improvements > 0 {
                self.save_choices(&choices);
            }
            self.interesting_examples.insert(
                origin.clone(),
                InterestingExample {
                    nodes: shrunk,
                    choices,
                    origin,
                },
            );

            // Mirrors `engine.py` lines 713-714: stop shrinking when the
            // budget is exhausted.
            if self.shrinks >= max_shrinks {
                self.exit_reason = Some(ExitReason::MaxShrinks);
                return;
            }
        }
    }

    /// Seeded replay entry point.  Mirrors
    /// `ConjectureRunner.cached_test_function` for the subset that the
    /// ported tests exercise: run the test function with `choices` as a
    /// forced prefix, update the runner's call / status / bug counters,
    /// and feed the resulting `nodes` into the data tree so the
    /// novel-prefix walker won't re-pick the same prefix later.
    ///
    /// A repeat call with a choice prefix that's already in the LRU
    /// returns its prior outcome without re-running the test function;
    /// `call_count` and the status counters are only bumped on cache
    /// miss, matching `engine.py::cached_test_function`.
    ///
    /// Returns a [`ConjectureRunResult`] describing the outcome (either
    /// freshly computed or reconstructed from the LRU cache).
    pub fn cached_test_function(&mut self, choices: &[ChoiceValue]) -> ConjectureRunResult {
        let key = crate::native::database::serialize_choices(choices);
        if let Some(cached) = self.test_cache.get(&key) {
            let cached = cached.clone();
            let result_choices: Vec<ChoiceValue> =
                cached.nodes.iter().map(|n| n.value.clone()).collect();
            return ConjectureRunResult {
                status: cached.status,
                nodes: cached.nodes,
                choices: result_choices,
                target_observations: cached.target_observations,
                origin: cached.origin,
                tags: HashSet::new(),
            };
        }
        // If `choices` is a strict prefix of a known path in the tree,
        // return EarlyStop without re-running the test.  Mirrors Python's
        // `simulate_best_attempt` which returns `Overrun` for incomplete
        // prefixes without invoking the test function.
        if is_prefix_of_known_path(&self.tree_root, choices) {
            return ConjectureRunResult {
                status: Status::EarlyStop,
                nodes: vec![],
                choices: choices.to_vec(),
                target_observations: HashMap::new(),
                origin: None,
                tags: HashSet::new(),
            };
        }
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);
        let ntc = NativeTestCase::for_choices(choices, None, None);
        let (status, nodes, origin, target_observations, tags, kill_depths) =
            run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
        self.call_count += 1;
        record_tree(&mut self.tree_root, &nodes, status, &kill_depths);
        self.test_cache.insert(
            key,
            CachedRun {
                status,
                nodes: nodes.clone(),
                origin: origin.clone(),
                target_observations: target_observations.clone(),
            },
        );
        let result_choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        let result = ConjectureRunResult {
            status,
            nodes: nodes.clone(),
            choices: result_choices,
            target_observations: target_observations.clone(),
            origin: origin.clone(),
            tags: HashSet::new(),
        };
        self.record_test_result(status, nodes, origin, target_observations, tags);
        result
    }

    /// Variant of [`cached_test_function`] that allows the test to draw
    /// `extend` extra choices beyond the forced prefix.  Mirrors
    /// `engine.py::cached_test_function(..., extend=n)`.
    ///
    /// Key differences from the no-extend version:
    /// - A cached `OVERRUN` for this prefix is **not** re-used (the test
    ///   may succeed with additional choices).
    /// - The result is **not** cached if the test drew beyond the prefix
    ///   (i.e. `nodes.len() > choices.len()`).
    pub fn cached_test_function_extend(
        &mut self,
        choices: &[ChoiceValue],
        extend: usize,
    ) -> ConjectureRunResult {
        self.cached_test_function_with_extend(choices, Some(extend))
    }

    /// Variant of [`cached_test_function`] where the test can draw an
    /// unlimited number of extra choices beyond the forced prefix.
    /// Mirrors `engine.py::cached_test_function(..., extend="full")`.
    pub fn cached_test_function_full(&mut self, choices: &[ChoiceValue]) -> ConjectureRunResult {
        self.cached_test_function_with_extend(choices, None)
    }

    /// Internal implementation shared by `cached_test_function_extend` and
    /// `cached_test_function_full`.  `max_extend` of `None` = unlimited
    /// (`extend="full"`); `Some(n)` = at most `n` extra choices.
    fn cached_test_function_with_extend(
        &mut self,
        choices: &[ChoiceValue],
        max_extend: Option<usize>,
    ) -> ConjectureRunResult {
        let key = crate::native::database::serialize_choices(choices);
        // Re-use cached result only if it's NOT an Overrun (per Hypothesis
        // semantics: a cached overrun might succeed when extended).
        if let Some(cached) = self.test_cache.get(&key) {
            let cached = cached.clone();
            if cached.status != Status::EarlyStop || max_extend == Some(0) {
                let result_choices: Vec<ChoiceValue> =
                    cached.nodes.iter().map(|n| n.value.clone()).collect();
                return ConjectureRunResult {
                    status: cached.status,
                    nodes: cached.nodes,
                    choices: result_choices,
                    target_observations: cached.target_observations,
                    origin: cached.origin,
                    tags: HashSet::new(),
                };
            }
        }
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);
        // Use a probe NTC so draws beyond the prefix use the runner's RNG.
        let max_size = match max_extend {
            Some(ext) => choices.len() + ext,
            None => CONJECTURE_BUFFER_SIZE,
        };
        let probe_rng = SmallRng::seed_from_u64(self.rng.random::<u64>());
        let ntc = NativeTestCase::for_probe(choices, probe_rng, max_size);
        let (status, nodes, origin, target_observations, tags, kill_depths) =
            run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
        self.call_count += 1;
        record_tree(&mut self.tree_root, &nodes, status, &kill_depths);
        let result_choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        // Only cache if extend was NOT consumed (test stayed within forced prefix).
        let extend_consumed = nodes.len() > choices.len();
        if !extend_consumed {
            self.test_cache.insert(
                key,
                CachedRun {
                    status,
                    nodes: nodes.clone(),
                    origin: origin.clone(),
                    target_observations: target_observations.clone(),
                },
            );
        }
        let result = ConjectureRunResult {
            status,
            nodes: nodes.clone(),
            choices: result_choices,
            target_observations: target_observations.clone(),
            origin: origin.clone(),
            tags: HashSet::new(),
        };
        self.record_test_result(status, nodes, origin, target_observations, tags);
        result
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
        NativeDataTreeView { runner: self }
    }

    /// Produce a novel choice-sequence prefix.  Mirrors
    /// `ConjectureRunner.generate_novel_prefix`.
    pub fn generate_novel_prefix(&mut self) -> Vec<ChoiceValue> {
        generate_novel_prefix(&self.tree_root, &mut self.rng)
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
            let ntc = NativeTestCase::for_choices(&choices, None, None);
            let (status, nodes, origin, _target_obs, _tags, _kill_depths) =
                run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
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
                let ntc = NativeTestCase::for_choices(&choices, None, None);
                let (status, nodes, origin, target_obs, tags, _kill_depths) =
                    run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
                self.call_count += 1;
                // Check if this replayed entry is still in the pareto front.
                // If not, delete it from the database.
                let pareto_result = ConjectureRunResult {
                    status,
                    nodes: nodes.clone(),
                    choices: nodes.iter().map(|n| n.value.clone()).collect(),
                    target_observations: target_obs.clone(),
                    origin: origin.clone(),
                    tags: tags.clone(),
                };
                let (still_in_front, evicted) = self.pareto_front.add(pareto_result);
                if !still_in_front {
                    db.delete(&pareto_key, existing);
                }
                for e in evicted {
                    db.delete(&pareto_key, &choices_to_bytes(&e.choices));
                }
                self.record_test_result(status, nodes, origin, target_obs, tags);
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

        for c in &corpus {
            let Some(choices) = choices_from_bytes(c) else {
                db.delete(&secondary, c);
                continue;
            };
            // `max_primary` is the shortlex-largest entry currently in
            // `interesting_examples`; a `cached_test_function` call may
            // have just replaced an entry via the `record_test_result`
            // shrink path, so it must be recomputed inside the loop to
            // mirror upstream's `clear_secondary_key`.
            let primary_set: std::collections::HashSet<Vec<u8>> = self
                .interesting_examples
                .values()
                .map(|e| choices_to_bytes(&e.choices))
                .collect();
            let max_primary = primary_set
                .iter()
                .max_by(|a, b| shortlex_cmp(a, b))
                .cloned();
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

    /// Pareto front accessor.  Mirrors `ConjectureRunner.pareto_front`
    /// (the `ParetoFront` object).
    pub fn pareto_front(&self) -> &ParetoFront {
        &self.pareto_front
    }

    /// Mutable accessor for the pareto front.
    pub fn pareto_front_mut(&mut self) -> &mut ParetoFront {
        &mut self.pareto_front
    }

    /// Run the pareto-front optimiser (shrink each front element toward a
    /// smaller result that still dominates the original).  Mirrors
    /// `engine.py::pareto_optimise` / `pareto.ParetoOptimiser.run`.
    ///
    /// Requires `allow_transition` support in `src/native/shrinker/` —
    /// not yet implemented.
    pub fn pareto_optimise(&mut self) {
        todo!(
            "pareto_optimise: needs allow_transition callback in the Shrinker \
             (see ParetoOptimiser in hypothesis/internal/conjecture/pareto.py)"
        )
    }

    /// Generate new test examples (generation phase).  Mirrors the
    /// generation loop from `engine.py::generate_new_examples`.
    /// Extracted so it can be called independently (e.g. by tests that
    /// need to run targeting without the full `run()` lifecycle) and so
    /// `optimise_targets` can be triggered mid-generation.
    pub fn generate_new_examples(&mut self) {
        let phases = self
            .settings
            .phases
            .clone()
            .unwrap_or_else(|| vec![Phase::Reuse, Phase::Generate, Phase::Shrink]);
        let do_shrink = phases.contains(&Phase::Shrink);
        let buffer_size_limit = self
            .settings
            .buffer_size_limit
            .unwrap_or(CONJECTURE_BUFFER_SIZE);

        let small_example_cap = (self.settings.max_examples / 10).min(50);
        let optimise_at = (self.settings.max_examples / 2)
            .max(small_example_cap + 1)
            .max(10);
        let mut ran_optimisations = false;

        // Health-check window state.  Mirrors `engine.py::record_for_health_check`:
        // counts are tracked for the first `hc_max_valid` valid examples, then the
        // window closes.  Hypothesis uses 10/50/20 as the valid/invalid/overrun caps.
        let hc_max_valid: usize = 10;
        let hc_max_invalid: usize = 50;
        let hc_max_overrun: usize = 20;
        // Threshold mirrors `max(1.0, 5 * deadline)` with the default 200 ms deadline.
        let hc_too_slow_threshold = std::time::Duration::from_secs(1);
        let mut hc_valid: usize = 0;
        let mut hc_invalid: usize = 0;
        let mut hc_overrun: usize = 0;
        let mut hc_draw_time = std::time::Duration::ZERO;

        // One-shot "all simplest" probe.
        if self.should_generate_more(do_shrink) && !self.tree_root.is_exhausted {
            let ntc = NativeTestCase::for_simplest(CONJECTURE_BUFFER_SIZE);
            let probe_start = std::time::Instant::now();
            let (status, nodes, origin, target_obs, tags, kill_depths) =
                run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
            let probe_elapsed = probe_start.elapsed();
            self.call_count += 1;
            record_tree(&mut self.tree_root, &nodes, status, &kill_depths);
            self.record_test_result(status, nodes, origin, target_obs, tags);

            // LargeBaseExample: the simplest possible input already overruns
            // the byte budget.  Mirrors `engine.py` lines 1163-1187.
            if status == Status::EarlyStop
                && self.interesting_examples.is_empty()
                && !self
                    .settings
                    .suppress_health_check
                    .contains(&HealthCheckLabel::LargeBaseExample)
            {
                panic!(
                    "FailedHealthCheck: LargeBaseExample — the smallest natural \
                     input for this test is very large. This makes it difficult \
                     for Hegel to generate good inputs, especially when trying to \
                     shrink failing inputs. Consider reducing the amount of data \
                     generated. If this is expected, suppress with \
                     suppress_health_check = [HealthCheck::LargeBaseExample]."
                );
            }

            // Update health-check window counters.
            if self.interesting_examples.is_empty() {
                match status {
                    Status::Valid => {
                        hc_valid += 1;
                        hc_draw_time += probe_elapsed;
                    }
                    Status::Invalid => {
                        hc_invalid += 1;
                        hc_draw_time += probe_elapsed;
                    }
                    Status::EarlyStop => {
                        hc_overrun += 1;
                        hc_draw_time += probe_elapsed;
                    }
                    Status::Interesting => {}
                }
            }
        }

        loop {
            if self.set_exit_reason_if_done() {
                break;
            }
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
            let tc_start = std::time::Instant::now();
            let (status, nodes, origin, target_obs, tags, kill_depths) =
                run_test_fn(&mut self.test_fn, ntc, buffer_size_limit);
            let tc_elapsed = tc_start.elapsed();
            self.call_count += 1;
            record_tree(&mut self.tree_root, &nodes, status, &kill_depths);
            self.record_test_result(status, nodes, origin, target_obs, tags);

            // Update health-check window and fire any triggered checks.
            // Once an interesting example is found (bug detected), the window
            // closes — mirrors `record_for_health_check`'s early return on INTERESTING.
            if self.interesting_examples.is_empty() && hc_valid < hc_max_valid {
                match status {
                    Status::Valid => {
                        hc_valid += 1;
                        hc_draw_time += tc_elapsed;
                    }
                    Status::Invalid => {
                        hc_invalid += 1;
                        hc_draw_time += tc_elapsed;
                        if hc_invalid >= hc_max_invalid
                            && !self
                                .settings
                                .suppress_health_check
                                .contains(&HealthCheckLabel::FilterTooMuch)
                        {
                            panic!(
                                "FailedHealthCheck: FilterTooMuch — it looks like \
                                 this test is filtering out too many inputs. \
                                 {hc_valid} valid inputs were generated, while \
                                 {hc_invalid} inputs were filtered out by assume(). \
                                 If this is expected, suppress with \
                                 suppress_health_check = [HealthCheck::FilterTooMuch]."
                            );
                        }
                    }
                    Status::EarlyStop => {
                        hc_overrun += 1;
                        hc_draw_time += tc_elapsed;
                        if hc_overrun >= hc_max_overrun
                            && !self
                                .settings
                                .suppress_health_check
                                .contains(&HealthCheckLabel::DataTooLarge)
                        {
                            panic!(
                                "FailedHealthCheck: DataTooLarge — generated inputs \
                                 routinely consumed more than the maximum allowed \
                                 entropy: {hc_valid} inputs were generated successfully, \
                                 while {hc_overrun} inputs exceeded the maximum allowed \
                                 entropy during generation. Try decreasing the amount \
                                 of data generated. If this is expected, suppress with \
                                 suppress_health_check = [HealthCheck::DataTooLarge]."
                            );
                        }
                    }
                    Status::Interesting => {}
                }

                // TooSlow: cumulative draw time in the health-check window
                // exceeds the threshold.  Mirrors `record_for_health_check`
                // lines 840-888 in `engine.py`.
                if hc_draw_time > hc_too_slow_threshold
                    && !self
                        .settings
                        .suppress_health_check
                        .contains(&HealthCheckLabel::TooSlow)
                {
                    panic!(
                        "FailedHealthCheck: TooSlow — input generation is slow: \
                         only {hc_valid} valid inputs after {:.2}s (threshold \
                         {:.2}s). Slow generation makes property testing much less \
                         effective. If this is expected, suppress with \
                         suppress_health_check = [HealthCheck::TooSlow].",
                        hc_draw_time.as_secs_f64(),
                        hc_too_slow_threshold.as_secs_f64(),
                    );
                }
            }

            if self.set_exit_reason_if_done() {
                break;
            }

            // Trigger optimise_targets once we've accumulated enough valid
            // examples.  Mirrors Hypothesis's `generate_new_examples` line
            // 1317-1323: fires unconditionally (regardless of phases) when
            // the valid-example budget crosses `optimise_at`.
            if !ran_optimisations && self.valid_examples >= optimise_at.max(small_example_cap) {
                ran_optimisations = true;
                self.optimise_targets_call_count += 1;
                self.optimise_targets();
            }
        }
    }

    /// Optimise all observed targets by hill-climbing from the best
    /// known choice sequence for each target.  Mirrors
    /// `engine.py::optimise_targets`.
    pub fn optimise_targets(&mut self) {
        let targets: Vec<String> = self.best_observed_targets.keys().cloned().collect();
        if targets.is_empty() {
            return;
        }
        let mut max_improvements: usize = 10;
        loop {
            let prev_valid = self.valid_examples;
            let mut any_improvements = false;
            for target in targets.clone() {
                let improvements = self.hill_climb(&target, max_improvements);
                if improvements > 0 {
                    any_improvements = true;
                }
            }
            max_improvements = max_improvements.saturating_mul(2);
            if !any_improvements || prev_valid == self.valid_examples {
                break;
            }
        }
    }

    /// Hill-climb from the best known choices for `target`, trying to
    /// increase the target score.  Returns the number of improvements
    /// found.  Mirrors `engine.py::_optimise_target`.
    fn hill_climb(&mut self, target: &str, max_improvements: usize) -> usize {
        let start_choices = match self.best_choices_for_target.get(target).cloned() {
            Some(c) => c,
            None => return 0,
        };
        let result = self.cached_test_function(&start_choices);
        if result.status < Status::Valid {
            return 0;
        }
        let mut current_choices = start_choices;
        let mut current_nodes = result.nodes.clone();
        let mut current_score = *result
            .target_observations
            .get(target)
            .unwrap_or(&f64::NEG_INFINITY);
        let mut improvements = 0;

        let mut i = current_nodes.len() as isize - 1;
        while i >= 0 && improvements <= max_improvements {
            let idx = i as usize;
            let node = &current_nodes[idx];
            if !node.was_forced {
                if let (ChoiceValue::Integer(_), ChoiceKind::Integer(_)) = (&node.value, &node.kind)
                {
                    improvements += self.find_integer_for_target(
                        target,
                        &mut current_choices,
                        &mut current_nodes,
                        &mut current_score,
                        max_improvements.saturating_sub(improvements),
                        idx,
                        1,
                    );
                }
            }
            i -= 1;
        }
        improvements
    }

    /// Try incrementing the integer at position `idx` in `current_choices`
    /// to increase the target score, using linear + exponential probing.
    /// Returns the number of improvements made.
    #[allow(clippy::too_many_arguments)]
    fn find_integer_for_target(
        &mut self,
        target: &str,
        current_choices: &mut Vec<ChoiceValue>,
        current_nodes: &mut Vec<ChoiceNode>,
        current_score: &mut f64,
        max_improvements: usize,
        idx: usize,
        step: i128,
    ) -> usize {
        let mut improvements = 0;
        let mut step = step;
        loop {
            if improvements >= max_improvements {
                break;
            }
            let current_val = match &current_choices[idx] {
                ChoiceValue::Integer(n) => *n,
                _ => break,
            };
            // Check integer constraint
            let max_val = match &current_nodes[idx].kind {
                ChoiceKind::Integer(ic) => ic.max_value,
                _ => break,
            };
            let new_val = current_val.saturating_add(step);
            if new_val > max_val {
                break;
            }
            let mut trial_choices = current_choices.clone();
            trial_choices[idx] = ChoiceValue::Integer(new_val);
            let result = self.cached_test_function(&trial_choices);
            if result.status < Status::Valid {
                break;
            }
            let new_score = *result
                .target_observations
                .get(target)
                .unwrap_or(&f64::NEG_INFINITY);
            if new_score > *current_score {
                *current_score = new_score;
                *current_choices = trial_choices;
                *current_nodes = result.nodes;
                // Update best for target
                self.best_observed_targets
                    .insert(target.to_string(), new_score);
                self.best_choices_for_target
                    .insert(target.to_string(), current_choices.clone());
                improvements += 1;
                step = step.saturating_mul(2);
            } else {
                break;
            }
        }
        improvements
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
pub fn fails_health_check<B>(label: HealthCheckLabel, build: B)
where
    B: FnOnce() -> NativeConjectureRunner,
{
    let prefix = match label {
        HealthCheckLabel::FilterTooMuch => "FailedHealthCheck: FilterTooMuch",
        HealthCheckLabel::TooSlow => "FailedHealthCheck: TooSlow",
        HealthCheckLabel::LargeBaseExample => "FailedHealthCheck: LargeBaseExample",
        HealthCheckLabel::DataTooLarge => "FailedHealthCheck: DataTooLarge",
    };
    let mut runner = build();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.run()));
    let payload = match result {
        Ok(()) => panic!(
            "expected a FailedHealthCheck panic with {prefix:?}, but run() returned normally"
        ),
        Err(p) => p,
    };
    let msg = if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else {
        panic!(
            "expected a FailedHealthCheck panic with {prefix:?}, \
             but got a non-string panic payload"
        )
    };
    assert!(
        msg.contains(prefix),
        "expected panic message to contain {prefix:?}, but got: {msg:?}"
    );
    assert!(
        runner.interesting_examples.is_empty(),
        "expected no interesting examples after FailedHealthCheck"
    );
}
