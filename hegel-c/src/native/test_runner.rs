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

use rand::RngExt;

use crate::backend::{Failure, RunError, TestCaseResult, TestRunResult};
use crate::control::InternalError;
use crate::exchange::CaseExchange;
use crate::native::core::{
    ChoiceNode, ChoiceValue, Divergence, MAX_SHRINKING_SECONDS, NativeTestCase, Span, Spans,
    Status, sort_key,
};
use crate::native::counterexample::{Counterexample, Counterexamples};
use crate::native::data_source::NativeDataSource;
#[cfg(not(target_family = "wasm"))]
use crate::native::database::DirectoryTestCaseDatabase;
use crate::native::database::{
    TestCaseDatabase, deserialize_choices, serialize_choices, serialize_nodes,
};
use crate::native::exec_cache::{ExecCache, KindLedger};
use crate::native::graph::{Graph, Run, Walked};
use crate::native::graph_shrink::{
    GraphProbe, GraphShrinker, Outcome as GraphOutcome, ProbeFuture as GraphProbeFuture,
};
use crate::native::nd;
use crate::native::rng::EngineRng;
use crate::native::shrinker::{ShrinkProbe, ShrinkRun, Shrinker, absorb_stop};
#[cfg(not(target_family = "wasm"))]
use crate::settings::Database;
use crate::settings::{
    Backend, HealthCheck, NondeterminismStrictness, Output, Phase, Settings, Verbosity,
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
    /// Where the replay first left its stored counterexample: `None` for a
    /// run every draw of which the stored state served —
    /// a run that stayed on its counterexample — and for fresh generation
    /// and cache hits.
    pub divergence: Option<Divergence>,
    /// Under a graph walk, the graph edges the run settled
    /// on, as `(node, edge index)`; empty otherwise.
    pub settled: Vec<(usize, usize)>,
    /// Under a graph walk, whether the run ended where the graph ends.
    pub ended: bool,
}

const RANDOM_GENERATION_BATCH: u64 = 10;

/// Stop generating after this many consecutive generation-phase cases whose
/// realized values had been executed before — the flat-cache replacement for
/// the tree's exhaustion stop, applied only while no valid case has
/// been generated. The stop exists so a tiny fully-filtered space reaches
/// the exhausted-space FilterTooMuch instead of grinding out the whole
/// invalid budget; once anything is valid the test-case budget bounds the
/// run, and a duplicate streak is routine mid-size-space behavior (at k of
/// S values seen, a streak of N duplicates has probability (k/S)^N, near 1
/// late in coupon collection — an unconditional stop would end a 32-way
/// `one_of` before reaching every alternative).
const DUPLICATE_STOP: u64 = RANDOM_GENERATION_BATCH;

/// Replays in the first-interesting determinism check, stop on first miss:
/// the discovering case is selection, not evidence, so all four replays
/// are fresh observations. Detection is
/// `1 - (p·s)^4` for a bug failing at rate `p` with seam survival `s`; a
/// deterministic origin pays exactly +4 executions.
const FIRST_CHECK_REPLAYS: u64 = 4;

/// Scan-replay cap for one backtrack: the geometric profile
/// plus binary refinement fit in ~2·log2(m) replays over the accept
/// segment, but the first pass also probes every raw sighting once, so a
/// raw-heavy history spends the cap on raws — the cap is the budget there,
/// not headroom.
const BACKTRACK_SCAN_REPLAYS: u64 = nd::CONFIRM_CAP;

const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Outcome of one measurement replay.
struct NdReplay {
    run: RunResult,
    failed: bool,
}

/// Outcome of one evidence batch: the replays, their
/// evidence, the first failing run, and the replayed graph with every
/// failing run grafted in.
struct NdBatch {
    /// Whether the discovery bar's arithmetic accepted. Decides admission
    /// for unconfirmed origins; for trusted origins the bar is only the
    /// batch's stopping rule and any failure is evidence enough. True only
    /// with `witness` set: an accept needs an in-batch reproduction.
    bar_accepted: bool,
    evidence: nd::Evidence,
    witness: Option<RunResult>,
    graph: Graph,
    /// The longest failing run `graph` holds, flattened.
    longest: usize,
}

/// What [`Engine::nd_reproduce`] replays: a counterexample graph with the
/// longest run it holds, or a bare choice sequence — a version-1 entry,
/// which carries no structure to walk.
enum ReproSource {
    Graph { graph: Arc<Graph>, longest: usize },
    Sequence(Vec<ChoiceValue>),
}

/// A database entry as the reuse phase decodes it: a version-1 choice
/// sequence or version-3 replay state.
enum StoredEntry {
    Choices(Vec<ChoiceValue>),
    Graph(crate::native::blob::NdReproState),
}

/// Outcome of one history backtrack.
enum Backtrack {
    /// A history entry cleared the discovery bar and is the origin's
    /// incumbent again; the origin is confirmed and the restored save
    /// superseded the barred one.
    Restored {
        nodes: Vec<ChoiceNode>,
        spans: Vec<Span>,
    },
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
/// independent of the reuse phase's two-strike hygiene.
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
/// the first failure. A nondeterministic blob walks its stored graph
/// through the same replay-until-failure sequence as database reuse, with
/// no fresh-generation tier: a fresh case could fail for a reason unrelated
/// to the blob. Every replay is stamped so the client captures the
/// reproducing execution's output and diagnostic.
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
            let mut rng = create_rng(settings, None);
            let budget =
                nd::continuation_budget(crate::native::core::flattened_values_len(&choices));
            let mut failures = Vec::new();
            for _ in 0..nd::V1_BLOB_REPLAYS {
                let mut ntc = NativeTestCase::for_probe(&choices, rng.spawn(), budget)?;
                ntc.set_should_capture();
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
                engine.nd_flip();
            }
            engine.capture_replays = true;
            let graph = Arc::new(state.graph);
            let longest = state.longest as usize;
            let source = ReproSource::Graph {
                graph: Arc::clone(&graph),
                longest,
            };
            let (run, evidence) = engine
                .nd_reproduce(None, &source, nd::reuse_replay_budget(), 0)
                .await?;
            let failures = match run.and_then(|run| run.origin) {
                Some(origin) => {
                    engine
                        .origins
                        .entry(&origin)
                        .trust(Some((graph, longest)), (evidence.fails(), evidence.runs()));
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

        if matches!(verbosity, Verbosity::Debug) {
            match &settings.config_path {
                Some(path) => output.line(&format!("loaded config: {path}")),
                None => output.line("no config file loaded"),
            }
        }

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
                    let entry = if let Some(stored_choices) = deserialize_choices(&raw) {
                        StoredEntry::Choices(stored_choices)
                    } else if let Some(state) = crate::native::blob::decode_nd_state(&raw) {
                        if self.settings.nondeterminism_strictness
                            != NondeterminismStrictness::Error
                        {
                            self.nd_flip();
                        }
                        StoredEntry::Graph(state)
                    } else {
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                        continue;
                    };
                    let nd_entry = matches!(entry, StoredEntry::Graph(_)) || self.nd_handling();
                    let (run, reuse_evidence, stored, aligned) = match entry {
                        StoredEntry::Choices(choices) if !nd_entry => {
                            let rng = self.rng.spawn();
                            let ntc =
                                NativeTestCase::for_probe(&choices, rng, self.choice_bound())?;
                            let (run, mismatch) = self.test_function(ntc).await?;
                            if let Some(err) = mismatch {
                                return Err(err);
                            }
                            let failed = run.status == Status::Interesting;
                            let aligned = realized_values(&run) == choices;
                            (failed.then_some(run), (u64::from(failed), 1), None, aligned)
                        }
                        entry => {
                            let (source, stored) = match entry {
                                StoredEntry::Choices(choices) => {
                                    (ReproSource::Sequence(choices), None)
                                }
                                StoredEntry::Graph(state) => {
                                    let graph = Arc::new(state.graph);
                                    let longest = state.longest as usize;
                                    (
                                        ReproSource::Graph {
                                            graph: Arc::clone(&graph),
                                            longest,
                                        },
                                        Some((graph, longest)),
                                    )
                                }
                            };
                            self.capture_replays = true;
                            self.reuse_replays = true;
                            let (run, evidence) = self
                                .nd_reproduce(None, &source, nd::reuse_replay_budget(), 0)
                                .await?;
                            self.reuse_replays = false;
                            self.capture_replays = false;
                            let aligned = run.as_ref().is_some_and(|run| match &source {
                                ReproSource::Sequence(choices) => realized_values(run) == *choices,
                                ReproSource::Graph { graph, .. } => {
                                    graph.walk_verdict(&run_of(run)) == Walked::Whole
                                }
                            });
                            (run, (evidence.fails(), evidence.runs()), stored, aligned)
                        }
                    };
                    if let Some(run) = run {
                        if let Some(o) = run.origin.as_deref() {
                            let trusted = self.origins.entry(o);
                            trusted.trust(stored.clone(), reuse_evidence);
                            trusted.mark_first_checked();
                        }
                        if i < primary_count {
                            found_interesting_in_primary = true;
                            if !aligned {
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
                .test_function(NativeTestCase::for_simplest(self.choice_bound())?)
                .await?;
            if let Some(err) = mismatch {
                return Err(err);
            }
            if let Some(msg) = large_initial_check(
                run.status == Status::EarlyStop,
                run.status,
                crate::native::core::flattened_len(&run.nodes),
                self.choice_bound(),
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
                let ntc =
                    NativeTestCase::new_random_with_params(case_rng, params, self.choice_bound());
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
                            // the remaining entries: one miss of a
                            // nondeterministic entry is not evidence that
                            // it no longer fails.
                            if self.nd_handling() {
                                break;
                            }
                        } else if crate::native::blob::decode_nd_state(&raw).is_some() {
                            // A v2 entry's hygiene lives in the reuse phase's
                            // budgeted strikes: a pre-shrink reproduction
                            // could change no outcome.
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
                    .get(&origin)
                    .and_then(|c| {
                        c.incumbent()
                            .map(|n| (n.to_vec(), c.incumbent_spans().to_vec()))
                    })
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
                    if counterexample.incumbent().is_some() {
                        let state = counterexample.repro_state()?;
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

    /// Assemble the run's failure report: blobs and replay-state caveats
    /// only for origins past
    /// confirmation — the same [`OriginLifecycle::needs_confirmation`]
    /// predicate the persistence filter uses — with the partition applied
    /// before the sort and the single-failure truncation, so a leaked
    /// unconfirmed origin can never displace a confirmed one. Unconfirmed
    /// origins (bar rejects and never-replayed report-time admissions
    /// alike) report caveat-only, and only when nothing confirmed or
    /// trusted survived.
    fn build_report(&mut self) -> Result<TestRunResult, InternalError> {
        let nd_blobs = self.nd_handling();
        let mut origins_sorted: Vec<(
            String,
            Vec<ChoiceNode>,
            Option<crate::native::blob::NdReproState>,
        )> = Vec::new();
        for (origin, c) in self.origins.iter_mut() {
            if nd_blobs && c.needs_confirmation() {
                continue;
            }
            let state = if nd_blobs && c.incumbent().is_some() {
                Some(c.repro_state()?)
            } else {
                None
            };
            if let Some(nodes) = c.evict() {
                origins_sorted.push((origin.to_string(), nodes, state));
            }
        }
        origins_sorted.sort_by(|a, b| sort_key(&b.1).cmp(&sort_key(&a.1)));

        if !self.settings.report_multiple_failures {
            if let Some(last) = origins_sorted.pop() {
                origins_sorted.clear();
                origins_sorted.push(last);
            }
        }

        let mut failures: Vec<Failure> = Vec::with_capacity(origins_sorted.len());
        for (origin, nodes, state) in origins_sorted {
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
            let (reproduce_blob, caveat) = if nd_blobs {
                let state = crate::control::hegel_internal_unwrap!(
                    state,
                    "build_report: {origin} has no replay state to encode"
                );
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
             successfully, while {overrun_test_cases} inputs overran the choice limit \
             during generation. Testing with inputs this large is slow and shrinks \
             poorly. Try reducing the amount of data generated, e.g. a smaller \
             min_size on collections like gs::vecs(). If this is expected, \
             suppress the check with \
             suppress_health_check = [HealthCheck::TestCasesTooLarge], which \
             also removes the limit on the size of a test case."
        ))
    } else {
        None
    }
}

/// Returns the `FailedHealthCheck: LargeInitialTestCase` message when the
/// smallest natural example either overran the choice bound or, while valid,
/// used more than half of it, unless the check is suppressed; otherwise
/// `None`. Mirrors Hypothesis's `large_base_example` health check.
pub(crate) fn large_initial_check(
    overran: bool,
    status: Status,
    node_count: usize,
    choice_bound: usize,
    suppressed: bool,
) -> Option<String> {
    if suppressed {
        return None;
    }
    let too_large =
        overran || (status == Status::Valid && node_count.saturating_mul(2) > choice_bound);
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
/// The first-interesting check's structural-miss diagnostic: names the
/// divergence position, richer than the tree's kind message. Used under
/// `error` strictness; quiet and warn flip instead.
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
/// Ctrl-C / SIGTERM mid-shrink loses nothing.
///
/// A superseded same-run save is deleted, never demoted: it never ended a
/// run as anyone's best example, so it earned no cross-run staleness
/// strike. A run-start primary entry (in `preexisting`) *did* end a run as
/// someone's best example, so superseding it demotes it to the secondary
/// key even when a reuse replay re-saved its
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
    /// superseded bytes keeps the primary key carrying the most recent
    /// validated incumbent at every instant, so an interrupted shrink
    /// loses nothing.
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
/// is what stops generation on an exhausted space.
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
    /// staleness, not nondeterminism.
    kind_ledger: KindLedger,
    /// Consecutive generation-phase conclusions whose realized values had
    /// been executed before. [`DUPLICATE_STOP`] of these ends generation
    /// while no valid case exists; a novel conclusion resets it. Frozen
    /// (at zero) under `nd_active`.
    pub(crate) consecutive_duplicates: u64,
    /// Per-origin tracking: each distinct panic site (file:line:col captured
    /// by [`crate::run_lifecycle::run_test_case`]) gets its own
    /// [`Counterexample`](crate::native::counterexample::Counterexample) —
    /// its incumbent, graph, standing, evidence, history, and budgets. This
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
    /// ([`Self::optimise_targets_nd`]). Never cleared within a run.
    pub(crate) nd_active: bool,
    /// Set while the first-interesting check's replays run: they count on
    /// the measurement statistics line despite running pre-flip, and a
    /// cache mismatch they trigger is the check's detection, not a
    /// generation flake.
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
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        settings: &'a Settings,
        database_key: Option<&'a str>,
        exchange: &'a CaseExchange,
    ) -> Result<Self, RunError> {
        crate::antithesis::check_environment()?;
        #[cfg(not(target_family = "wasm"))]
        let db: Option<Box<dyn TestCaseDatabase>> = match &settings.database {
            Database::Path(path) => Some(Box::new(DirectoryTestCaseDatabase::new(path))),
            Database::Unset => Some(Box::new(DirectoryTestCaseDatabase::new(".hegel/examples"))),
            Database::Disabled => None,
        };
        #[cfg(target_family = "wasm")]
        let db: Option<Box<dyn TestCaseDatabase>> = None;
        Ok(Engine {
            settings,
            database_key,
            exchange,
            rng: create_rng(settings, database_key),
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
        })
    }

    /// Whether the full nondeterministic pipeline — discovery confirmation,
    /// the shrink gauntlet, the graph, validated persistence, caveated
    /// reporting — is driving this run. Concurrent-machine runs flow
    /// through it like any other nondeterministic run.
    fn nd_handling(&self) -> bool {
        self.nd_active
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

    /// One measurement replay of `timeline` positionally, with the standard
    /// continuation budget: a version-1 entry or a boost mutant, sequences
    /// with no structure to walk.
    async fn nd_replay_once(
        &mut self,
        timeline: &[ChoiceValue],
        origin: Option<&str>,
    ) -> Result<NdReplay, RunError> {
        let budget = nd::continuation_budget(crate::native::core::flattened_values_len(timeline));
        let ntc = NativeTestCase::for_probe(timeline, self.rng.spawn(), budget)?;
        self.nd_measure(ntc, origin).await
    }

    /// One measurement replay of a counterexample graph as
    /// one test case, drawing at random past it up to `max_size` choices.
    async fn nd_replay_graph(
        &mut self,
        graph: Arc<Graph>,
        max_size: usize,
        origin: Option<&str>,
    ) -> Result<NdReplay, RunError> {
        let ntc = NativeTestCase::for_graph(graph, self.rng.spawn(), max_size)?;
        self.nd_measure(ntc, origin).await
    }

    /// Run one measurement replay: reports whether the run reproduced
    /// `origin` (any interesting origin when `None`). One Bernoulli trial
    /// of the test case, whatever the replay realized. A
    /// choice-tree mismatch aborts under `Error` strictness like any other
    /// execution.
    async fn nd_measure(
        &mut self,
        ntc: NativeTestCase,
        origin: Option<&str>,
    ) -> Result<NdReplay, RunError> {
        let (run, mismatch) = self.measure(ntc).await?;
        if let Some(divergence) = &run.divergence {
            if self.settings.verbosity == Verbosity::Debug {
                self.settings.output.line(&format!(
                    "replay left its counterexample at position {} of stream {:?}",
                    divergence.position, divergence.stream,
                ));
            }
        }
        if let Some(err) = mismatch {
            return Err(err);
        }
        let failed = run.status == Status::Interesting
            && origin.is_none_or(|o| run.origin.as_deref() == Some(o));
        Ok(NdReplay { run, failed })
    }

    /// Replay-until-failure over stored ND state: the
    /// counterexample as one test case, up to `attempts` times, then up to
    /// `fresh` fresh generations. Returns the first reproducing run plus
    /// the evidence accumulated across every attempt, for the caller's
    /// hygiene verdict; the fresh tier is a rescue, not a replay of the
    /// stored state, so only its failures enter the evidence.
    async fn nd_reproduce(
        &mut self,
        origin: Option<&str>,
        source: &ReproSource,
        attempts: u64,
        fresh: u64,
    ) -> Result<(Option<RunResult>, nd::Evidence), RunError> {
        let mut evidence = nd::Evidence::default();
        for _ in 0..attempts {
            let replay = match source {
                ReproSource::Graph { graph, longest } => {
                    self.nd_replay_graph(
                        Arc::clone(graph),
                        nd::continuation_budget(*longest),
                        origin,
                    )
                    .await?
                }
                ReproSource::Sequence(timeline) => self.nd_replay_once(timeline, origin).await?,
            };
            evidence.record(replay.failed);
            if replay.failed {
                return Ok((Some(replay.run), evidence));
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
    /// the graph, then [`nd::FINAL_REPLAY_FRESH`] fresh
    /// generations, up to the standard reuse budget — and the evidence
    /// lands in the lifecycle: a reproducing replay on a yet-unconfirmed
    /// origin is a sighting whose realized run then faces the standard bar
    /// on the origin's remaining attempts; a dry confirmed
    /// origin switches its caveat's wording instead of unreporting the
    /// failure; a dry unconfirmed origin is evicted like a
    /// bar reject and reaches the report only through the caveat-only
    /// fallback.
    /// One origin's shrink pass: the pre-shrink verify, admission (stashed
    /// witness, trusted batch, or the discovery bar), optional boost, and
    /// the shrinker run. A flip during the shrink probes requeues the
    /// origin once, from its verified pre-shrink nodes: single-run
    /// progress made before the flip was never checked against
    /// nondeterminism, and the second pass is gauntleted. Returns
    /// whether the shrinker hit the deadline. A method rather than shrink-
    /// loop code so report-time backtracking can re-enter a per-origin
    /// shrink. Under nondeterministic handling the shrink is
    /// the graph shrinker's, whose accepts install the moved
    /// counterexample as they happen ([`EngineGraphProbe`]); for a trusted
    /// origin the admission batch's bar arithmetic is only its stopping
    /// rule, since admission happened at reuse.
    async fn shrink_origin(
        &mut self,
        origin: String,
        initial: (Vec<ChoiceNode>, Vec<Span>),
        verbosity: Verbosity,
        output: &Output,
        shrink_deadline: Option<crate::sys::Instant>,
        shrunk_origins: &mut crate::native::HashSet<String>,
    ) -> Result<bool, RunError> {
        let (initial, initial_spans) = initial;
        let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value()).collect();
        let mut probe_anchor = 0.0f64;
        let deterministic_verify = if self.nd_handling() {
            None
        } else {
            let verify_ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
            let outcome = self.test_function(verify_ntc).await;
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
            let (graph, longest) = self.replay_source(&origin)?;
            let batch = self
                .nd_evidence_batch(&origin, graph, longest, None)
                .await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if let Some(witness) = batch.witness {
                probe_anchor = batch.evidence.lower_bound();
                self.confirm_batch(&origin, probe_anchor, batch.graph, batch.longest, evidence)?;
                witness
            } else {
                self.origins.entry(&origin).record_trusted_batch(evidence);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
        } else {
            if self.has_history(&origin) {
                match self.backtrack(&origin).await? {
                    Backtrack::Restored { .. } => return Ok(false),
                    Backtrack::Exhausted { evidence } => {
                        self.reject_origin(&origin, evidence);
                        shrunk_origins.insert(origin);
                        return Ok(false);
                    }
                }
            }
            if !self.origins.entry(&origin).spend_bar_attempt() {
                self.reject_origin(&origin, (0, 0));
                shrunk_origins.insert(origin);
                return Ok(false);
            }
            let (graph, longest) = self.replay_source(&origin)?;
            let batch = self
                .nd_evidence_batch(&origin, graph, longest, None)
                .await?;
            let evidence = (batch.evidence.fails(), batch.evidence.runs());
            if !batch.bar_accepted {
                self.reject_origin(&origin, evidence);
                shrunk_origins.insert(origin);
                return Ok(false);
            }
            let witness = crate::control::hegel_internal_unwrap!(
                batch.witness,
                "nd_evidence_batch: bar accept without a witness for {origin}"
            );
            probe_anchor = batch.evidence.lower_bound();
            self.confirm_batch(&origin, probe_anchor, batch.graph, batch.longest, evidence)?;
            witness
        };

        let mut verify = verify;
        if self.nd_handling() && probe_anchor < nd::BOOST_RELIABILITY_FLOOR {
            if let Some((witness, lcb)) = self.nd_boost(&origin, &verify, probe_anchor).await? {
                verify = witness;
                probe_anchor = lcb;
            }
        }

        let timed_out = if self.nd_handling() {
            let (graph, longest) = self.replay_source(&origin)?;
            let graph = graph.with_run(&run_of(&verify));
            if verbosity == Verbosity::Debug {
                output.line(&format!(
                    "nd shrink start: origin={origin} edges={} anchor={probe_anchor:.3}",
                    graph.edge_count()
                ));
            }
            let mut shrinker = GraphShrinker::new(graph, verify.nodes, verify.spans, probe_anchor);
            shrinker.set_longest(longest);
            shrinker.deadline = shrink_deadline;
            let mut probe = EngineGraphProbe {
                engine: &mut *self,
                origin: origin.clone(),
                verbosity,
                output: output.clone(),
            };
            shrinker.shrink(&mut probe).await?;
            let anchor = self
                .origins
                .get(&origin)
                .and_then(Counterexample::anchor)
                .unwrap_or(probe_anchor);
            if verbosity == Verbosity::Debug {
                output.line(&format!(
                    "nd shrink done: origin={origin} edges={} anchor={anchor:.3} timed_out={}",
                    self.replay_source(&origin)?.0.edge_count(),
                    shrinker.timed_out
                ));
            }
            shrunk_origins.insert(origin);
            shrinker.timed_out
        } else {
            let (shrunk, shrunk_spans, timed_out) = {
                let probe = EngineShrinkProbe {
                    engine: &mut *self,
                    target_origin: origin.clone(),
                    verbosity,
                    output: output.clone(),
                };
                let mut shrinker =
                    Shrinker::with_probe(Box::new(probe), verify.nodes, Spans::from(verify.spans));
                shrinker.deadline = shrink_deadline;
                absorb_stop(shrinker.initial_coarse_reduction().await)?;
                if verbosity == Verbosity::Debug {
                    let output = output.clone();
                    shrinker.set_debug(move |msg| output.line(msg));
                }
                shrinker.shrink().await?;
                (
                    core::mem::take(&mut shrinker.current_nodes),
                    core::mem::take(&mut shrinker.current_spans).into_vec(),
                    shrinker.timed_out,
                )
            };
            if self.nd_handling() {
                self.origins.entry(&origin).replace(initial, initial_spans);
            } else {
                self.origins.entry(&origin).replace(shrunk, shrunk_spans);
                shrunk_origins.insert(origin);
            }
            timed_out
        };
        Ok(timed_out)
    }

    /// The engine-owned final replay: one exact replay per
    /// origin while the run is deterministic, the counterexample's
    /// replay-until-failure under ND handling. A deterministic miss flips
    /// the run; a never-confirmed origin with history then backtracks — a
    /// restored incumbent re-shrinks under the gauntlet on the
    /// shrink deadline's remaining budget before its replay, an exhausted
    /// backtrack rejects into the caveat-only report. The same backtrack
    /// runs when a never-confirmed origin's review itself comes up dry with
    /// history on record. Origins exactly replayed before a flip —
    /// a later origin's, or one detected inside their own successful
    /// replay — re-enter the queue for the review: their single replay
    /// predates what the run now knows. A reproducing review run on an
    /// unconfirmed origin is a sighting, not a confirmation: grafted into
    /// the origin's graph, it faces the standard bar on the origin's
    /// remaining attempt budget.
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
                let ntc = NativeTestCase::for_choices(&choices, Some(&nodes), None);
                let outcome = self.measure(ntc).await;
                self.capture_replays = false;
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
                    self.nd_flip();
                    if self.origins.needs_confirmation(&origin) && self.has_history(&origin) {
                        match self.backtrack(&origin).await? {
                            Backtrack::Restored { nodes, spans } => {
                                if reshrink {
                                    let mut shrunk = crate::native::HashSet::default();
                                    self.shrink_origin(
                                        origin.clone(),
                                        (nodes, spans),
                                        verbosity,
                                        output,
                                        shrink_deadline,
                                        &mut shrunk,
                                    )
                                    .await?;
                                }
                            }
                            Backtrack::Exhausted { evidence } => {
                                self.reject_origin(&origin, evidence);
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
            let (graph, longest) = self.replay_source(&origin)?;
            let source = ReproSource::Graph {
                graph: Arc::clone(&graph),
                longest,
            };
            self.capture_replays = true;
            let (reproduction, evidence) = self
                .nd_reproduce(
                    Some(&origin),
                    &source,
                    nd::reuse_replay_budget(),
                    nd::FINAL_REPLAY_FRESH,
                )
                .await?;
            self.capture_replays = false;
            let batch = (evidence.fails(), evidence.runs());
            if self.origins.needs_confirmation(&origin) {
                let mut confirmed = false;
                let mut review_evidence = (0, 0);
                if let Some(run) = reproduction {
                    if self.origins.entry(&origin).spend_bar_attempt() {
                        let reviewed_graph = Arc::new(graph.with_run(&run_of(&run)));
                        let reviewed_longest =
                            longest.max(crate::native::core::flattened_len(&run.nodes));
                        let review = self
                            .nd_evidence_batch(
                                &origin,
                                reviewed_graph,
                                reviewed_longest,
                                shrink_deadline,
                            )
                            .await?;
                        review_evidence = (review.evidence.fails(), review.evidence.runs());
                        if review.bar_accepted {
                            let reviewed = self.origins.entry(&origin);
                            let confirmed_origin = reviewed.confirm(
                                review.evidence.lower_bound(),
                                None,
                                review.graph,
                                review.longest,
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
                            Backtrack::Restored { nodes, spans } => {
                                if reshrink {
                                    let mut shrunk = crate::native::HashSet::default();
                                    self.shrink_origin(
                                        origin.clone(),
                                        (nodes, spans),
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
                    self.reject_origin(&origin, reject_evidence);
                }
            } else {
                self.origins.entry(&origin).record_final_replay(batch);
            }
        }
        Ok(())
    }

    /// One evidence batch: replay `graph` (with the continuation budget
    /// for `longest`) with capture-at-confirmation, each replay one plain
    /// trial of the test case, until the discovery bar
    /// ([`nd::discovery_bar`]) decides, starting from the
    /// origin's first-check seed when one exists. Two uses: the bar's
    /// driver for admitting unconfirmed origins, and an
    /// evidence-gathering batch for trusted origins, where the bar
    /// arithmetic is only the stopping rule. The triggering run is
    /// selection, not evidence — only these fresh replays count. An accept
    /// requires a reproducing replay in *this* batch as its witness: a
    /// first-check seed can carry the bar's whole failure quota, and a
    /// seeded quota with no in-batch reproduction rejects at
    /// [`nd::CONFIRM_CAP`] runs instead of confirming an origin the batch
    /// never saw fail. An accept extends to [`nd::ANCHOR_SEED_RUNS`] runs,
    /// so the anchor a caller seeds from the batch is not biased by the
    /// bar's stopping rule; a reject stops at the bar. An
    /// expired `deadline` (passed only by the final replay's review)
    /// rejects before the next replay — a batch cut short proves nothing;
    /// the accept extension runs unchecked, bounded by
    /// [`nd::ANCHOR_SEED_RUNS`]. Every failing run is grafted into the
    /// graph (unless foreign) and the next replay walks the grafted graph:
    /// the counterexample under confirmation is the one the batch will
    /// store, and a failure with many structures — each
    /// reproducing rarely from a single run — confirms as the batch learns
    /// them.
    async fn nd_evidence_batch(
        &mut self,
        origin: &str,
        graph: Arc<Graph>,
        longest: usize,
        deadline: Option<crate::sys::Instant>,
    ) -> Result<NdBatch, RunError> {
        let mut evidence = self.origins.entry(origin).take_seed().unwrap_or_default();
        let mut witness = None;
        let mut grafted = (*graph).clone();
        let mut grafted_longest = longest;
        let capture_entry = self.capture_replays;
        self.capture_replays = true;
        let mut current = graph;
        let mut accepted = false;
        let bar_accepted = loop {
            if accepted && evidence.runs() >= nd::ANCHOR_SEED_RUNS {
                break true;
            }
            if !accepted
                && deadline.is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d))
            {
                break false;
            }
            let replay = self
                .nd_replay_graph(
                    Arc::clone(&current),
                    nd::continuation_budget(grafted_longest),
                    Some(origin),
                )
                .await?;
            evidence.record(replay.failed);
            if replay.failed {
                if graft_failure(&mut grafted, &mut grafted_longest, &replay.run) {
                    current = Arc::new(grafted.clone());
                }
                if witness.is_none() {
                    witness = Some(replay.run);
                }
            }
            if accepted {
                continue;
            }
            match nd::discovery_bar(&evidence) {
                nd::BarVerdict::Accept => {
                    if witness.is_some() {
                        accepted = true;
                    } else if evidence.runs() >= nd::CONFIRM_CAP {
                        break false;
                    }
                }
                nd::BarVerdict::Reject => break false,
                nd::BarVerdict::Continue => {}
            }
        };
        self.capture_replays = capture_entry;
        Ok(NdBatch {
            bar_accepted,
            evidence,
            witness,
            graph: grafted,
            longest: grafted_longest,
        })
    }

    /// Backtrack over `origin`'s history for the reproduction boundary —
    /// the newest entry that still reproduces. Probes are single
    /// continuation-tolerant replays: the accept
    /// segment at geometric offsets from the newest plus its oldest entry
    /// and every raw sighting, then binary refinement between the newest
    /// reproducing probe and its nearest newer non-reproducing one, capped
    /// at [`BACKTRACK_SCAN_REPLAYS`] in total. The best candidate faces
    /// the full discovery bar, spending the origin's
    /// [`nd::BACKTRACK_BAR_ATTEMPTS`]-batch budget — held across
    /// backtracks of the same origin; a
    /// reject resumes the scan on the older side, and with no reproducing
    /// probe the remaining replay budget goes on a second pass before
    /// giving up. A cleared bar confirms the origin — witness and anchor
    /// from the batch's extension, the scan's other reproducing entries
    /// grafted into its graph — and the restored incumbent supersedes the
    /// barred shrunk save. Scan errors bias old: a too-old restore
    /// re-shrinks under the gauntlet, a too-new one anchors
    /// low or gets rejected. Every probe walks the entry's run as a graph.
    async fn backtrack(&mut self, origin: &str) -> Result<Backtrack, RunError> {
        let mut entries: Vec<(Arc<Graph>, usize, bool)> = Vec::new();
        let mut entry_values: Vec<Vec<ChoiceValue>> = Vec::new();
        let mut entry_keys: Vec<Vec<u8>> = Vec::new();
        if let Some(c) = self.origins.get(origin) {
            for e in c.history().entries() {
                let values: Vec<ChoiceValue> = e.nodes.iter().map(|n| n.value()).collect();
                entry_keys.push(serialize_executed_choices(&values)?);
                entry_values.push(values);
                entries.push((
                    Arc::new(Graph::from_run(&e.run())),
                    nd::continuation_budget(crate::native::core::flattened_len(&e.nodes)),
                    e.accept,
                ));
            }
        }
        let attempts_left = self
            .origins
            .get(origin)
            .is_some_and(|c| c.backtrack_attempts_left());
        if entries.is_empty() || !attempts_left {
            return Ok(Backtrack::Exhausted { evidence: (0, 0) });
        }
        let accepts: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].2).collect();
        let raws: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].2).collect();

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
            let replay = self
                .nd_replay_graph(Arc::clone(&entries[idx].0), entries[idx].1, Some(origin))
                .await?;
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
                        let replay = self
                            .nd_replay_graph(
                                Arc::clone(&entries[mid].0),
                                entries[mid].1,
                                Some(origin),
                            )
                            .await?;
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
                    let replay = self
                        .nd_replay_graph(Arc::clone(&entries[idx].0), entries[idx].1, Some(origin))
                        .await?;
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
            let (candidate_graph, candidate_len) = {
                let c = self.origins.entry(origin);
                let entry = c.history().entries().get(candidate);
                (
                    Arc::clone(&entries[candidate].0),
                    entry.map_or(0, |e| crate::native::core::flattened_len(&e.nodes)),
                )
            };
            let batch = self
                .nd_evidence_batch(origin, candidate_graph, candidate_len, None)
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
            let mut graph = batch.graph;
            let mut longest = batch.longest;
            let (nodes, spans) = self
                .origins
                .get(origin)
                .and_then(|c| c.history().entries().get(candidate))
                .map(|e| (e.nodes.clone(), e.spans.clone()))
                .unwrap_or_default();
            if let Some(c) = self.origins.get(origin) {
                for (i, e) in c.history().entries().iter().enumerate() {
                    if i != candidate && status[i] == Some(true) {
                        graft_run(&mut graph, &mut longest, &e.nodes, &e.spans);
                    }
                }
            }
            let confirmed = self.origins.entry(origin).confirm(
                anchor,
                Some(witness),
                graph,
                longest,
                (batch.evidence.fails(), batch.evidence.runs()),
            );
            confirmed?;
            let restored = self.origins.entry(origin);
            restored.replace(nodes.clone(), spans.clone());
            let state = restored.repro_state()?;
            self.persister.supersede_nd(origin, &nodes, &state)?;
            return Ok(Backtrack::Restored { nodes, spans });
        }
    }

    /// The boost phase — successive halving over the
    /// incumbent and probe mutants of it, scored by failure rate under
    /// budgeted replay. Returns a witness run and new anchor when the
    /// winner's holdout LCB beats the confirmation anchor (holdout because
    /// the in-race rate of a halving winner is selection-biased upward). A
    /// winner other than the incumbent is installed as the counterexample:
    /// its witness run grafted into the stored graph, or alone when foreign
    /// to it. Run before shrinking only when the anchor
    /// sits below [`nd::BOOST_RELIABILITY_FLOOR`].
    async fn nd_boost(
        &mut self,
        origin: &str,
        incumbent: &RunResult,
        anchor: f64,
    ) -> Result<Option<(RunResult, f64)>, RunError> {
        if self.settings.verbosity == Verbosity::Debug {
            self.settings.output.line(&format!(
                "nd boost: origin={origin} racing from anchor {anchor:.3}"
            ));
        }
        let incumbent = realized_values(incumbent);
        let mut candidates: Vec<Vec<ChoiceValue>> = Vec::from([incumbent.clone()]);
        let mut attempts = 0;
        while candidates.len() < nd::BOOST_POOL && attempts < nd::BOOST_POOL * 3 {
            attempts += 1;
            let cut = self.rng.random_range(0..=incumbent.len());
            let budget = crate::native::core::flattened_values_len(&incumbent) + 8;
            let ntc = NativeTestCase::for_probe(&incumbent[..cut], self.rng.spawn(), budget)?;
            let (run, _mismatch) = self.measure(ntc).await?;
            let realized = realized_values(&run);
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
                if winner == incumbent {
                    self.origins.entry(origin).raise_anchor(lcb);
                } else {
                    let run = run_of(&witness);
                    let (stored, longest) = self.replay_source(origin)?;
                    let witness_len = crate::native::core::flattened_len(&witness.nodes);
                    let longest = if stored.walk_verdict(&run) == Walked::Foreign {
                        witness_len
                    } else {
                        longest.max(witness_len)
                    };
                    self.origins.entry(origin).install(
                        stored.with_run(&run),
                        witness.nodes.clone(),
                        witness.spans.clone(),
                        lcb,
                        longest,
                    );
                    self.record_nd_incumbent(origin)?;
                }
                Some((witness, lcb))
            }
            _ => None,
        })
    }

    /// Targeting under ND handling: the
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

    /// The universal first-interesting determinism check: before anything
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
    /// admitted at shrink verify or final replay are checked there instead.
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

    /// Confirm every interesting origin that hasn't passed the discovery
    /// bar yet. Swept after each generation iteration (and once
    /// after the loop) rather than keyed on the iteration's own run, because
    /// span-mutation and targeting executions also fill vacant origins.
    /// Loops because confirmation replays can themselves discover origins.
    /// Each batch spends the origin's per-run bar budget; at
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
            let Some(origin) = self
                .origins
                .live()
                .find(|(o, _)| self.origins.needs_confirmation(o))
                .map(|(o, _)| o.to_string())
            else {
                return Ok(());
            };
            if !self.origins.entry(&origin).spend_bar_attempt() {
                if verbosity == Verbosity::Debug {
                    output.line(&format!(
                        "nd discovery confirm: origin={origin} out of bar attempts"
                    ));
                }
                self.reject_origin(&origin, (0, 0));
                continue;
            }
            let (graph, longest) = self.replay_source(&origin)?;
            let batch = self
                .nd_evidence_batch(&origin, graph, longest, None)
                .await?;
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
                let confirmed = self.origins.entry(&origin).confirm(
                    batch.evidence.lower_bound(),
                    batch.witness,
                    batch.graph,
                    batch.longest,
                    evidence,
                );
                confirmed?;
                self.record_nd_incumbent(&origin)?;
            } else {
                self.reject_origin(&origin, evidence);
            }
        }
    }

    fn db(&self) -> Option<&dyn TestCaseDatabase> {
        self.persister.db.as_deref()
    }

    /// The run's per-test-case choice bound (see [`Settings::unbounded_choices`]).
    pub(crate) fn choice_bound(&self) -> usize {
        self.settings.choice_bound()
    }

    /// The graph `origin` replays by and the flattened length of the
    /// longest failing run it holds ([`Counterexample::replay_graph`]).
    /// Every caller holds a live or confirmed origin, which has one.
    fn replay_source(&self, origin: &str) -> Result<(Arc<Graph>, usize), InternalError> {
        let counterexample = self.origins.get(origin);
        let graph = crate::control::hegel_internal_unwrap!(
            counterexample.and_then(Counterexample::replay_graph),
            "replay_source: {origin} holds no counterexample to replay"
        );
        Ok((graph, counterexample.map_or(0, Counterexample::longest)))
    }

    /// Whether `origin` has pre-flip history for a backtrack to scan.
    fn has_history(&self, origin: &str) -> bool {
        self.origins
            .get(origin)
            .is_some_and(|c| !c.history().is_empty())
    }

    /// The discovery bar rejected `origin` with `evidence`: record it and
    /// evict the incumbent unless the origin is trusted or confirmed
    /// ([`Counterexample::reject`]).
    fn reject_origin(&mut self, origin: &str, evidence: (u64, u64)) {
        self.origins.entry(origin).reject(evidence);
    }

    /// Confirm `origin` from an evidence batch's graph, longest run and
    /// physical evidence, without a witness of its own, and persist it.
    fn confirm_batch(
        &mut self,
        origin: &str,
        anchor: f64,
        graph: Graph,
        longest: usize,
        evidence: (u64, u64),
    ) -> Result<(), InternalError> {
        self.origins
            .entry(origin)
            .confirm(anchor, None, graph, longest, evidence)?;
        self.record_nd_incumbent(origin)
    }

    fn record_nd_incumbent(&mut self, origin: &str) -> Result<(), InternalError> {
        let counterexample = self.origins.entry(origin);
        let nodes = counterexample
            .incumbent()
            .map(<[ChoiceNode]>::to_vec)
            .unwrap_or_default();
        let state = counterexample.repro_state()?;
        self.persister.record_nd(origin, &nodes, &state)
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
        let tc_start = crate::sys::Instant::now();
        let run = self.execute(ntc).await?;
        let elapsed = tc_start.map_or(core::time::Duration::ZERO, |start| start.elapsed());
        let mut mismatch = self.record_run(&run, elapsed, measurement)?;
        if mismatch.is_some()
            && self.settings.nondeterminism_strictness != NondeterminismStrictness::Error
        {
            self.nd_flip();
            mismatch = None;
        }
        Ok((run, mismatch))
    }

    /// Record one executed test case: the execution cache and kind ledger
    /// (via [`Self::record_execution`]), counters, test time, triviality,
    /// the targeting observations (generation runs only; under `nd_active`
    /// they are selection-biased seed material for the measured race), the
    /// per-origin interesting
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
                    let accept = counterexample.adopt(run.nodes.clone(), run.spans.clone());
                    counterexample.record_sighting(&run.nodes, &run.spans, accept)?;
                }
            } else if self.origins.incumbent(&origin).is_none() {
                self.origins
                    .entry(&origin)
                    .adopt(run.nodes.clone(), run.spans.clone());
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
    /// and `Flaky` for a verdict change; the caller
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
        let settled = NativeDataSource::take_settled(&handle);
        let ended = NativeDataSource::take_ended(&handle);
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
            settled,
            ended,
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
    /// truncated-proposal overruns, pun resolution) are gone: measured over
    /// the suite, its serves were almost all exact repeats. Under
    /// nondeterministic handling nothing is served: identical choices need
    /// not produce identical outcomes, so every replay executes the body.
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
                    settled: Vec::new(),
                    ended: false,
                });
            }
        }
        let ntc = if extend == 0 {
            NativeTestCase::for_choices(choices, nodes, None)
        } else {
            let budget = crate::native::core::flattened_values_len(choices).saturating_add(extend);
            NativeTestCase::for_probe(choices, self.rng_spawn(), budget)?
        };
        let (run, mismatch) = self.test_function(ntc).await?;
        if let Some(err) = mismatch {
            return Err(err);
        }
        Ok(run)
    }
}

/// The realized values of a run.
fn realized_values(run: &RunResult) -> Vec<ChoiceValue> {
    run.nodes.iter().map(|n| n.value()).collect()
}

/// A run with the address of every draw, for the graph.
fn run_of(run: &RunResult) -> Run {
    Run::from_nodes(&run.nodes, &run.spans)
}

/// Graft a failing replay into `graph` (unless foreign to it) and stretch
/// `longest` to it.
fn graft_failure(graph: &mut Graph, longest: &mut usize, run: &RunResult) -> bool {
    graft_run(graph, longest, &run.nodes, &run.spans)
}

/// Graft a failing run — its nodes and the spans they were realized under
/// — into `graph` (unless foreign to it) and stretch `longest` to it,
/// returning whether the graph changed.
fn graft_run(graph: &mut Graph, longest: &mut usize, nodes: &[ChoiceNode], spans: &[Span]) -> bool {
    let grafted = graph.graft(&Run::from_nodes(nodes, spans));
    if grafted {
        *longest = (*longest).max(crate::native::core::flattened_len(nodes));
    }
    grafted
}

/// The engine side of the shrinker's [`ShrinkProbe`] for a deterministic
/// run: routes every requested run through [`Engine::cached_test_function`]
/// and reports whether the run reproduced the origin being shrunk. Borrows
/// the engine for the duration of the shrink, so the shrinker's executions
/// record into the engine's tree and counters like any other run. Under
/// nondeterministic handling the shrink runs through the graph shrinker
/// and [`EngineGraphProbe`] instead.
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

/// The engine side of the graph shrinker's [`GraphProbe`]:
/// every replay is a measurement run of the candidate graph, every charge
/// goes to the origin's gauntlet budget, and an accepted candidate is
/// installed as the origin's counterexample and persisted at once.
struct EngineGraphProbe<'e, 'a> {
    engine: &'e mut Engine<'a>,
    origin: String,
    verbosity: Verbosity,
    output: Output,
}

impl GraphProbe for EngineGraphProbe<'_, '_> {
    fn replay<'s>(&'s mut self, graph: Arc<Graph>, max_size: usize) -> GraphProbeFuture<'s> {
        Box::pin(async move {
            if self.verbosity == Verbosity::Verbose {
                self.output.line("Running test case");
            }
            let replay = self
                .engine
                .nd_replay_graph(graph, max_size, Some(&self.origin))
                .await?;
            let run = replay.run;
            Ok(GraphOutcome {
                failed: replay.failed,
                divergence: run.divergence.is_some(),
                ended: run.ended,
                settled: run.settled,
                nodes: run.nodes,
                spans: run.spans,
            })
        })
    }

    fn charge(&mut self, anchor: f64, drive: bool) -> u64 {
        self.engine
            .origins
            .entry(&self.origin)
            .gauntlet_spend
            .charge(&nd::Evidence::default(), anchor, drive, None)
    }

    fn adopted(
        &mut self,
        graph: &Graph,
        witness: (&[ChoiceNode], &[Span]),
        anchor: f64,
        longest: usize,
    ) -> Result<(), RunError> {
        if self.verbosity == Verbosity::Debug {
            self.output.line(&format!(
                "nd graph accept: origin={} edges={} anchor={anchor:.3}",
                self.origin,
                graph.edge_count()
            ));
        }
        self.engine.origins.entry(&self.origin).install(
            graph.clone(),
            witness.0.to_vec(),
            witness.1.to_vec(),
            anchor,
            longest,
        );
        self.engine.record_nd_incumbent(&self.origin)?;
        Ok(())
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
        let mut by_label: crate::native::HashMap<u64, crate::native::HashSet<(usize, usize)>> =
            crate::native::HashMap::default();
        for span in spans.iter() {
            by_label
                .entry(span.label)
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

            let extend = self
                .choice_bound()
                .saturating_sub(crate::native::core::flattened_values_len(&attempt));
            let run = self.cached_test_function(&attempt, None, extend).await?;
            if run.status == Status::Interesting {
                return Ok(());
            }
        }
        Ok(())
    }
}

fn create_rng(settings: &Settings, database_key: Option<&str>) -> EngineRng {
    if settings.backend == Backend::Urandom {
        return EngineRng::urandom();
    }
    if let Some(seed) = settings.seed {
        EngineRng::seeded(seed)
    } else if settings.derandomize {
        let key = database_key.unwrap_or("unnamed-test");
        EngineRng::seeded(crate::native::database::fnv1a(key.as_bytes()))
    } else {
        EngineRng::from_os()
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "../../tests/embedded/native/test_runner_tests.rs"]
mod tests;
