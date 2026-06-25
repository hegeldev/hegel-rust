//! Native engine driver.
//!
//! [`explore`] is the engine driver: it owns the database replay,
//! generation, and shrinking phases, using the supplied `run_case` callback
//! to actually execute each test body, and returns the report — every
//! distinct bug's shrunk counterexample as a [`Failure`] carrying the
//! reproduce blob. The caller ([`crate::embed::run_native`], and ultimately
//! the client) replays each blob to produce the final report. Every test case
//! `explore` runs is non-final.
//!
//! Inside, [`Engine`] wraps the `run_case` callback together with
//! a shrink-result cache, exposing `run` / `run_shrink_with_origin` /
//! `run_probe_with_origin` so the surrounding shrinker
//! and span-mutation passes can drive replays.

use std::collections::{HashMap, hash_map::Entry};

use rand::RngExt;

use crate::backend::{DataSource, Failure, RunError, TestCaseResult};
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, MAX_SHRINKING_SECONDS, NativeTestCase, Span, SpanEvent,
    Spans, Status, sort_key,
};
use crate::native::data_source::NativeDataSource;
use crate::native::database::{
    DirectoryTestCaseDatabase, TestCaseDatabase, deserialize_choices, serialize_choices,
};
use crate::native::rng::EngineRng;
use crate::native::shrinker::{ShrinkRun, Shrinker};
use crate::settings::{Backend, Database, HealthCheck, Phase, Settings, Verbosity};

/// One run's worth of results: status, the realised choice nodes and
/// spans, and (for `Status::Interesting`) the opaque origin string
/// identifying *where* the panic happened.  The origin is supplied by
/// [`crate::run_lifecycle::run_test_case`] from the captured panic
/// `file:line:col`; per-origin shrinking and database storage key on it.
#[derive(Clone)]
pub struct RunResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub spans: Vec<Span>,
    pub origin: Option<String>,
    /// `tc.target()` observations recorded during the test case, keyed by
    /// label. Empty for tests that don't call `tc.target()`.
    pub target_observations: HashMap<String, f64>,
    /// Live span open/close events (with draw positions) from this execution,
    /// for folding into the choice tree. Empty on a result reconstructed from
    /// the tree (the events are already recorded there).
    pub span_events: Vec<(usize, SpanEvent)>,
}

const RANDOM_GENERATION_BATCH: u64 = 10;
const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Maximum number of *total* filtered (assume()-failed) test cases — counted
/// while fewer than [`HEALTH_CHECK_MAX_VALID`] valid test cases have been seen —
/// before FilterTooMuch is reported. Mirrors Hypothesis's `max_invalid_draws`
/// (`engine.py`).
const FILTER_TOO_MUCH_THRESHOLD: u64 = 50;

/// Target valid rate `r` below which the generation phase gives up, and the
/// confidence `c` with which we want to conclude the true valid rate is below
/// it before doing so. Hypothesis uses `r = 0.01`, `c = 0.99` to feed
/// `_invalid_thresholds`; see
/// <https://github.com/HypothesisWorks/hypothesis/issues/4623> for the
/// derivation. With these, [`invalid_thresholds`] yields `(458, 100)`, so an
/// always-reject test gives up after 459 cases.
const INVALID_TARGET_RATE: f64 = 0.01;
const INVALID_TARGET_CONFIDENCE: f64 = 0.99;

/// Cumulative wall-clock threshold across the generation phase before
/// TooSlow fires.
///
/// Hegel-Rust deliberately doesn't have a `deadline` setting (tight timing
/// on tests tends to be more trouble than it's worth in this ecosystem),
/// so 30s is a generous fixed budget rather than a per-deadline scaling.
const TOO_SLOW_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(30);

/// Health checks (TooSlow / FilterTooMuch / TestCasesTooLarge) are evaluated
/// only while the run has fewer than this many valid examples on record.
const HEALTH_CHECK_MAX_VALID: u64 = 10;

/// Number of oversized (overrun) test cases — those that exhaust the choice
/// buffer before completing — that trips TestCasesTooLarge while the run still
/// has fewer than `HEALTH_CHECK_MAX_VALID` valid examples. Mirrors
/// Hypothesis's `max_overrun_draws`.
const MAX_OVERRUN_DRAWS: u64 = 20;

/// Run the exploration half of a test run — database replay, generation, and
/// shrinking — and return one [`Failure`] per distinct bug, each carrying the
/// origin the engine grouped on and the base64 reproduce blob encoding the
/// minimal counterexample's choices. `Err` means the run itself failed (health
/// check, nondeterminism) before reaching a verdict.
///
/// The caller replays each blob (via `hegel_test_case_from_blob`) to produce
/// the final report. Every test case this runs is non-final.
pub(crate) fn explore(
    settings: &Settings,
    database_key: Option<&str>,
    run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>),
) -> Result<Vec<Failure>, RunError> {
    run_main(
        settings,
        database_key,
        run_case,
        TOO_SLOW_THRESHOLD,
        std::time::Duration::from_secs(MAX_SHRINKING_SECONDS),
    )
}

// (The old `ReproduceRunner` TestRunner lived here. Blob replay now goes
// through the C ABI's `hegel_test_case_from_blob` / `data_source_for_blob`,
// driven by hegeltest's `drive_blob_replay`, so the runner is gone.)

/// Run one test case (used by `Mode::SingleTestCase`) and return its
/// failure, if any.
///
/// A single test case is not a property-test run — there is no exploration,
/// shrinking, or replay — so it bypasses [`explore`] entirely; `run_case`
/// runs the one case, which is its own report.
pub(crate) fn run_single_case(
    settings: &Settings,
    database_key: Option<&str>,
    run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>),
) -> Option<Failure> {
    // Honour `settings.seed` / `settings.derandomize` here for the same
    // reason `run_main` does: callers (Antithesis runs especially) pass
    // a deterministic seed expecting `Mode::SingleTestCase` to replay
    // the same draws on every invocation. Without this, a `seed(Some(42))`
    // is silently ignored and each call produces fresh OS-random draws.
    // The database key seeds derandomized runs, so different tests draw
    // different streams.
    let mut rng = create_rng(settings, database_key);
    let ntc = NativeTestCase::new_random(rng.spawn());
    let (data_source, handle) = NativeDataSource::new(ntc);
    run_case(Box::new(data_source));
    match NativeDataSource::take_outcome(&handle) {
        TestCaseResult::Interesting(failure) => Some(failure),
        _ => None,
    }
}

/// The full multi-test-case engine: database replay, generation, and
/// shrinking, ending at the exploration report.
fn run_main(
    settings: &Settings,
    database_key: Option<&str>,
    run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>),
    // Injected (rather than read from the `TOO_SLOW_THRESHOLD` constant) so a
    // test can trip the TooSlow check deterministically without a 30s sleep.
    too_slow_threshold: std::time::Duration,
    // Wall-clock budget for the whole shrinking phase. Injected (rather than
    // read from `MAX_SHRINKING_SECONDS`) so a test can trip the slow-shrink
    // cutoff deterministically with a zero budget instead of a 5-minute wait.
    shrink_budget: std::time::Duration,
) -> Result<Vec<Failure>, RunError> {
    Engine::new(settings, database_key, run_case).run(too_slow_threshold, shrink_budget)
}

impl<'a> Engine<'a> {
    /// The engine driver — Hypothesis's `ConjectureRunner._run`: database
    /// replay, generation (with targeting and span mutation), shrinking,
    /// and the end-of-run database reconciliation, finishing with the
    /// exploration report of every distinct bug's shrunk counterexample.
    fn run(
        &mut self,
        too_slow_threshold: std::time::Duration,
        shrink_budget: std::time::Duration,
    ) -> Result<Vec<Failure>, RunError> {
        let settings = self.settings;
        let database_key = self.database_key;
        let max_test_cases = settings.test_cases;
        let verbosity = settings.verbosity;
        let log_phase = move |name: &str, edge: &str| {
            if matches!(verbosity, Verbosity::Verbose | Verbosity::Debug) {
                eprintln!("{edge}ing phase: {name}");
            }
        };

        let mut target_schedule = crate::native::targeting::TargetingSchedule::new(max_test_cases);
        let target_enabled = settings.phases.contains(&Phase::Target);
        // `(base, per_valid)` of the generation-phase invalid budget, computed once
        // per run (the formula uses floating-point `ln`/`ceil`, which can't run in
        // const context).
        let invalid_budget = invalid_thresholds(INVALID_TARGET_RATE, INVALID_TARGET_CONFIDENCE);
        let mut replay_aligned = false;
        let report_multiple = settings.report_multiple_failures;

        // --- Database replay phase ---
        //
        // Every stored value is replayed, not just the first interesting
        // one. A test that previously discovered N distinct bugs has N
        // stored choice sequences in the DB; each must be replayed so each
        // bug's shrunk counterexample re-surfaces in `interesting`.
        //
        // `replay_aligned` tracks whether *every* interesting replay's
        // realised choice sequence matches the stored prefix length —
        // when true the runner can skip the shrink phase because each
        // stored value is already minimal.  Any single divergence flips
        // it to false so the shrinker re-runs over the full set.
        if settings.phases.contains(&Phase::Reuse) {
            if let (Some(_), Some(key)) = (self.db(), database_key) {
                log_phase("Reuse", "Start");
                let key_bytes = key.as_bytes().to_vec();
                let secondary_key = crate::native::data_tree::sub_key(&key_bytes, b"secondary");
                let mut values = self.db().map(|db| db.fetch(&key_bytes)).unwrap_or_default();
                values.sort_by(|a, b| shortlex(a, b));
                replay_aligned = !values.is_empty();
                let primary_count = values.len();
                // When the primary corpus is small, top it up with a sample of
                // the secondary (historical near-miss) corpus, mirroring
                // engine.py's reuse_existing_examples: a stale primary entry
                // can stop reproducing while an older secondary one still does.
                let desired_factor = if settings.phases.contains(&Phase::Generate) {
                    0.1
                } else {
                    1.0
                };
                let desired_size =
                    (((max_test_cases as f64) * desired_factor).ceil() as usize).max(2);
                if values.len() < desired_size {
                    let mut extra = self
                        .db()
                        .map(|db| db.fetch(&secondary_key))
                        .unwrap_or_default();
                    let shortfall = desired_size - values.len();
                    if extra.len() > shortfall {
                        // Random sample without replacement: partial
                        // Fisher-Yates over the first `shortfall` slots.
                        for i in 0..shortfall {
                            let j = self.rng.random_range(i..extra.len());
                            extra.swap(i, j);
                        }
                        extra.truncate(shortfall);
                    }
                    extra.sort_by(|a, b| shortlex(a, b));
                    values.extend(extra);
                }
                let mut found_interesting_in_primary = false;
                for (i, raw) in values.into_iter().enumerate() {
                    // Fast path: if a primary entry reproduced, skip the
                    // secondary portion entirely.
                    if i >= primary_count && found_interesting_in_primary {
                        break;
                    }
                    let Some(stored_choices) = deserialize_choices(&raw) else {
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                        continue;
                    };
                    // Replay with extension (Hypothesis's extend="full"): if the
                    // test now draws more choices than the stored prefix holds,
                    // continue with fresh random draws rather than treating the
                    // entry as stale.
                    let ntc =
                        NativeTestCase::for_probe(&stored_choices, self.rng.spawn(), BUFFER_SIZE);
                    // `test_function` re-saves the realised choice sequence (the
                    // stored raw bytes may not match `serialize_choices` of the
                    // realised nodes if the replay realised a shorter prefix);
                    // stale raw bytes still in primary are reconciled to
                    // `secondary` by the end-of-run save.
                    let (run, mismatch) = self.test_function(ntc);
                    if let Some(msg) = mismatch {
                        return Err(RunError::NonDeterministic(msg));
                    }
                    if run.status == Status::Interesting {
                        if i < primary_count {
                            found_interesting_in_primary = true;
                            if run.nodes.len() != stored_choices.len() {
                                replay_aligned = false;
                            }
                        } else {
                            // A secondary entry is by construction not a shrunk
                            // example.
                            replay_aligned = false;
                        }
                        if !report_multiple {
                            // Single-failure reporting: one reproduced bug is
                            // all we need.
                            break;
                        }
                    } else {
                        // Non-interesting (or invalid) replay: the stored
                        // value no longer reproduces the bug, drop it from
                        // both corpora.
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                    }
                }
                if self.interesting.is_empty() {
                    // No replay survived — fall back to the pre-replay
                    // alignment state so the shrink phase decides based on
                    // generation results instead.
                    replay_aligned = false;
                }
                log_phase("Reuse", "End");
            }
        }

        // --- Generation phase ---
        //
        // Pre-bug we run until either the `max_test_cases` budget or the choice
        // tree is exhausted; post-bug we keep running for a bounded extra
        // window so that a test with multiple distinct failure origins
        // surfaces all of them, not just the first one to fire.
        let shrink_enabled = settings.phases.contains(&Phase::Shrink);
        // Hypothesis skips generation entirely when the database replay already
        // reproduced a failure: "we'd rather report that they're still failing
        // ASAP than take the time to look for new ones"
        // (engine.py::generate_new_examples).
        let found_in_reuse = !self.interesting.is_empty();

        // The generation phase runs (simplest pre-trial + random batches) unless
        // it is disabled, the reuse phase already reproduced a bug, or the test
        // is trivial.
        let actually_generate =
            settings.phases.contains(&Phase::Generate) && !found_in_reuse && !self.test_is_trivial;
        if actually_generate {
            log_phase("Generate", "Start");
        }

        // All-simplest pre-trial: a deterministic "draw every choice at its
        // shrink target" probe before random generation starts. Gives
        // find-any tests over multi-component generators (e.g. midnight =
        // h=m=s=μ=0 across four draws) a chance to hit the all-zeros joint
        // event before
        // random sampling — the joint event grows vanishingly unlikely as
        // the number of components increases.
        if settings.phases.contains(&Phase::Generate)
            && !self.test_is_trivial
            && self.within_invalid_budget(invalid_budget)
            && !found_in_reuse
        {
            // The reuse phase may already have fed the tree, so even this first
            // generation probe can contradict it under a non-deterministic
            // generator.
            let (run, mismatch) = self.test_function(NativeTestCase::for_simplest(BUFFER_SIZE));
            if let Some(msg) = mismatch {
                return Err(RunError::NonDeterministic(msg));
            }
            // The simplest example is Hypothesis's "zero" example: if even it
            // overruns or already uses more than half the buffer, shrinking will
            // be ineffective.
            if let Some(msg) = large_initial_check(
                run.status == Status::EarlyStop,
                run.status,
                run.nodes.len(),
                settings
                    .suppress_health_check
                    .contains(&HealthCheck::LargeInitialTestCase),
            ) {
                return Err(RunError::HealthCheck(msg));
            }
        }

        while settings.phases.contains(&Phase::Generate)
            && !found_in_reuse
            && !self.test_is_trivial
            && self.valid_test_cases < max_test_cases
            && self.within_invalid_budget(invalid_budget)
            && !self.tree_root.is_exhausted
            && should_generate_more(
                self.interesting.is_empty(),
                self.calls,
                self.first_bug_at,
                self.last_bug_at,
                shrink_enabled,
                report_multiple,
                self.first_bug_time.map(|t| t.elapsed()),
            )
        {
            for _ in 0..RANDOM_GENERATION_BATCH {
                if self.test_is_trivial
                    || self.valid_test_cases >= max_test_cases
                    || !self.within_invalid_budget(invalid_budget)
                    || self.tree_root.is_exhausted
                    || !should_generate_more(
                        self.interesting.is_empty(),
                        self.calls,
                        self.first_bug_at,
                        self.last_bug_at,
                        shrink_enabled,
                        report_multiple,
                        self.first_bug_time.map(|t| t.elapsed()),
                    )
                {
                    break;
                }

                let batch_rng = self.rng.spawn();
                let prefix =
                    crate::native::data_tree::generate_novel_prefix(&self.tree_root, &mut self.rng);
                let ntc = if prefix.is_empty() {
                    NativeTestCase::new_random(batch_rng)
                } else {
                    NativeTestCase::for_probe(&prefix, batch_rng, BUFFER_SIZE)
                };
                if verbosity == Verbosity::Verbose {
                    eprintln!("Running test case");
                }

                let (run, mismatch) = self.test_function(ntc);
                if let Some(msg) = mismatch {
                    return Err(RunError::NonDeterministic(msg));
                }

                if verbosity == Verbosity::Debug {
                    eprintln!(
                        "test case #{}: status = {:?}, choices = {}",
                        self.calls,
                        run.status,
                        run.nodes.len()
                    );
                }

                // "Once we've actually found a bug, there's no point in trying
                // to run health checks - they'll just mask the actually
                // important information." (engine.py, record_for_health_check.)
                // `self.interesting` already includes the current run, so
                // the iteration that discovers the first bug is exempt too.
                if self.interesting.is_empty() {
                    if run.status == Status::Invalid
                        && self.invalid_test_cases >= FILTER_TOO_MUCH_THRESHOLD
                        && self.valid_test_cases < HEALTH_CHECK_MAX_VALID
                        && !settings
                            .suppress_health_check
                            .contains(&HealthCheck::FilterTooMuch)
                    {
                        return Err(RunError::HealthCheck(format!(
                            "FailedHealthCheck: FilterTooMuch — it looks like this \
                         test is filtering out too many inputs. \
                         {} inputs were filtered out by assume() \
                         while only {} valid inputs were \
                         generated. If this is expected, suppress the check with \
                         suppress_health_check = [HealthCheck::FilterTooMuch].",
                            self.invalid_test_cases, self.valid_test_cases
                        )));
                    }
                    if let Some(msg) = too_large_check(
                        self.valid_test_cases,
                        self.overrun_test_cases,
                        settings
                            .suppress_health_check
                            .contains(&HealthCheck::TestCasesTooLarge),
                    ) {
                        return Err(RunError::HealthCheck(msg));
                    }

                    if let Some(msg) = too_slow_check(
                        self.valid_test_cases,
                        self.total_test_time,
                        too_slow_threshold,
                        settings
                            .suppress_health_check
                            .contains(&HealthCheck::TooSlow),
                    ) {
                        return Err(RunError::HealthCheck(msg));
                    }
                }

                // Fire `optimise_targets` periodically once enough valid
                // test cases have accumulated. Counts share the generation
                // budget — targeting trials count toward `valid_test_cases`
                // and `calls`, so `max_test_cases` remains a hard cap across
                // both. Skipped once a bug has been found (matching
                // `optimise_targets`'s own short-circuit).
                if target_enabled
                    && self.interesting.is_empty()
                    && !self.targeting.is_empty()
                    && target_schedule.should_fire(self.valid_test_cases)
                {
                    let mut optimiser = crate::native::targeting::Optimiser {
                        engine: &mut *self,
                        max_valid: max_test_cases,
                        max_calls: max_test_cases * 10,
                    };
                    optimiser.optimise_targets();
                }

                // Span mutation runs only once the health-check warm-up is over,
                // as in Hypothesis (generate_mutations_from is gated on
                // `health_check_state is None`): mutated probes routinely
                // overrun, and with every probe now recorded like any other run,
                // counting those overruns against TestCasesTooLarge during
                // warm-up would punish the test for the mutator's appetite.
                if run.status == Status::Valid
                    && (self.valid_test_cases >= HEALTH_CHECK_MAX_VALID
                        || !self.interesting.is_empty())
                {
                    self.try_span_mutation(&run.nodes, &run.spans);
                }
            }
        }

        // Tree-exhaustion fallback: a small choice domain (e.g. integer in
        // [0, 10] = 11 children) can exhaust the tree well before
        // FILTER_TOO_MUCH_THRESHOLD rejections; re-fire the check here.
        if self.tree_root.is_exhausted
            && self.valid_test_cases == 0
            && self.interesting.is_empty()
            && !self.test_is_trivial
            && !settings
                .suppress_health_check
                .contains(&HealthCheck::FilterTooMuch)
            && self.invalid_test_cases > 0
        {
            return Err(RunError::HealthCheck(format!(
                "FailedHealthCheck: FilterTooMuch — every reachable input was \
             filtered out by assume() before any valid input was generated. \
             {} inputs were filtered out across the full search \
             space. If this is expected, suppress the check with \
             suppress_health_check = [HealthCheck::FilterTooMuch].",
                self.invalid_test_cases
            )));
        }

        if actually_generate {
            log_phase("Generate", "End");
        }

        // --- Shrinking phase ---
        if !self.interesting.is_empty()
            && !replay_aligned
            && settings.phases.contains(&Phase::Shrink)
        {
            log_phase("Shrink", "Start");
            if verbosity == Verbosity::Debug {
                let total: usize = self.interesting.values().map(|n| n.len()).sum();
                eprintln!(
                    "Shrinking: {} origin(s), initial total length = {}",
                    self.interesting.len(),
                    total
                );
            }
            // Try stored secondary-corpus entries smaller than the current
            // examples as shrink jump-starts, then drop them — each is either
            // promoted to primary by the persister or worse than what we already
            // hold. Port of engine.py's clear_secondary_key; this is also what
            // keeps the secondary corpus from growing without bound across runs.
            if let (Some(_), Some(key)) = (self.db(), database_key) {
                let key_bytes = key.as_bytes().to_vec();
                let secondary_key = crate::native::data_tree::sub_key(&key_bytes, b"secondary");
                let mut entries = self
                    .db()
                    .map(|db| db.fetch(&secondary_key))
                    .unwrap_or_default();
                entries.sort_by(|a, b| shortlex(a, b));
                let primary_max: Option<Vec<u8>> = self
                    .interesting
                    .values()
                    .map(|nodes| {
                        let choices: Vec<ChoiceValue> =
                            nodes.iter().map(|n| n.value.clone()).collect();
                        serialize_choices(&choices)
                    })
                    .max_by(|a, b| shortlex(a, b));
                for raw in entries {
                    if primary_max
                        .as_ref()
                        .is_some_and(|m| shortlex(&raw, m) == std::cmp::Ordering::Greater)
                    {
                        break;
                    }
                    if let Some(stored_choices) = deserialize_choices(&raw) {
                        let ntc = NativeTestCase::for_choices(&stored_choices, None, None);
                        // A tree mismatch here is dropped: a generator that is
                        // non-deterministic during earlier phases was already
                        // caught by their recordings, and one that only diverges
                        // now will fail the per-origin re-verify below as flaky.
                        let _ = self.test_function(ntc);
                    }
                    // Unconditionally removed: now primary, or worse than the
                    // primary example of its origin.
                    if let Some(db) = self.db() {
                        db.delete(&secondary_key, &raw);
                    }
                }
            }

            // One wall-clock deadline shared across every origin's shrink, matching
            // Hypothesis's single `finish_shrinking_deadline` for the whole phase.
            let shrink_deadline = std::time::Instant::now() + shrink_budget;
            let mut shrink_timed_out = false;
            // Worklist rather than a fixed snapshot: a shrink run can stumble
            // onto a bug with a *new* origin, which `cached_test_function`'s
            // `record_run` folds straight into `self.interesting` (sort-key
            // gated, like every interesting result). Re-scanning the keys each
            // iteration therefore picks those up and shrinks them too, exactly
            // as Hypothesis's
            // `while len(self.shrunk_examples) < len(self.interesting_examples)`
            // loop does. Origins are processed in sorted order for determinism
            // (`interesting` is a HashMap with randomised iteration order).
            let mut shrunk_origins: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            loop {
                let mut pending: Vec<String> = self
                    .interesting
                    .keys()
                    .filter(|o| !shrunk_origins.contains(o.as_str()))
                    .cloned()
                    .collect();
                if pending.is_empty() {
                    break;
                }
                pending.sort();
                let origin = pending.remove(0);
                let initial = self.interesting.get(&origin).cloned().unwrap_or_default();

                // Re-validate that this origin's example still fails. If not,
                // the test is flaky.
                let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value.clone()).collect();
                let verify_ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
                // A tree mismatch here is dropped for the same reason as in the
                // secondary-corpus drain above: the flaky-replay check below is
                // the authoritative non-determinism report for this phase.
                let (verify, _) = self.test_function(verify_ntc);
                if verify.status != Status::Interesting {
                    return Err(RunError::Flaky(flaky_diagnostic()));
                }

                let target_origin = origin.clone();
                let initial_spans = Spans::from(verify.spans.clone());
                let shrunk = {
                    let this: &mut Engine<'_> = &mut *self;
                    let mut shrinker = Shrinker::with_probe(
                        Box::new(|req: ShrinkRun| {
                            if verbosity == Verbosity::Verbose {
                                eprintln!("Running test case");
                            }
                            // Single replay chokepoint: `cached_test_function`
                            // serves a path the lossless tree already records
                            // (no execution, any status), and otherwise executes
                            // and records it. The interesting-origin filter is
                            // applied here, to the result — replay and matching
                            // are not entangled, mirroring Hypothesis's shrink
                            // predicate.
                            let run = match req {
                                ShrinkRun::Full(nodes) => {
                                    let choices: Vec<ChoiceValue> =
                                        nodes.iter().map(|n| n.value.clone()).collect();
                                    this.cached_test_function(&choices, Some(nodes), 0)
                                }
                                ShrinkRun::Probe { prefix, max_size } => this.cached_test_function(
                                    prefix,
                                    None,
                                    max_size.saturating_sub(prefix.len()),
                                ),
                            };
                            let matches = run.status == Status::Interesting
                                && run.origin.as_deref() == Some(target_origin.as_str());
                            (matches, run.nodes, Spans::from(run.spans))
                        }),
                        verify.nodes,
                        initial_spans,
                    );
                    shrinker.deadline = Some(shrink_deadline);
                    // Pre-shrink coarse reduction — runs once before the
                    // main shrink loop to rerandomise small one_of-style
                    // branch selectors. A `ShrinkStop` here just means the
                    // deadline passed; `shrink()` below is a no-op in that case
                    // and `timed_out` is already latched.
                    let _ = shrinker.initial_coarse_reduction();
                    if verbosity == Verbosity::Debug {
                        shrinker.set_debug(|msg| eprintln!("{msg}"));
                    }
                    shrinker.shrink();
                    shrink_timed_out |= shrinker.timed_out;
                    shrinker.current_nodes
                };
                self.interesting.insert(origin.clone(), shrunk);
                shrunk_origins.insert(origin);
            }

            // The shrink phase ran past its wall-clock budget and bailed with the
            // best example so far. Warn unless output is suppressed, mirroring
            // Hypothesis's slow-shrink notice.
            if shrink_timed_out && verbosity != Verbosity::Quiet {
                eprintln!("{}", slow_shrink_warning());
            }

            if verbosity == Verbosity::Debug {
                let total: usize = self.interesting.values().map(|n| n.len()).sum();
                eprintln!(
                    "Shrinking complete: {} origin(s), final total length = {}",
                    self.interesting.len(),
                    total
                );
            }
            log_phase("Shrink", "End");
        } else if self.interesting.is_empty() && verbosity == Verbosity::Debug {
            // No bug found — nothing to shrink; left for symmetry with the
            // `Test done.` line below.
        } else if replay_aligned && verbosity == Verbosity::Debug {
            eprintln!("Skipping shrink: reused aligned database replay");
        }

        // --- Save to database ---
        //
        // For each interesting origin, save the shrunk counterexample to
        // primary. Any *displaced* primary entry — present at start of
        // run but no longer in `interesting` — moves to the
        // `<key>.secondary` sub-corpus rather than disappearing. The
        // secondary key is the historical fallback corpus the next reuse
        // pass consults if primary doesn't have enough entries.
        if let (Some(db), Some(key)) = (self.db(), database_key) {
            let key_bytes = key.as_bytes();
            let secondary_key = crate::native::data_tree::sub_key(key_bytes, b"secondary");
            let new_entries: std::collections::HashSet<Vec<u8>> = self
                .interesting
                .values()
                .map(|nodes| {
                    let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                    serialize_choices(&choices)
                })
                .collect();
            let primary_now = db.fetch(key_bytes);
            for old in primary_now {
                if !new_entries.contains(&old) {
                    db.move_value(key_bytes, &secondary_key, &old);
                }
            }
            for new_bytes in &new_entries {
                db.save(key_bytes, new_bytes);
            }
        }

        if verbosity == Verbosity::Debug {
            eprintln!(
                "Test done. interesting_test_cases={}",
                self.interesting.len()
            );
        }

        // --- Report ---
        //
        // Surface each origin's shrunk counterexample (origin + reproduce
        // blob) in shortlex-descending order: the smallest counterexample is
        // listed *last*, so when the client replays them in order a user-side
        // `Mutex<Option<…>>` that overwrites on each panic ends up holding
        // the simplest example.
        let mut origins_sorted: Vec<(String, Vec<ChoiceNode>)> =
            std::mem::take(&mut self.interesting).into_iter().collect();
        // Descending sort_key order. `sort_by` instead of `sort_by_key` because
        // `NodesSortKey` borrows from the origin's nodes and the key would
        // otherwise outlive its borrow.
        origins_sorted.sort_by(|a, b| sort_key(&b.1).cmp(&sort_key(&a.1)));

        // When `report_multiple_failures` is `false`, drop all but the
        // smallest origin (the one observed *last* under the
        // shortlex-descending sort above), so the runner surfaces a single
        // failure rather than every distinct bug Hegel found.
        if !settings.report_multiple_failures {
            if let Some(last) = origins_sorted.pop() {
                origins_sorted.clear();
                origins_sorted.push(last);
            }
        }

        // Surface each counterexample as a `Failure` carrying the origin it
        // grouped on and a reproduce blob for the client to replay.
        Ok(origins_sorted
            .into_iter()
            .map(|(origin, nodes)| {
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                Failure {
                    origin,
                    reproduce_blob: Some(crate::native::blob::encode_failure(&choices)),
                }
            })
            .collect())
    }
}

/// Pre-bug we always keep generating; post-bug we keep going just long
/// enough to surface other distinct origins. The window is
/// `min(first_bug + 1000, last_bug * 2)`, with a minimum-call floor
/// (`MIN_TEST_CALLS`) so very-cheap tests still produce a few extra probes.
///
/// Special case: if `interesting` was populated from the **database** via
/// the Reuse phase (i.e. no bug was found in generation, so `first_bug_at`
/// is `None`), we stop immediately — the user already had this example
/// stored, so re-running the generation loop just to look for more bugs is
/// wasted work. The replay-logic test (`test_does_not_shrink_on_replay`)
/// pins this behaviour at exactly 2 calls (replay + final replay).
const MIN_TEST_CALLS: u64 = 10;
const POST_BUG_EXTRA_CALLS: u64 = 1000;

/// Returns the `FailedHealthCheck: TooSlow` message when input generation
/// has consumed more than `threshold` of wall-clock time without producing
/// `HEALTH_CHECK_MAX_VALID` valid test cases, unless the user has explicitly
/// suppressed the check; otherwise returns `None`.
///
/// The caller wraps the message as [`RunError::HealthCheck`]. Extracted
/// from the runner's main loop so a unit test can exercise both branches
/// without stalling the in-process harness for `TOO_SLOW_THRESHOLD` of
/// real time.
pub(crate) fn too_slow_check(
    valid_test_cases: u64,
    total_test_time: std::time::Duration,
    threshold: std::time::Duration,
    suppressed: bool,
) -> Option<String> {
    if valid_test_cases < HEALTH_CHECK_MAX_VALID && total_test_time > threshold && !suppressed {
        Some(format!(
            "FailedHealthCheck: TooSlow — input generation is slow: \
             only {valid_test_cases} valid inputs after {:?} (threshold \
             {:?}). Slow generation makes property testing much less \
             effective. If this is expected, suppress the check with \
             suppress_health_check = [HealthCheck::TooSlow].",
            total_test_time, threshold
        ))
    } else {
        None
    }
}

/// Returns the `FailedHealthCheck: TestCasesTooLarge` message once
/// `MAX_OVERRUN_DRAWS` test cases have overrun the choice buffer while the run
/// still has fewer than `HEALTH_CHECK_MAX_VALID` valid examples, unless the
/// check is suppressed; otherwise `None`. Mirrors Hypothesis's `data_too_large`
/// health check.
pub(crate) fn too_large_check(
    valid_test_cases: u64,
    overrun_test_cases: u64,
    suppressed: bool,
) -> Option<String> {
    if valid_test_cases < HEALTH_CHECK_MAX_VALID
        && overrun_test_cases >= MAX_OVERRUN_DRAWS
        && !suppressed
    {
        Some(format!(
            "FailedHealthCheck: TestCasesTooLarge — generated inputs routinely \
             exceeded the maximum size: {valid_test_cases} inputs were generated \
             successfully, while {overrun_test_cases} inputs overran the buffer during \
             generation. Testing with inputs this large is slow and shrinks \
             poorly. Try reducing the amount of data generated, e.g. a smaller \
             min_size on collections like gs::vecs(). If this is expected, \
             suppress the check with \
             suppress_health_check = [HealthCheck::TestCasesTooLarge]."
        ))
    } else {
        None
    }
}

/// Returns the `FailedHealthCheck: LargeInitialTestCase` message when the
/// smallest natural example either overran the buffer or, while valid, used
/// more than half of it, unless the check is suppressed; otherwise `None`.
/// Mirrors Hypothesis's `large_base_example` health check.
pub(crate) fn large_initial_check(
    overran: bool,
    status: Status,
    node_count: usize,
    suppressed: bool,
) -> Option<String> {
    if suppressed {
        return None;
    }
    let too_large =
        overran || (status == Status::Valid && node_count.saturating_mul(2) > BUFFER_SIZE);
    if too_large {
        Some(
            "FailedHealthCheck: LargeInitialTestCase — the smallest natural input \
             for this test is very large, which makes it hard to generate and \
             shrink good inputs. Consider reducing the amount of data generated, \
             or introducing small alternatives (e.g. `gs::one_of` with an empty \
             option). If this is expected, suppress the check with \
             suppress_health_check = [HealthCheck::LargeInitialTestCase]."
                .to_string(),
        )
    } else {
        None
    }
}

/// Message for a flaky test — one whose outcome changed when re-run with
/// the same generated data. Wrapped as [`RunError::Flaky`] at the sites
/// that detect it.
pub(crate) fn flaky_diagnostic() -> String {
    "Flaky test detected: Your test produced different outcomes \
     when run with the same generated data — it failed when it \
     previously succeeded, or succeeded when it previously failed. \
     This usually means your test depends on external state such as \
     global variables, system time, or external random number generators."
        .to_string()
}

/// Warning emitted when shrinking exhausts its wall-clock budget
/// ([`MAX_SHRINKING_SECONDS`]) and stops early. Unlike a health-check
/// failure this is not a failure: the smallest counterexample found so far is
/// still reported. Returned as a string (rather than printed inline) so it can
/// be asserted directly in tests. Mirrors Hypothesis's slow-shrink notice.
pub(crate) fn slow_shrink_warning() -> String {
    format!(
        "WARNING: Shrinking has been running for more than {MAX_SHRINKING_SECONDS} seconds \
         and is making very slow progress, so it has been stopped. The smallest failing \
         example found so far will be reported. Re-running the test will resume shrinking \
         from there, and may take this long again before stopping."
    )
}

/// Port of Hypothesis's `_invalid_thresholds` (`engine.py`): returns the
/// `(base, per_valid)` terms of the generation-phase invalid budget, derived so
/// that once `(invalid_test_cases + overrun_test_cases)` exceeds
/// `base + per_valid * valid_test_cases` we are `c`-confident the true valid
/// rate is below `r`.
///
/// ```text
/// base    = ceil(log(1 - c) / log(1 - r)) - 1
/// per_valid = ceil(1 / r)
/// ```
fn invalid_thresholds(r: f64, c: f64) -> (u64, u64) {
    let base = ((1.0 - c).ln() / (1.0 - r).ln()).ceil() - 1.0;
    let per_valid = (1.0 / r).ceil();
    (base as u64, per_valid as u64)
}

/// Hypothesis's invalid-rate stop condition for the generation phase
/// (`engine.py`'s `should_generate_more`): the run keeps generating while
/// `(invalid_test_cases + overrun_test_cases)` stays within
/// `base + per_valid * valid_test_cases`, with `budget = (base, per_valid)`
/// from [`invalid_thresholds`]. Returns `true` while there is still budget.
fn within_invalid_budget(
    invalid_test_cases: u64,
    overrun_test_cases: u64,
    valid_test_cases: u64,
    budget: (u64, u64),
) -> bool {
    let (base, per_valid) = budget;
    (invalid_test_cases + overrun_test_cases) <= base + per_valid * valid_test_cases
}

/// Shortlex ordering over serialized choice sequences: by length first, then
/// lexicographically. Mirrors Hypothesis's `shortlex` database ordering.
fn shortlex(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn should_generate_more(
    no_bug_yet: bool,
    calls: u64,
    first_bug_at: Option<u64>,
    last_bug_at: Option<u64>,
    shrink_enabled: bool,
    report_multiple: bool,
    first_bug_elapsed: Option<std::time::Duration>,
) -> bool {
    if no_bug_yet {
        return true;
    }
    // Once a bug is found, the post-bug probing window exists to surface
    // *other* origins so each can be shrunk independently. If `Phase::Shrink`
    // isn't in the active phases there will be no shrinking, so additional
    // origins add nothing — stop generation immediately. This is what
    // `tests/test_phases.rs::test_disabling_shrink_limits_interesting_calls`
    // asserts (body called at most twice: initial discovery + final replay).
    // The same goes for single-failure reporting: extra origins would be
    // discarded, so don't spend calls hunting them.
    if !shrink_enabled || !report_multiple {
        return false;
    }
    // For slow tests the call-count window can take far too long; stop
    // probing for additional origins 10 seconds after the first bug
    // (engine.py's first_bug_found_time cutoff).
    if first_bug_elapsed.is_some_and(|d| d > std::time::Duration::from_secs(10)) {
        return false;
    }
    let Some(first) = first_bug_at else {
        return false;
    };
    let last = last_bug_at.unwrap_or(first);
    let heuristic = first
        .saturating_add(POST_BUG_EXTRA_CALLS)
        .min(last.saturating_mul(2));
    calls < MIN_TEST_CALLS || calls < heuristic
}

/// Insert a fresh shrunk-result for `origin` if it's the first sighting,
/// or replace the existing one if `nodes` shortlex-precedes it.
fn update_interesting(
    interesting: &mut HashMap<String, Vec<ChoiceNode>>,
    origin: String,
    nodes: Vec<ChoiceNode>,
) {
    match interesting.entry(origin) {
        Entry::Vacant(e) => {
            e.insert(nodes);
        }
        Entry::Occupied(mut e) => {
            if sort_key(&nodes) < sort_key(e.get()) {
                e.insert(nodes);
            }
        }
    }
}

/// Incremental database-save bookkeeping. Every time a new interesting
/// result is found (or an existing one is shortlex-improved), the
/// realised choice sequence is saved to the primary key and the
/// displaced previous entry is moved to the secondary key.
///
/// Persisting incrementally — rather than only at the end of `run_main` — is
/// what guarantees that a failure survives a Ctrl-C / SIGTERM mid-shrink:
/// the moment the runner discovers the failure (and at every subsequent
/// improvement), the bytes are on disk.
struct Persister<'a> {
    db: Option<Box<dyn TestCaseDatabase>>,
    database_key: Option<&'a str>,
    /// For each origin we've saved at least once, the choice-node sequence
    /// of the most recent save. Used to (a) decide whether a new result is
    /// shortlex-smaller and therefore worth saving, and (b) compute the
    /// bytes to downgrade when it is.
    last_saved: HashMap<String, Vec<ChoiceNode>>,
}

impl<'a> Persister<'a> {
    fn new(db: Option<Box<dyn TestCaseDatabase>>, database_key: Option<&'a str>) -> Self {
        Persister {
            db,
            database_key,
            last_saved: HashMap::new(),
        }
    }

    /// Record an interesting result for `origin`. If this is the first
    /// sighting, or shortlex-precedes the previous save, the new bytes are
    /// written to the primary key and any previously-saved bytes for this
    /// origin are downgraded to the secondary key.
    fn record(&mut self, origin: &str, nodes: &[ChoiceNode]) {
        let Some(db) = self.db.as_deref() else { return };
        let Some(key) = self.database_key else { return };
        let key_bytes = key.as_bytes();
        let new_choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        let new_bytes = serialize_choices(&new_choices);

        let needs_save = match self.last_saved.get(origin) {
            None => true,
            Some(prev) => sort_key(nodes) < sort_key(prev),
        };
        if !needs_save {
            return;
        }

        if let Some(prev) = self.last_saved.get(origin) {
            let prev_choices: Vec<ChoiceValue> = prev.iter().map(|n| n.value.clone()).collect();
            let prev_bytes = serialize_choices(&prev_choices);
            let secondary_key = crate::native::data_tree::sub_key(key_bytes, b"secondary");
            db.move_value(key_bytes, &secondary_key, &prev_bytes);
        }
        db.save(key_bytes, &new_bytes);
        self.last_saved.insert(origin.to_string(), nodes.to_vec());
    }
}

/// The native engine — Hegel's analogue of Hypothesis's `ConjectureRunner`.
///
/// One object owns everything a test run touches: the `run_case` executor
/// callback, the RNG, the example database (via the [`Persister`]), the choice
/// tree, the per-origin interesting map, targeting observations, and all
/// run-level counters. The choice tree is the single source of truth for
/// already-seen paths: it is *lossless* (each conclusion records nodes via the
/// path, plus span events, status, origin, and target observations), so any
/// recorded path is replayed by [`data_tree::simulate_full`] without re-running
/// the body — there is no separate result cache.
///
/// Every execution records into the tree via [`Self::record_run`].
/// [`Self::test_function`] is the raw executor+recorder (generation's novel
/// prefixes go straight through it); [`Self::cached_test_function`] is the
/// single replay chokepoint shared by generation-phase span mutation and
/// shrinking — it serves a recorded path from the tree and otherwise falls
/// through to `test_function`. `cached_test_function` returns the realised
/// result; the interesting-origin filter is applied by its caller, and bugs
/// with new origins surface through the same [`update_interesting`] path as
/// generation.
pub(crate) struct Engine<'a> {
    settings: &'a Settings,
    database_key: Option<&'a str>,
    run_case: &'a mut dyn FnMut(Box<dyn DataSource + Send + Sync>),
    rng: EngineRng,
    persister: Persister<'a>,
    pub(crate) tree_root: crate::native::data_tree::DataTreeNode,
    /// Per-origin tracking: each distinct panic site (file:line:col captured
    /// by [`crate::run_lifecycle::run_test_case`]) gets its own shrunk
    /// counterexample. This is what makes a single test that fails with
    /// several distinct bugs surface each one.
    pub(crate) interesting: HashMap<String, Vec<ChoiceNode>>,
    pub(crate) targeting: crate::native::targeting::TargetingState,
    pub(crate) calls: u64,
    pub(crate) valid_test_cases: u64,
    pub(crate) invalid_test_cases: u64,
    pub(crate) overrun_test_cases: u64,
    pub(crate) total_test_time: std::time::Duration,
    pub(crate) test_is_trivial: bool,
    pub(crate) first_bug_at: Option<u64>,
    pub(crate) last_bug_at: Option<u64>,
    pub(crate) first_bug_time: Option<std::time::Instant>,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        settings: &'a Settings,
        database_key: Option<&'a str>,
        run_case: &'a mut dyn FnMut(Box<dyn DataSource + Send + Sync>),
    ) -> Self {
        // `Database::Unset` is the non-CI default (set by `Settings::new` in
        // `src/runner.rs`); it means "the user didn't pick, so use the
        // sensible default." That default is `.hegel/examples` relative to
        // cwd. `Disabled` is the explicit opt-out; `Path(p)` is the explicit
        // choice.
        let db: Option<Box<dyn TestCaseDatabase>> = match &settings.database {
            Database::Path(path) => Some(Box::new(DirectoryTestCaseDatabase::new(path))),
            Database::Unset => Some(Box::new(DirectoryTestCaseDatabase::new(".hegel/examples"))),
            Database::Disabled => None,
        };
        Engine {
            settings,
            database_key,
            run_case,
            rng: create_rng(settings, database_key),
            persister: Persister::new(db, database_key),
            tree_root: crate::native::data_tree::DataTreeNode::default(),
            interesting: HashMap::new(),
            targeting: crate::native::targeting::TargetingState::new(),
            calls: 0,
            valid_test_cases: 0,
            invalid_test_cases: 0,
            overrun_test_cases: 0,
            total_test_time: std::time::Duration::ZERO,
            test_is_trivial: false,
            first_bug_at: None,
            last_bug_at: None,
            first_bug_time: None,
        }
    }

    fn db(&self) -> Option<&dyn TestCaseDatabase> {
        self.persister.db.as_deref()
    }

    /// Spawn an independent RNG from the engine's, for components (probes,
    /// replays) that need their own stream without perturbing the engine's
    /// trajectory.
    pub(crate) fn rng_spawn(&mut self) -> EngineRng {
        self.rng.spawn()
    }

    /// Execute one test case and record everything about its outcome —
    /// Hypothesis's `ConjectureRunner.test_function`. Returns the run plus
    /// the choice-tree non-determinism diagnostic, if recording the run's
    /// path contradicted an earlier run.
    pub(crate) fn test_function(&mut self, ntc: NativeTestCase) -> (RunResult, Option<String>) {
        let tc_start = std::time::Instant::now();
        let run = self.execute(ntc);
        let mismatch = self.record_run(&run, tc_start.elapsed());
        (run, mismatch)
    }

    /// Record one executed test case: the choice tree (losslessly — nodes,
    /// span events, and the full conclusion), counters, test time, triviality,
    /// the targeting observations, the per-origin interesting map (with its
    /// incremental database save), and the bug-window markers.
    ///
    /// Every execution feeds the tree, so a later replay of the same path is
    /// served by [`data_tree::simulate_full`] without re-running the body.
    ///
    fn record_run(&mut self, run: &RunResult, elapsed: std::time::Duration) -> Option<String> {
        let mismatch = crate::native::data_tree::record_tree_full(
            &mut self.tree_root,
            &run.nodes,
            run.status,
            run.origin.as_deref(),
            &run.target_observations,
            &run.span_events,
            &[],
        );
        self.calls += 1;
        // Test time accrues for every status (Hypothesis records draw times
        // regardless of outcome), so a slow generator that is mostly
        // assume()-rejected still trips TooSlow.
        self.total_test_time += elapsed;
        if run.nodes.is_empty() && run.status >= Status::Invalid {
            self.test_is_trivial = true;
        }
        if run.status >= Status::Valid && !run.target_observations.is_empty() {
            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value.clone()).collect();
            self.targeting.record(&choices, &run.target_observations);
        }
        match run.status {
            // Only exactly-valid runs count toward the budget; Hypothesis
            // never counts INTERESTING results as valid examples.
            Status::Valid => self.valid_test_cases += 1,
            Status::Invalid => self.invalid_test_cases += 1,
            Status::EarlyStop => self.overrun_test_cases += 1,
            Status::Interesting => {
                if self.first_bug_at.is_none() {
                    self.first_bug_at = Some(self.calls);
                    self.first_bug_time = Some(std::time::Instant::now());
                }
                self.last_bug_at = Some(self.calls);
                let origin = run.origin.clone().unwrap_or_default();
                self.persister.record(&origin, &run.nodes);
                update_interesting(&mut self.interesting, origin, run.nodes.clone());
            }
        }
        mismatch
    }

    /// Whether the generation-phase invalid/overrun budget still has room.
    fn within_invalid_budget(&self, budget: (u64, u64)) -> bool {
        within_invalid_budget(
            self.invalid_test_cases,
            self.overrun_test_cases,
            self.valid_test_cases,
            budget,
        )
    }

    /// Execute one test case via `run_case`, recording the trie and
    /// returning a [`RunResult`] populated from the outcome reported by the
    /// data source's `mark_complete` plus the [`NativeTestCase`]'s realized
    /// choice nodes. Always a non-final execution.
    fn execute(&mut self, ntc: NativeTestCase) -> RunResult {
        let (data_source, handle) = NativeDataSource::new(ntc);
        (self.run_case)(Box::new(data_source));
        let nodes = NativeDataSource::take_nodes(&handle);
        let spans = NativeDataSource::take_spans(&handle);
        let span_events = NativeDataSource::take_span_events(&handle);
        let target_observations = NativeDataSource::take_target_observations(&handle);
        let tc_result = NativeDataSource::take_outcome(&handle);

        let (status, origin) = match tc_result {
            TestCaseResult::Valid => (Status::Valid, None),
            TestCaseResult::Invalid => (Status::Invalid, None),
            TestCaseResult::Overrun => (Status::EarlyStop, None),
            TestCaseResult::Interesting(f) => (Status::Interesting, Some(f.origin)),
        };

        RunResult {
            status,
            nodes,
            spans,
            origin,
            target_observations,
            span_events,
        }
    }

    /// The single replay chokepoint — Hypothesis's `cached_test_function` —
    /// shared by generation-phase span mutation and shrinking. Replays
    /// `choices` (drawing up to `extend` further choices beyond them) and
    /// returns the realised [`RunResult`]. Any predicate (e.g. the
    /// interesting-origin filter) is applied by the caller, so replay and
    /// matching are not entangled.
    ///
    /// With `extend == 0` the realised path is known up front, so a path the
    /// lossless tree already records is served by [`data_tree::simulate_full`]
    /// with its full outcome — nodes, spans, status, origin, observations —
    /// without running the body, for *any* status (interesting included). With
    /// `extend > 0` the random continuation isn't known ahead of time, so it
    /// always executes. A genuine miss (a novel or undetermined path) runs
    /// through [`Self::test_function`], which records the run into the tree so a
    /// later replay of the same path is served. There is no separate result
    /// cache: the tree is the single source of truth.
    fn cached_test_function(
        &mut self,
        choices: &[ChoiceValue],
        nodes: Option<&[ChoiceNode]>,
        extend: usize,
    ) -> RunResult {
        if extend == 0 {
            if let Some(out) = crate::native::data_tree::simulate_full(&self.tree_root, choices) {
                return RunResult {
                    status: out.status,
                    nodes: out.nodes,
                    spans: out.spans,
                    origin: out.origin,
                    target_observations: out.target_observations,
                    span_events: Vec::new(),
                };
            }
        }
        let ntc = if extend == 0 {
            NativeTestCase::for_choices(choices, nodes, None)
        } else {
            NativeTestCase::for_probe(choices, self.rng_spawn(), choices.len() + extend)
        };
        // A miss: run and record through `test_function`, which feeds the tree
        // like any other executed case. The non-determinism mismatch is dropped
        // — the per-origin flaky re-verify is authoritative for shrinking, and a
        // generation-mutation mismatch is benign.
        let (run, _mismatch) = self.test_function(ntc);
        run
    }
}

/// Try span mutation: find two spans with the same label and either duplicate
/// the parent's prefix (when one contains the other, e.g. recursive tree
/// structures) or replace both with identical choices from one donor.
/// Anything interesting it finds lands in the engine's `interesting` map;
/// the probe loop stops at the first such find.
///
/// Makes up to [`SPAN_MUTATION_ATTEMPTS`] probes through
/// [`Engine::cached_test_function`], so a proposed sequence whose path the
/// lossless choice tree already records costs no test-body execution — matching
/// Hypothesis, which routes mutations through `cached_test_function`. Each probe
/// that *does* execute is recorded into the tree through [`Self::record_run`],
/// so it counts toward the same budgets as a freshly generated example and a
/// later identical proposal is served from the tree; tree-served probes are not
/// re-recorded, exactly as Hypothesis's cache hits cost nothing.
impl<'a> Engine<'a> {
    fn try_span_mutation(&mut self, nodes: &[ChoiceNode], spans: &[Span]) {
        // Fast, non-DoS-resistant hashers: these maps are keyed by our own span
        // labels / extents (never adversarial) and are rebuilt for every recorded
        // result, so the default SipHash showed up prominently in generation
        // profiles. FxHash is a clear win here.
        let mut by_label: rustc_hash::FxHashMap<&str, rustc_hash::FxHashSet<(usize, usize)>> =
            rustc_hash::FxHashMap::default();
        for span in spans.iter() {
            by_label
                .entry(span.label.as_str())
                .or_default()
                .insert((span.start, span.end));
        }
        let multi: Vec<Vec<(usize, usize)>> = by_label
            .into_values()
            .filter(|v| v.len() >= 2)
            .map(|v| {
                let mut items: Vec<(usize, usize)> = v.into_iter().collect();
                items.sort();
                items
            })
            .collect();
        if multi.is_empty() {
            return;
        }

        let values: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();

        for _ in 0..SPAN_MUTATION_ATTEMPTS {
            // A mutation probe is a generated example: once the example budget is
            // full there is no room for another, so stop proposing.
            if self.valid_test_cases >= self.settings.test_cases {
                break;
            }
            let group_idx = self.rng.random_range(0..multi.len());
            let group = &multi[group_idx];
            let i_a = self.rng.random_range(0..group.len());
            let mut i_b = self.rng.random_range(0..group.len() - 1);
            if i_b >= i_a {
                i_b += 1;
            }

            let (mut start_a, mut end_a) = group[i_a];
            let (mut start_b, mut end_b) = group[i_b];
            if start_a > start_b {
                std::mem::swap(&mut start_a, &mut start_b);
                std::mem::swap(&mut end_a, &mut end_b);
            }

            let attempt: Vec<ChoiceValue> = if start_a <= start_b && end_b <= end_a {
                let mut out = Vec::with_capacity(values.len() + (start_b - start_a));
                out.extend_from_slice(&values[..start_b]);
                out.extend_from_slice(&values[start_a..]);
                out
            } else {
                let (donor_start, donor_end) = if self.rng.random::<bool>() {
                    (start_a, end_a)
                } else {
                    (start_b, end_b)
                };
                let replacement: &[ChoiceValue] = &values[donor_start..donor_end];
                let mid = if end_a <= start_b {
                    &values[end_a..start_b]
                } else {
                    &[][..]
                };
                let mut out = Vec::new();
                out.extend_from_slice(&values[..start_a]);
                out.extend_from_slice(replacement);
                out.extend_from_slice(mid);
                out.extend_from_slice(replacement);
                out.extend_from_slice(&values[end_b..]);
                out
            };

            let run = self.cached_test_function(&attempt, None, 0);
            if run.status == Status::Interesting {
                return;
            }
        }
    }
}

fn create_rng(settings: &Settings, database_key: Option<&str>) -> EngineRng {
    // The urandom backend reads fresh OS entropy on every draw, so the seed /
    // derandomize knobs (which only control a PRNG seed) don't apply to it.
    if settings.resolved_backend(crate::antithesis_detect::is_running_in_antithesis())
        == Backend::Urandom
    {
        return EngineRng::urandom();
    }
    if let Some(seed) = settings.seed {
        EngineRng::seeded(seed)
    } else if settings.derandomize {
        let key = database_key.unwrap_or("unnamed-test");
        EngineRng::seeded(hash_string(key))
    } else {
        EngineRng::from_os()
    }
}

fn hash_string(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "../../tests/embedded/native/test_runner_tests.rs"]
mod tests;
