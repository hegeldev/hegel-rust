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

use std::collections::HashMap;
use std::sync::Arc;

use rand::rngs::SmallRng;

use crate::native::core::{ChoiceNode, ChoiceValue};
use crate::native::database::ExampleDatabase;

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
}

/// Construct an `InterestingOrigin` with the given stable id, so
/// `interesting_origin(n) == interesting_origin(m) iff n == m`.
/// Mirrors the `tests/conjecture/common.py::interesting_origin`
/// fixture.
pub fn interesting_origin(n: Option<i64>) -> InterestingOrigin {
    InterestingOrigin { id: n }
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

/// Test-case surface passed to the user's runner callback.  Stubbed:
/// each method's body is `todo!()` and is to be filled in as the
/// specific test that exercises it is ported.  Mirrors the subset of
/// `ConjectureData` methods used by `test_engine.py`.
#[non_exhaustive]
pub struct NativeConjectureData {
    _private: (),
}

impl NativeConjectureData {
    pub fn draw_bytes(&mut self, _min_size: usize, _max_size: usize) -> Vec<u8> {
        todo!("NativeConjectureData::draw_bytes")
    }

    pub fn draw_integer(&mut self, _min_value: i128, _max_value: i128) -> i128 {
        todo!("NativeConjectureData::draw_integer")
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

    pub fn mark_interesting(&mut self, _origin: InterestingOrigin) -> ! {
        todo!("NativeConjectureData::mark_interesting")
    }

    pub fn mark_invalid(&mut self) -> ! {
        todo!("NativeConjectureData::mark_invalid")
    }

    pub fn start_span(&mut self, _label: u64) {
        todo!("NativeConjectureData::start_span")
    }

    pub fn stop_span(&mut self) {
        todo!("NativeConjectureData::stop_span")
    }

    pub fn nodes(&self) -> &[ChoiceNode] {
        todo!("NativeConjectureData::nodes")
    }

    pub fn choices(&self) -> Vec<ChoiceValue> {
        todo!("NativeConjectureData::choices")
    }

    /// Accessor for the status recorded on the underlying test case.
    /// Used by `new_shrinker` predicates (`|d| d.status() ==
    /// Status::Interesting`).
    pub fn status(&self) -> crate::native::core::Status {
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

    /// Externally-visible bookkeeping.  `run()` populates these; tests
    /// read them back.  All `todo!()` accessors lift from here once the
    /// backing state is wired up.
    pub interesting_examples: HashMap<InterestingOrigin, InterestingExample>,
    pub exit_reason: Option<ExitReason>,
    pub shrinks: usize,
    pub call_count: usize,
    pub valid_examples: usize,
    pub statistics: HashMap<String, String>,
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
        NativeConjectureRunner {
            test_fn: Box::new(test_fn),
            settings,
            rng,
            database_key: None,
            interesting_examples: HashMap::new(),
            exit_reason: None,
            shrinks: 0,
            call_count: 0,
            valid_examples: 0,
            statistics: HashMap::new(),
            ignore_limits: false,
        }
    }

    pub fn with_database_key(mut self, key: Vec<u8>) -> Self {
        self.database_key = Some(key);
        self
    }

    /// Main entry point.  Runs the generation + shrink phases to
    /// completion and populates `interesting_examples` / `exit_reason`
    /// / `shrinks` / `call_count` / `valid_examples`.
    pub fn run(&mut self) {
        todo!("NativeConjectureRunner::run — generation + shrink loop")
    }

    /// Run only the shrink phase against an already-populated
    /// `interesting_examples`.  Used by `test_shrink_after_max_examples`
    /// / `test_shrink_after_max_iterations`.
    pub fn shrink_interesting_examples(&mut self) {
        todo!("NativeConjectureRunner::shrink_interesting_examples")
    }

    /// Seeded replay entry point.  Mirrors
    /// `ConjectureRunner.cached_test_function`.
    pub fn cached_test_function(&mut self, _choices: &[ChoiceValue]) -> NativeConjectureData {
        todo!("NativeConjectureRunner::cached_test_function")
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
        todo!("NativeConjectureRunner::secondary_key")
    }

    /// Key under which the runner stores the pareto front.  Mirrors
    /// `ConjectureRunner.pareto_key`.
    pub fn pareto_key(&self) -> Vec<u8> {
        todo!("NativeConjectureRunner::pareto_key")
    }

    /// Primary database key (as passed to `with_database_key`).
    pub fn database_key(&self) -> Option<&[u8]> {
        self.database_key.as_deref()
    }

    /// Save a choice sequence under the primary database key.  Mirrors
    /// `ConjectureRunner.save_choices`.
    pub fn save_choices(&mut self, _choices: &[ChoiceValue]) {
        todo!("NativeConjectureRunner::save_choices")
    }

    /// Load every stored value for `database_key` and replay them as
    /// the first phase of generation.  Mirrors
    /// `ConjectureRunner.reuse_existing_examples`.
    pub fn reuse_existing_examples(&mut self) {
        todo!("NativeConjectureRunner::reuse_existing_examples")
    }

    /// Delete every stored value under `secondary_key`.  Mirrors
    /// `ConjectureRunner.clear_secondary_key`.
    pub fn clear_secondary_key(&mut self) {
        todo!("NativeConjectureRunner::clear_secondary_key")
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
