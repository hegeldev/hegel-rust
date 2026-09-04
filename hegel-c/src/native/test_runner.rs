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
use hashbrown::hash_map::Entry;

use rand::RngExt;

use crate::backend::{Failure, RunError, TestCaseResult, TestRunResult};
use crate::exchange::CaseExchange;
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, MAX_SHRINKING_SECONDS, NativeTestCase, Span, SpanEvent,
    Spans, Status, sort_key,
};
use crate::native::data_source::NativeDataSource;
use crate::native::data_tree::generate_novel_prefix;
use crate::native::database::{
    DirectoryTestCaseDatabase, TestCaseDatabase, deserialize_choices, serialize_choices,
};
use crate::native::nd;
use crate::native::nd::lifecycle::OriginLifecycle;
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
    /// Live span open/close events (with draw positions) from this execution,
    /// for folding into the choice tree. Empty on a result reconstructed from
    /// the tree (the events are already recorded there).
    pub span_events: Vec<(usize, SpanEvent)>,
    /// `tc.event()` / `tc.event_value()` observations from this execution,
    /// in recording order. Empty for tests that record no events and on a
    /// result reconstructed from the tree.
    pub events: Vec<(String, Option<f64>)>,
}

const RANDOM_GENERATION_BATCH: u64 = 10;
const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Outcome of one [`Engine::nd_replay_once`] measurement replay.
struct NdReplayOnce {
    run: RunResult,
    realized: Vec<ChoiceValue>,
    failed: bool,
    /// Evidence weight of a miss: the verbatim watermark against the
    /// replayed timeline. 1.0 for failures.
    weight: f64,
}

/// Outcome of one evidence batch (experiment 005): the replays, their
/// weighted evidence, the first failing run, and the failing timelines.
struct NdBatch {
    /// Whether the discovery bar's arithmetic accepted. Decides admission
    /// for unconfirmed origins; for trusted origins the bar is only the
    /// batch's stopping rule and any failure is evidence enough.
    bar_accepted: bool,
    evidence: nd::Evidence,
    witness: Option<RunResult>,
    captured: Vec<Vec<ChoiceValue>>,
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
/// distinct bug, each carrying the origin the engine grouped on and (unless
/// the run turned nondeterministic) the base64 reproduce blob encoding the
/// minimal counterexample's choices. `Err` means the run itself failed
/// (health check, nondeterminism mismatch) before reaching a verdict.
///
/// The caller replays each blob (via `hegel_test_case_from_blob`) to produce
/// the final report. Every test case this runs is non-final.
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
/// A deterministic blob replays its choices once. A nondeterministic blob
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
                    nd::reuse_replay_budget() as f64 / state.timelines.len() as f64,
                    nd::REPRODUCE_SPLICES,
                    0,
                )
                .await?;
            let failures = match run.and_then(|run| run.origin) {
                Some(origin) => {
                    engine.nd_origins.trust(
                        &origin,
                        state.timelines,
                        (evidence.fails(), evidence.runs()),
                    );
                    let caveat = engine.nd_origins.caveat(&origin);
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
                        if let Some(msg) = mismatch {
                            return Err(RunError::NonDeterministic(msg));
                        }
                        let failed = run.status == Status::Interesting;
                        (failed.then_some(run), (u64::from(failed), 1))
                    } else {
                        self.capture_replays = true;
                        let (run, evidence) = self
                            .nd_reproduce(
                                None,
                                &stored,
                                nd::reuse_replay_budget() as f64 / stored.len() as f64,
                                nd::REPRODUCE_SPLICES,
                                0,
                            )
                            .await?;
                        self.capture_replays = false;
                        (run, (evidence.fails(), evidence.runs()))
                    };
                    if let Some(run) = run {
                        if let Some(o) = run.origin.as_deref() {
                            self.nd_origins.trust(o, stored.clone(), reuse_evidence);
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
                if self.interesting.is_empty() {
                    replay_aligned = false;
                }
                log_phase("Reuse", "End");
            }
        }

        let shrink_phase = settings.phases.contains(&Phase::Shrink);
        let found_in_reuse = !self.interesting.is_empty();

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
            if let Some(msg) = mismatch {
                return Err(RunError::NonDeterministic(msg));
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
            && !self.tree_root.is_exhausted
            && should_generate_more(
                self.interesting.is_empty(),
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
                    || self.tree_root.is_exhausted
                    || !should_generate_more(
                        self.interesting.is_empty(),
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
                // Draw this test case's swarm parameters once, then use them for
                // both the novel-prefix walk and the test case itself so the
                // whole case generates from one consistent distribution.
                let params = crate::native::core::GenerationParameters::draw(&mut case_rng)?;
                let prefix = if self.nd_active {
                    Vec::new()
                } else {
                    generate_novel_prefix(&self.tree_root, &mut self.rng, params)?
                };
                let ntc = if prefix.is_empty() {
                    NativeTestCase::new_random_with_params(case_rng, params)
                } else {
                    NativeTestCase::for_probe_with_params(&prefix, case_rng, BUFFER_SIZE, params)
                };
                if verbosity == Verbosity::Verbose {
                    output.line("Running test case");
                }

                let (run, mismatch) = self.test_function(ntc).await?;
                if let Some(msg) = mismatch {
                    return Err(RunError::NonDeterministic(msg));
                }

                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "test case #{}: status = {:?}, choices = {}",
                        self.calls,
                        run.status,
                        crate::native::core::flattened_len(&run.nodes)
                    ));
                }

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

                if target_phase
                    && !self.nd_active
                    && self.interesting.is_empty()
                    && !self.targeting.is_empty()
                    && target_schedule.should_fire(self.valid_test_cases)
                {
                    let mut optimiser = crate::native::targeting::Optimiser {
                        engine: &mut *self,
                        max_valid: max_test_cases,
                        max_calls: max_test_cases * 10,
                    };
                    optimiser.optimise_targets().await?;
                }

                if run.status == Status::Valid
                    && (self.valid_test_cases >= HEALTH_CHECK_MAX_VALID
                        || !self.interesting.is_empty())
                {
                    self.try_span_mutation(&run.nodes, &run.spans).await?;
                }

                self.nd_discovery_sweep(verbosity, &output).await?;
            }
        }

        self.nd_discovery_sweep(verbosity, &output).await?;
        self.capture_discoveries = false;

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
        self.collect_statistics = false;

        if !self.interesting.is_empty() && !replay_aligned && shrink_phase {
            log_phase("Shrink", "Start");
            if verbosity == Verbosity::Debug {
                let total: usize = self.interesting.values().map(|n| n.len()).sum();
                output.line(&format!(
                    "Shrinking: {} origin(s), initial total length = {}",
                    self.interesting.len(),
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
                        .interesting
                        .values()
                        .map(|nodes| {
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
                            self.test_function(ntc).await?;
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

            let shrink_deadline = crate::sys::Instant::now().map(|now| now + shrink_budget);
            let mut shrink_timed_out = false;
            let mut shrunk_origins: crate::native::HashSet<String> =
                crate::native::HashSet::default();
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
                let total: usize = self.interesting.values().map(|n| n.len()).sum();
                output.line(&format!(
                    "Shrinking complete: {} origin(s), final total length = {}",
                    self.interesting.len(),
                    total
                ));
            }
            log_phase("Shrink", "End");
        } else if replay_aligned && verbosity == Verbosity::Debug {
            output.line("Skipping shrink: reused aligned database replay");
        }

        self.final_replay().await?;

        if let (Some(db), Some(key)) = (self.db(), database_key) {
            let key_bytes = key.as_bytes();
            let secondary_key = crate::native::database::sub_key(key_bytes, b"secondary");
            let new_entries: crate::native::HashSet<Vec<u8>> = if self.nd_handling() {
                let persistable: Vec<(String, Vec<ChoiceValue>)> = self
                    .interesting
                    .iter()
                    .filter(|(o, _)| !self.nd_origins.needs_confirmation(o))
                    .map(|(o, nodes)| (o.clone(), nodes.iter().map(|n| n.value()).collect()))
                    .collect();
                persistable
                    .into_iter()
                    .map(|(origin, choices)| {
                        let state = self.nd_state_for(&origin, choices);
                        crate::native::blob::encode_nd_state(&state)
                    })
                    .collect()
            } else {
                self.interesting
                    .values()
                    .map(|nodes| {
                        let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
                        serialize_choices(&choices)
                    })
                    .collect()
            };
            let primary_now = db.fetch(key_bytes);
            for old in primary_now {
                if new_entries.contains(&old) {
                    continue;
                }
                if self.persister.saved_this_run.contains(&old) {
                    db.delete(key_bytes, &old);
                } else {
                    db.move_value(key_bytes, &secondary_key, &old);
                }
            }
            for new_bytes in &new_entries {
                db.save(key_bytes, new_bytes);
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
                self.interesting.len()
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
        let mut origins_sorted: Vec<(String, Vec<ChoiceNode>)> =
            core::mem::take(&mut self.interesting)
                .into_iter()
                .filter(|(origin, _)| !nd_blobs || !self.nd_origins.needs_confirmation(origin))
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
                    self.nd_origins.caveat(&origin),
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
            let unconfirmed: Vec<String> =
                self.nd_origins.unconfirmed().map(str::to_string).collect();
            for origin in unconfirmed {
                failures.push(Failure {
                    caveat: self.nd_origins.caveat(&origin),
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

/// The stored-timeline set for one origin: the incumbent first, then
/// deduplicated pool entries, capped at [`nd::POOL_CAP`] timelines in
/// total, incumbent included. Every pool the engine stores, persists, or
/// replays is built here, so the cap comparison is written once.
fn pooled_timelines(
    incumbent: Vec<ChoiceValue>,
    rest: impl IntoIterator<Item = Vec<ChoiceValue>>,
) -> Vec<Vec<ChoiceValue>> {
    let mut timelines = Vec::from([incumbent]);
    for timeline in rest {
        if timelines.len() < nd::POOL_CAP && !timelines.contains(&timeline) {
            timelines.push(timeline);
        }
    }
    timelines
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
/// strike. The run-start primary entry is left in place, and end-of-run
/// reconciliation demotes it (decision 11's strike one), using
/// `saved_this_run` to tell it from same-run leftovers.
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
}

impl<'a> Persister<'a> {
    fn new(db: Option<Box<dyn TestCaseDatabase>>, database_key: Option<&'a str>) -> Self {
        Persister {
            db,
            database_key,
            last_saved: HashMap::default(),
            saved_this_run: crate::native::HashSet::default(),
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

    fn record_bytes(&mut self, origin: &str, nodes: &[ChoiceNode], new_bytes: Vec<u8>) {
        let Some(db) = self.db.as_deref() else { return };
        let Some(key) = self.database_key else { return };
        let key_bytes = key.as_bytes();

        let needs_save = match self.last_saved.get(origin) {
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
                db.delete(key_bytes, prev_bytes);
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
/// test cases through, the RNG, the example database (via the [`Persister`]), the choice
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
    exchange: &'a CaseExchange,
    rng: EngineRng,
    persister: Persister<'a>,
    pub(crate) tree_root: crate::native::data_tree::DataTreeNode,
    /// Per-origin tracking: each distinct panic site (file:line:col captured
    /// by [`crate::run_lifecycle::run_test_case`]) gets its own shrunk
    /// counterexample. This is what makes a single test that fails with
    /// several distinct bugs surface each one.
    pub(crate) interesting: HashMap<String, Vec<ChoiceNode>>,
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
    /// behavior — a choice-tree mismatch, a verify status/origin flake, or
    /// a concurrent state machine — or `Settings::nd_force` started it
    /// flipped. While set, the run trusts no cached prediction: data-tree
    /// recording, tree-served replays, novel-prefix generation, and
    /// targeting are all off. Never cleared within a run.
    pub(crate) nd_active: bool,
    /// Sticky flag for the concurrency subset of `nd_active`, flipped by
    /// the first executed test case that creates a state machine with
    /// `max_concurrency > 1` (see
    /// [`crate::native::core::FamilyCore::concurrent_machine`]). Declared
    /// concurrency enters nondeterministic handling unconditionally — the
    /// user asked for real threads, so even `error` strictness handles the
    /// resulting nondeterminism rather than aborting on it.
    pub(crate) concurrent: bool,
    /// Per-origin confirmation lifecycle under ND handling: admission,
    /// trust, confirmation state (anchor/witness/pool), and the caveated
    /// unconfirmed report. See [`OriginLifecycle`].
    nd_origins: OriginLifecycle,
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
            tree_root: crate::native::data_tree::DataTreeNode::default(),
            interesting: HashMap::default(),
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
            concurrent: false,
            nd_origins: OriginLifecycle::default(),
            capture_replays: false,
            capture_discoveries: false,
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
                .interesting
                .iter()
                .map(|(origin, nodes)| {
                    (origin.clone(), nodes.iter().map(|n| n.value()).collect())
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
        if self.settings.nondeterminism_strictness == NondeterminismStrictness::Warn
            && self.settings.verbosity != Verbosity::Quiet
        {
            self.settings.output.line(nondeterminism_notice());
        }
    }

    /// One measurement replay of `timeline` with the standard continuation
    /// budget: reports whether the run reproduced `origin` (any interesting
    /// origin when `None`), the realized timeline, and the verbatim-
    /// watermark weight of a miss ([`nd::verbatim_weight`]). A choice-tree
    /// mismatch aborts under `Error` strictness like any other execution.
    async fn nd_replay_once(
        &mut self,
        timeline: &[ChoiceValue],
        origin: Option<&str>,
    ) -> Result<NdReplayOnce, RunError> {
        let budget = nd::continuation_budget(crate::native::core::flattened_values_len(timeline));
        let ntc = NativeTestCase::for_probe(timeline, self.rng.spawn(), budget)?;
        let (run, mismatch) = self.measure(ntc).await?;
        if let Some(msg) = mismatch {
            return Err(RunError::NonDeterministic(msg));
        }
        let realized: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
        let failed = run.status == Status::Interesting
            && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
        let weight = if failed {
            1.0
        } else {
            nd::verbatim_weight(timeline, &realized)
        };
        #[cfg(feature = "__bench")]
        nd::watermark_dump::record(timeline, &realized, weight, failed);
        Ok(NdReplayOnce {
            run,
            realized,
            failed,
            weight,
        })
    }

    /// Replay-until-failure over stored ND state (decision 25): each
    /// timeline first-fit under a weighted per-timeline budget (physical
    /// cap at twice that), then positional splices of random timeline
    /// pairs, then up to `fresh` fresh generations. Returns the first
    /// reproducing run plus the evidence accumulated across every attempt,
    /// for the caller's hygiene verdict; fresh misses carry no weight
    /// (they say nothing about the stored timelines).
    async fn nd_reproduce(
        &mut self,
        origin: Option<&str>,
        timelines: &[Vec<ChoiceValue>],
        per_timeline_budget: f64,
        splices: u64,
        fresh: u64,
    ) -> Result<(Option<RunResult>, nd::Evidence), RunError> {
        let mut evidence = nd::Evidence::default();
        let physical_cap = libm::ceil(2.0 * per_timeline_budget) as u64;
        for timeline in timelines {
            let mut weighted = 0.0f64;
            let mut physical = 0u64;
            while weighted < per_timeline_budget && physical < physical_cap {
                let replay = self.nd_replay_once(timeline, origin).await?;
                evidence.record(replay.failed, replay.weight);
                if replay.failed {
                    return Ok((Some(replay.run), evidence));
                }
                weighted += replay.weight;
                physical += 1;
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
                evidence.record(replay.failed, replay.weight);
                if replay.failed {
                    return Ok((Some(replay.run), evidence));
                }
            }
        }
        for _ in 0..fresh {
            let ntc = NativeTestCase::new_random(self.rng.spawn())?;
            let (run, mismatch) = self.measure(ntc).await?;
            if let Some(msg) = mismatch {
                return Err(RunError::NonDeterministic(msg));
            }
            let failed = run.status == Status::Interesting
                && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
            evidence.record(failed, 0.0);
            if failed {
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
    /// lands in the lifecycle: a reproducing replay confirms a
    /// yet-unconfirmed origin; a dry confirmed origin switches its
    /// caveat's wording instead of unreporting the failure (decision 3); a
    /// dry unconfirmed origin is evicted like a bar reject and reaches the
    /// report only through the caveat-only fallback (decision 24).
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
            let (verify, mismatch) = self.test_function(verify_ntc).await?;
            if let Some(msg) = mismatch {
                return Err(RunError::NonDeterministic(msg));
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
        } else if let Some((witness, anchor)) = self.nd_origins.take_witness(&origin) {
            probe_anchor = anchor;
            witness
        } else if !self.nd_origins.needs_confirmation(&origin) {
            // Trusted: already admitted (decision 24), so any
            // failure in the batch promotes with the batch's LCB
            // as anchor — the bar arithmetic is only the stopping
            // rule.
            let batch = self.nd_evidence_batch(&origin, &choices).await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if let Some(witness) = batch.witness {
                probe_anchor = batch.evidence.lower_bound();
                let stored = self.nd_origins.pool(&origin).to_vec();
                let pool =
                    pooled_timelines(choices.clone(), batch.captured.into_iter().chain(stored));
                self.nd_origins
                    .confirm(&origin, probe_anchor, None, pool, evidence)?;
                self.record_nd_incumbent(&origin, &initial);
                witness
            } else {
                self.nd_origins.record_trusted_batch(&origin, evidence);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
        } else {
            let batch = self.nd_evidence_batch(&origin, &choices).await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if !batch.bar_accepted {
                if self.nd_origins.reject(&origin, evidence) {
                    #[cfg(feature = "__bench")]
                    nd::seam_dump::record(nd::seam_dump::SeamEvent::Evict {
                        origin: origin.clone(),
                        values: choices.clone(),
                        at_final_replay: false,
                    });
                    self.interesting.remove(&origin);
                }
                shrunk_origins.insert(origin);
                return Ok(false);
            }
            let witness = crate::control::hegel_internal_unwrap!(
                batch.witness,
                "nd_evidence_batch: bar accept without a witness for {origin}"
            );
            probe_anchor = batch.evidence.lower_bound();
            let pool = pooled_timelines(choices.clone(), batch.captured);
            self.nd_origins
                .confirm(&origin, probe_anchor, None, pool, evidence)?;
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
            self.interesting.insert(origin, initial);
        } else {
            self.interesting.insert(origin.clone(), shrunk);
            shrunk_origins.insert(origin);
        }
        Ok(timed_out)
    }

    async fn final_replay(&mut self) -> Result<(), RunError> {
        if self.interesting.is_empty() {
            return Ok(());
        }
        let mut origins: Vec<String> = self.interesting.keys().cloned().collect();
        origins.sort();
        for origin in origins {
            let nodes = self.interesting.get(&origin).cloned().unwrap_or_default();
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            if !self.nd_handling() {
                self.capture_replays = true;
                let ntc = NativeTestCase::for_choices(&choices, Some(&nodes), None);
                let (run, mismatch) = self.measure(ntc).await?;
                self.capture_replays = false;
                if let Some(msg) = mismatch {
                    return Err(RunError::NonDeterministic(msg));
                }
                if run.status == Status::Interesting
                    && run.origin.as_deref() == Some(origin.as_str())
                {
                    continue;
                }
                if self.settings.nondeterminism_strictness == NondeterminismStrictness::Error {
                    return Err(RunError::Flaky(flaky_diagnostic()));
                }
                #[cfg(feature = "__bench")]
                self.seam_flip(nd::seam_dump::FlipSite::FinalReplay);
                self.nd_flip();
            }
            let timelines = pooled_timelines(choices, self.nd_origins.pool(&origin).to_vec());
            self.capture_replays = true;
            let (_, evidence) = self
                .nd_reproduce(
                    Some(&origin),
                    &timelines,
                    nd::reuse_replay_budget() as f64 / timelines.len() as f64,
                    nd::REPRODUCE_SPLICES,
                    nd::FINAL_REPLAY_FRESH,
                )
                .await?;
            self.capture_replays = false;
            let batch = (evidence.fails(), evidence.runs());
            if self.nd_origins.needs_confirmation(&origin) {
                if batch.0 > 0 {
                    let confirmed = self.nd_origins.confirm(
                        &origin,
                        evidence.lower_bound(),
                        None,
                        timelines,
                        batch,
                    );
                    confirmed?;
                } else {
                    self.nd_origins.observe(&origin);
                    if self.nd_origins.reject(&origin, batch) {
                        #[cfg(feature = "__bench")]
                        nd::seam_dump::record(nd::seam_dump::SeamEvent::Evict {
                            origin: origin.clone(),
                            values: nodes.iter().map(|n| n.value()).collect(),
                            at_final_replay: true,
                        });
                        self.interesting.remove(&origin);
                    }
                }
            } else {
                self.nd_origins.record_final_replay(&origin, batch);
            }
        }
        Ok(())
    }

    /// One evidence batch: replay `choices` with capture-at-confirmation
    /// and divergence-weighted misses (decision 22) until the discovery
    /// bar ([`nd::discovery_bar`], decision 23) decides. Two uses: the
    /// bar's driver for admitting unconfirmed origins (experiment 005),
    /// and an evidence-gathering batch for trusted origins, where the bar
    /// arithmetic is only the stopping rule. The triggering run is
    /// selection, not evidence — only these fresh replays count. An accept
    /// extends to [`nd::ANCHOR_SEED_RUNS`] physical runs (decision 54), so
    /// the anchor a caller seeds from the batch is not biased by the bar's
    /// stopping rule; a reject stops at the bar.
    async fn nd_evidence_batch(
        &mut self,
        origin: &str,
        choices: &[ChoiceValue],
    ) -> Result<NdBatch, RunError> {
        let mut evidence = nd::Evidence::default();
        let mut witness = None;
        let mut captured: Vec<Vec<ChoiceValue>> = Vec::new();
        let capture_entry = self.capture_replays;
        self.capture_replays = true;
        let bar_accepted = loop {
            let replay = self.nd_replay_once(choices, Some(origin)).await?;
            evidence.record(replay.failed, replay.weight);
            if replay.failed {
                if captured.len() < nd::POOL_CAP && !captured.contains(&replay.realized) {
                    captured.push(replay.realized);
                }
                if witness.is_none() {
                    witness = Some(replay.run);
                }
            }
            match nd::discovery_bar(&evidence) {
                nd::BarVerdict::Accept => break true,
                nd::BarVerdict::Reject => break false,
                nd::BarVerdict::Continue => {}
            }
        };
        while bar_accepted && evidence.runs() < nd::ANCHOR_SEED_RUNS {
            let replay = self.nd_replay_once(choices, Some(origin)).await?;
            evidence.record(replay.failed, replay.weight);
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
        for timeline in self.nd_origins.pool(origin) {
            if candidates.len() < nd::BOOST_POOL && !candidates.contains(timeline) {
                candidates.push(timeline.clone());
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
            holdout.record(replay.failed, replay.weight);
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
                self.nd_origins.raise_anchor(origin, lcb);
                Some((witness, lcb))
            }
            _ => None,
        })
    }

    /// Experiment 005: confirm every interesting origin that hasn't passed
    /// the discovery bar yet. Swept after each generation iteration (and once
    /// after the loop) rather than keyed on the iteration's own run, because
    /// span-mutation and targeting executions also fill vacant origins.
    /// Loops because confirmation replays can themselves discover origins.
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
                .interesting
                .iter()
                .find(|(o, _)| self.nd_origins.needs_confirmation(o.as_str()))
                .map(|(o, n)| (o.clone(), n.clone()))
            else {
                return Ok(());
            };
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            let batch = self.nd_evidence_batch(&origin, &choices).await?;
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
                let confirmed = self.nd_origins.confirm(
                    &origin,
                    batch.evidence.lower_bound(),
                    batch.witness,
                    pool,
                    evidence,
                );
                confirmed?;
                self.record_nd_incumbent(&origin, &nodes);
            } else if self.nd_origins.reject(&origin, evidence) {
                self.interesting.remove(&origin);
            }
        }
    }

    fn db(&self) -> Option<&dyn TestCaseDatabase> {
        self.persister.db.as_deref()
    }

    /// The replay state persisted and emitted for `origin` with `incumbent`:
    /// the incumbent first, then its captured pool, with content-hash
    /// entropy (so identical state re-encodes identically across runs) and
    /// the standard continuation extension.
    fn nd_state_for(
        &self,
        origin: &str,
        incumbent: Vec<ChoiceValue>,
    ) -> crate::native::blob::NdReproState {
        let len = crate::native::core::flattened_values_len(&incumbent);
        let timelines = pooled_timelines(incumbent, self.nd_origins.pool(origin).to_vec());
        let mut content = Vec::new();
        for timeline in &timelines {
            content.extend_from_slice(&serialize_choices(timeline));
        }
        crate::native::blob::NdReproState {
            timelines,
            entropy: crate::native::database::fnv1a(&content),
            extension: (nd::continuation_budget(len) - len) as u32,
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
    /// the choice-tree non-determinism diagnostic, if recording the run's
    /// path contradicted an earlier run. `Err` means the driver violated
    /// the run contract (see [`NativeDataSource::take_outcome`]).
    pub(crate) async fn test_function(
        &mut self,
        ntc: NativeTestCase,
    ) -> Result<(RunResult, Option<String>), RunError> {
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
    ) -> Result<(RunResult, Option<String>), RunError> {
        self.test_function_tagged(ntc, true).await
    }

    async fn test_function_tagged(
        &mut self,
        mut ntc: NativeTestCase,
        measurement: bool,
    ) -> Result<(RunResult, Option<String>), RunError> {
        if self.capture_replays || (self.nd_active && self.capture_discoveries && !measurement) {
            ntc.set_should_capture();
        }
        let family = alloc::sync::Arc::clone(ntc.family());
        family.set_stateful_step_count(self.settings.stateful_step_count);
        let tc_start = crate::sys::Instant::now();
        let run = self.execute(ntc).await?;
        let elapsed = tc_start.map_or(core::time::Duration::ZERO, |start| start.elapsed());
        if !self.concurrent && family.concurrent_machine() {
            self.concurrent = true;
            #[cfg(feature = "__bench")]
            self.seam_flip(nd::seam_dump::FlipSite::Concurrency);
            self.nd_flip();
        }
        let mut mismatch = self.record_run(&run, elapsed, measurement);
        if mismatch.is_some()
            && self.settings.nondeterminism_strictness != NondeterminismStrictness::Error
        {
            #[cfg(feature = "__bench")]
            self.seam_flip(nd::seam_dump::FlipSite::TreeMismatch);
            self.nd_flip();
            mismatch = None;
        }
        Ok((run, mismatch))
    }

    /// Record one executed test case: the choice tree (losslessly — nodes,
    /// span events, and the full conclusion), counters, test time, triviality,
    /// the targeting observations (deterministic runs only — targeting is
    /// fully off under `nd_active`, decision 29), the per-origin interesting
    /// map (with its incremental database save), and the bug-window markers.
    ///
    /// Every execution feeds the tree, so a later replay of the same path is
    /// served by [`data_tree::simulate_full`] without re-running the body.
    ///
    fn record_run(
        &mut self,
        run: &RunResult,
        elapsed: core::time::Duration,
        measurement: bool,
    ) -> Option<String> {
        let mismatch = if self.nd_active {
            None
        } else {
            crate::native::data_tree::record_tree_full(
                &mut self.tree_root,
                &run.nodes,
                run.status,
                run.origin.as_deref(),
                &run.target_observations,
                &run.span_events,
                &[],
            )
        };
        if measurement {
            if self.nd_active {
                self.statistics
                    .record_measurement(run.status == Status::Interesting);
            }
        } else {
            self.calls += 1;
            self.total_test_time += elapsed;
            if run.nodes.is_empty() && run.status >= Status::Invalid {
                self.test_is_trivial = true;
            }
            if run.status >= Status::Valid && !self.nd_active && !run.target_observations.is_empty()
            {
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
                self.persister.record(&origin, &run.nodes);
                update_interesting(&mut self.interesting, origin, run.nodes.clone());
            } else if !self.interesting.contains_key(&origin) {
                self.nd_origins.observe(&origin);
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

    /// Execute one test case by offering it through the exchange, recording
    /// the trie and returning a [`RunResult`] populated from the outcome
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
        let span_events = NativeDataSource::take_span_events(&handle);
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
            span_events,
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
    /// A path the lossless tree already records completely is served by
    /// [`data_tree::simulate_full`] with its full outcome — nodes, spans,
    /// status, origin, observations — without running the body, for *any*
    /// status (interesting included). That holds for any `extend`: a
    /// tree-determined path concludes within `choices`, so the continuation
    /// budget is irrelevant to it. A *predicted overrun* — `choices` runs out
    /// on recorded territory where the tree still expects a draw — is served
    /// as `EarlyStop` when `extend == 0` (the bare replay would conclude
    /// exactly that without the body learning anything new); with a random
    /// continuation budget it is not predictive, so the run executes. A
    /// genuine miss (a novel path) runs through [`Self::test_function`] —
    /// bare when `extend == 0`, with up to `extend` random draws past the end
    /// of `choices` otherwise — which records the run into the tree so a
    /// later replay of the same path is served. There is no separate result
    /// cache: the tree is the single source of truth. Under nondeterministic
    /// handling nothing is served: identical choices need not produce
    /// identical outcomes, so every replay executes the body
    /// (`notes/experiments/002-cache-seam`).
    pub(crate) async fn cached_test_function(
        &mut self,
        choices: &[ChoiceValue],
        nodes: Option<&[ChoiceNode]>,
        extend: usize,
    ) -> Result<RunResult, RunError> {
        if !self.nd_active {
            if let Some(out) =
                crate::native::data_tree::simulate_full(&self.tree_root, choices, nodes)?
            {
                if out.status != Status::EarlyStop || extend == 0 {
                    return Ok(RunResult {
                        status: out.status,
                        nodes: out.nodes,
                        spans: out.spans,
                        origin: out.origin,
                        target_observations: out.target_observations,
                        span_events: Vec::new(),
                        events: Vec::new(),
                    });
                }
            }
        }
        let ntc = if extend == 0 {
            NativeTestCase::for_choices(choices, nodes, None)
        } else {
            let budget = crate::native::core::flattened_values_len(choices) + extend;
            NativeTestCase::for_probe(choices, self.rng_spawn(), budget)?
        };
        let (run, _mismatch) = self.test_function(ntc).await?;
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
    /// starting over.
    gauntlet: bool,
    /// Cumulative evidence per candidate, keyed by serialized realized
    /// choices — a candidate whose replay punned into another realization
    /// merges evidence with it, deliberately: the realized timeline is
    /// what the evidence is about, whatever proposal produced it.
    ledger: HashMap<Vec<u8>, nd::Evidence>,
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

impl EngineShrinkProbe<'_, '_> {
    fn matches(&self, run: &RunResult) -> bool {
        run.status == Status::Interesting
            && run.origin.as_deref() == Some(self.target_origin.as_str())
    }

    fn record_evidence(&mut self, key: &[u8], matched: bool, weight: f64) {
        self.ledger
            .entry(key.to_vec())
            .or_default()
            .record(matched, weight);
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
                .nd_origins
                .raise_anchor(&self.target_origin, accept.lower_bound);
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
            self.record_evidence(&key, matched, 1.0);
            if !matched && self.sweep == SweepMode::Fast {
                return Ok((false, run.nodes, Spans::from(run.spans)));
            }
            let mut accepted = false;
            loop {
                let evidence = *self.ledger.get(&key).unwrap();
                if !accepted {
                    match nd::gauntlet(&evidence, self.anchor) {
                        nd::GauntletVerdict::Accept => accepted = true,
                        nd::GauntletVerdict::Reject => {
                            return Ok((false, run.nodes, Spans::from(run.spans)));
                        }
                        nd::GauntletVerdict::Continue => {}
                    }
                }
                // The ledger is topped up to ANCHOR_SEED_RUNS before
                // the accept returns, so the anchor moves on a bound the
                // stopping rule didn't bias (decision 54).
                if accepted && evidence.runs() >= nd::ANCHOR_SEED_RUNS {
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
                self.record_evidence(&key, rerun.failed, rerun.weight);
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
/// [`Engine::cached_test_function`], so a proposed sequence whose path the
/// lossless choice tree already records costs no test-body execution — matching
/// Hypothesis, which routes mutations through `cached_test_function`. Each probe
/// that *does* execute is recorded into the tree through [`Self::record_run`],
/// so it counts toward the same budgets as a freshly generated example and a
/// later identical proposal is served from the tree; tree-served probes are not
/// re-recorded, exactly as Hypothesis's cache hits cost nothing.
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
