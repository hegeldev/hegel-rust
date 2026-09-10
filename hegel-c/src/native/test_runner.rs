//! Native engine driver.
//!
//! [`explore`] is the engine driver: it owns the database replay,
//! generation, and shrinking phases, handing each test case it wants run to
//! its driver through the supplied [`CaseExchange`] (see [`crate::exchange`]
//! for the alternation protocol), and returns the report — every distinct
//! bug's shrunk counterexample as a [`Failure`] carrying the reproduce blob.
//! The caller ([`crate::embed::run_native_async`]'s driver, and ultimately
//! the client)
//! replays each blob to produce the final report. Every test case `explore`
//! runs is non-final.
//!
//! Everything here is async purely so execution can suspend at the exchange:
//! there is no executor and no scheduled wakeup anywhere — the engine future
//! only progresses when its driver polls it.
//!
//! Inside, [`Engine`] wraps the exchange together with
//! a shrink-result cache, exposing `run` / `run_shrink_with_origin` /
//! `run_probe_with_origin` so the surrounding shrinker
//! and span-mutation passes can drive replays.

use crate::native::HashMap;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use rand::RngExt;

use crate::backend::{Failure, RunError, TestCaseResult, TestRunResult};
use crate::exchange::CaseExchange;
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, MAX_SHRINKING_SECONDS, NativeTestCase, Span, Spans,
    Status, sort_key,
};
use crate::native::counterexample::{Counterexample, Counterexamples, pooled_timelines};
use crate::native::data_source::NativeDataSource;
use crate::native::database::{
    DirectoryTestCaseDatabase, TestCaseDatabase, deserialize_choices, serialize_choices,
    serialize_nodes,
};
use crate::native::exec_cache::{ExecCache, KindLedger};
use crate::native::nd;
use crate::native::rng::EngineRng;
use crate::native::shrinker::{ShrinkProbe, ShrinkRun, Shrinker, SweepMode, absorb_stop};
use crate::settings::{
    Backend, Database, HealthCheck, NondeterminismStrictness, Output, Phase, Settings, Verbosity,
};

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
    /// `tc.event()` / `tc.event_value()` observations from this execution,
    /// in recording order. Empty for tests that record no events and on a
    /// result served from the execution cache.
    pub events: Vec<(String, Option<f64>)>,
}

const RANDOM_GENERATION_BATCH: u64 = 10;

/// Stop generating after this many consecutive generation-phase cases whose
/// realized values had been executed before — the flat-cache replacement for
/// the tree's exhaustion stop (G22), applied only while no valid case has
/// been generated. The stop exists so a tiny fully-filtered space reaches
/// the exhausted-space FilterTooMuch instead of grinding out the whole
/// invalid budget; once anything is valid the test-case budget bounds the
/// run, and a duplicate streak is routine mid-size-space behavior (at k of
/// S values seen, a streak of N duplicates has probability (k/S)^N, near 1
/// late in coupon collection — an unconditional stop would end a 32-way
/// `one_of` before reaching every alternative).
const DUPLICATE_STOP: u64 = RANDOM_GENERATION_BATCH;

/// Replays in the first-interesting determinism check, stop on first miss
/// (gate G23 option (a)): the discovering case is selection, not evidence,
/// so all four replays are fresh observations. Detection is
/// `1 - (p·s)^4` for a bug failing at rate `p` with seam survival `s`; a
/// deterministic origin pays exactly +4 executions.
const FIRST_CHECK_REPLAYS: u64 = 4;

/// Scan-replay cap for one backtrack (gate G25): the geometric profile
/// plus binary refinement fit in ~2·log2(m) replays over the accept
/// segment, but the first pass also probes every raw sighting once, so a
/// raw-heavy history spends the cap on raws — the cap is the budget there,
/// not headroom.
const BACKTRACK_SCAN_REPLAYS: u64 = nd::CONFIRM_CAP;

const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Outcome of one [`Engine::nd_replay_once`] measurement replay.
struct NdReplayOnce {
    run: RunResult,
    realized: Vec<ChoiceValue>,
    failed: bool,
}

/// Outcome of one evidence batch (experiment 005): the replays, their
/// evidence, the first failing run, and the failing timelines.
struct NdBatch {
    /// Whether the discovery bar's arithmetic accepted. Decides admission
    /// for unconfirmed origins; for trusted origins the bar is only the
    /// batch's stopping rule and any failure is evidence enough. True only
    /// with `witness` set: an accept needs an in-batch reproduction.
    bar_accepted: bool,
    evidence: nd::Evidence,
    witness: Option<RunResult>,
    captured: Vec<Vec<ChoiceValue>>,
}

/// Outcome of one history backtrack (seam plan step 4, gate G25).
enum Backtrack {
    /// A history entry cleared the discovery bar and is the origin's
    /// incumbent again; the origin is confirmed and the restored save
    /// superseded the barred one.
    Restored { nodes: Vec<ChoiceNode> },
    /// No entry cleared the bar within the scan and bar budgets. Carries
    /// the accumulated physical (fails, runs) for the caller's reject.
    Exhausted { evidence: (u64, u64) },
}

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
const TOO_SLOW_THRESHOLD: core::time::Duration = core::time::Duration::from_secs(30);

/// Health checks (TooSlow / FilterTooMuch / TestCasesTooLarge) are evaluated
/// only while the run has fewer than this many valid examples on record.
const HEALTH_CHECK_MAX_VALID: u64 = 10;

/// Number of oversized (overrun) test cases — those that exhaust the choice
/// buffer before completing — that trips TestCasesTooLarge while the run still
/// has fewer than `HEALTH_CHECK_MAX_VALID` valid examples. Mirrors
/// Hypothesis's `max_overrun_draws`.
const MAX_OVERRUN_DRAWS: u64 = 20;

/// Cap on secondary-corpus entries per database key: end-of-run
/// reconciliation evicts the shortlex-largest above it — a resource bound
/// outside decision 11's two-strike hygiene (decision 44).
const SECONDARY_CORPUS_CAP: usize = 50;

/// Run the exploration half of a test run — database replay, generation, and
/// shrinking — and return a [`TestRunResult`] with one [`Failure`] per
/// distinct bug, each carrying the origin the engine grouped on and the
/// base64 reproduce blob for the minimal counterexample: its choices, or its
/// replay state under nondeterministic handling. Only caveat-only
/// unconfirmed failures carry no blob. `Err` means the run itself failed
/// (health check, nondeterminism mismatch) before reaching a verdict.
///
/// The client builds the final report from the stamped final-replay captures;
/// the blob is for later reproduction via `hegel_run_start_blob`. Every test
/// case this runs is non-final.
pub(crate) async fn explore(
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
) -> Result<TestRunResult, RunError> {
    run_main(
        settings,
        database_key,
        exchange,
        TOO_SLOW_THRESHOLD,
        core::time::Duration::from_secs(MAX_SHRINKING_SECONDS),
    )
    .await
}

/// Run one test case (used by `Mode::SingleTestCase`) and return its
/// failure, if any.
///
/// A single test case is not a property-test run — there is no exploration,
/// shrinking, or replay — so it bypasses [`explore`] entirely; the one case
/// offered through the exchange is its own report.
pub(crate) async fn run_single_case(
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
) -> Result<Option<Failure>, RunError> {
    let mut rng = create_rng(settings, database_key)?;
    let ntc = NativeTestCase::new_random(rng.spawn())?;
    ntc.family().set_state_machine_steps_unbounded();
    let (data_source, handle) = NativeDataSource::new(ntc);
    exchange.offer(Box::new(data_source)).await;
    match NativeDataSource::take_outcome(&handle)? {
        TestCaseResult::Interesting(failure) => Ok(Some(failure)),
        _ => Ok(None),
    }
}

/// Replay a reproduce blob (used by `hegel_run_start_blob`) and return the
/// reproducing failure, if any.
///
/// A deterministic blob replays its choices up to [`nd::V1_BLOB_REPLAYS`]
/// times, each attempt with the standard continuation budget, stopping at
/// the first failure. A nondeterministic blob
/// replays its stored timelines through the same replay-until-failure
/// sequence as database reuse — per-timeline first-fit, then positional
/// splices — with no fresh-generation tier: a fresh case could fail for a
/// reason unrelated to the blob. Every replay is stamped so the client
/// captures the reproducing execution's output and diagnostic.
///
/// A run with no failures means the blob is stale; an undecodable blob is
/// the run's error.
pub(crate) async fn reproduce_blob(
    settings: &Settings,
    blob: &str,
    exchange: &CaseExchange,
) -> Result<TestRunResult, RunError> {
    match crate::native::blob::decode_blob(blob) {
        None => Err(RunError::UsageError(
            "the supplied failure blob could not be decoded. It may be corrupt or from an \
             incompatible Hegel version."
                .to_string(),
        )),
        Some(crate::native::blob::DecodedBlob::Choices(choices)) => {
            let mut rng = create_rng(settings, None)?;
            let budget =
                nd::continuation_budget(crate::native::core::flattened_values_len(&choices));
            let mut failures = Vec::new();
            for _ in 0..nd::V1_BLOB_REPLAYS {
                let mut ntc = NativeTestCase::for_probe(&choices, rng.spawn(), budget)?;
                ntc.set_should_capture();
                ntc.family()
                    .set_stateful_step_count(settings.stateful_step_count);
                let (data_source, handle) = NativeDataSource::new(ntc);
                exchange.offer(Box::new(data_source)).await;
                if let TestCaseResult::Interesting(failure) =
                    NativeDataSource::take_outcome(&handle)?
                {
                    failures.push(failure);
                    break;
                }
            }
            Ok(TestRunResult { failures })
        }
        Some(crate::native::blob::DecodedBlob::Nd(state)) => {
            let mut engine = Engine::new(settings, None, exchange)?;
            if settings.nondeterminism_strictness != NondeterminismStrictness::Error {
                #[cfg(feature = "__bench")]
                engine.seam_flip(nd::seam_dump::FlipSite::StoredV2Blob);
                engine.nd_flip();
            }
            engine.capture_replays = true;
            let (run, evidence) = engine
                .nd_reproduce(
                    None,
                    &state.timelines,
                    nd::reuse_replay_budget().div_ceil(state.timelines.len() as u64),
                    nd::REPRODUCE_SPLICES,
                    0,
                )
                .await?;
            let failures = match run.and_then(|run| run.origin) {
                Some(origin) => {
                    engine
                        .origins
                        .entry(&origin)
                        .trust(state.timelines, (evidence.fails(), evidence.runs()));
                    let caveat = engine.origins.caveat(&origin);
                    Vec::from([Failure {
                        origin,
                        reproduce_blob: None,
                        caveat,
                    }])
                }
                None => Vec::new(),
            };
            Ok(TestRunResult { failures })
        }
    }
}

/// The full multi-test-case engine: database replay, generation, and
/// shrinking, ending at the exploration report.
async fn run_main(
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
    too_slow_threshold: core::time::Duration,
    shrink_budget: core::time::Duration,
) -> Result<TestRunResult, RunError> {
    Engine::new(settings, database_key, exchange)?
        .run(too_slow_threshold, shrink_budget)
        .await
}

impl<'a> Engine<'a> {
    /// The engine driver — Hypothesis's `ConjectureRunner._run`: database
    /// replay, generation (with targeting and span mutation), shrinking,
    /// and the end-of-run database reconciliation, finishing with the
    /// exploration report of every distinct bug's shrunk counterexample.
    async fn run(
        &mut self,
        too_slow_threshold: core::time::Duration,
        shrink_budget: core::time::Duration,
    ) -> Result<TestRunResult, RunError> {
        let settings = self.settings;
        let database_key = self.database_key;
        let max_test_cases = settings.test_cases;
        let verbosity = settings.verbosity;
        let output = settings.output.clone();
        let log_phase = {
            let output = output.clone();
            move |name: &str, edge: &str| {
                if matches!(verbosity, Verbosity::Verbose | Verbosity::Debug) {
                    output.line(&format!("{edge}ing phase: {name}"));
                }
            }
        };

        let mut target_schedule = crate::native::targeting::TargetingSchedule::new(max_test_cases);
        let target_phase = settings.phases.contains(&Phase::Target);
        let invalid_budget = invalid_thresholds(INVALID_TARGET_RATE, INVALID_TARGET_CONFIDENCE);
        let mut replay_aligned = false;
        let report_multiple = settings.report_multiple_failures;

        if settings.phases.contains(&Phase::Reuse) {
            if let (Some(_), Some(key)) = (self.db(), database_key) {
                log_phase("Reuse", "Start");
                let key_bytes = key.as_bytes().to_vec();
                let secondary_key = crate::native::database::sub_key(&key_bytes, b"secondary");
                let mut values = self.db().map(|db| db.fetch(&key_bytes)).unwrap_or_default();
                values.sort_by(|a, b| shortlex(a, b));
                replay_aligned = !values.is_empty();
                let primary_count = values.len();
                let desired_factor = if settings.phases.contains(&Phase::Generate) {
                    0.1
                } else {
                    1.0
                };
                let desired_size =
                    ((libm::ceil((max_test_cases as f64) * desired_factor)) as usize).max(2);
                if values.len() < desired_size {
                    let mut extra = self
                        .db()
                        .map(|db| db.fetch(&secondary_key))
                        .unwrap_or_default();
                    extra.retain(|e| !values.contains(e));
                    let shortfall = desired_size - values.len();
                    if extra.len() > shortfall {
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
                    if i >= primary_count && found_interesting_in_primary {
                        break;
                    }
                    let stored: Vec<Vec<ChoiceValue>>;
                    let is_v2;
                    if let Some(stored_choices) = deserialize_choices(&raw) {
                        stored = Vec::from([stored_choices]);
                        is_v2 = false;
                    } else if let Some(state) = crate::native::blob::decode_nd_state(&raw) {
                        if self.settings.nondeterminism_strictness
                            != NondeterminismStrictness::Error
                        {
                            #[cfg(feature = "__bench")]
                            self.seam_flip(nd::seam_dump::FlipSite::StoredV2Reuse);
                            self.nd_flip();
                        }
                        stored = state.timelines;
                        is_v2 = true;
                    } else {
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                        continue;
                    }
                    let nd_entry = is_v2 || self.nd_handling();
                    let (run, reuse_evidence) = if !nd_entry {
                        let rng = self.rng.spawn();
                        let ntc = NativeTestCase::for_probe(&stored[0], rng, BUFFER_SIZE)?;
                        let (run, mismatch) = self.test_function(ntc).await?;
                        if let Some(err) = mismatch {
                            return Err(err);
                        }
                        let failed = run.status == Status::Interesting;
                        (failed.then_some(run), (u64::from(failed), 1))
                    } else {
                        self.capture_replays = true;
                        self.reuse_replays = true;
                        let (run, evidence) = self
                            .nd_reproduce(
                                None,
                                &stored,
                                nd::reuse_replay_budget().div_ceil(stored.len() as u64),
                                nd::REPRODUCE_SPLICES,
                                0,
                            )
                            .await?;
                        self.reuse_replays = false;
                        self.capture_replays = false;
                        (run, (evidence.fails(), evidence.runs()))
                    };
                    if let Some(run) = run {
                        if let Some(o) = run.origin.as_deref() {
                            let trusted = self.origins.entry(o);
                            trusted.trust(stored.clone(), reuse_evidence);
                            trusted.mark_first_checked();
                        }
                        let incumbent = &stored[0];
                        if i < primary_count {
                            found_interesting_in_primary = true;
                            if run.nodes.len() != incumbent.len()
                                || run
                                    .nodes
                                    .iter()
                                    .zip(incumbent)
                                    .any(|(node, stored)| node.data.value_ref() != *stored)
                            {
                                replay_aligned = false;
                            }
                        } else {
                            replay_aligned = false;
                        }
                        if !report_multiple {
                            break;
                        }
                    } else if let Some(db) = self.db() {
                        if !nd_entry {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        } else if i < primary_count {
                            db.move_value(&key_bytes, &secondary_key, &raw);
                        } else {
                            db.delete(&secondary_key, &raw);
                        }
                    }
                }
                if !self.origins.any_live() {
                    replay_aligned = false;
                }
                log_phase("Reuse", "End");
            }
        }

        let shrink_phase = settings.phases.contains(&Phase::Shrink);
        let found_in_reuse = self.origins.any_live();

        let actually_generate =
            settings.phases.contains(&Phase::Generate) && !found_in_reuse && !self.test_is_trivial;
        if actually_generate {
            log_phase("Generate", "Start");
        }
        self.collect_statistics = true;

        if settings.phases.contains(&Phase::Generate)
            && !self.test_is_trivial
            && self.within_invalid_budget(invalid_budget)
            && !found_in_reuse
        {
            let (run, mismatch) = self
                .test_function(NativeTestCase::for_simplest(BUFFER_SIZE)?)
                .await?;
            if let Some(err) = mismatch {
                return Err(err);
            }
            if let Some(msg) = large_initial_check(
                run.status == Status::EarlyStop,
                run.status,
                crate::native::core::flattened_len(&run.nodes),
                settings
                    .suppress_health_check
                    .contains(&HealthCheck::LargeInitialTestCase),
            ) {
                return Err(RunError::HealthCheck(msg));
            }
        }

        self.capture_discoveries = true;
        while settings.phases.contains(&Phase::Generate)
            && !found_in_reuse
            && !self.test_is_trivial
            && self.valid_test_cases < max_test_cases
            && self.within_invalid_budget(invalid_budget)
            && !(self.valid_test_cases == 0 && self.consecutive_duplicates >= DUPLICATE_STOP)
            && should_generate_more(
                !self.origins.any_live(),
                self.calls,
                self.first_bug_at,
                self.last_bug_at,
                shrink_phase,
                report_multiple,
                self.first_bug_time.map(|t| t.elapsed()),
            )
        {
            for _ in 0..RANDOM_GENERATION_BATCH {
                if self.test_is_trivial
                    || self.valid_test_cases >= max_test_cases
                    || !self.within_invalid_budget(invalid_budget)
                    || (self.valid_test_cases == 0 && self.consecutive_duplicates >= DUPLICATE_STOP)
                    || !should_generate_more(
                        !self.origins.any_live(),
                        self.calls,
                        self.first_bug_at,
                        self.last_bug_at,
                        shrink_phase,
                        report_multiple,
                        self.first_bug_time.map(|t| t.elapsed()),
                    )
                {
                    break;
                }

                let mut case_rng = self.rng.spawn();
                let params = crate::native::core::GenerationParameters::draw(&mut case_rng)?;
                let ntc = NativeTestCase::new_random_with_params(case_rng, params);
                if verbosity == Verbosity::Verbose {
                    output.line("Running test case");
                }

                let (run, mismatch) = self.test_function(ntc).await?;
                if let Some(err) = mismatch {
                    return Err(err);
                }

                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "test case #{}: status = {:?}, choices = {}",
                        self.calls,
                        run.status,
                        crate::native::core::flattened_len(&run.nodes)
                    ));
                }

                if !self.origins.any_live() {
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

                if target_phase
                    && !self.origins.any_live()
                    && !self.targeting.is_empty()
                    && target_schedule.should_fire(self.valid_test_cases)
                {
                    if self.nd_active {
                        self.optimise_targets_nd().await?;
                    } else {
                        let mut optimiser = crate::native::targeting::Optimiser {
                            engine: &mut *self,
                            max_valid: max_test_cases,
                            max_calls: max_test_cases * 10,
                        };
                        optimiser.optimise_targets().await?;
                    }
                }

                if run.status == Status::Valid
                    && (self.valid_test_cases >= HEALTH_CHECK_MAX_VALID || self.origins.any_live())
                {
                    self.try_span_mutation(&run.nodes, &run.spans).await?;
                }

                self.first_check_sweep().await?;
                self.nd_discovery_sweep(verbosity, &output).await?;
            }
        }

        self.first_check_sweep().await?;
        self.nd_discovery_sweep(verbosity, &output).await?;
        self.capture_discoveries = false;

        if self.consecutive_duplicates >= DUPLICATE_STOP
            && self.valid_test_cases == 0
            && !self.origins.any_live()
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
        self.collect_statistics = false;

        let mut shrink_deadline: Option<crate::sys::Instant> = None;
        if self.origins.any_live() && !replay_aligned && shrink_phase {
            log_phase("Shrink", "Start");
            if verbosity == Verbosity::Debug {
                let total: usize = self.origins.live().map(|(_, n)| n.len()).sum();
                output.line(&format!(
                    "Shrinking: {} origin(s), initial total length = {}",
                    self.origins.live().count(),
                    total
                ));
            }
            if !self.nd_handling() {
                if let (Some(_), Some(key)) = (self.db(), database_key) {
                    let key_bytes = key.as_bytes().to_vec();
                    let secondary_key = crate::native::database::sub_key(&key_bytes, b"secondary");
                    let mut entries = self
                        .db()
                        .map(|db| db.fetch(&secondary_key))
                        .unwrap_or_default();
                    entries.sort_by(|a, b| shortlex(a, b));
                    let primary_max: Option<Vec<u8>> = self
                        .origins
                        .live()
                        .map(|(_, nodes)| {
                            let choices: Vec<ChoiceValue> =
                                nodes.iter().map(|n| n.value()).collect();
                            serialize_choices(&choices)
                        })
                        .max_by(|a, b| shortlex(a, b));
                    for raw in entries {
                        if primary_max
                            .as_ref()
                            .is_some_and(|m| shortlex(&raw, m) == core::cmp::Ordering::Greater)
                        {
                            break;
                        }
                        if let Some(stored_choices) = deserialize_choices(&raw) {
                            let ntc = NativeTestCase::for_choices(&stored_choices, None, None);
                            let (_, mismatch) = self.test_function(ntc).await?;
                            if let Some(err) = mismatch {
                                return Err(err);
                            }
                            if let Some(db) = self.db() {
                                db.delete(&secondary_key, &raw);
                            }
                            // A flip makes single-replay deletes unsound for
                            // the remaining entries (decision 11's budget
                            // derivation).
                            if self.nd_handling() {
                                break;
                            }
                        } else if crate::native::blob::decode_nd_state(&raw).is_some() {
                            // A v2 entry's hygiene lives in the reuse phase's
                            // budgeted strikes: a pre-shrink reproduction
                            // could change no outcome (decisions 20/24).
                            continue;
                        } else if let Some(db) = self.db() {
                            db.delete(&secondary_key, &raw);
                        }
                    }
                }
            }

            shrink_deadline = crate::sys::Instant::now().map(|now| now + shrink_budget);
            let mut shrink_timed_out = false;
            let mut shrunk_origins: crate::native::HashSet<String> =
                crate::native::HashSet::default();
            loop {
                let mut pending: Vec<String> = self
                    .origins
                    .live_origins()
                    .into_iter()
                    .filter(|o| !shrunk_origins.contains(o.as_str()))
                    .collect();
                if pending.is_empty() {
                    break;
                }
                pending.sort();
                let origin = pending.remove(0);
                let initial = self
                    .origins
                    .incumbent(&origin)
                    .map(<[ChoiceNode]>::to_vec)
                    .unwrap_or_default();
                shrink_timed_out |= self
                    .shrink_origin(
                        origin,
                        initial,
                        verbosity,
                        &output,
                        shrink_deadline,
                        &mut shrunk_origins,
                    )
                    .await?;
            }

            if shrink_timed_out && verbosity != Verbosity::Quiet {
                output.line(&slow_shrink_warning());
            }

            if verbosity == Verbosity::Debug {
                let total: usize = self.origins.live().map(|(_, n)| n.len()).sum();
                output.line(&format!(
                    "Shrinking complete: {} origin(s), final total length = {}",
                    self.origins.live().count(),
                    total
                ));
            }
            log_phase("Shrink", "End");
        } else if replay_aligned && verbosity == Verbosity::Debug {
            output.line("Skipping shrink: reused aligned database replay");
        }

        let final_deadline =
            shrink_deadline.or_else(|| crate::sys::Instant::now().map(|now| now + shrink_budget));
        self.final_replay(verbosity, &output, final_deadline, shrink_phase)
            .await?;

        if let (Some(db), Some(key)) = (self.db(), database_key) {
            let key_bytes = key.as_bytes();
            let secondary_key = crate::native::database::sub_key(key_bytes, b"secondary");
            let new_entries: crate::native::HashSet<Vec<u8>> = if self.nd_handling() {
                self.origins
                    .iter()
                    .filter(|(_, c)| !c.needs_confirmation())
                    .filter_map(|(_, c)| c.incumbent_values().map(|v| c.repro_state(v)))
                    .map(|state| crate::native::blob::encode_nd_state(&state))
                    .collect()
            } else {
                self.origins
                    .live()
                    .map(|(_, nodes)| {
                        let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
                        serialize_choices(&choices)
                    })
                    .collect()
            };
            let primary_now = db.fetch(key_bytes);
            for new_bytes in &new_entries {
                db.save(key_bytes, new_bytes);
            }
            for old in primary_now {
                if new_entries.contains(&old) {
                    continue;
                }
                if self.persister.saved_this_run.contains(&old)
                    && !self.persister.preexisting.contains(&old)
                {
                    db.delete(key_bytes, &old);
                } else {
                    db.move_value(key_bytes, &secondary_key, &old);
                }
            }
            let mut secondary_now = db.fetch(&secondary_key);
            if secondary_now.len() > SECONDARY_CORPUS_CAP {
                secondary_now.sort_by(|a, b| shortlex(a, b));
                for evicted in &secondary_now[SECONDARY_CORPUS_CAP..] {
                    db.delete(&secondary_key, evicted);
                }
            }
        }

        if verbosity == Verbosity::Debug {
            output.line(&format!(
                "Test done. interesting_test_cases={}",
                self.origins.live().count()
            ));
        }

        if settings.show_statistics {
            for line in self.statistics.render() {
                output.line(&line);
            }
        }

        Ok(self.build_report())
    }

    /// Assemble the run's failure report, enforcing decision 24 at the
    /// seam: blobs and replay-state caveats only for origins past
    /// confirmation — the same [`OriginLifecycle::needs_confirmation`]
    /// predicate the persistence filter uses — with the partition applied
    /// before the sort and the single-failure truncation, so a leaked
    /// unconfirmed origin can never displace a confirmed one. Unconfirmed
    /// origins (bar rejects and never-replayed report-time admissions
    /// alike) report caveat-only, and only when nothing confirmed or
    /// trusted survived (decision 3).
    fn build_report(&mut self) -> TestRunResult {
        let nd_blobs = self.nd_handling();
        let mut origins_sorted: Vec<(String, Vec<ChoiceNode>)> = self
            .origins
            .iter_mut()
            .filter(|(_, c)| !nd_blobs || !c.needs_confirmation())
            .filter_map(|(origin, c)| c.evict().map(|nodes| (origin.to_string(), nodes)))
            .collect();
        origins_sorted.sort_by(|a, b| sort_key(&b.1).cmp(&sort_key(&a.1)));

        if !self.settings.report_multiple_failures {
            if let Some(last) = origins_sorted.pop() {
                origins_sorted.clear();
                origins_sorted.push(last);
            }
        }

        let mut failures: Vec<Failure> = Vec::with_capacity(origins_sorted.len());
        for (origin, nodes) in origins_sorted {
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            let (reproduce_blob, caveat) = if nd_blobs {
                let state = self.nd_state_for(&origin, choices);
                (
                    Some(crate::native::blob::encode_nd_failure(&state)),
                    self.origins.caveat(&origin),
                )
            } else {
                (Some(crate::native::blob::encode_failure(&choices)), None)
            };
            failures.push(Failure {
                origin,
                reproduce_blob,
                caveat,
            });
        }
        if failures.is_empty() && nd_blobs {
            let mut unconfirmed: Vec<String> =
                self.origins.unconfirmed().map(str::to_string).collect();
            if !self.settings.report_multiple_failures {
                unconfirmed.truncate(1);
            }
            for origin in unconfirmed {
                failures.push(Failure {
                    caveat: self.origins.caveat(&origin),
                    origin,
                    reproduce_blob: None,
                });
            }
        }
        TestRunResult { failures }
    }
}

/// Pre-bug we always keep generating; post-bug we keep going just long
/// enough to surface other distinct origins. The window is
/// `min(first_bug + 1000, last_bug * 2)`, with a minimum-call floor
/// (`MIN_TEST_CALLS`) so very-cheap tests still produce a few extra probes.
///
/// A bug replayed from the **database** never reaches this heuristic: the
/// generation loop is gated on `!found_in_reuse` at the call site, so the
/// stored example is not followed by a fresh generation pass at all. The
/// replay-logic test (`test_does_not_shrink_on_replay`) pins this behaviour
/// at exactly 2 calls (replay + final replay). The `first_bug_at == None`
/// branch below is therefore a defensive default, not the reuse path.
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
    total_test_time: core::time::Duration,
    threshold: core::time::Duration,
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
/// The first-interesting check's structural-miss diagnostic (decision 30,
/// amended by G26): names the divergence position, richer than the tree's
/// kind message. Used under `error` strictness; quiet and warn flip
/// instead.
fn first_check_diagnostic(expected: &[ChoiceValue], realized: &[ChoiceValue]) -> String {
    let at = expected
        .iter()
        .zip(realized)
        .take_while(|(e, r)| *e == *r)
        .count();
    let detail = if at == expected.len().min(realized.len()) {
        format!(
            "{} choices were recorded but the replay realized {}",
            expected.len(),
            realized.len()
        )
    } else {
        format!("{:?} became {:?}", expected[at], realized[at])
    };
    format!(
        "Your test is non-deterministic: replaying the discovered failing \
         example diverged from its recorded choices at position {at} ({detail}). \
         This usually means the test or a generator depends on global mutable \
         state."
    )
}

pub(crate) fn flaky_diagnostic() -> String {
    "Flaky test detected: Your test produced different outcomes \
     when run with the same generated data — it failed when it \
     previously succeeded, or succeeded when it previously failed. \
     This usually means your test depends on external state such as \
     global variables, system time, or external random number generators."
        .to_string()
}

/// Notice emitted once per run under
/// [`NondeterminismStrictness::Warn`], when detection first switches the
/// run into nondeterministic handling.
pub(crate) fn nondeterminism_notice() -> &'static str {
    "Nondeterministic test behavior detected: failures are now confirmed by \
     repeated replay before being shrunk or persisted, and unconfirmed \
     failures are reported with a caveat. Set nondeterminism_strictness to \
     quiet to silence this notice, or to error to abort instead."
}

/// Warning emitted when shrinking exhausts its wall-clock budget and stops
/// early. Unlike a health-check failure this is not a failure: the smallest
/// counterexample found so far is still reported. Returned as a string
/// (rather than printed inline) so it can be asserted directly in tests.
/// Mirrors Hypothesis's slow-shrink notice.
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
    let base = libm::ceil(libm::log(1.0 - c) / libm::log(1.0 - r)) - 1.0;
    let per_valid = libm::ceil(1.0 / r);
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
fn shortlex(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn should_generate_more(
    no_bug_yet: bool,
    calls: u64,
    first_bug_at: Option<u64>,
    last_bug_at: Option<u64>,
    shrink_enabled: bool,
    report_multiple: bool,
    first_bug_elapsed: Option<core::time::Duration>,
) -> bool {
    if no_bug_yet {
        return true;
    }
    if !shrink_enabled || !report_multiple {
        return false;
    }
    if first_bug_elapsed.is_some_and(|d| d > core::time::Duration::from_secs(10)) {
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

/// Incremental database-save bookkeeping. Every time a new interesting
/// result is found (or an existing one is shortlex-improved), the realised
/// choice sequence is saved to the primary key, then the bytes it
/// supersedes are deleted. Saving before deleting keeps the primary key
/// carrying the most recent validated incumbent at every instant, so a
/// Ctrl-C / SIGTERM mid-shrink loses nothing (decision 44).
///
/// A superseded same-run save is deleted, never demoted: it never ended a
/// run as anyone's best example, so it earned no cross-run staleness
/// strike. A run-start primary entry (in `preexisting`) *did* end a run as
/// someone's best example, so superseding it demotes it to the secondary
/// key (decision 11's strike one) even when a reuse replay re-saved its
/// bytes this run; end-of-run reconciliation demotes the rest, using
/// `saved_this_run` to tell run-start entries from same-run leftovers.
/// Bytes another origin's last save still points at are never deleted:
/// entries are content-addressed, so two origins can share one file.
struct Persister<'a> {
    db: Option<Box<dyn TestCaseDatabase>>,
    database_key: Option<&'a str>,
    /// For each origin we've saved at least once, the choice-node sequence
    /// of the most recent save and the exact bytes written. Used to (a)
    /// decide whether a new result is shortlex-smaller and therefore worth
    /// saving, and (b) know the bytes to delete when it is.
    last_saved: HashMap<String, (Vec<ChoiceNode>, Vec<u8>)>,
    /// Every byte string saved this run, so end-of-run reconciliation can
    /// delete superseded same-run leftovers instead of demoting them.
    saved_this_run: crate::native::HashSet<Vec<u8>>,
    /// The primary key's entries at run start: superseding one demotes it
    /// instead of deleting it, whether mid-run or at reconciliation.
    preexisting: crate::native::HashSet<Vec<u8>>,
}

impl<'a> Persister<'a> {
    fn new(db: Option<Box<dyn TestCaseDatabase>>, database_key: Option<&'a str>) -> Self {
        let preexisting = match (db.as_deref(), database_key) {
            (Some(db), Some(key)) => db.fetch(key.as_bytes()).into_iter().collect(),
            _ => crate::native::HashSet::default(),
        };
        Persister {
            db,
            database_key,
            last_saved: HashMap::default(),
            saved_this_run: crate::native::HashSet::default(),
            preexisting,
        }
    }

    /// Record an interesting result for `origin`. If this is the first
    /// sighting, or shortlex-precedes the previous save, the new bytes are
    /// written to the primary key and any previously-saved bytes for this
    /// origin are then deleted.
    fn record(&mut self, origin: &str, nodes: &[ChoiceNode]) {
        let new_choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
        let new_bytes = serialize_choices(&new_choices);
        self.record_bytes(origin, nodes, new_bytes);
    }

    /// [`Self::record`] for a nondeterministic origin: the entry is the
    /// version-2 format carrying the incumbent's replay state
    /// ([`crate::native::blob::encode_nd_state`]).
    fn record_nd(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
        state: &crate::native::blob::NdReproState,
    ) {
        let new_bytes = crate::native::blob::encode_nd_state(state);
        self.record_bytes(origin, nodes, new_bytes);
    }

    /// [`Self::record_nd`] for a backtrack-restored incumbent: the restored
    /// nodes are shortlex-larger than the barred shrunk save, which
    /// `record_bytes`'s monotone `needs_save` would refuse, leaving the
    /// shrunk bytes as primary. Saving first and then deleting the
    /// superseded bytes preserves decision 44's ordering.
    fn supersede_nd(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
        state: &crate::native::blob::NdReproState,
    ) {
        let new_bytes = crate::native::blob::encode_nd_state(state);
        self.record_bytes_forced(origin, nodes, new_bytes, true);
    }

    fn record_bytes(&mut self, origin: &str, nodes: &[ChoiceNode], new_bytes: Vec<u8>) {
        self.record_bytes_forced(origin, nodes, new_bytes, false);
    }

    fn record_bytes_forced(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
        new_bytes: Vec<u8>,
        force: bool,
    ) {
        let Some(db) = self.db.as_deref() else { return };
        let Some(key) = self.database_key else { return };
        let key_bytes = key.as_bytes();

        let needs_save = force
            || match self.last_saved.get(origin) {
                None => true,
                Some((prev, prev_bytes)) => {
                    sort_key(nodes) < sort_key(prev)
                        || (sort_key(nodes) == sort_key(prev) && *prev_bytes != new_bytes)
                }
            };
        if !needs_save {
            return;
        }

        db.save(key_bytes, &new_bytes);
        if let Some((_, prev_bytes)) = self.last_saved.get(origin) {
            if *prev_bytes != new_bytes {
                let shared = self
                    .last_saved
                    .iter()
                    .any(|(o, (_, bytes))| o != origin && bytes == prev_bytes);
                if !shared {
                    if self.preexisting.contains(prev_bytes) {
                        let secondary_key =
                            crate::native::database::sub_key(key_bytes, b"secondary");
                        db.move_value(key_bytes, &secondary_key, prev_bytes);
                    } else {
                        db.delete(key_bytes, prev_bytes);
                    }
                }
            }
        }
        self.saved_this_run.insert(new_bytes.clone());
        self.last_saved
            .insert(origin.to_string(), (nodes.to_vec(), new_bytes));
    }
}

/// The native engine — Hegel's analogue of Hypothesis's `ConjectureRunner`.
///
/// One object owns everything a test run touches: the exchange it offers
/// test cases through, the RNG, the example database (via the [`Persister`]),
/// the execution cache, the per-origin interesting map, targeting
/// observations, and all run-level counters. The [`ExecCache`] keys every
/// executed conclusion on its realized choice values: exact repeats are
/// served without re-running the body, a repeat concluding differently is
/// nondeterminism evidence, and the consecutive-duplicate counter it feeds
/// is what stops generation on an exhausted space
/// (`notes/experiments/010-tree-value`).
///
/// Every execution records into the cache via [`Self::record_run`].
/// [`Self::test_function`] is the raw executor+recorder (generation goes
/// straight through it — its duplicates must execute, they are the stop
/// signal); [`Self::cached_test_function`] is the single replay chokepoint
/// shared by generation-phase span mutation and shrinking — it serves an
/// exact repeat from the cache and otherwise falls through to
/// `test_function`. `cached_test_function` returns the realised result; the
/// interesting-origin filter is applied by its caller, and bugs with new
/// origins surface through the same [`update_interesting`] path as
/// generation.
pub(crate) struct Engine<'a> {
    settings: &'a Settings,
    database_key: Option<&'a str>,
    exchange: &'a CaseExchange,
    rng: EngineRng,
    persister: Persister<'a>,
    pub(crate) exec_cache: ExecCache,
    /// Error-strictness generation-nondeterminism detector: within-run,
    /// cross-execution kind drift at a shared value prefix aborts with the
    /// tree's diagnostic. Maintained only under
    /// [`NondeterminismStrictness::Error`] — under quiet/warn, verdict
    /// flips (the cache) and replay checks carry detection instead — and
    /// never fed between runs: a stored entry that stops reproducing is
    /// staleness, not nondeterminism (decision 9).
    kind_ledger: KindLedger,
    /// Consecutive generation-phase conclusions whose realized values had
    /// been executed before. [`DUPLICATE_STOP`] of these ends generation
    /// while no valid case exists; a novel conclusion resets it. Frozen
    /// (at zero) under `nd_active`.
    pub(crate) consecutive_duplicates: u64,
    /// Per-origin tracking: each distinct panic site (file:line:col captured
    /// by [`crate::run_lifecycle::run_test_case`]) gets its own
    /// [`Counterexample`](crate::native::counterexample::Counterexample) —
    /// its incumbent, pool, standing, evidence, history, and budgets. This
    /// is what makes a single test that fails with several distinct bugs
    /// surface each one.
    pub(crate) origins: Counterexamples,
    pub(crate) targeting: crate::native::targeting::TargetingState,
    /// Event statistics for the end-of-run report, folded in by
    /// [`Self::record_run`] while [`Self::collect_statistics`] is set.
    pub(crate) statistics: crate::native::events::RunStatistics,
    /// Set for the duration of the generation phase, the only phase whose
    /// cases feed [`Self::statistics`]: shrinking replays the same target
    /// over and over and would swamp the reported distributions.
    pub(crate) collect_statistics: bool,
    pub(crate) calls: u64,
    pub(crate) valid_test_cases: u64,
    pub(crate) invalid_test_cases: u64,
    pub(crate) overrun_test_cases: u64,
    pub(crate) total_test_time: core::time::Duration,
    pub(crate) test_is_trivial: bool,
    pub(crate) first_bug_at: Option<u64>,
    pub(crate) last_bug_at: Option<u64>,
    pub(crate) first_bug_time: Option<crate::sys::Instant>,
    /// Sticky detection flag: the run observed nondeterministic test
    /// behavior — a cache verdict mismatch or a verify status/origin
    /// flake — or `Settings::nd_force` started it flipped. While set, the
    /// run trusts no cached prediction: execution-cache recording and
    /// serving and the duplicate stop are off, and targeting switches from
    /// single-run hill climbing to the measured race
    /// ([`Self::optimise_targets_nd`], decision 68). Never cleared within a
    /// run.
    pub(crate) nd_active: bool,
    /// Set while the first-interesting check's replays run: they count on
    /// the measurement statistics line despite running pre-flip (decision
    /// 51, amended), and a cache mismatch they trigger is the check's
    /// detection, not a generation flake (the seam dump's site).
    check_window: bool,
    /// Set around the reuse phase's `nd_reproduce` replays: they are
    /// measurement runs, but their reproductions must still displace and
    /// persist — under `error` strictness a v2 entry reproduces with
    /// `nd_active` still false, and populating `interesting` there is what
    /// makes the run skip generation. Every other measurement run leaves
    /// the incumbent and the database alone.
    reuse_replays: bool,
    /// While set, every measurement execution is stamped for capture
    /// (`hegel_test_case_should_capture`), telling the client to buffer
    /// its output and diagnostic — the material for the failure report.
    /// Set around confirmation batches, database-reuse replays, the
    /// final replay, and ND blob replays (the deterministic choices-blob
    /// path stamps its case directly); shrink-gauntlet and boost replays
    /// stay cheap and unstamped.
    capture_replays: bool,
    /// While set, generation-phase executions under ND handling are
    /// stamped too, so an origin the run discovers but never confirms
    /// still reports its discovering case's draws and diagnostic instead
    /// of a bare caveat. Not stamped: a gauntlet-discovered origin (its
    /// probes are measurement runs), and the case that itself flips the
    /// run (its stamp decision predates the flip).
    capture_discoveries: bool,
    /// Seam-dump site for a cache-mismatch flip detected inside the
    /// execution this is set around (the shrink verify and the
    /// deterministic final replay), so experiment 011's flip-site table
    /// attributes an aligned outcome-only miss to its calling site instead
    /// of the generic cache channel.
    #[cfg(feature = "__bench")]
    flip_site_hint: Option<nd::seam_dump::FlipSite>,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        settings: &'a Settings,
        database_key: Option<&'a str>,
        exchange: &'a CaseExchange,
    ) -> Result<Self, RunError> {
        let db: Option<Box<dyn TestCaseDatabase>> = match &settings.database {
            Database::Path(path) => Some(Box::new(DirectoryTestCaseDatabase::new(path))),
            Database::Unset => Some(Box::new(DirectoryTestCaseDatabase::new(".hegel/examples"))),
            Database::Disabled => None,
        };
        Ok(Engine {
            settings,
            database_key,
            exchange,
            rng: create_rng(settings, database_key)?,
            persister: Persister::new(db, database_key),
            exec_cache: ExecCache::default(),
            kind_ledger: KindLedger::default(),
            consecutive_duplicates: 0,
            origins: Counterexamples::default(),
            targeting: crate::native::targeting::TargetingState::new(),
            statistics: crate::native::events::RunStatistics::default(),
            collect_statistics: false,
            calls: 0,
            valid_test_cases: 0,
            invalid_test_cases: 0,
            overrun_test_cases: 0,
            total_test_time: core::time::Duration::ZERO,
            test_is_trivial: false,
            first_bug_at: None,
            last_bug_at: None,
            first_bug_time: None,
            nd_active: settings.nd_force,
            check_window: false,
            reuse_replays: false,
            capture_replays: false,
            capture_discoveries: false,
            #[cfg(feature = "__bench")]
            flip_site_hint: None,
        })
    }

    /// Whether the full nondeterministic pipeline — discovery confirmation,
    /// the shrink gauntlet, pools, validated persistence, caveated
    /// reporting — is driving this run. Concurrent-machine runs flow
    /// through it like any other nondeterministic run (experiment 007).
    fn nd_handling(&self) -> bool {
        self.nd_active
    }

    /// Record a flip event for [`nd::seam_dump`] (experiment 011): the
    /// detection site, the call count, and the interesting map at flip
    /// time. Call before `nd_flip` at each detection site; a no-op when
    /// the run is already flipped or the dump is unarmed.
    #[cfg(feature = "__bench")]
    fn seam_flip(&self, site: nd::seam_dump::FlipSite) {
        if self.nd_active {
            return;
        }
        nd::seam_dump::record(nd::seam_dump::SeamEvent::Flip {
            site,
            calls: self.calls,
            incumbents: self
                .origins
                .live()
                .map(|(origin, nodes)| {
                    (
                        origin.to_string(),
                        nodes.iter().map(|n| n.value()).collect(),
                    )
                })
                .collect(),
        });
    }

    /// Switch the run into nondeterministic handling, per
    /// [`crate::settings::NondeterminismStrictness`]. Idempotent; callers
    /// abort instead of flipping under `Error`.
    fn nd_flip(&mut self) {
        if self.nd_active {
            return;
        }
        self.nd_active = true;
        self.exec_cache.clear();
        self.kind_ledger.clear();
        self.consecutive_duplicates = 0;
        if self.settings.nondeterminism_strictness == NondeterminismStrictness::Warn
            && self.settings.verbosity != Verbosity::Quiet
        {
            self.settings.output.line(nondeterminism_notice());
        }
    }

    /// One measurement replay of `timeline` with the standard continuation
    /// budget: reports whether the run reproduced `origin` (any interesting
    /// origin when `None`) and the realized timeline. One Bernoulli trial
    /// of the test case, whatever the replay realized (decision 71). A
    /// choice-tree mismatch aborts under `Error` strictness like any other
    /// execution.
    async fn nd_replay_once(
        &mut self,
        timeline: &[ChoiceValue],
        origin: Option<&str>,
    ) -> Result<NdReplayOnce, RunError> {
        let budget = nd::continuation_budget(crate::native::core::flattened_values_len(timeline));
        let ntc = NativeTestCase::for_probe(timeline, self.rng.spawn(), budget)?;
        let (run, mismatch) = self.measure(ntc).await?;
        if let Some(err) = mismatch {
            return Err(err);
        }
        let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
        let failed = run.status == Status::Interesting
            && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
        Ok(NdReplayOnce {
            run,
            realized,
            failed,
        })
    }

    /// Replay-until-failure over stored ND state (decision 25): each
    /// timeline first-fit under a per-timeline replay budget, then
    /// positional splices of random timeline pairs, then up to `fresh`
    /// fresh generations. Returns the first reproducing run plus the
    /// evidence accumulated across every attempt, for the caller's hygiene
    /// verdict; the fresh tier is a rescue, not a replay of the stored
    /// state, so only its failures enter the evidence.
    async fn nd_reproduce(
        &mut self,
        origin: Option<&str>,
        timelines: &[Vec<ChoiceValue>],
        per_timeline_budget: u64,
        splices: u64,
        fresh: u64,
    ) -> Result<(Option<RunResult>, nd::Evidence), RunError> {
        let mut evidence = nd::Evidence::default();
        for timeline in timelines {
            for _ in 0..per_timeline_budget {
                let replay = self.nd_replay_once(timeline, origin).await?;
                evidence.record(replay.failed);
                if replay.failed {
                    return Ok((Some(replay.run), evidence));
                }
            }
        }
        if timelines.len() >= 2 {
            for _ in 0..splices {
                let a = self.rng.random_range(0..timelines.len());
                let mut b = self.rng.random_range(0..timelines.len() - 1);
                if b >= a {
                    b += 1;
                }
                let (left, right) = (&timelines[a], &timelines[b]);
                let cut = self.rng.random_range(0..=left.len().min(right.len()));
                let mut spliced = Vec::with_capacity(cut + right.len() - cut);
                spliced.extend_from_slice(&left[..cut]);
                spliced.extend_from_slice(&right[cut..]);
                let replay = self.nd_replay_once(&spliced, origin).await?;
                evidence.record(replay.failed);
                if replay.failed {
                    return Ok((Some(replay.run), evidence));
                }
            }
        }
        for _ in 0..fresh {
            let ntc = NativeTestCase::new_random(self.rng.spawn())?;
            let (run, mismatch) = self.measure(ntc).await?;
            if let Some(err) = mismatch {
                return Err(err);
            }
            let failed = run.status == Status::Interesting
                && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
            if failed {
                evidence.record(true);
                return Ok((Some(run), evidence));
            }
        }
        Ok((None, evidence))
    }

    /// The report-time final replay: every failure re-executes before it
    /// is reported, stamped so the client captures the failing execution's
    /// output and diagnostic — the failure report's material. A
    /// deterministic run replays each shrunk incumbent once; a replay that
    /// no longer fails is nondeterminism detected post-shrink — today's
    /// Flaky abort under `error` strictness, a flip into ND handling
    /// otherwise. Under ND handling each origin replays until failure —
    /// incumbent, pool, splices, then [`nd::FINAL_REPLAY_FRESH`] fresh
    /// generations, up to the standard reuse budget — and the evidence
    /// lands in the lifecycle: a reproducing replay on a yet-unconfirmed
    /// origin is a sighting whose realized run then faces the standard bar
    /// on the origin's remaining attempts (decision 72); a dry confirmed
    /// origin switches its caveat's wording instead of unreporting the
    /// failure (decision 3); a dry unconfirmed origin is evicted like a
    /// bar reject and reaches the report only through the caveat-only
    /// fallback (decision 24).
    /// One origin's shrink pass: the pre-shrink verify, admission (stashed
    /// witness, trusted batch, or the discovery bar), optional boost, and
    /// the shrinker run, with decision 38's requeue semantics. Returns
    /// whether the shrinker hit the deadline. A method rather than shrink-
    /// loop code so report-time backtracking can re-enter a per-origin
    /// shrink (seam plan).
    async fn shrink_origin(
        &mut self,
        origin: String,
        initial: Vec<ChoiceNode>,
        verbosity: Verbosity,
        output: &Output,
        shrink_deadline: Option<crate::sys::Instant>,
        shrunk_origins: &mut crate::native::HashSet<String>,
    ) -> Result<bool, RunError> {
        let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value()).collect();
        let mut probe_anchor = 0.0f64;
        let deterministic_verify = if self.nd_handling() {
            None
        } else {
            let verify_ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
            #[cfg(feature = "__bench")]
            {
                self.flip_site_hint = Some(nd::seam_dump::FlipSite::ShrinkVerify);
            }
            let outcome = self.test_function(verify_ntc).await;
            #[cfg(feature = "__bench")]
            {
                self.flip_site_hint = None;
            }
            let (verify, mismatch) = outcome?;
            if let Some(err) = mismatch {
                return Err(err);
            }
            if verify.status == Status::Interesting
                && verify.origin.as_deref() == Some(origin.as_str())
            {
                (!self.nd_handling()).then_some(verify)
            } else if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
                return Err(RunError::Flaky(flaky_diagnostic()));
            } else {
                #[cfg(feature = "__bench")]
                self.seam_flip(nd::seam_dump::FlipSite::ShrinkVerify);
                self.nd_flip();
                None
            }
        };
        let verify = if let Some(verify) = deterministic_verify {
            verify
        } else if let Some((witness, anchor)) =
            self.origins.get_mut(&origin).and_then(|c| c.take_witness())
        {
            probe_anchor = anchor;
            witness
        } else if !self.origins.needs_confirmation(&origin) {
            // For a trusted origin the bar arithmetic is only the batch's
            // stopping rule: admission happened at reuse (decision 24).
            let batch = self.nd_evidence_batch(&origin, &choices, None).await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if let Some(witness) = batch.witness {
                probe_anchor = batch.evidence.lower_bound();
                let trusted = self.origins.entry(&origin);
                let pool = pooled_timelines(
                    choices.clone(),
                    batch.captured.into_iter().chain(trusted.pool().to_vec()),
                );
                trusted.confirm(probe_anchor, None, pool, evidence)?;
                self.record_nd_incumbent(&origin, &initial);
                witness
            } else {
                self.origins.entry(&origin).record_trusted_batch(evidence);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
        } else {
            if self.has_history(&origin) {
                match self.backtrack(&origin).await? {
                    Backtrack::Restored { nodes } => {
                        self.origins.entry(&origin).replace(nodes);
                        return Ok(false);
                    }
                    Backtrack::Exhausted { evidence } => {
                        self.reject_origin(&origin, evidence, false);
                        shrunk_origins.insert(origin);
                        return Ok(false);
                    }
                }
            }
            if !self.origins.entry(&origin).spend_bar_attempt() {
                self.reject_origin(&origin, (0, 0), false);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
            let batch = self.nd_evidence_batch(&origin, &choices, None).await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if !batch.bar_accepted {
                self.reject_origin(&origin, evidence, false);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
            let witness = crate::control::hegel_internal_unwrap!(
                batch.witness,
                "nd_evidence_batch: bar accept without a witness for {origin}"
            );
            probe_anchor = batch.evidence.lower_bound();
            let pool = pooled_timelines(choices.clone(), batch.captured);
            self.origins
                .entry(&origin)
                .confirm(probe_anchor, None, pool, evidence)?;
            self.record_nd_incumbent(&origin, &initial);
            witness
        };

        let mut verify = verify;
        if self.nd_handling() && probe_anchor < nd::BOOST_RELIABILITY_FLOOR {
            let incumbent: Vec<ChoiceValue> = verify.nodes.iter().map(|n| n.value()).collect();
            if let Some((witness, lcb)) = self.nd_boost(&origin, &incumbent, probe_anchor).await? {
                verify = witness;
                probe_anchor = lcb;
            }
        }

        let initial_spans = Spans::from(verify.spans.clone());
        let gauntleted = self.nd_handling();
        let (shrunk, timed_out) = {
            let probe = EngineShrinkProbe {
                engine: &mut *self,
                target_origin: origin.clone(),
                verbosity,
                output: output.clone(),
                gauntlet: gauntleted,
                ledger: HashMap::default(),
                anchor: probe_anchor,
                sweep: SweepMode::Fast,
                raised: crate::native::HashSet::default(),
                pending_accept: None,
            };
            let mut shrinker = Shrinker::with_probe(Box::new(probe), verify.nodes, initial_spans);
            shrinker.deadline = shrink_deadline;
            absorb_stop(shrinker.initial_coarse_reduction().await)?;
            if verbosity == Verbosity::Debug {
                let output = output.clone();
                shrinker.set_debug(move |msg| output.line(msg));
            }
            shrinker.shrink().await?;
            (shrinker.current_nodes, shrinker.timed_out)
        };
        if !gauntleted && self.nd_handling() {
            self.origins.entry(&origin).replace(initial);
        } else {
            self.origins.entry(&origin).replace(shrunk);
            shrunk_origins.insert(origin);
        }
        Ok(timed_out)
    }

    /// The engine-owned final replay (decision 30): one exact replay per
    /// origin while the run is deterministic, the pooled reproduction under
    /// ND handling. A deterministic miss flips the run; a never-confirmed
    /// origin with history then backtracks (gate G25) — a restored
    /// incumbent re-shrinks under the gauntlet on the shrink deadline's
    /// remaining budget before its pooled replay, an exhausted backtrack
    /// rejects into the caveat-only report. The same backtrack runs when a
    /// never-confirmed origin's pooled review itself comes up dry with
    /// history on record. Origins exactly replayed before a flip —
    /// a later origin's, or one detected inside their own successful
    /// replay — re-enter the queue for the pooled review: their single
    /// replay predates what the run now knows.
    async fn final_replay(
        &mut self,
        verbosity: Verbosity,
        output: &Output,
        shrink_deadline: Option<crate::sys::Instant>,
        reshrink: bool,
    ) -> Result<(), RunError> {
        if !self.origins.any_live() {
            return Ok(());
        }
        let mut pending: Vec<String> = self.origins.live_origins();
        let mut replayed: Vec<String> = Vec::new();
        while !pending.is_empty() {
            let origin = pending.remove(0);
            let nodes = self
                .origins
                .incumbent(&origin)
                .map(<[ChoiceNode]>::to_vec)
                .unwrap_or_default();
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            if !self.nd_handling() {
                self.capture_replays = true;
                #[cfg(feature = "__bench")]
                {
                    self.flip_site_hint = Some(nd::seam_dump::FlipSite::FinalReplay);
                }
                let ntc = NativeTestCase::for_choices(&choices, Some(&nodes), None);
                let outcome = self.measure(ntc).await;
                self.capture_replays = false;
                #[cfg(feature = "__bench")]
                {
                    self.flip_site_hint = None;
                }
                let (run, mismatch) = outcome?;
                if let Some(err) = mismatch {
                    return Err(err);
                }
                let reproduced = run.status == Status::Interesting
                    && run.origin.as_deref() == Some(origin.as_str());
                if reproduced && !self.nd_handling() {
                    replayed.push(origin);
                    continue;
                }
                pending.append(&mut replayed);
                if !reproduced {
                    if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
                        return Err(RunError::Flaky(flaky_diagnostic()));
                    }
                    #[cfg(feature = "__bench")]
                    self.seam_flip(nd::seam_dump::FlipSite::FinalReplay);
                    self.nd_flip();
                    if self.origins.needs_confirmation(&origin) && self.has_history(&origin) {
                        match self.backtrack(&origin).await? {
                            Backtrack::Restored { nodes } => {
                                self.origins.entry(&origin).replace(nodes.clone());
                                if reshrink {
                                    let mut shrunk = crate::native::HashSet::default();
                                    self.shrink_origin(
                                        origin.clone(),
                                        nodes,
                                        verbosity,
                                        output,
                                        shrink_deadline,
                                        &mut shrunk,
                                    )
                                    .await?;
                                }
                            }
                            Backtrack::Exhausted { evidence } => {
                                self.reject_origin(&origin, evidence, true);
                                continue;
                            }
                        }
                    }
                }
            }
            crate::control::hegel_internal_assert!(
                self.origins.incumbent(&origin).is_some(),
                "final_replay: {origin} lost its incumbent without continuing"
            );
            let timelines = self.origins.entry(&origin).timelines();
            self.capture_replays = true;
            let (reproduction, evidence) = self
                .nd_reproduce(
                    Some(&origin),
                    &timelines,
                    nd::reuse_replay_budget().div_ceil(timelines.len() as u64),
                    nd::REPRODUCE_SPLICES,
                    nd::FINAL_REPLAY_FRESH,
                )
                .await?;
            self.capture_replays = false;
            let batch = (evidence.fails(), evidence.runs());
            if self.origins.needs_confirmation(&origin) {
                // A reproducing review run is a sighting, not a
                // confirmation: it faces the standard bar on the origin's
                // remaining attempt budget (decision 72).
                let mut confirmed = false;
                let mut review_evidence = (0, 0);
                if let Some(run) = reproduction {
                    if self.origins.entry(&origin).spend_bar_attempt() {
                        let reproduced: Vec<ChoiceValue> =
                            run.nodes.iter().map(|n| n.value()).collect();
                        let review = self
                            .nd_evidence_batch(&origin, &reproduced, shrink_deadline)
                            .await?;
                        review_evidence = (review.evidence.fails(), review.evidence.runs());
                        if review.bar_accepted {
                            let pool = pooled_timelines(
                                timelines[0].clone(),
                                review.captured.into_iter().chain(timelines),
                            );
                            let reviewed = self.origins.entry(&origin);
                            reviewed.confirm(
                                review.evidence.lower_bound(),
                                None,
                                pool,
                                review_evidence,
                            )?;
                            reviewed.record_final_replay(batch);
                            confirmed = true;
                        }
                    }
                }
                if !confirmed {
                    let mut reject_evidence =
                        (batch.0 + review_evidence.0, batch.1 + review_evidence.1);
                    if self.has_history(&origin) {
                        match self.backtrack(&origin).await? {
                            Backtrack::Restored { nodes: restored } => {
                                self.origins.entry(&origin).replace(restored.clone());
                                if reshrink {
                                    let mut shrunk = crate::native::HashSet::default();
                                    self.shrink_origin(
                                        origin.clone(),
                                        restored,
                                        verbosity,
                                        output,
                                        shrink_deadline,
                                        &mut shrunk,
                                    )
                                    .await?;
                                }
                                pending.push(origin);
                                continue;
                            }
                            Backtrack::Exhausted { evidence } => {
                                reject_evidence = (
                                    reject_evidence.0 + evidence.0,
                                    reject_evidence.1 + evidence.1,
                                );
                            }
                        }
                    }
                    self.reject_origin(&origin, reject_evidence, true);
                }
            } else {
                self.origins.entry(&origin).record_final_replay(batch);
            }
        }
        Ok(())
    }

    /// One evidence batch: replay `choices` with capture-at-confirmation,
    /// each replay one plain trial of the test case (decision 71), until
    /// the discovery bar ([`nd::discovery_bar`], decision 23) decides,
    /// starting from the origin's first-check seed when one exists. Two uses: the
    /// bar's driver for admitting unconfirmed origins (experiment 005),
    /// and an evidence-gathering batch for trusted origins, where the bar
    /// arithmetic is only the stopping rule. The triggering run is
    /// selection, not evidence — only these fresh replays count. An accept
    /// requires a reproducing replay in *this* batch as its witness: a
    /// first-check seed can carry the bar's whole failure quota, and a
    /// seeded quota with no in-batch reproduction rejects at
    /// [`nd::CONFIRM_CAP`] runs instead of confirming an origin
    /// the batch never saw fail. An accept
    /// extends to [`nd::ANCHOR_SEED_RUNS`] runs (decision 54), so
    /// the anchor a caller seeds from the batch is not biased by the bar's
    /// stopping rule; a reject stops at the bar. An expired `deadline`
    /// (passed only by the final replay's review) rejects before the next
    /// replay — a batch cut short proves nothing; the accept extension
    /// runs unchecked, bounded by [`nd::ANCHOR_SEED_RUNS`].
    async fn nd_evidence_batch(
        &mut self,
        origin: &str,
        choices: &[ChoiceValue],
        deadline: Option<crate::sys::Instant>,
    ) -> Result<NdBatch, RunError> {
        let mut evidence = self.origins.entry(origin).take_seed().unwrap_or_default();
        let mut witness = None;
        let mut captured: Vec<Vec<ChoiceValue>> = Vec::new();
        let capture_entry = self.capture_replays;
        self.capture_replays = true;
        let bar_accepted = loop {
            if deadline.is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d)) {
                break false;
            }
            let replay = self.nd_replay_once(choices, Some(origin)).await?;
            evidence.record(replay.failed);
            if replay.failed {
                if captured.len() < nd::POOL_CAP && !captured.contains(&replay.realized) {
                    captured.push(replay.realized);
                }
                if witness.is_none() {
                    witness = Some(replay.run);
                }
            }
            match nd::discovery_bar(&evidence) {
                nd::BarVerdict::Accept => {
                    if witness.is_some() {
                        break true;
                    }
                    if evidence.runs() >= nd::CONFIRM_CAP {
                        break false;
                    }
                }
                nd::BarVerdict::Reject => break false,
                nd::BarVerdict::Continue => {}
            }
        };
        while bar_accepted && evidence.runs() < nd::ANCHOR_SEED_RUNS {
            let replay = self.nd_replay_once(choices, Some(origin)).await?;
            evidence.record(replay.failed);
            if replay.failed
                && captured.len() < nd::POOL_CAP
                && !captured.contains(&replay.realized)
            {
                captured.push(replay.realized);
            }
        }
        self.capture_replays = capture_entry;
        Ok(NdBatch {
            bar_accepted,
            evidence,
            witness,
            captured,
        })
    }

    /// Backtrack over `origin`'s history for the reproduction boundary —
    /// the newest entry that still reproduces (seam plan step 4, gate
    /// G25). Probes are single continuation-tolerant replays: the accept
    /// segment at geometric offsets from the newest plus its oldest entry
    /// and every raw sighting, then binary refinement between the newest
    /// reproducing probe and its nearest newer non-reproducing one, capped
    /// at [`BACKTRACK_SCAN_REPLAYS`] in total. The best candidate faces
    /// the full discovery bar, spending the origin's
    /// [`nd::BACKTRACK_BAR_ATTEMPTS`]-batch budget — held across
    /// backtracks of the same origin (decision 72); a
    /// reject resumes the scan on the older side, and with no reproducing
    /// probe the remaining replay budget goes on a second pass before
    /// giving up. A cleared bar confirms the origin — witness and anchor
    /// from the batch's extension, the scan's other reproducing entries
    /// pooled — and the restored incumbent supersedes the barred shrunk
    /// save. Scan errors bias old: a too-old restore re-shrinks under the
    /// gauntlet (decision 2), a too-new one anchors low or gets rejected.
    async fn backtrack(&mut self, origin: &str) -> Result<Backtrack, RunError> {
        let entries: Vec<(Vec<ChoiceValue>, bool)> = self
            .origins
            .get(origin)
            .map(|c| {
                c.history()
                    .entries()
                    .iter()
                    .map(|e| (e.nodes.iter().map(|n| n.value()).collect(), e.accept))
                    .collect()
            })
            .unwrap_or_default();
        let attempts_left = self
            .origins
            .get(origin)
            .is_some_and(|c| c.backtrack_attempts_left());
        if entries.is_empty() || !attempts_left {
            return Ok(Backtrack::Exhausted { evidence: (0, 0) });
        }
        let accepts: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].1).collect();
        let raws: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].1).collect();

        let mut replays_left = BACKTRACK_SCAN_REPLAYS;
        let mut fails = 0u64;
        let mut runs = 0u64;
        let mut status: Vec<Option<bool>> = entries.iter().map(|_| None).collect();

        let mut probe_order: Vec<usize> = Vec::new();
        if let Some(&newest) = accepts.last() {
            let top = accepts.len() - 1;
            let mut offset = 1usize;
            while offset <= top {
                probe_order.push(accepts[top - offset]);
                offset *= 2;
            }
            if top > 0 && !probe_order.contains(&accepts[0]) {
                probe_order.push(accepts[0]);
            }
            if accepts.len() == 1 {
                probe_order.push(newest);
            }
        }
        probe_order.extend(raws.iter().copied());

        let mut second_pass_done = false;
        for idx in probe_order {
            if replays_left == 0 {
                break;
            }
            replays_left -= 1;
            let replay = self.nd_replay_once(&entries[idx].0, Some(origin)).await?;
            runs += 1;
            fails += u64::from(replay.failed);
            status[idx] = Some(replay.failed);
        }
        loop {
            let candidate = {
                let boundary = accepts
                    .iter()
                    .rev()
                    .position(|&i| status[i] == Some(true))
                    .map(|rev_pos| accepts.len() - 1 - rev_pos);
                if let Some(pos) = boundary {
                    let mut low = pos;
                    let mut high = accepts
                        .iter()
                        .enumerate()
                        .skip(pos + 1)
                        .find(|&(_, &i)| status[i] == Some(false))
                        .map_or(accepts.len(), |(p, _)| p);
                    while high - low > 1 && replays_left > 0 {
                        let mid = accepts[low + (high - low) / 2];
                        replays_left -= 1;
                        let replay = self.nd_replay_once(&entries[mid].0, Some(origin)).await?;
                        runs += 1;
                        fails += u64::from(replay.failed);
                        status[mid] = Some(replay.failed);
                        if replay.failed {
                            low = low + (high - low) / 2;
                        } else {
                            high = low + (high - low) / 2;
                        }
                    }
                    Some(accepts[low])
                } else {
                    raws.iter()
                        .filter(|&&i| status[i] == Some(true))
                        .min_by(|&&a, &&b| {
                            shortlex(
                                &serialize_choices(&entries[a].0),
                                &serialize_choices(&entries[b].0),
                            )
                        })
                        .copied()
                }
            };
            let Some(candidate) = candidate else {
                if second_pass_done || replays_left == 0 {
                    return Ok(Backtrack::Exhausted {
                        evidence: (fails, runs),
                    });
                }
                second_pass_done = true;
                for idx in (0..entries.len()).rev() {
                    if status[idx] == Some(true) || replays_left == 0 {
                        continue;
                    }
                    replays_left -= 1;
                    let replay = self.nd_replay_once(&entries[idx].0, Some(origin)).await?;
                    runs += 1;
                    fails += u64::from(replay.failed);
                    status[idx] = Some(replay.failed);
                    if replay.failed {
                        break;
                    }
                }
                continue;
            };
            if !self.origins.entry(origin).spend_backtrack_attempt() {
                return Ok(Backtrack::Exhausted {
                    evidence: (fails, runs),
                });
            }
            let batch = self
                .nd_evidence_batch(origin, &entries[candidate].0, None)
                .await?;
            runs += batch.evidence.runs();
            fails += batch.evidence.fails();
            if !batch.bar_accepted {
                status[candidate] = Some(false);
                continue;
            }
            let witness = crate::control::hegel_internal_unwrap!(
                batch.witness,
                "backtrack: bar accept without a witness for {origin}"
            );
            let anchor = batch.evidence.lower_bound();
            let others = (0..entries.len())
                .filter(|&i| i != candidate && status[i] == Some(true))
                .map(|i| entries[i].0.clone());
            let pool = pooled_timelines(
                entries[candidate].0.clone(),
                batch.captured.into_iter().chain(others),
            );
            #[cfg(feature = "__bench")]
            let history_bytes: usize = self.origins.get(origin).map_or(0, |c| {
                c.history()
                    .entries()
                    .iter()
                    .map(|e| e.nodes.len() * core::mem::size_of::<ChoiceNode>())
                    .sum()
            });
            let nodes = self
                .origins
                .get(origin)
                .and_then(|c| c.history().entries().get(candidate))
                .map(|e| e.nodes.clone())
                .unwrap_or_default();
            self.origins.entry(origin).confirm(
                anchor,
                Some(witness),
                pool,
                (batch.evidence.fails(), batch.evidence.runs()),
            )?;
            #[cfg(feature = "__bench")]
            {
                let best = accepts.last().copied().unwrap_or(candidate);
                nd::seam_dump::record(nd::seam_dump::SeamEvent::Backtrack {
                    origin: origin.to_string(),
                    restored: entries[candidate].0.clone(),
                    history_best: entries[best].0.clone(),
                    history_bytes,
                });
            }
            let incumbent: Vec<ChoiceValue> = entries[candidate].0.clone();
            let state = self.nd_state_for(origin, incumbent);
            self.persister.supersede_nd(origin, &nodes, &state);
            return Ok(Backtrack::Restored { nodes });
        }
    }

    /// The boost phase (experiment 006) — successive halving over the
    /// incumbent, its pool, and probe mutants, scored by failure rate under
    /// budgeted replay. Returns a witness run and new anchor when the
    /// winner's holdout LCB beats the confirmation anchor (holdout because
    /// the in-race rate of a halving winner is selection-biased upward).
    /// Run before shrinking only when the anchor sits below
    /// [`nd::BOOST_RELIABILITY_FLOOR`] (gate G2).
    async fn nd_boost(
        &mut self,
        origin: &str,
        incumbent: &[ChoiceValue],
        anchor: f64,
    ) -> Result<Option<(RunResult, f64)>, RunError> {
        if self.settings.verbosity == Verbosity::Debug {
            self.settings.output.line(&format!(
                "nd boost: origin={origin} racing from anchor {anchor:.3}"
            ));
        }
        let mut candidates: Vec<Vec<ChoiceValue>> = Vec::from([incumbent.to_vec()]);
        if let Some(counterexample) = self.origins.get(origin) {
            for timeline in counterexample.pool() {
                if candidates.len() < nd::BOOST_POOL && !candidates.contains(timeline) {
                    candidates.push(timeline.clone());
                }
            }
        }
        let mut attempts = 0;
        while candidates.len() < nd::BOOST_POOL && attempts < nd::BOOST_POOL * 3 {
            attempts += 1;
            let cut = self.rng.random_range(0..=incumbent.len());
            let budget = crate::native::core::flattened_values_len(incumbent) + 8;
            let ntc = NativeTestCase::for_probe(&incumbent[..cut], self.rng.spawn(), budget)?;
            let (run, _mismatch) = self.measure(ntc).await?;
            let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
            if !candidates.contains(&realized) {
                candidates.push(realized);
            }
        }
        let mut scores: Vec<(usize, u64, u64)> = (0..candidates.len()).map(|i| (i, 0, 0)).collect();
        let mut replays_per_round: u64 = 2;
        while scores.len() > 1 {
            for (idx, fails, runs) in scores.iter_mut() {
                let candidate = &candidates[*idx];
                let budget =
                    nd::continuation_budget(crate::native::core::flattened_values_len(candidate));
                for _ in 0..replays_per_round {
                    let ntc = NativeTestCase::for_probe(candidate, self.rng.spawn(), budget)?;
                    let (run, _mismatch) = self.measure(ntc).await?;
                    *runs += 1;
                    if run.status == Status::Interesting && run.origin.as_deref() == Some(origin) {
                        *fails += 1;
                    }
                }
            }
            scores.sort_by(|a, b| {
                let rate_a = a.1 as f64 / a.2.max(1) as f64;
                let rate_b = b.1 as f64 / b.2.max(1) as f64;
                rate_b.total_cmp(&rate_a)
            });
            scores.truncate(nd::boost_keep(scores.len()));
            replays_per_round *= 2;
        }
        let winner = candidates[scores[0].0].clone();
        let mut holdout = nd::Evidence::default();
        let mut witness = None;
        for _ in 0..nd::BOOST_HOLDOUT {
            let replay = self.nd_replay_once(&winner, Some(origin)).await?;
            holdout.record(replay.failed);
            if replay.failed && witness.is_none() {
                witness = Some(replay.run);
            }
        }
        let lcb = holdout.lower_bound();
        Ok(match (witness, lcb > anchor) {
            (Some(witness), true) => {
                if self.settings.verbosity == Verbosity::Debug {
                    self.settings.output.line(&format!(
                        "nd boost: origin={origin} anchor {anchor:.3} -> {lcb:.3}"
                    ));
                }
                self.origins.entry(origin).raise_anchor(lcb);
                Some((witness, lcb))
            }
            _ => None,
        })
    }

    /// Targeting under ND handling (decision 68, experiment 013): the
    /// per-label counterpart of [`crate::native::targeting::Optimiser`],
    /// with every single-run trust point replaced by measurement. Each
    /// label's recorded best is selection-biased seed material, never a
    /// baseline: the label's reference score is the median of a fresh
    /// replay batch, raced candidates are ranked by mean observed score
    /// under successive halving, and a winner is adopted only when a fresh
    /// holdout clears [`nd::target_adopt`]'s sign test against the
    /// reference, which is then re-estimated on another fresh batch and
    /// only ever raised. Races stop at [`nd::TARGET_ND_RACES`] per firing,
    /// after a full label pass with no adoption, or as soon as any
    /// interesting origin exists — at which point the run's replay budget
    /// belongs to confirmation and shrinking.
    async fn optimise_targets_nd(&mut self) -> Result<(), RunError> {
        let seeds = self.targeting.seeds();
        let mut races = 0u64;
        loop {
            let mut adopted = false;
            for (label, best_score, best_choices) in &seeds {
                if races >= nd::TARGET_ND_RACES || self.origins.any_live() {
                    return Ok(());
                }
                if self.targeting.nd_target(label).is_none() {
                    self.nd_target_reference(label, best_choices).await?;
                }
                let (reference, nodes, timeline) = match self.targeting.nd_target(label) {
                    Some(t) if !t.dead => (t.reference, t.nodes.clone(), t.timeline()),
                    _ => continue,
                };
                races += 1;
                if self
                    .nd_target_race(
                        label,
                        reference,
                        &nodes,
                        &timeline,
                        *best_score,
                        best_choices,
                    )
                    .await?
                {
                    adopted = true;
                }
            }
            if !adopted {
                return Ok(());
            }
        }
    }

    /// Establish `label`'s ND reference from a fresh batch replaying the
    /// recorded seed: the reference score is the batch's median observed
    /// score and the node view comes from its first concluded run. A batch
    /// that observes no score marks the label dead — the body no longer
    /// reports it — unless the batch was cut short by a discovery, in
    /// which case nothing is recorded and the next firing retries.
    async fn nd_target_reference(
        &mut self,
        label: &str,
        seed: &[ChoiceValue],
    ) -> Result<(), RunError> {
        let (scores, nodes) = self
            .nd_target_scores(seed, label, nd::TARGET_ND_HOLDOUT)
            .await?;
        let target = match (nd::target_median(&scores), nodes) {
            (Some(reference), Some(nodes)) => crate::native::targeting::NdTarget {
                nodes,
                reference,
                dead: false,
            },
            _ => {
                if self.origins.any_live() {
                    return Ok(());
                }
                crate::native::targeting::NdTarget {
                    nodes: Vec::new(),
                    reference: f64::NEG_INFINITY,
                    dead: true,
                }
            }
        };
        self.targeting.set_nd_target(label.to_string(), target);
        Ok(())
    }

    /// One race of the ND targeting loop: build a pool of perturbations of
    /// the reference timeline (plus the recorded best, when its raw score
    /// still exceeds the reference and it is not the reference itself),
    /// successive-halve it on mean observed score, then put the winner to
    /// the holdout sign test. Adoption moves the label onto the winner's
    /// fresh-batch node view and raises the reference to at most that
    /// batch's median — never estimated from the runs that won the race.
    /// Returns whether an adoption happened.
    async fn nd_target_race(
        &mut self,
        label: &str,
        reference: f64,
        nodes: &[ChoiceNode],
        timeline: &[ChoiceValue],
        best_score: f64,
        best_choices: &[ChoiceValue],
    ) -> Result<bool, RunError> {
        let mut candidates: Vec<Vec<ChoiceValue>> = Vec::new();
        if best_score > reference && best_choices != timeline {
            candidates.push(best_choices.to_vec());
        }
        let mut attempts = 0;
        while candidates.len() < nd::TARGET_ND_POOL && attempts < nd::TARGET_ND_POOL * 3 {
            attempts += 1;
            let Some(candidate) = self.nd_target_perturb(nodes, timeline).await? else {
                continue;
            };
            if candidate != timeline && !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        if candidates.is_empty() {
            return Ok(false);
        }
        let mut scores: Vec<(usize, f64, u64)> =
            (0..candidates.len()).map(|i| (i, 0.0, 0)).collect();
        let mut replays_per_round: u64 = 2;
        while scores.len() > 1 {
            for (idx, sum, runs) in scores.iter_mut() {
                for _ in 0..replays_per_round {
                    let Some(run) = self.nd_target_run(&candidates[*idx]).await? else {
                        break;
                    };
                    if run.status < Status::Valid {
                        continue;
                    }
                    if let Some(&score) = run.target_observations.get(label) {
                        *sum += score;
                        *runs += 1;
                    }
                }
            }
            let mean = |entry: &(usize, f64, u64)| {
                if entry.2 == 0 {
                    f64::NEG_INFINITY
                } else {
                    entry.1 / entry.2 as f64
                }
            };
            scores.sort_by(|a, b| mean(b).total_cmp(&mean(a)));
            scores.truncate(nd::boost_keep(scores.len()));
            replays_per_round *= 2;
        }
        let (winner_idx, _, winner_runs) = scores[0];
        if winner_runs == 0 {
            return Ok(false);
        }
        let winner = candidates[winner_idx].clone();
        let (holdout_scores, _nodes) = self
            .nd_target_scores(&winner, label, nd::TARGET_ND_HOLDOUT)
            .await?;
        let beats = holdout_scores.iter().filter(|&&s| s > reference).count() as u64;
        if !nd::target_adopt(beats, nd::TARGET_ND_HOLDOUT) {
            return Ok(false);
        }
        self.nd_target_adopt(label, &winner, reference).await
    }

    /// Complete an adoption: re-estimate the reference on a fresh batch of
    /// the winner and move the label onto that batch's node view. A batch
    /// observing nothing — cut short by a discovery, or a winner whose
    /// scores stopped arriving — abandons the adoption.
    async fn nd_target_adopt(
        &mut self,
        label: &str,
        winner: &[ChoiceValue],
        reference: f64,
    ) -> Result<bool, RunError> {
        let (fresh_scores, fresh_nodes) = self
            .nd_target_scores(winner, label, nd::TARGET_ND_HOLDOUT)
            .await?;
        let (Some(median), Some(new_nodes)) = (nd::target_median(&fresh_scores), fresh_nodes)
        else {
            return Ok(false);
        };
        self.targeting.adopt_nd(label, new_nodes, median);
        if self.settings.verbosity == Verbosity::Debug {
            self.settings.output.line(&format!(
                "nd targeting: label={label:?} reference {reference:.3} -> {median:.3}"
            ));
        }
        Ok(true)
    }

    /// A fresh batch of up to `n` measured replays of `timeline`,
    /// collecting the scores observed for `label` and the first concluded
    /// run's realized nodes. Stops early when a discovery arrives.
    async fn nd_target_scores(
        &mut self,
        timeline: &[ChoiceValue],
        label: &str,
        n: u64,
    ) -> Result<(Vec<f64>, Option<Vec<ChoiceNode>>), RunError> {
        let mut scores = Vec::new();
        let mut nodes = None;
        for _ in 0..n {
            let Some(run) = self.nd_target_run(timeline).await? else {
                break;
            };
            if run.status < Status::Valid {
                continue;
            }
            if let Some(&score) = run.target_observations.get(label) {
                scores.push(score);
            }
            if nodes.is_none() {
                nodes = Some(run.nodes);
            }
        }
        Ok((scores, nodes))
    }

    /// One measured replay of `timeline` under the standard continuation
    /// budget, or `None` once an interesting origin exists — targeting
    /// yields the run's replay budget to the failure machinery.
    async fn nd_target_run(
        &mut self,
        timeline: &[ChoiceValue],
    ) -> Result<Option<RunResult>, RunError> {
        if self.origins.any_live() {
            return Ok(None);
        }
        let budget = nd::continuation_budget(crate::native::core::flattened_values_len(timeline));
        let ntc = NativeTestCase::for_probe(timeline, self.rng.spawn(), budget)?;
        let (run, _mismatch) = self.measure(ntc).await?;
        Ok(Some(run))
    }

    /// One candidate perturbation of the reference: half the time (when the
    /// node view has a steppable node) a single climbable node stepped by a
    /// random power-of-two delta in either direction, otherwise a
    /// prefix-cut probe regenerating a fresh tail — boost's mutant move,
    /// and the only lever on structure the stepper cannot reach, such as
    /// clone streams. `None` when the step fell outside the node's
    /// constraints or a discovery arrived.
    async fn nd_target_perturb(
        &mut self,
        nodes: &[ChoiceNode],
        timeline: &[ChoiceValue],
    ) -> Result<Option<Vec<ChoiceValue>>, RunError> {
        let climbable: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                !node.was_forced && crate::native::targeting::is_climbable(&node.data)
            })
            .map(|(i, _)| i)
            .collect();
        if !climbable.is_empty() && self.rng.random_range(0..2) == 0 {
            let idx = climbable[self.rng.random_range(0..climbable.len())];
            let magnitude = 1i128 << self.rng.random_range(0..7);
            let delta = if self.rng.random_range(0..2) == 0 {
                magnitude
            } else {
                -magnitude
            };
            let Some(value) = crate::native::targeting::step_choice(&nodes[idx], delta) else {
                return Ok(None);
            };
            let mut candidate = timeline.to_vec();
            candidate[idx] = value;
            return Ok(Some(candidate));
        }
        if self.origins.any_live() {
            return Ok(None);
        }
        let cut = self.rng.random_range(0..=timeline.len());
        let budget = crate::native::core::flattened_values_len(timeline) + 8;
        let ntc = NativeTestCase::for_probe(&timeline[..cut], self.rng.spawn(), budget)?;
        let (run, _mismatch) = self.measure(ntc).await?;
        Ok(Some(run.nodes.iter().map(|n| n.value()).collect()))
    }

    /// The universal first-interesting determinism check (seam plan step
    /// 2, extending decision 21's principle to every run): before anything
    /// else consumes a generation-discovered origin, its incumbent sighting
    /// at sweep time (in-batch displacement may already have replaced the
    /// discovery) replays [`FIRST_CHECK_REPLAYS`] times exactly, stopping at
    /// the first miss. A replay reproduces when it concludes interesting
    /// at the same origin with the same realized values. Any miss flips
    /// the run — under `error` strictness a structural divergence aborts
    /// with a position-naming diagnostic and an aligned outcome change
    /// aborts as flaky, matching the cache's mismatch channel — and the check's
    /// observations seed the origin's evidence, so the discovery bar
    /// starts partially filled. All-reproduce marks the origin checked.
    /// Pre-flip only (`nd_force` starts flipped and skips it); reuse
    /// reproductions are exempted at the reuse site, and origins first
    /// admitted at shrink verify or final replay keep decision 35's path.
    /// Shares [`Self::nd_discovery_sweep`]'s call sites, running first.
    async fn first_check_sweep(&mut self) -> Result<(), RunError> {
        while !self.nd_handling() {
            let Some((origin, nodes)) = self
                .origins
                .iter()
                .find(|(_, c)| c.incumbent().is_some() && !c.first_checked())
                .and_then(|(o, c)| c.incumbent().map(|n| (o.to_string(), n.to_vec())))
            else {
                return Ok(());
            };
            self.origins.entry(&origin).mark_first_checked();
            let capture_entry = self.capture_replays;
            self.capture_replays = true;
            self.check_window = true;
            let outcome = self.first_check_replays(&origin, &nodes).await;
            self.capture_replays = capture_entry;
            self.check_window = false;
            let (miss, evidence) = outcome?;
            if let Some(err) = miss {
                if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
                    return Err(err);
                }
                self.origins.entry(&origin).seed_evidence(evidence);
                #[cfg(feature = "__bench")]
                self.seam_flip(nd::seam_dump::FlipSite::FirstCheck);
                self.nd_flip();
            }
        }
        Ok(())
    }

    /// [`Self::first_check_sweep`]'s replay loop: up to
    /// [`FIRST_CHECK_REPLAYS`] exact replays of `nodes`, stopping at the
    /// first miss. Returns the miss (typed for `error` strictness) and the
    /// evidence gathered; the caller owns the capture flags.
    async fn first_check_replays(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
    ) -> Result<(Option<RunError>, nd::Evidence), RunError> {
        let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
        let mut evidence = nd::Evidence::default();
        for _ in 0..FIRST_CHECK_REPLAYS {
            let ntc = NativeTestCase::for_choices(&choices, Some(nodes), None);
            let (run, mismatch) = self.measure(ntc).await?;
            if let Some(err) = mismatch {
                return Err(err);
            }
            let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
            let failed = run.status == Status::Interesting && run.origin.as_deref() == Some(origin);
            evidence.record(failed);
            if !failed || realized != choices {
                let miss = if realized == choices {
                    RunError::Flaky(flaky_diagnostic())
                } else {
                    RunError::NonDeterministic(first_check_diagnostic(&choices, &realized))
                };
                return Ok((Some(miss), evidence));
            }
        }
        Ok((None, evidence))
    }

    /// Experiment 005: confirm every interesting origin that hasn't passed
    /// the discovery bar yet. Swept after each generation iteration (and once
    /// after the loop) rather than keyed on the iteration's own run, because
    /// span-mutation and targeting executions also fill vacant origins.
    /// Loops because confirmation replays can themselves discover origins.
    /// Each batch spends the origin's per-run bar budget (decision 72); at
    /// the cap the origin is rejected and evicted without a batch.
    async fn nd_discovery_sweep(
        &mut self,
        verbosity: Verbosity,
        output: &crate::settings::Output,
    ) -> Result<(), RunError> {
        if !self.nd_handling() {
            return Ok(());
        }
        loop {
            let Some((origin, nodes)) = self
                .origins
                .live()
                .find(|(o, _)| self.origins.needs_confirmation(o))
                .map(|(o, n)| (o.to_string(), n.to_vec()))
            else {
                return Ok(());
            };
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            if !self.origins.entry(&origin).spend_bar_attempt() {
                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "nd discovery confirm: origin={origin} out of bar attempts"
                    ));
                }
                self.reject_origin(&origin, (0, 0), false);
                continue;
            }
            let batch = self.nd_evidence_batch(&origin, &choices, None).await?;
            if verbosity == Verbosity::Debug {
                output.line(&format!(
                    "nd discovery confirm: origin={origin} fails={}/{} accepted={}",
                    batch.evidence.fails(),
                    batch.evidence.runs(),
                    batch.bar_accepted
                ));
            }
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if batch.bar_accepted {
                let pool = pooled_timelines(choices, batch.captured);
                self.origins.entry(&origin).confirm(
                    batch.evidence.lower_bound(),
                    batch.witness,
                    pool,
                    evidence,
                )?;
                self.record_nd_incumbent(&origin, &nodes);
            } else {
                self.reject_origin(&origin, evidence, false);
            }
        }
    }

    fn db(&self) -> Option<&dyn TestCaseDatabase> {
        self.persister.db.as_deref()
    }

    /// The replay state persisted and emitted for `origin` with `incumbent`
    /// in front of its captured pool
    /// ([`Counterexample::repro_state`]); an origin the run never recorded
    /// has an empty pool.
    fn nd_state_for(
        &self,
        origin: &str,
        incumbent: Vec<ChoiceValue>,
    ) -> crate::native::blob::NdReproState {
        match self.origins.get(origin) {
            Some(counterexample) => counterexample.repro_state(incumbent),
            None => Counterexample::default().repro_state(incumbent),
        }
    }

    /// Whether `origin` has pre-flip history for a backtrack to scan.
    fn has_history(&self, origin: &str) -> bool {
        self.origins
            .get(origin)
            .is_some_and(|c| !c.history().is_empty())
    }

    /// The discovery bar rejected `origin` with `evidence`: record it and
    /// evict the incumbent unless the origin is trusted or confirmed
    /// ([`Counterexample::reject`]), logging an eviction for the seam dump.
    #[cfg_attr(not(feature = "__bench"), allow(unused_variables))]
    fn reject_origin(&mut self, origin: &str, evidence: (u64, u64), at_final_replay: bool) {
        let evicted = self.origins.entry(origin).reject(evidence);
        #[cfg(feature = "__bench")]
        if let Some(nodes) = evicted {
            nd::seam_dump::record(nd::seam_dump::SeamEvent::Evict {
                origin: origin.to_string(),
                values: nodes.iter().map(|n| n.value()).collect(),
                at_final_replay,
            });
        }
    }

    /// Persist `origin`'s new incumbent as a version-2 entry carrying its
    /// timeline pool — the validated-persistence points under ND handling
    /// (confirmation and gauntlet accepts).
    fn record_nd_incumbent(&mut self, origin: &str, nodes: &[ChoiceNode]) {
        let incumbent: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
        let state = self.nd_state_for(origin, incumbent);
        self.persister.record_nd(origin, nodes, &state);
    }

    /// Spawn an independent RNG from the engine's, for components (probes,
    /// replays) that need their own stream without perturbing the engine's
    /// trajectory.
    pub(crate) fn rng_spawn(&mut self) -> EngineRng {
        self.rng.spawn()
    }

    /// Execute one test case and record everything about its outcome —
    /// Hypothesis's `ConjectureRunner.test_function`. Returns the run plus
    /// the nondeterminism abort, if recording the run contradicted an
    /// earlier execution under `error` strictness (kind drift or a verdict
    /// change — see [`Self::record_execution`]). `Err` means the driver
    /// violated the run contract (see [`NativeDataSource::take_outcome`]).
    pub(crate) async fn test_function(
        &mut self,
        ntc: NativeTestCase,
    ) -> Result<(RunResult, Option<RunError>), RunError> {
        self.test_function_tagged(ntc, false).await
    }

    /// [`Self::test_function`] for measurement runs — executions the ND
    /// machinery makes to measure reproduction (confirmation batches,
    /// gauntlet reruns, boost, replay-until-failure). They detect
    /// nondeterminism and admit interesting origins like any run, but move
    /// none of the runner's quantitative state — case counters, the invalid
    /// budget, health checks, event statistics, targeting, bug-window
    /// markers — which describes generation, not measurement.
    async fn measure(
        &mut self,
        ntc: NativeTestCase,
    ) -> Result<(RunResult, Option<RunError>), RunError> {
        self.test_function_tagged(ntc, true).await
    }

    async fn test_function_tagged(
        &mut self,
        mut ntc: NativeTestCase,
        measurement: bool,
    ) -> Result<(RunResult, Option<RunError>), RunError> {
        if self.capture_replays || (self.nd_active && self.capture_discoveries && !measurement) {
            ntc.set_should_capture();
        }
        let family = alloc::sync::Arc::clone(ntc.family());
        family.set_stateful_step_count(self.settings.stateful_step_count);
        let tc_start = crate::sys::Instant::now();
        let run = self.execute(ntc).await?;
        let elapsed = tc_start.map_or(core::time::Duration::ZERO, |start| start.elapsed());
        let mut mismatch = self.record_run(&run, elapsed, measurement);
        if mismatch.is_some()
            && self.settings.nondeterminism_strictness != NondeterminismStrictness::Error
        {
            #[cfg(feature = "__bench")]
            self.seam_flip(self.flip_site_hint.unwrap_or(if self.check_window {
                nd::seam_dump::FlipSite::FirstCheck
            } else {
                nd::seam_dump::FlipSite::CacheMismatch
            }));
            self.nd_flip();
            mismatch = None;
        }
        Ok((run, mismatch))
    }

    /// Record one executed test case: the execution cache and kind ledger
    /// (via [`Self::record_execution`]), counters, test time, triviality,
    /// the targeting observations (generation runs only; under `nd_active`
    /// they are selection-biased seed material for the measured race,
    /// decision 68), the per-origin interesting
    /// map (with its incremental database save and history entry), and the
    /// bug-window markers. Pre-flip, a measurement run leaves the
    /// interesting map, the database, and history untouched — check and
    /// scan replays must not displace or persist — except under
    /// [`Self::reuse_replays`].
    fn record_run(
        &mut self,
        run: &RunResult,
        elapsed: core::time::Duration,
        measurement: bool,
    ) -> Option<RunError> {
        let mismatch = if self.nd_active {
            None
        } else {
            self.record_execution(run, measurement)
        };
        if measurement {
            if self.nd_active || self.check_window {
                self.statistics
                    .record_measurement(run.status == Status::Interesting);
            }
        } else {
            self.calls += 1;
            self.total_test_time += elapsed;
            if run.nodes.is_empty() && run.status >= Status::Invalid {
                self.test_is_trivial = true;
            }
            if run.status >= Status::Valid && !run.target_observations.is_empty() {
                let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
                self.targeting.record(&choices, &run.target_observations);
            }
            if self.collect_statistics && matches!(run.status, Status::Valid | Status::Interesting)
            {
                self.statistics.record_case(&run.events);
            }
            match run.status {
                Status::Valid => self.valid_test_cases += 1,
                Status::Invalid => self.invalid_test_cases += 1,
                Status::EarlyStop => self.overrun_test_cases += 1,
                Status::Interesting => {
                    if self.first_bug_at.is_none() {
                        self.first_bug_at = Some(self.calls);
                        self.first_bug_time = crate::sys::Instant::now();
                    }
                    self.last_bug_at = Some(self.calls);
                }
            }
        }
        if run.status == Status::Interesting {
            let origin = run.origin.clone().unwrap_or_default();
            if !self.nd_active {
                if !measurement || self.reuse_replays {
                    self.persister.record(&origin, &run.nodes);
                    let counterexample = self.origins.entry(&origin);
                    let accept = counterexample.adopt(run.nodes.clone());
                    counterexample.record_sighting(&run.nodes, accept);
                }
            } else if self.origins.incumbent(&origin).is_none() {
                self.origins.entry(&origin).adopt(run.nodes.clone());
            }
        }
        mismatch
    }

    /// Feed one executed run to the detectors the tree used to be: the kind
    /// ledger (`error` strictness only), then — for conclusions; an overrun
    /// concluded nothing — the execution cache, whose digest hit both drives
    /// the duplicate-stop counter (generation-window, non-measurement cases
    /// only) and, on a verdict change, reports the flake the tree could
    /// never see. The returned error is `NonDeterministic` for kind drift
    /// and `Flaky` for a verdict change (decision 30's split); the caller
    /// keeps it under `error` strictness and flips otherwise.
    fn record_execution(&mut self, run: &RunResult, measurement: bool) -> Option<RunError> {
        if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
            if let Some(msg) = self.kind_ledger.observe(&run.nodes) {
                return Some(RunError::NonDeterministic(msg));
            }
        }
        if run.status == Status::EarlyStop {
            return None;
        }
        let recorded = self.exec_cache.record(
            serialize_nodes(&run.nodes),
            run.status,
            run.origin.as_deref(),
            &run.nodes,
            &run.spans,
            !self.collect_statistics,
        );
        if self.collect_statistics && !measurement {
            if recorded.duplicate {
                self.consecutive_duplicates += 1;
            } else {
                self.consecutive_duplicates = 0;
            }
        }
        recorded
            .verdict_mismatch
            .then(|| RunError::Flaky(flaky_diagnostic()))
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

    /// Execute one test case by offering it through the exchange, returning
    /// a [`RunResult`] populated from the outcome
    /// reported by the data source's `mark_complete` plus the
    /// [`NativeTestCase`]'s realized choice nodes. Always a non-final
    /// execution. `Err` means the driver violated the run contract by
    /// resuming the engine without concluding the offered case (see
    /// [`NativeDataSource::take_outcome`]).
    async fn execute(&mut self, ntc: NativeTestCase) -> Result<RunResult, RunError> {
        let (data_source, handle) = NativeDataSource::new(ntc);
        self.exchange.offer(Box::new(data_source)).await;
        let nodes = NativeDataSource::take_nodes(&handle);
        let spans = NativeDataSource::take_spans(&handle);
        let target_observations = NativeDataSource::take_target_observations(&handle);
        let events = NativeDataSource::take_events(&handle);
        let tc_result = NativeDataSource::take_outcome(&handle)?;

        let (status, origin) = match tc_result {
            TestCaseResult::Valid => (Status::Valid, None),
            TestCaseResult::Invalid => (Status::Invalid, None),
            TestCaseResult::Overrun => (Status::EarlyStop, None),
            TestCaseResult::Interesting(f) => (Status::Interesting, Some(f.origin)),
        };

        Ok(RunResult {
            status,
            nodes,
            spans,
            origin,
            target_observations,
            events,
        })
    }

    /// The single replay chokepoint — Hypothesis's `cached_test_function` —
    /// shared by generation-phase span mutation and shrinking. Replays
    /// `choices` (drawing up to `extend` further choices beyond them) and
    /// returns the realised [`RunResult`]. Any predicate (e.g. the
    /// interesting-origin filter) is applied by the caller, so replay and
    /// matching are not entangled.
    ///
    /// An exact repeat of an executed conclusion — `choices` equal to some
    /// earlier run's realized values — is served from the [`ExecCache`] with
    /// its full outcome (status, origin, nodes, spans) without running the
    /// body, for *any* status (interesting included) and any `extend`: the
    /// cached conclusion consumed exactly those choices, so the continuation
    /// budget is irrelevant to it. Anything else executes through
    /// [`Self::test_function`] — bare when `extend == 0`, with up to
    /// `extend` random draws past the end of `choices` otherwise — and its
    /// conclusion enters the cache so a later repeat is served. The tree's
    /// predictions beyond exact repeats (trailing-unread proposals,
    /// truncated-proposal overruns, pun resolution) are gone by measurement:
    /// serves were ≈ exact repeats (`notes/experiments/010-tree-value`).
    /// Under nondeterministic handling nothing is served: identical choices
    /// need not produce identical outcomes, so every replay executes the
    /// body (`notes/experiments/002-cache-seam`).
    pub(crate) async fn cached_test_function(
        &mut self,
        choices: &[ChoiceValue],
        nodes: Option<&[ChoiceNode]>,
        extend: usize,
    ) -> Result<RunResult, RunError> {
        if !self.nd_active {
            if let Some(hit) = self.exec_cache.serve(&serialize_choices(choices)) {
                return Ok(RunResult {
                    status: hit.status,
                    nodes: hit.nodes,
                    spans: hit.spans,
                    origin: hit.origin,
                    target_observations: HashMap::default(),
                    events: Vec::new(),
                });
            }
        }
        let ntc = if extend == 0 {
            NativeTestCase::for_choices(choices, nodes, None)
        } else {
            let budget = crate::native::core::flattened_values_len(choices) + extend;
            NativeTestCase::for_probe(choices, self.rng_spawn(), budget)?
        };
        let (run, mismatch) = self.test_function(ntc).await?;
        if let Some(err) = mismatch {
            return Err(err);
        }
        Ok(run)
    }
}

/// The engine side of the shrinker's [`ShrinkProbe`]: routes every requested
/// run through [`Engine::cached_test_function`] and reports whether the run
/// reproduced the origin being shrunk. Borrows the engine for the duration of
/// the shrink, so the shrinker's executions record into the engine's tree and
/// counters like any other run.
struct EngineShrinkProbe<'e, 'a> {
    engine: &'e mut Engine<'a>,
    target_origin: String,
    verbosity: Verbosity,
    output: Output,
    /// Under nondeterministic handling a matching first run is not an
    /// accept — the candidate keeps executing until its cumulative ledger
    /// evidence clears the anchor-derived threshold or is proven below it
    /// ([`nd::gauntlet`]). The ledger persists for the whole shrink of one
    /// origin, so pass repetitions add power to retried rejects instead of
    /// starting over — and a fast-sweep miss cannot reject a realized
    /// timeline whose ledger already holds a conclusive accept (a nested
    /// clone shrink's final splice re-proposes exactly such timelines).
    gauntlet: bool,
    /// Per-candidate gauntlet state, keyed by serialized realized choices
    /// — a candidate whose replay punned into another realization merges
    /// evidence with it, deliberately: the realized run is the test case
    /// an accept would adopt, whatever proposal produced it, and its
    /// replays are plain trials of that test case however they realize
    /// (decision 71). Every proposal on an unbound ledger is charged
    /// against the origin's alpha budget before it runs (decision 72).
    ledger: HashMap<Vec<u8>, CandidateLedger>,
    anchor: f64,
    /// Under [`SweepMode::Confirm`] a non-matching first run is not a
    /// reject — the ledger is driven to a bound verdict either way
    /// (decision 18); only gauntleted probes distinguish the modes.
    sweep: SweepMode,
    /// Timelines whose first accept already raised the anchor. Later
    /// accepts of the same timeline draw on ever-growing replay evidence,
    /// and replay-sourced evidence must not keep raising the anchor or the
    /// incumbent prices fresh candidates out (decision 19).
    raised: crate::native::HashSet<Vec<u8>>,
    /// The most recent gauntlet accept, held until the shrinker either
    /// adopts it ([`ShrinkProbe::candidate_adopted`], the point where the
    /// anchor raise and incumbent persistence happen) or runs the next
    /// candidate. A gauntlet accept the shrinker discards — a punned
    /// realization, or a sort-key-larger mutation probe — must move
    /// nothing: the anchor bounds the *incumbent's* rate, and a
    /// never-adopted candidate never becomes the incumbent.
    pending_accept: Option<PendingAccept>,
}

/// See [`EngineShrinkProbe::pending_accept`].
struct PendingAccept {
    key: Vec<u8>,
    lower_bound: f64,
    nodes: Vec<ChoiceNode>,
}

/// One realized timeline's gauntlet state within the shrink of one origin.
struct CandidateLedger {
    evidence: nd::Evidence,
    /// Failure minimum pinned by the candidate's first charge (decision
    /// 72): the stopping rule never changes mid-test, so budget escalation
    /// only positions candidates not yet proposed.
    min_fails: u64,
    /// The evidence loop's verdict, latched — a bound is final. A latched
    /// reject spends no further budget or replays; a latched accept keeps
    /// re-proposals of a conclusively accepted timeline acceptable however
    /// their recruiting run went (a nested clone shrink's final splice
    /// re-proposes exactly such timelines).
    verdict: Option<bool>,
}

impl EngineShrinkProbe<'_, '_> {
    fn matches(&self, run: &RunResult) -> bool {
        run.status == Status::Interesting
            && run.origin.as_deref() == Some(self.target_origin.as_str())
    }
}

impl ShrinkProbe for EngineShrinkProbe<'_, '_> {
    fn set_sweep_mode(&mut self, mode: SweepMode) -> Option<SweepMode> {
        let previous = self.sweep;
        self.sweep = mode;
        self.gauntlet.then_some(previous)
    }

    fn candidate_adopted(&mut self) {
        let Some(accept) = self.pending_accept.take() else {
            return;
        };
        let first_accept = self.raised.insert(accept.key);
        if first_accept && accept.lower_bound > self.anchor {
            self.anchor = accept.lower_bound;
            self.engine
                .origins
                .entry(&self.target_origin)
                .raise_anchor(accept.lower_bound);
        }
        self.engine
            .record_nd_incumbent(&self.target_origin, &accept.nodes);
    }

    fn run<'s>(&'s mut self, req: ShrinkRun<'s>) -> crate::native::shrinker::ProbeFuture<'s> {
        Box::pin(async move {
            self.pending_accept = None;
            if self.verbosity == Verbosity::Verbose {
                self.output.line("Running test case");
            }
            let run = match req {
                ShrinkRun::Full(nodes) => {
                    let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
                    self.engine
                        .cached_test_function(&choices, Some(nodes), 0)
                        .await?
                }
                ShrinkRun::Probe { prefix, max_size } => {
                    self.engine
                        .cached_test_function(prefix, None, max_size.saturating_sub(prefix.len()))
                        .await?
                }
            };
            let matched = self.matches(&run);
            if !self.gauntlet {
                return Ok((matched, run.nodes, Spans::from(run.spans)));
            }
            let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
            let key = serialize_choices(&realized);
            if self.ledger.get(&key).is_none_or(|l| l.verdict.is_none()) {
                let (seed, pinned) = self
                    .ledger
                    .get(&key)
                    .map_or((nd::Evidence::default(), None), |l| {
                        (l.evidence, Some(l.min_fails))
                    });
                let min_fails = self
                    .engine
                    .origins
                    .entry(&self.target_origin)
                    .gauntlet_spend
                    .charge(&seed, self.anchor, self.sweep == SweepMode::Confirm, pinned);
                self.ledger.entry(key.clone()).or_insert(CandidateLedger {
                    evidence: nd::Evidence::default(),
                    min_fails,
                    verdict: None,
                });
            }
            let entry = self.ledger.get_mut(&key).unwrap();
            entry.evidence.record(matched);
            let min_fails = entry.min_fails;
            if let Some(accepted) = entry.verdict {
                if !accepted {
                    return Ok((false, run.nodes, Spans::from(run.spans)));
                }
                self.pending_accept = Some(PendingAccept {
                    key,
                    lower_bound: entry.evidence.lower_bound(),
                    nodes: run.nodes.clone(),
                });
                return Ok((true, run.nodes, Spans::from(run.spans)));
            }
            if !matched && self.sweep == SweepMode::Fast {
                return Ok((false, run.nodes, Spans::from(run.spans)));
            }
            let mut accepted = false;
            loop {
                let evidence = self.ledger.get(&key).unwrap().evidence;
                if !accepted {
                    match nd::gauntlet(&evidence, self.anchor, min_fails) {
                        nd::GauntletVerdict::Accept => accepted = true,
                        nd::GauntletVerdict::Reject => {
                            self.ledger.get_mut(&key).unwrap().verdict = Some(false);
                            return Ok((false, run.nodes, Spans::from(run.spans)));
                        }
                        nd::GauntletVerdict::Continue => {}
                    }
                }
                // The anchor must move on a bound the stopping rule didn't
                // bias (decision 54).
                if accepted && evidence.runs() >= nd::ANCHOR_SEED_RUNS {
                    self.ledger.get_mut(&key).unwrap().verdict = Some(true);
                    self.pending_accept = Some(PendingAccept {
                        key,
                        lower_bound: evidence.lower_bound(),
                        nodes: run.nodes.clone(),
                    });
                    return Ok((true, run.nodes, Spans::from(run.spans)));
                }
                let rerun = self
                    .engine
                    .nd_replay_once(&realized, Some(self.target_origin.as_str()))
                    .await?;
                self.ledger
                    .get_mut(&key)
                    .unwrap()
                    .evidence
                    .record(rerun.failed);
            }
        })
    }
}

/// Try span mutation: find two spans with the same label and either duplicate
/// the parent's prefix (when one contains the other, e.g. recursive tree
/// structures) or replace both with identical choices from one donor.
/// Anything interesting it finds lands in the engine's `interesting` map;
/// the probe loop stops at the first such find.
///
/// Makes up to [`SPAN_MUTATION_ATTEMPTS`] probes through
/// [`Engine::cached_test_function`], so a proposal repeating an executed
/// conclusion exactly costs no test-body execution — matching Hypothesis,
/// which routes mutations through `cached_test_function`. Each probe that
/// *does* execute is recorded through [`Self::record_run`], so it counts
/// toward the same budgets as a freshly generated example and a later exact
/// repeat is served; served probes are not re-recorded, exactly as
/// Hypothesis's cache hits cost nothing.
///
/// A mutated sequence often diverges from the path its donor took and would
/// run out of data as a bare replay. Rather than discarding such a proposal
/// (Hypothesis's behavior), every probe allows random draws past the end of
/// the spliced choices, so a diverged attempt becomes a complete test case
/// seeded with the mutation instead of an overrun.
impl<'a> Engine<'a> {
    async fn try_span_mutation(
        &mut self,
        nodes: &[ChoiceNode],
        spans: &[Span],
    ) -> Result<(), RunError> {
        let mut by_label: crate::native::HashMap<&str, crate::native::HashSet<(usize, usize)>> =
            crate::native::HashMap::default();
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
            return Ok(());
        }

        let values: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();

        for _ in 0..SPAN_MUTATION_ATTEMPTS {
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
                core::mem::swap(&mut start_a, &mut start_b);
                core::mem::swap(&mut end_a, &mut end_b);
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

            let extend =
                BUFFER_SIZE.saturating_sub(crate::native::core::flattened_values_len(&attempt));
            let run = self.cached_test_function(&attempt, None, extend).await?;
            if run.status == Status::Interesting {
                return Ok(());
            }
        }
        Ok(())
    }
}

fn create_rng(settings: &Settings, database_key: Option<&str>) -> Result<EngineRng, RunError> {
    if settings.resolved_backend(crate::antithesis_detect::is_running_in_antithesis()?)
        == Backend::Urandom
    {
        return Ok(EngineRng::urandom());
    }
    if let Some(seed) = settings.seed {
        Ok(EngineRng::seeded(seed))
    } else if settings.derandomize {
        let key = database_key.unwrap_or("unnamed-test");
        Ok(EngineRng::seeded(crate::native::database::fnv1a(
            key.as_bytes(),
        )))
    } else {
        Ok(EngineRng::from_os())
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/test_runner_tests.rs"]
mod tests;
