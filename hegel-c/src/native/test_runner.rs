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
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;

use rand::RngExt;

use crate::backend::{Failure, RunError, TestCaseResult, TestRunResult};
use crate::control::InternalError;
use crate::exchange::CaseExchange;
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, Divergence, MAX_SHRINKING_SECONDS, NativeTestCase, Span,
    Spans, Status, sort_key,
};
use crate::native::counterexample::{
    Counterexample, Counterexamples, pooled_timelines, set_order, timeline_order,
};
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
use crate::sys::sync::Mutex;

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
    /// Where the replay first left its stored timelines (decision 74):
    /// `None` for a run every draw of which a stored timeline served — a
    /// run that stayed on its counterexample — and for fresh generation
    /// and cache hits.
    pub divergence: Option<Divergence>,
    /// Which of the replayed counterexample's timelines the whole run
    /// stayed on, in counterexample order (`[true]` for a proposal replay;
    /// empty for fresh generation and cache hits).
    pub live: Vec<bool>,
    /// Which of the replayed counterexample's timelines the run realized
    /// (decision 77): live at its end, or left the live set at the
    /// divergence by their own misfit, never while another timeline served.
    /// Empty where `live` is.
    pub realized: Vec<bool>,
    /// Whether a replayed proposal ran out of values and the tail was drawn
    /// at random (decision 77): too short for the test rather than wrong.
    pub ran_out: bool,
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
    /// Whether the run stayed live on the replayed set's first timeline
    /// (decision 75): the one measurement that is evidence about that
    /// timeline rather than about the set.
    on_timeline: bool,
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

/// One multiverse census (decision 75): which timelines served a failing
/// replay, and one such replay per timeline as its witness.
struct Census {
    served: Vec<bool>,
    witnesses: Vec<Option<RunResult>>,
}

/// The verdict of one structural shrink candidate — a whole counterexample
/// — under the gauntlet (decision 75).
struct SetVerdict {
    accepted: bool,
    /// A failing run that stayed live on the candidate's first timeline,
    /// whose nodes can serve as the new incumbent.
    witness: Option<RunResult>,
}

/// Replays in one multiverse census (decision 75): enough that a branch the
/// failure takes one time in five is seen with probability above 0.9998,
/// so a timeline the census never saw serve describes a branch too rare to
/// cost the counterexample anything measurable when dropped.
const CENSUS_RUNS: u64 = 40;

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
                    nd::reuse_replay_budget(),
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
                                nd::reuse_replay_budget(),
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
                        if i < primary_count {
                            found_interesting_in_primary = true;
                            let realized: Vec<ChoiceValue> =
                                run.nodes.iter().map(|n| n.value()).collect();
                            if !stored.contains(&realized) {
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
            && max_test_cases > 1
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
                settings.health_check_suppressed(HealthCheck::LargeInitialTestCase),
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
                        && !settings.health_check_suppressed(HealthCheck::FilterTooMuch)
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
                        settings.health_check_suppressed(HealthCheck::TestCasesTooLarge),
                    ) {
                        return Err(RunError::HealthCheck(msg));
                    }

                    if let Some(msg) = too_slow_check(
                        self.valid_test_cases,
                        self.total_test_time,
                        too_slow_threshold,
                        settings.health_check_suppressed(HealthCheck::TooSlow),
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

        if self.test_is_trivial
            && self.valid_test_cases == 0
            && !self.origins.any_live()
            && self.invalid_test_cases > 0
        {
            return Err(RunError::Unsatisfiable(
                "Unsatisfiable: unable to satisfy the test's assumptions. The \
             test draws no data, and assume() rejected its only possible input."
                    .to_string(),
            ));
        }

        if self.consecutive_duplicates >= DUPLICATE_STOP
            && self.valid_test_cases == 0
            && !self.origins.any_live()
            && !self.test_is_trivial
            && !settings.health_check_suppressed(HealthCheck::FilterTooMuch)
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
                        .map(|(_, nodes)| serialize_executed_nodes(nodes))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
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

        self.reconcile_database()?;

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

        Ok(self.build_report()?)
    }

    /// End-of-run database reconciliation: save every surviving failure's
    /// bytes — the version-2 replay state for confirmed and trusted origins
    /// under nondeterministic handling, the incumbent's choices otherwise —
    /// then dispatch each displaced primary entry by provenance (same-run
    /// leftovers are deleted, run-start entries demote to the secondary
    /// key) and evict the shortlex-largest secondary entries above
    /// [`SECONDARY_CORPUS_CAP`].
    fn reconcile_database(&self) -> Result<(), InternalError> {
        if let (Some(db), Some(key)) = (self.db(), self.database_key) {
            let key_bytes = key.as_bytes();
            let secondary_key = crate::native::database::sub_key(key_bytes, b"secondary");
            let mut new_entries: crate::native::HashSet<Vec<u8>> =
                crate::native::HashSet::default();
            if self.nd_handling() {
                for (_, counterexample) in self.origins.iter() {
                    if counterexample.needs_confirmation() {
                        continue;
                    }
                    if let Some(values) = counterexample.incumbent_values() {
                        let state = counterexample.repro_state(values)?;
                        new_entries.insert(encode_nd_state_checked(&state)?);
                    }
                }
            } else {
                for (_, nodes) in self.origins.live() {
                    new_entries.insert(serialize_executed_nodes(nodes)?);
                }
            }
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
        Ok(())
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
    fn build_report(&mut self) -> Result<TestRunResult, InternalError> {
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
                let state = self.nd_state_for(&origin, choices)?;
                let blob = crate::control::hegel_internal_unwrap!(
                    crate::native::blob::encode_nd_failure(&state),
                    "a failing test case's clone values nest deeper than MAX_CLONE_DEPTH"
                );
                (Some(blob), self.origins.caveat(&origin))
            } else {
                let blob = crate::control::hegel_internal_unwrap!(
                    crate::native::blob::encode_failure(&choices),
                    "a failing test case's clone values nest deeper than MAX_CLONE_DEPTH"
                );
                (Some(blob), None)
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
        Ok(TestRunResult { failures })
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

/// [`flaky_diagnostic`] naming the failure whose replay disagreed, for the
/// `error`-strictness aborts that know it: the first-interesting check,
/// the shrink verify, and the final replay.
pub(crate) fn flaky_diagnostic_for(origin: &str) -> String {
    format!(
        "{}\nThe failure that did not reproduce was: {origin}",
        flaky_diagnostic()
    )
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

/// The database bytes for an executed test case's nodes. The engine bounds
/// clone nesting at `MAX_CLONE_DEPTH` as the case runs, so the serializer
/// refusing its values is a violated internal invariant.
fn serialize_executed_nodes(nodes: &[ChoiceNode]) -> Result<Vec<u8>, InternalError> {
    Ok(crate::control::hegel_internal_unwrap!(
        serialize_nodes(nodes),
        "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
    ))
}

/// [`serialize_executed_nodes`] for a realized timeline's values.
fn serialize_executed_choices(choices: &[ChoiceValue]) -> Result<Vec<u8>, InternalError> {
    Ok(crate::control::hegel_internal_unwrap!(
        serialize_choices(choices),
        "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
    ))
}

/// The version-2 entry bytes for replay state built from executed
/// timelines, under the same invariant as [`serialize_executed_nodes`].
fn encode_nd_state_checked(
    state: &crate::native::blob::NdReproState,
) -> Result<Vec<u8>, InternalError> {
    Ok(crate::control::hegel_internal_unwrap!(
        crate::native::blob::encode_nd_state(state),
        "a stored timeline's clone values nest deeper than MAX_CLONE_DEPTH"
    ))
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
    fn record(&mut self, origin: &str, nodes: &[ChoiceNode]) -> Result<(), InternalError> {
        let new_bytes = serialize_executed_nodes(nodes)?;
        self.record_bytes(origin, nodes, new_bytes);
        Ok(())
    }

    /// [`Self::record`] for a nondeterministic origin: the entry is the
    /// version-2 format carrying the incumbent's replay state
    /// ([`crate::native::blob::encode_nd_state`]).
    fn record_nd(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
        state: &crate::native::blob::NdReproState,
    ) -> Result<(), InternalError> {
        let new_bytes = encode_nd_state_checked(state)?;
        self.record_bytes(origin, nodes, new_bytes);
        Ok(())
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
    ) -> Result<(), InternalError> {
        let new_bytes = encode_nd_state_checked(state)?;
        self.record_bytes_forced(origin, nodes, new_bytes, true);
        Ok(())
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
            && !self.settings.in_antithesis
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
        self.nd_replay_set(core::slice::from_ref(&timeline.to_vec()), origin)
            .await
    }

    /// One measurement replay of a whole counterexample — `timelines` in
    /// order, as one test case under the live-set semantics (decision 74)
    /// — with the standard continuation budget for its longest timeline.
    async fn nd_replay_set(
        &mut self,
        timelines: &[Vec<ChoiceValue>],
        origin: Option<&str>,
    ) -> Result<NdReplayOnce, RunError> {
        let budget = nd::continuation_budget(
            timelines
                .iter()
                .map(|t| crate::native::core::flattened_values_len(t))
                .max()
                .unwrap_or(0),
        );
        let ntc = NativeTestCase::for_counterexample(timelines, self.rng.spawn(), budget)?;
        let (run, mismatch) = self.measure(ntc).await?;
        if let Some(divergence) = &run.divergence {
            if self.settings.verbosity == Verbosity::Debug {
                self.settings.output.line(&format!(
                    "replay left its counterexample at position {} of stream {:?} (set of {} timelines)",
                    divergence.position,
                    divergence.stream,
                    timelines.len()
                ));
            }
        }
        if let Some(err) = mismatch {
            return Err(err);
        }
        let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
        let failed = run.status == Status::Interesting
            && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
        let on_timeline = run.live.first().copied().unwrap_or(false);
        Ok(NdReplayOnce {
            run,
            realized,
            failed,
            on_timeline,
        })
    }

    /// The gauntlet over one structural shrink candidate — `set`, a whole
    /// counterexample — driven to a bound (decision 75): every replay is a
    /// trial of the set, so all of them are evidence, charged against the
    /// origin's alpha budget like any proposal. `needs_witness` asks for a
    /// failing run that stayed live on the set's first timeline, the nodes
    /// a changed incumbent is installed from; without one such an accept
    /// is refused.
    async fn nd_evaluate_set(
        &mut self,
        origin: &str,
        set: &[Vec<ChoiceValue>],
        anchor: f64,
        needs_witness: bool,
    ) -> Result<SetVerdict, RunError> {
        let min_fails = self.origins.entry(origin).gauntlet_spend.charge(
            &nd::Evidence::default(),
            anchor,
            true,
            None,
        );
        let mut evidence = nd::Evidence::default();
        let mut witness = None;
        loop {
            match nd::gauntlet(&evidence, anchor, min_fails) {
                nd::GauntletVerdict::Accept => {
                    if evidence.runs() >= nd::ANCHOR_SEED_RUNS {
                        return Ok(SetVerdict {
                            accepted: !needs_witness || witness.is_some(),
                            witness,
                        });
                    }
                }
                nd::GauntletVerdict::Reject => {
                    return Ok(SetVerdict {
                        accepted: false,
                        witness,
                    });
                }
                nd::GauntletVerdict::Continue => {}
            }
            let replay = self.nd_replay_set(set, Some(origin)).await?;
            evidence.record(replay.failed);
            if replay.failed && replay.on_timeline && witness.is_none() {
                witness = Some(replay.run);
            }
        }
    }

    /// Measure the counterexample `set` as one test case: [`nd::ANCHOR_SEED_RUNS`]
    /// replays, every one a trial of the set (decision 75). The confirmation
    /// batch of a discovery-time origin measured its first timeline alone —
    /// the pool did not exist yet — so the shrink's anchor starts from this
    /// measurement when a pool has been captured since.
    async fn nd_measure_set(
        &mut self,
        origin: &str,
        set: &[Vec<ChoiceValue>],
    ) -> Result<nd::Evidence, RunError> {
        let mut evidence = nd::Evidence::default();
        for _ in 0..nd::ANCHOR_SEED_RUNS {
            let replay = self.nd_replay_set(set, Some(origin)).await?;
            evidence.record(replay.failed);
        }
        Ok(evidence)
    }

    /// One census of `set` (decision 75): [`CENSUS_RUNS`] replays of the
    /// whole counterexample, recording which timeline each failing run
    /// followed — the first one still live at its end — and one such run
    /// per timeline as its witness. A timeline that never served a failing
    /// run describes no branch the failure takes at a rate the census
    /// could see, and deleting it changes nothing about how the
    /// counterexample reproduces.
    async fn nd_census(
        &mut self,
        origin: &str,
        set: &[Vec<ChoiceValue>],
    ) -> Result<Census, RunError> {
        let mut served = alloc::vec![false; set.len()];
        let mut witnesses: Vec<Option<RunResult>> = (0..set.len()).map(|_| None).collect();
        for _ in 0..CENSUS_RUNS {
            let replay = self.nd_replay_set(set, Some(origin)).await?;
            if replay.failed {
                if let Some(k) = replay.run.live.iter().position(|live| *live) {
                    served[k] = true;
                    if witnesses[k].is_none() {
                        witnesses[k] = Some(replay.run);
                    }
                }
            }
        }
        Ok(Census { served, witnesses })
    }

    /// The multiverse passes (decision 75): shrink the counterexample as a
    /// set, under [`set_order`]. Each round first takes a census
    /// ([`Self::nd_census`]) and drops every pool timeline that served no
    /// failing run — the one deletion that costs no reproduction — then
    /// proposes swapping adjacent components toward sorted order (which
    /// timeline serves first at a disagreement is state, and sorted is the
    /// fixpoint) and replacing a component with a positional splice of
    /// another's prefix onto it when the splice is smaller; those
    /// candidates are whole sets judged by [`Self::nd_evaluate_set`], and a
    /// changed first component is installed from the witness that stayed
    /// on it. Every accept is strictly smaller under the order, so the
    /// rounds end on their own; the deadline, checked per round, bounds
    /// them too.
    async fn nd_multiverse_shrink(
        &mut self,
        origin: &str,
        anchor: f64,
        deadline: Option<crate::sys::Instant>,
        verbosity: Verbosity,
        output: &Output,
    ) -> Result<(), RunError> {
        let expired = |d: Option<crate::sys::Instant>| {
            d.is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d))
        };
        loop {
            let set = self.origins.entry(origin).timelines();
            if set.len() < 2 || expired(deadline) {
                return Ok(());
            }
            let served = self.nd_census(origin, &set).await?.served;
            let kept: Vec<Vec<ChoiceValue>> = set
                .iter()
                .enumerate()
                .filter(|(k, _)| *k == 0 || served[*k])
                .map(|(_, timeline)| timeline.clone())
                .collect();
            if verbosity == Verbosity::Debug {
                output.line(&format!(
                    "nd multiverse census: origin={origin} kept {} of {} timelines (served {served:?})",
                    kept.len(),
                    set.len()
                ));
            }
            if kept != set {
                self.origins.entry(origin).install_set(&kept, None);
                self.persist_incumbent(origin)?;
                continue;
            }
            let mut candidates: Vec<(&'static str, Vec<Vec<ChoiceValue>>)> = Vec::new();
            for k in 1..set.len() {
                if timeline_order(&set[k], &set[k - 1]) == core::cmp::Ordering::Less {
                    let mut candidate = set.clone();
                    candidate.swap(k - 1, k);
                    candidates.push(("reorder", candidate));
                }
            }
            for k in 0..set.len() {
                let mut j = self.rng.random_range(0..set.len() - 1);
                if j >= k {
                    j += 1;
                }
                let bound = set[j].len().min(set[k].len());
                if bound < 2 {
                    continue;
                }
                let cut = bound - 1;
                let mut spliced = set[j][..cut].to_vec();
                spliced.extend_from_slice(&set[k][cut..]);
                if timeline_order(&spliced, &set[k]) == core::cmp::Ordering::Less
                    && !set.contains(&spliced)
                {
                    let mut candidate = set.clone();
                    candidate[k] = spliced;
                    candidates.push(("splice", candidate));
                }
            }
            let mut changed = false;
            for (pass, candidate) in candidates {
                crate::control::hegel_internal_assert!(
                    set_order(&candidate, &set) == core::cmp::Ordering::Less,
                    "nd_multiverse_shrink: a {pass} candidate is not smaller than its set"
                );
                let needs_witness = candidate[0] != set[0];
                let verdict = self
                    .nd_evaluate_set(origin, &candidate, anchor, needs_witness)
                    .await?;
                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "nd multiverse {pass}: origin={origin} timelines={} accepted={}",
                        candidate.len(),
                        verdict.accepted
                    ));
                }
                if !verdict.accepted {
                    continue;
                }
                let nodes = verdict.witness.filter(|_| needs_witness).map(|w| w.nodes);
                self.origins.entry(origin).install_set(&candidate, nodes);
                self.persist_incumbent(origin)?;
                changed = true;
                break;
            }
            if !changed {
                return Ok(());
            }
        }
    }

    /// Persist `origin`'s current incumbent and pool.
    fn persist_incumbent(&mut self, origin: &str) -> Result<(), InternalError> {
        let incumbent = self
            .origins
            .incumbent(origin)
            .map(<[ChoiceNode]>::to_vec)
            .unwrap_or_default();
        self.record_nd_incumbent(origin, &incumbent)
    }

    /// Replay-until-failure over stored ND state (decisions 25 and 74): the
    /// whole counterexample as one test case, up to `attempts` times, then
    /// positional splices of random timeline pairs, then up to `fresh`
    /// fresh generations. Returns the first reproducing run plus the
    /// evidence accumulated across every attempt, for the caller's hygiene
    /// verdict; the fresh tier is a rescue, not a replay of the stored
    /// state, so only its failures enter the evidence.
    async fn nd_reproduce(
        &mut self,
        origin: Option<&str>,
        timelines: &[Vec<ChoiceValue>],
        attempts: u64,
        splices: u64,
        fresh: u64,
    ) -> Result<(Option<RunResult>, nd::Evidence), RunError> {
        let mut evidence = nd::Evidence::default();
        if !timelines.is_empty() {
            for _ in 0..attempts {
                let replay = self.nd_replay_set(timelines, origin).await?;
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
                return Err(RunError::Flaky(flaky_diagnostic_for(&origin)));
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
                self.record_nd_incumbent(&origin, &initial)?;
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
            let admitted = self.origins.entry(&origin);
            admitted.confirm(probe_anchor, None, pool, evidence)?;
            self.record_nd_incumbent(&origin, &initial)?;
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

        let gauntleted = self.nd_handling();
        if gauntleted {
            let set = self.origins.entry(&origin).timelines();
            if set.len() > 1 {
                let measured = self.nd_measure_set(&origin, &set).await?;
                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "nd set anchor: origin={origin} timelines={} fails={}/{} lcb={:.3} (was {probe_anchor:.3})",
                        set.len(),
                        measured.fails(),
                        measured.runs(),
                        measured.lower_bound()
                    ));
                }
                if measured.lower_bound() > probe_anchor {
                    probe_anchor = measured.lower_bound();
                    self.origins.entry(&origin).raise_anchor(probe_anchor);
                }
            }
        }
        let initial_spans = Spans::from(verify.spans.clone());
        if verbosity == Verbosity::Debug {
            output.line(&format!(
                "nd shrink start: origin={origin} timelines={} anchor={probe_anchor:.3}",
                self.origins.entry(&origin).timelines().len()
            ));
        }
        let (shrunk, timed_out) = if gauntleted {
            let mut starts: Vec<(Vec<ChoiceNode>, Vec<Span>)> = Vec::new();
            let set = self.origins.entry(&origin).timelines();
            if set.len() > 1 {
                let census = self.nd_census(&origin, &set).await?;
                let mut kept: Vec<Vec<ChoiceValue>> = Vec::with_capacity(set.len());
                for (k, witness) in census.witnesses.into_iter().enumerate() {
                    if let Some(witness) = witness {
                        kept.push(set[k].clone());
                        starts.push((witness.nodes, witness.spans));
                    }
                }
                if kept.is_empty() {
                    kept.push(set[0].clone());
                    starts.push((verify.nodes, verify.spans));
                }
                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "nd parallel shrink: origin={origin} lanes={} of {} timelines (served {:?})",
                        kept.len(),
                        set.len(),
                        census.served
                    ));
                }
                if kept != set {
                    let incumbent = (kept[0] != set[0]).then(|| starts[0].0.clone());
                    self.origins.entry(&origin).install_set(&kept, incumbent);
                    self.persist_incumbent(&origin)?;
                }
            } else {
                starts.push((verify.nodes, verify.spans));
            }
            let result = self
                .nd_parallel_shrink(
                    &origin,
                    starts,
                    probe_anchor,
                    shrink_deadline,
                    verbosity,
                    output,
                )
                .await?;
            let mut set = result.set.into_iter();
            let shrunk = crate::control::hegel_internal_unwrap!(
                set.next(),
                "parallel shrink: no lane for {origin}"
            );
            (shrunk, result.timed_out)
        } else {
            let probe = EngineShrinkProbe {
                engine: &mut *self,
                target_origin: origin.clone(),
                verbosity,
                output: output.clone(),
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
        let anchor = self
            .origins
            .get(&origin)
            .and_then(Counterexample::anchor)
            .unwrap_or(probe_anchor);
        if verbosity == Verbosity::Debug {
            output.line(&format!(
                "nd shrink done: origin={origin} timelines={} anchor={anchor:.3} timed_out={timed_out}",
                self.origins.entry(&origin).timelines().len()
            ));
        }
        if !gauntleted && self.nd_handling() {
            self.origins.entry(&origin).replace(initial);
        } else {
            self.origins.entry(&origin).replace(shrunk);
            if gauntleted && !timed_out {
                self.nd_multiverse_shrink(&origin, anchor, shrink_deadline, verbosity, output)
                    .await?;
            }
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
                        return Err(RunError::Flaky(flaky_diagnostic_for(&origin)));
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
                    nd::reuse_replay_budget(),
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
                            let confirmed_origin = reviewed.confirm(
                                review.evidence.lower_bound(),
                                None,
                                pool,
                                review_evidence,
                            );
                            confirmed_origin?;
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
        let set = self.origins.entry(origin).timelines_from(choices.to_vec());
        let capture_entry = self.capture_replays;
        self.capture_replays = true;
        let bar_accepted = loop {
            if deadline.is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d)) {
                break false;
            }
            let replay = self.nd_replay_set(&set, Some(origin)).await?;
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
            let replay = self.nd_replay_set(&set, Some(origin)).await?;
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
        let entry_keys: Vec<Vec<u8>> = entries
            .iter()
            .map(|(timeline, _)| serialize_executed_choices(timeline))
            .collect::<Result<_, _>>()?;
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
                        .min_by(|&&a, &&b| shortlex(&entry_keys[a], &entry_keys[b]))
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
            let confirmed = self.origins.entry(origin).confirm(
                anchor,
                Some(witness),
                pool,
                (batch.evidence.fails(), batch.evidence.runs()),
            );
            confirmed?;
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
            let state = self.nd_state_for(origin, incumbent)?;
            self.persister.supersede_nd(origin, &nodes, &state)?;
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
                    RunError::Flaky(flaky_diagnostic_for(origin))
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
                let confirmed = self.origins.entry(&origin).confirm(
                    batch.evidence.lower_bound(),
                    batch.witness,
                    pool,
                    evidence,
                );
                confirmed?;
                self.record_nd_incumbent(&origin, &nodes)?;
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
    ) -> Result<crate::native::blob::NdReproState, InternalError> {
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
    fn record_nd_incumbent(
        &mut self,
        origin: &str,
        nodes: &[ChoiceNode],
    ) -> Result<(), InternalError> {
        let incumbent: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
        let state = self.nd_state_for(origin, incumbent)?;
        self.persister.record_nd(origin, nodes, &state)
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
        let mut mismatch = self.record_run(&run, elapsed, measurement)?;
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
    ) -> Result<Option<RunError>, InternalError> {
        let mismatch = if self.nd_active {
            None
        } else {
            self.record_execution(run, measurement)?
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
                    self.persister.record(&origin, &run.nodes)?;
                    let counterexample = self.origins.entry(&origin);
                    let accept = counterexample.adopt(run.nodes.clone());
                    counterexample.record_sighting(&run.nodes, accept)?;
                }
            } else if self.origins.incumbent(&origin).is_none() {
                self.origins.entry(&origin).adopt(run.nodes.clone());
            }
        }
        Ok(mismatch)
    }

    /// Feed one executed run to the detectors the tree used to be: the kind
    /// ledger (`error` strictness only), then — for conclusions; an overrun
    /// concluded nothing — the execution cache, whose digest hit both drives
    /// the duplicate-stop counter (generation-window, non-measurement cases
    /// only) and, on a verdict change, reports the flake the tree could
    /// never see. The returned error is `NonDeterministic` for kind drift
    /// and `Flaky` for a verdict change (decision 30's split); the caller
    /// keeps it under `error` strictness and flips otherwise.
    fn record_execution(
        &mut self,
        run: &RunResult,
        measurement: bool,
    ) -> Result<Option<RunError>, InternalError> {
        if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
            if let Some(msg) = self.kind_ledger.observe(&run.nodes)? {
                return Ok(Some(RunError::NonDeterministic(msg)));
            }
        }
        if run.status == Status::EarlyStop {
            return Ok(None);
        }
        let recorded = self.exec_cache.record(
            serialize_executed_nodes(&run.nodes)?,
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
        Ok(recorded.verdict_mismatch.then(|| {
            RunError::Flaky(match &recorded.mismatched_origin {
                Some(origin) => flaky_diagnostic_for(origin),
                None => flaky_diagnostic(),
            })
        }))
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
        let divergence = NativeDataSource::take_divergence(&handle);
        let live = NativeDataSource::take_live(&handle);
        let realized = NativeDataSource::take_realized(&handle);
        let ran_out = NativeDataSource::take_ran_out(&handle);
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
            divergence,
            live,
            realized,
            ran_out,
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
            let key = crate::control::hegel_internal_unwrap!(
                serialize_choices(choices),
                "a replayed test case's clone values nest deeper than MAX_CLONE_DEPTH"
            );
            if let Some(hit) = self.exec_cache.serve(&key) {
                return Ok(RunResult {
                    status: hit.status,
                    nodes: hit.nodes,
                    spans: hit.spans,
                    origin: hit.origin,
                    target_observations: HashMap::default(),
                    events: Vec::new(),
                    divergence: None,
                    live: Vec::new(),
                    realized: Vec::new(),
                    ran_out: false,
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

/// The engine side of the shrinker's [`ShrinkProbe`] for a deterministic
/// run: routes every requested run through [`Engine::cached_test_function`]
/// and reports whether the run reproduced the origin being shrunk. Borrows
/// the engine for the duration of the shrink, so the shrinker's executions
/// record into the engine's tree and counters like any other run. Under
/// nondeterministic handling the shrink runs through
/// [`Engine::nd_parallel_shrink`] instead.
struct EngineShrinkProbe<'e, 'a> {
    engine: &'e mut Engine<'a>,
    target_origin: String,
    verbosity: Verbosity,
    output: Output,
}

impl ShrinkProbe for EngineShrinkProbe<'_, '_> {
    fn run<'s>(&'s mut self, req: ShrinkRun<'s>) -> crate::native::shrinker::ProbeFuture<'s> {
        Box::pin(async move {
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
            let matched = run.status == Status::Interesting
                && run.origin.as_deref() == Some(self.target_origin.as_str());
            Ok((matched, run.nodes, Spans::from(run.spans)))
        })
    }
}

/// A shrinker's requested run, owned by the engine while the shrinker is
/// suspended awaiting its outcome (decision 77).
enum OwnedRun {
    Full(Vec<ChoiceNode>),
    Probe {
        prefix: Vec<ChoiceValue>,
        max_size: usize,
    },
}

impl OwnedRun {
    fn values(&self) -> Vec<ChoiceValue> {
        match self {
            OwnedRun::Full(nodes) => nodes.iter().map(|n| n.value()).collect(),
            OwnedRun::Probe { prefix, .. } => prefix.clone(),
        }
    }
}

/// Runs a proposal's misfit is put down to the test's own nondeterminism
/// before it is taken for the proposal's edit and punned. With hidden
/// branches a value edit before the branch point prunes every other
/// timeline, so the branch's draw diverges on the proposal alone and looked
/// like the edit's doing; punning it there handed the shrinker a hybrid on
/// every such run and its mutation pass spent its deep divergence budget on
/// each (experiment 016, campaign 8: 15k of 24k executions, and 19k of 23k
/// on a pool missing two of four shapes). An edit that truly changes the
/// path misfits on every run, so it is realized after this many; a branch
/// the test takes with probability q slips through as a hybrid with
/// probability q^6 (1.6% at a coin, 18% at three other equiprobable
/// branches).
const MISFIT_DEFERRALS: u64 = 6;

/// The exchange between one per-timeline shrinker and the engine's
/// parallel shrink driver (decision 77): the shrinker posts a request and
/// suspends until a response is there; the driver reads the request, and
/// the sweep mode and adoption the shrinker reports.
struct Slot {
    request: Option<OwnedRun>,
    response: Option<crate::native::shrinker::ShrinkResult<(bool, Vec<ChoiceNode>, Spans)>>,
    sweep: SweepMode,
    adopted: bool,
}

/// The [`ShrinkProbe`] handed to each shrinker of a parallel shrink: its
/// side of a [`Slot`].
struct SlotProbe {
    slot: Arc<Mutex<Slot>>,
}

impl ShrinkProbe for SlotProbe {
    fn run<'s>(&'s mut self, req: ShrinkRun<'s>) -> crate::native::shrinker::ProbeFuture<'s> {
        let owned = match req {
            ShrinkRun::Full(nodes) => OwnedRun::Full(nodes.to_vec()),
            ShrinkRun::Probe { prefix, max_size } => OwnedRun::Probe {
                prefix: prefix.to_vec(),
                max_size,
            },
        };
        self.slot.lock().request = Some(owned);
        let slot = Arc::clone(&self.slot);
        Box::pin(core::future::poll_fn(move |_| {
            match slot.lock().response.take() {
                Some(response) => core::task::Poll::Ready(response),
                None => core::task::Poll::Pending,
            }
        }))
    }

    fn set_sweep_mode(&mut self, mode: SweepMode) -> Option<SweepMode> {
        Some(core::mem::replace(&mut self.slot.lock().sweep, mode))
    }

    fn candidate_adopted(&mut self) -> Result<(), InternalError> {
        self.slot.lock().adopted = true;
        Ok(())
    }
}

/// See [`Lane::pending_accept`].
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
    /// Runs led by the candidate that left it (decision 77): no evidence
    /// about the candidate on its own timeline, only about the set.
    bounces: u64,
    /// Every run the candidate led, for the starvation allowance.
    led: u64,
    /// Every run the candidate led — with it served first — as evidence
    /// about the counterexample its accept would install (decision 75): a
    /// candidate must not lower the set's reproduction past the gauntlet
    /// threshold (decision 2), however reliably it fails when the test
    /// stays on it.
    set_evidence: nd::Evidence,
}

/// One timeline of a lane's contribution to a set replay
/// ([`Lane::components`]): its values, the proposal's nodes when known, and
/// whether it is an unrealized proposal (a pun timeline) that insists on
/// its misfits being punned.
struct Component {
    values: Vec<ChoiceValue>,
    nodes: Option<Vec<ChoiceNode>>,
    pun: bool,
    insist: bool,
}

/// A proposal in flight: its request, and once a run has realized it, the
/// realization the ledger keys on and the shrinker is answered with.
struct Candidate {
    request: OwnedRun,
    realized: Option<Realized>,
    /// Runs that left the proposal at a misfit — another stored timeline
    /// served the draw, or none did — which is a branch the test took on
    /// its own, or a path the proposal's edit opened. Deferred that many
    /// times, the proposal is not realized by the run; at
    /// [`MISFIT_DEFERRALS`] it insists ([`Component::insist`]): the misfit
    /// is taken for the edit's own and punned (decision 77).
    deferrals: u64,
}

struct Realized {
    key: Vec<u8>,
    values: Vec<ChoiceValue>,
    nodes: Vec<ChoiceNode>,
    spans: Vec<Span>,
}

type LaneFuture =
    Pin<Box<dyn Future<Output = crate::native::shrinker::ShrinkResult<Shrinker<'static>>> + Send>>;

/// One timeline of the counterexample under a parallel shrink (decision
/// 77): its shrinker (until it finishes), its current timeline, the
/// proposal it has in flight, and its gauntlet ledgers.
struct Lane {
    slot: Arc<Mutex<Slot>>,
    shrinking: Option<LaneFuture>,
    current: Vec<ChoiceNode>,
    candidate: Option<Candidate>,
    ledger: HashMap<Vec<u8>, CandidateLedger>,
    /// Whether the lane was stopped for starvation — a realized candidate
    /// whose branch is too rare to gauntlet within the allowance — so that
    /// its shrinker's early return is not a timeout.
    stopped: bool,
    /// The most recent gauntlet accept, held until the shrinker either
    /// adopts it (the point where the anchor raise and persistence
    /// happen) or posts its next request. A gauntlet accept the shrinker
    /// discards — a punned realization, or a sort-key-larger mutation
    /// probe — must move nothing.
    pending_accept: Option<PendingAccept>,
}

impl Lane {
    fn new(slot: Arc<Mutex<Slot>>, shrinking: LaneFuture, current: Vec<ChoiceNode>) -> Self {
        Lane {
            slot,
            shrinking: Some(shrinking),
            current,
            candidate: None,
            ledger: HashMap::default(),
            stopped: false,
            pending_accept: None,
        }
    }

    /// The lane's components of one set replay, in order: its candidate,
    /// then its current timeline — the branch the lane stands for, so that
    /// a run taking that branch always has a timeline to stay on, whatever
    /// the candidate did. A proposal not yet realized is a pun timeline; a
    /// run that stays on the current timeline behind it has realized the
    /// proposal as the current.
    fn components(&self) -> Vec<Component> {
        let current: Vec<ChoiceValue> = self.current.iter().map(|n| n.value()).collect();
        let stored = |values: Vec<ChoiceValue>| Component {
            values,
            nodes: None,
            pun: false,
            insist: false,
        };
        match &self.candidate {
            Some(Candidate {
                realized: Some(realized),
                ..
            }) => {
                let mut components = alloc::vec![stored(realized.values.clone())];
                if realized.values != current {
                    components.push(stored(current));
                }
                components
            }
            Some(Candidate {
                request,
                realized: None,
                deferrals,
            }) => {
                let nodes = match request {
                    OwnedRun::Full(nodes) => Some(nodes.clone()),
                    OwnedRun::Probe { .. } => None,
                };
                alloc::vec![
                    Component {
                        values: request.values(),
                        nodes,
                        pun: true,
                        insist: *deferrals >= MISFIT_DEFERRALS,
                    },
                    stored(current),
                ]
            }
            None => alloc::vec![stored(current)],
        }
    }

    fn respond(&mut self, verdict: bool, nodes: Vec<ChoiceNode>, spans: Vec<Span>) {
        self.slot.lock().response = Some(Ok((verdict, nodes, Spans::from(spans))));
        self.candidate = None;
    }

    fn stop(&mut self) {
        self.slot.lock().response = Some(Err(crate::native::shrinker::ShrinkHalt::Stop));
        self.candidate = None;
        self.stopped = true;
    }
}

/// The outcome of a parallel shrink: every lane's final timeline, in the
/// counterexample's order.
struct ParallelShrink {
    set: Vec<Vec<ChoiceNode>>,
    timed_out: bool,
}

/// The lanes of a driven parallel shrink, as they ended.
struct DrivenLanes {
    lanes: Vec<Lane>,
    timed_out: bool,
}

/// Set evidence past which a candidate with a full on-timeline ledger is
/// rejected as undecidable: its reproduction as a counterexample sits too
/// close to the threshold to resolve. Also, per lane, the led runs a
/// realized candidate may spend before its lane is stopped as a branch too
/// rare to gauntlet — on-timeline evidence arrives at the branch's share
/// of the runs, so the allowance scales with the number of lanes.
const SET_EVIDENCE_CAP: u64 = 2 * nd::GAUNTLET_CAP;

impl<'a> Engine<'a> {
    /// The parallel shrink (decision 77): one shrinker per timeline of the
    /// counterexample, each suspended on a [`Slot`] while the engine runs
    /// the whole set of their proposals as one test case. Each execution
    /// is led by one lane in rotation — its component served first, the
    /// rest in order — and whichever timelines the run stayed on it is a
    /// trial of. An unrealized proposal is realized by the first run that
    /// executed it as itself ([`RunResult::realized`]: live to the end, or
    /// out of the live set at the divergence by its own misfit or its own
    /// end — a proposal that runs out draws its tail at random, never from
    /// the current timeline it was a prefix of); from then on its
    /// realization is the component, and runs live on it are its
    /// on-timeline evidence. The lane's current timeline stays in the set
    /// as the branch's shadow ([`Lane::components`]), so a run that takes
    /// the branch with other values has a timeline to stay on.
    /// Every run a lane led is evidence about the set it would install
    /// (decision 75); a led run that left the candidate is no evidence
    /// about the candidate itself and costs it nothing but the led-run cap
    /// ([`SET_EVIDENCE_CAP`], decision 77). A candidate is answered when
    /// its ledgers reach a bound; the shrinker's
    /// adoption raises the shared anchor, installs the lane's timeline, and
    /// persists the set. Lanes that have finished keep their final timeline
    /// in the set and never lead.
    async fn nd_parallel_shrink(
        &mut self,
        origin: &str,
        starts: Vec<(Vec<ChoiceNode>, Vec<Span>)>,
        anchor: f64,
        deadline: Option<crate::sys::Instant>,
        verbosity: Verbosity,
        output: &Output,
    ) -> Result<ParallelShrink, RunError> {
        let mut lanes: Vec<Lane> = Vec::with_capacity(starts.len());
        for (nodes, spans) in starts {
            let slot = Arc::new(Mutex::new(Slot {
                request: None,
                response: None,
                sweep: SweepMode::Fast,
                adopted: false,
            }));
            let mut shrinker = Shrinker::with_probe(
                Box::new(SlotProbe {
                    slot: Arc::clone(&slot),
                }),
                nodes.clone(),
                Spans::from(spans),
            );
            shrinker.deadline = deadline;
            let debug = (verbosity == Verbosity::Debug).then(|| output.clone());
            let shrinking: LaneFuture = Box::pin(async move {
                shrinker.initial_coarse_reduction().await?;
                if let Some(output) = debug {
                    shrinker.set_debug(move |msg| output.line(msg));
                }
                shrinker
                    .shrink()
                    .await
                    .map_err(crate::native::shrinker::ShrinkHalt::Error)?;
                Ok(shrinker)
            });
            lanes.push(Lane::new(slot, shrinking, nodes));
        }
        let driven = self
            .nd_drive_lanes(origin, lanes, anchor, deadline, verbosity, output)
            .await?;
        Ok(ParallelShrink {
            set: driven.lanes.into_iter().map(|lane| lane.current).collect(),
            timed_out: driven.timed_out,
        })
    }

    /// Drive `lanes` to the end of every shrinker: see
    /// [`Self::nd_parallel_shrink`], whose loop this is.
    async fn nd_drive_lanes(
        &mut self,
        origin: &str,
        mut lanes: Vec<Lane>,
        anchor: f64,
        deadline: Option<crate::sys::Instant>,
        verbosity: Verbosity,
        output: &Output,
    ) -> Result<DrivenLanes, RunError> {
        let mut anchor = anchor;
        let mut raised: crate::native::HashSet<Vec<u8>> = crate::native::HashSet::default();
        let mut cursor = 0usize;
        let mut timed_out = false;
        let expired = |d: Option<crate::sys::Instant>| {
            d.is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d))
        };
        loop {
            let mut changed = false;
            for (k, lane) in lanes.iter_mut().enumerate() {
                if let Some(shrinking) = lane.shrinking.as_mut() {
                    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
                    if let core::task::Poll::Ready(finished) = shrinking.as_mut().poll(&mut cx) {
                        match finished {
                            Ok(shrinker) => {
                                lane.current = shrinker.current_nodes;
                                timed_out |= shrinker.timed_out;
                            }
                            Err(crate::native::shrinker::ShrinkHalt::Stop) => {
                                timed_out |= !lane.stopped;
                            }
                            Err(crate::native::shrinker::ShrinkHalt::Error(e)) => return Err(e),
                        }
                        lane.shrinking = None;
                    }
                }
                let adopted = core::mem::take(&mut lane.slot.lock().adopted);
                if adopted {
                    if let Some(accept) = lane.pending_accept.take() {
                        let lower_bound = accept.lower_bound.min(nd::anchor_ceiling());
                        if raised.insert(accept.key) && lower_bound > anchor {
                            anchor = lower_bound;
                            self.origins.entry(origin).raise_anchor(anchor);
                        }
                        lane.current = accept.nodes;
                        changed = true;
                        if verbosity == Verbosity::Debug {
                            output.line(&format!(
                                "nd lane adopted: origin={origin} lane={k} anchor={anchor:.3}"
                            ));
                        }
                    }
                }
                if lane.candidate.is_none() {
                    if let Some(request) = lane.slot.lock().request.take() {
                        lane.pending_accept = None;
                        lane.candidate = Some(Candidate {
                            request,
                            realized: None,
                            deferrals: 0,
                        });
                    }
                }
            }
            if changed {
                let set: Vec<Vec<ChoiceValue>> = lanes
                    .iter()
                    .map(|lane| lane.current.iter().map(|n| n.value()).collect())
                    .collect();
                self.origins
                    .entry(origin)
                    .install_set(&set, Some(lanes[0].current.clone()));
                self.persist_incumbent(origin)?;
            }
            if lanes.iter().all(|lane| lane.shrinking.is_none()) {
                break;
            }
            let active: Vec<usize> = (0..lanes.len())
                .filter(|&k| lanes[k].candidate.is_some())
                .collect();
            if active.is_empty() {
                crate::control::hegel_internal_error!(
                    "parallel shrink: every shrinker is suspended without a request"
                );
            }
            if expired(deadline) {
                timed_out = true;
                for &k in &active {
                    let nodes = lanes[k].current.clone();
                    lanes[k].respond(false, nodes, Vec::new());
                }
                continue;
            }
            cursor %= active.len();
            let leader = active[cursor];
            cursor += 1;
            let mut order: Vec<usize> = Vec::with_capacity(lanes.len());
            order.push(leader);
            order.extend((0..lanes.len()).filter(|&k| k != leader));
            let mut timelines: Vec<Vec<ChoiceValue>> = Vec::new();
            let mut nodes: Vec<Option<Vec<ChoiceNode>>> = Vec::new();
            let mut puns: Vec<bool> = Vec::new();
            let mut insists: Vec<bool> = Vec::new();
            let mut positions: Vec<(usize, usize)> = Vec::new();
            let mut max_size = 0usize;
            for &k in &order {
                if let Some(Candidate {
                    request: OwnedRun::Probe { max_size: size, .. },
                    realized: None,
                    ..
                }) = &lanes[k].candidate
                {
                    max_size = max_size.max(*size);
                }
                let start = timelines.len();
                for component in lanes[k].components() {
                    timelines.push(component.values);
                    nodes.push(component.nodes);
                    puns.push(component.pun);
                    insists.push(component.insist);
                }
                positions.push((start, timelines.len()));
            }
            let longest = timelines
                .iter()
                .map(|t| crate::native::core::flattened_values_len(t))
                .max()
                .unwrap_or(0);
            let max_size = max_size.max(nd::continuation_budget(longest));
            let rng = self.rng.spawn();
            let ntc =
                NativeTestCase::for_shrink_set(&timelines, nodes, puns, insists, rng, max_size)?;
            let (run, _mismatch) = self.measure(ntc).await?;
            let failed = run.status == Status::Interesting && run.origin.as_deref() == Some(origin);
            let realized_values: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
            let flag =
                |flags: &[bool], position: usize| flags.get(position).copied().unwrap_or(false);
            let kinds =
                |nodes: &[ChoiceNode]| nodes.iter().map(|n| n.data.kind()).collect::<Vec<_>>();
            let shapes: Vec<Vec<_>> = lanes.iter().map(|lane| kinds(&lane.current)).collect();
            let run_shape = kinds(&run.nodes);
            for (&k, &(start, _)) in order.iter().zip(&positions) {
                let live = flag(&run.live, start);
                let realized = flag(&run.realized, start);
                let is_leader = k == leader;
                let sweep = lanes[k].slot.lock().sweep;
                let lane = &mut lanes[k];
                let Some(candidate) = lane.candidate.as_mut() else {
                    continue;
                };
                match candidate.realized.as_ref() {
                    None => {
                        let ran_out = realized && run.ran_out;
                        if ran_out && !failed && matches!(candidate.request, OwnedRun::Full(_)) {
                            lane.respond(false, run.nodes.clone(), run.spans.clone());
                            continue;
                        }
                        let left_at_a_misfit = !live && !ran_out;
                        if left_at_a_misfit
                            && (is_leader || realized)
                            && candidate.deferrals < MISFIT_DEFERRALS
                        {
                            candidate.deferrals += 1;
                            continue;
                        }
                        if !realized {
                            continue;
                        }
                        if failed && sort_key(&run.nodes) > sort_key(&lane.current) {
                            lane.respond(false, run.nodes.clone(), run.spans.clone());
                            continue;
                        }
                        if !live && run_shape != shapes[k] && shapes.contains(&run_shape) {
                            let nodes = match &candidate.request {
                                OwnedRun::Full(nodes) => nodes.clone(),
                                OwnedRun::Probe { .. } => run.nodes.clone(),
                            };
                            lane.respond(false, nodes, run.spans.clone());
                            continue;
                        }
                        let key = crate::control::hegel_internal_unwrap!(
                            serialize_choices(&realized_values),
                            "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
                        );
                        if lane.ledger.get(&key).is_none_or(|l| l.verdict.is_none()) {
                            let (seed, pinned) = lane
                                .ledger
                                .get(&key)
                                .map_or((nd::Evidence::default(), None), |l| {
                                    (l.evidence, Some(l.min_fails))
                                });
                            let min_fails = self.origins.entry(origin).gauntlet_spend.charge(
                                &seed,
                                anchor,
                                sweep == SweepMode::Confirm,
                                pinned,
                            );
                            lane.ledger.entry(key.clone()).or_insert(CandidateLedger {
                                evidence: nd::Evidence::default(),
                                min_fails,
                                verdict: None,
                                bounces: 0,
                                led: 0,
                                set_evidence: nd::Evidence::default(),
                            });
                        }
                        let entry = lane.ledger.get_mut(&key).unwrap();
                        entry.evidence.record(failed);
                        if is_leader {
                            entry.led += 1;
                            entry.set_evidence.record(failed);
                        }
                        let fast_miss =
                            !failed && sweep == SweepMode::Fast && entry.verdict.is_none();
                        candidate.realized = Some(Realized {
                            key,
                            values: realized_values.clone(),
                            nodes: run.nodes.clone(),
                            spans: run.spans.clone(),
                        });
                        if fast_miss {
                            lane.respond(false, run.nodes.clone(), run.spans.clone());
                        }
                    }
                    Some(realized) => {
                        let entry = lane.ledger.get_mut(&realized.key).unwrap();
                        if live {
                            entry.evidence.record(failed);
                        }
                        if is_leader {
                            entry.led += 1;
                            if !live {
                                entry.bounces += 1;
                            }
                            if live || flag(&run.realized, start) {
                                entry.set_evidence.record(failed);
                            }
                        }
                    }
                }
            }
            let threshold = nd::gauntlet_threshold(anchor);
            let starvation_allowance = SET_EVIDENCE_CAP * lanes.len() as u64;
            for (k, lane) in lanes.iter_mut().enumerate() {
                let Some(Candidate {
                    realized: Some(realized),
                    ..
                }) = &lane.candidate
                else {
                    continue;
                };
                let ledger = lane.ledger.get_mut(&realized.key).unwrap();
                let mut starve = false;
                let verdict = if let Some(verdict) = ledger.verdict {
                    Some(verdict)
                } else if ledger.set_evidence.runs() > 0
                    && ledger.set_evidence.upper_bound() < threshold
                {
                    Some(false)
                } else {
                    match nd::gauntlet(&ledger.evidence, anchor, ledger.min_fails) {
                        nd::GauntletVerdict::Reject => Some(false),
                        nd::GauntletVerdict::Accept
                            if ledger.set_evidence.lower_bound() >= threshold
                                && ledger.evidence.runs() >= nd::ANCHOR_SEED_RUNS =>
                        {
                            Some(true)
                        }
                        _ if ledger.set_evidence.runs() >= SET_EVIDENCE_CAP
                            && ledger.evidence.runs() >= nd::ANCHOR_SEED_RUNS =>
                        {
                            if verbosity == Verbosity::Debug {
                                output.line(&format!(
                                    "gauntlet abandoned a candidate: undecided after {} led runs ({} left the timeline; on-timeline {}/{}, set {}/{})",
                                    ledger.set_evidence.runs(),
                                    ledger.bounces,
                                    ledger.evidence.fails(),
                                    ledger.evidence.runs(),
                                    ledger.set_evidence.fails(),
                                    ledger.set_evidence.runs()
                                ));
                            }
                            Some(false)
                        }
                        _ if ledger.led >= starvation_allowance => {
                            starve = true;
                            None
                        }
                        _ => None,
                    }
                };
                if starve {
                    if verbosity == Verbosity::Debug {
                        output.line(&format!(
                            "nd lane starved: origin={origin} lane={k}: its candidate led {} runs and stayed on it for {}; its shrink stops",
                            ledger.led,
                            ledger.evidence.runs()
                        ));
                    }
                    lane.stop();
                    continue;
                }
                let Some(verdict) = verdict else {
                    continue;
                };
                ledger.verdict = Some(verdict);
                if verdict {
                    lane.pending_accept = Some(PendingAccept {
                        key: realized.key.clone(),
                        lower_bound: ledger.set_evidence.lower_bound(),
                        nodes: realized.nodes.clone(),
                    });
                }
                let nodes = realized.nodes.clone();
                let spans = realized.spans.clone();
                lane.respond(verdict, nodes, spans);
            }
        }
        let set: Vec<Vec<ChoiceValue>> = lanes
            .iter()
            .map(|lane| lane.current.iter().map(|n| n.value()).collect())
            .collect();
        self.origins
            .entry(origin)
            .install_set(&set, Some(lanes[0].current.clone()));
        Ok(DrivenLanes { lanes, timed_out })
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
