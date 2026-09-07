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
use crate::control::{InternalError, hegel_internal_unwrap};
use crate::exchange::CaseExchange;
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, MAX_SHRINKING_SECONDS, NativeTestCase, Span, Spans,
    Status, sort_key,
};
use crate::native::data_source::NativeDataSource;
#[cfg(not(target_family = "wasm"))]
use crate::native::database::DirectoryTestCaseDatabase;
use crate::native::database::{
    TestCaseDatabase, deserialize_choices, serialize_choices, serialize_nodes,
};
use crate::native::exec_cache::{ExecCache, KindLedger};
use crate::native::rng::EngineRng;
use crate::native::shrinker::{ShrinkProbe, ShrinkRun, Shrinker, absorb_stop};
#[cfg(not(target_family = "wasm"))]
use crate::settings::Database;
use crate::settings::{Backend, HealthCheck, Output, Phase, Settings, Verbosity};

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
/// the tree's exhaustion stop, applied only while no valid case has been
/// generated. The stop exists so a tiny fully-filtered space reaches the
/// exhausted-space FilterTooMuch instead of grinding out the whole invalid
/// budget; once anything is valid the test-case budget bounds the run, and a
/// duplicate streak is routine mid-size-space behavior (at k of S values
/// seen, a streak of N duplicates has probability (k/S)^N, near 1 late in
/// coupon collection — an unconditional stop would end a 32-way `one_of`
/// before reaching every alternative).
const DUPLICATE_STOP: u64 = RANDOM_GENERATION_BATCH;
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
/// reconciliation evicts the shortlex-largest above it.
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
                    let Some(stored_choices) = deserialize_choices(&raw) else {
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                        continue;
                    };
                    let ntc =
                        NativeTestCase::for_probe(&stored_choices, self.rng.spawn(), BUFFER_SIZE)?;
                    let (run, mismatch) = self.test_function(ntc).await?;
                    if let Some(err) = mismatch {
                        return Err(err);
                    }
                    if run.status == Status::Interesting {
                        if i < primary_count {
                            found_interesting_in_primary = true;
                            if run.nodes.len() != stored_choices.len()
                                || run
                                    .nodes
                                    .iter()
                                    .zip(&stored_choices)
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
                    } else {
                        if let Some(db) = self.db() {
                            db.delete(&key_bytes, &raw);
                            db.delete(&secondary_key, &raw);
                        }
                    }
                    if self.nondeterministic {
                        replay_aligned = false;
                        break;
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

        // The simplest-example probe counts against the test-case budget, so
        // a one-case budget skips it: the whole budget goes to the randomly
        // generated case, instead of every run executing only the
        // deterministic simplest case.
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

        while settings.phases.contains(&Phase::Generate)
            && !found_in_reuse
            && !self.test_is_trivial
            && self.valid_test_cases < max_test_cases
            && self.within_invalid_budget(invalid_budget)
            && !(self.valid_test_cases == 0 && self.consecutive_duplicates >= DUPLICATE_STOP)
            && should_generate_more(
                self.interesting.is_empty(),
                self.calls,
                self.first_bug_at,
                self.last_bug_at,
                shrink_phase && !self.nondeterministic,
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
                        self.interesting.is_empty(),
                        self.calls,
                        self.first_bug_at,
                        self.last_bug_at,
                        shrink_phase && !self.nondeterministic,
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

                if self.interesting.is_empty() {
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
                    && !self.nondeterministic
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

                if !self.nondeterministic
                    && run.status == Status::Valid
                    && (self.valid_test_cases >= HEALTH_CHECK_MAX_VALID
                        || !self.interesting.is_empty())
                {
                    self.try_span_mutation(&run.nodes, &run.spans).await?;
                }
            }
        }

        if self.test_is_trivial
            && self.valid_test_cases == 0
            && self.interesting.is_empty()
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
            && self.interesting.is_empty()
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

        if !self.interesting.is_empty() && !replay_aligned && shrink_phase && !self.nondeterministic
        {
            log_phase("Shrink", "Start");
            if verbosity == Verbosity::Debug {
                let total: usize = self.interesting.values().map(|n| n.len()).sum();
                output.line(&format!(
                    "Shrinking: {} origin(s), initial total length = {}",
                    self.interesting.len(),
                    total
                ));
            }
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
                    .map(|nodes| serialize_executed_nodes(nodes))
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
                        self.test_function(ntc).await?;
                    }
                    if let Some(db) = self.db() {
                        db.delete(&secondary_key, &raw);
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

                let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value()).collect();
                let verify_ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
                let (verify, mismatch) = self.test_function(verify_ntc).await?;
                if let Some(err) = mismatch {
                    return Err(err);
                }
                if verify.status != Status::Interesting
                    || verify.origin.as_deref() != Some(origin.as_str())
                {
                    return Err(RunError::Flaky(flaky_diagnostic()));
                }

                let initial_spans = Spans::from(verify.spans.clone());
                let shrunk = {
                    let probe = EngineShrinkProbe {
                        engine: &mut *self,
                        target_origin: origin.clone(),
                        verbosity,
                        output: output.clone(),
                    };
                    let mut shrinker =
                        Shrinker::with_probe(Box::new(probe), verify.nodes, initial_spans);
                    shrinker.deadline = shrink_deadline;
                    absorb_stop(shrinker.initial_coarse_reduction().await)?;
                    if verbosity == Verbosity::Debug {
                        let output = output.clone();
                        shrinker.set_debug(move |msg| output.line(msg));
                    }
                    shrinker.shrink().await?;
                    shrink_timed_out |= shrinker.timed_out;
                    shrinker.current_nodes
                };
                self.interesting.insert(origin.clone(), shrunk);
                shrunk_origins.insert(origin);
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

        self.reconcile_database()?;

        if verbosity == Verbosity::Debug {
            output.line(&format!(
                "Test done. interesting_test_cases={}",
                self.interesting.len()
            ));
        }

        let mut origins_sorted: Vec<(String, Vec<ChoiceNode>)> =
            core::mem::take(&mut self.interesting).into_iter().collect();
        origins_sorted.sort_by(|a, b| sort_key(&b.1).cmp(&sort_key(&a.1)));

        if !settings.report_multiple_failures {
            if let Some(last) = origins_sorted.pop() {
                origins_sorted.clear();
                origins_sorted.push(last);
            }
        }

        if settings.show_statistics {
            for line in self.statistics.render() {
                output.line(&line);
            }
        }

        let nondeterministic = self.nondeterministic;
        let mut failures = Vec::with_capacity(origins_sorted.len());
        for (origin, nodes) in origins_sorted {
            let reproduce_blob = if nondeterministic {
                None
            } else {
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value()).collect();
                Some(hegel_internal_unwrap!(
                    crate::native::blob::encode_failure(&choices),
                    "a failing test case's clone values nest deeper than MAX_CLONE_DEPTH"
                ))
            };
            failures.push(Failure {
                origin,
                reproduce_blob,
            });
        }
        Ok(TestRunResult {
            failures,
            nondeterministic,
        })
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

/// Notice emitted once, when the run first executes a test case that
/// created a state machine with `max_concurrency > 1` and the run flips
/// into nondeterministic mode (see [`Engine::nondeterministic`]).
/// Informational rather than a warning: the concurrency was asked for
/// explicitly, but the user should learn why their failure is reported
/// unshrunk and without a reproduce blob. Not printed inside Antithesis,
/// which is deterministic and does its own reproduction, so none of the
/// caveats apply there.
pub(crate) fn concurrent_machine_notice() -> &'static str {
    "Concurrent state machine detected: this run is nondeterministic, so failures \
     are reported from the execution that discovered them, without shrinking, \
     replay, database persistence, or a reproduce blob."
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
    Ok(hegel_internal_unwrap!(
        serialize_nodes(nodes),
        "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
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
/// key even when a reuse replay re-saved its bytes this run; end-of-run
/// reconciliation demotes the rest, using `saved_this_run` to tell
/// run-start entries from same-run leftovers. Bytes another origin's last
/// save still points at are never removed: entries are content-addressed,
/// so two origins can share one entry.
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
    /// origin are then deleted (or demoted, for a run-start entry).
    fn record(&mut self, origin: &str, nodes: &[ChoiceNode]) -> Result<(), InternalError> {
        let (Some(db), Some(key)) = (self.db.as_deref(), self.database_key) else {
            return Ok(());
        };
        let key_bytes = key.as_bytes();
        let new_bytes = serialize_executed_nodes(nodes)?;

        let needs_save = match self.last_saved.get(origin) {
            None => true,
            Some((prev, _)) => sort_key(nodes) < sort_key(prev),
        };
        if !needs_save {
            return Ok(());
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
        Ok(())
    }
}

/// The native engine — Hegel's analogue of Hypothesis's `ConjectureRunner`.
///
/// One object owns everything a test run touches: the exchange it offers
/// test cases through, the RNG, the example database (via the [`Persister`]),
/// the execution cache and kind ledger, the per-origin interesting map,
/// targeting observations, and all run-level counters.
///
/// Every execution records into the cache and ledger via
/// [`Self::record_run`]. [`Self::test_function`] is the raw
/// executor+recorder (generation goes straight through it — its duplicates
/// must execute, they are the stop signal); [`Self::cached_test_function`]
/// is the single replay chokepoint shared by generation-phase span mutation
/// and shrinking — it serves an exact repeat from the cache and otherwise
/// falls through to `test_function`. `cached_test_function` returns the
/// realised result; the interesting-origin filter is applied by its caller,
/// and bugs with new origins surface through the same
/// [`update_interesting`] path as generation.
pub(crate) struct Engine<'a> {
    settings: &'a Settings,
    database_key: Option<&'a str>,
    exchange: &'a CaseExchange,
    rng: EngineRng,
    persister: Persister<'a>,
    pub(crate) exec_cache: ExecCache,
    /// Generation-nondeterminism detector: within-run, cross-execution kind
    /// drift at a shared value prefix aborts with the tree's diagnostic.
    /// Never fed between runs: a stored entry that stops reproducing is
    /// staleness, not nondeterminism.
    kind_ledger: KindLedger,
    /// Consecutive generation-phase conclusions whose realized values had
    /// been executed before. [`DUPLICATE_STOP`] of these ends generation
    /// while no valid case exists; a novel conclusion resets it. Frozen (at
    /// zero) while the run is nondeterministic.
    pub(crate) consecutive_duplicates: u64,
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
    /// Sticky run-level nondeterminism flag, flipped by the first executed
    /// test case that creates a state machine with `max_concurrency > 1`
    /// (see [`crate::native::core::FamilyCore::concurrent_machine`]): the
    /// test asked for real concurrency, so nothing that assumes
    /// deterministic replay can be trusted. While set, the run skips
    /// execution-cache recording and serving (and with it the kind-ledger
    /// check and the duplicate stop), span mutation, targeting, the verify +
    /// shrink pass (so generation stops at the first bug), database
    /// persistence and reuse, and reproduce-blob emission — failures are
    /// reported faithfully from the execution that discovered them.
    pub(crate) nondeterministic: bool,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        settings: &'a Settings,
        database_key: Option<&'a str>,
        exchange: &'a CaseExchange,
    ) -> Result<Self, RunError> {
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
            rng: create_rng(settings, database_key)?,
            persister: Persister::new(db, database_key),
            exec_cache: ExecCache::default(),
            kind_ledger: KindLedger::default(),
            consecutive_duplicates: 0,
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
            nondeterministic: false,
        })
    }

    fn db(&self) -> Option<&dyn TestCaseDatabase> {
        self.persister.db.as_deref()
    }

    /// End-of-run database reconciliation: save every surviving failure's
    /// bytes, then dispatch each displaced primary entry by provenance —
    /// same-run leftovers are deleted, run-start entries demote to the
    /// secondary key — and evict the shortlex-largest secondary entries
    /// above [`SECONDARY_CORPUS_CAP`].
    fn reconcile_database(&self) -> Result<(), InternalError> {
        if let (false, Some(db), Some(key)) = (self.nondeterministic, self.db(), self.database_key)
        {
            let key_bytes = key.as_bytes();
            let secondary_key = crate::native::database::sub_key(key_bytes, b"secondary");
            let new_entries: crate::native::HashSet<Vec<u8>> = self
                .interesting
                .values()
                .map(|nodes| serialize_executed_nodes(nodes))
                .collect::<Result<_, _>>()?;
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

    /// Spawn an independent RNG from the engine's, for components (probes,
    /// replays) that need their own stream without perturbing the engine's
    /// trajectory.
    pub(crate) fn rng_spawn(&mut self) -> EngineRng {
        self.rng.spawn()
    }

    /// Execute one test case and record everything about its outcome —
    /// Hypothesis's `ConjectureRunner.test_function`. Returns the run plus
    /// the nondeterminism abort, if recording the run contradicted an
    /// earlier execution (kind drift or a verdict change — see
    /// [`Self::record_execution`]). `Err` means the driver violated the run
    /// contract (see [`NativeDataSource::take_outcome`]).
    pub(crate) async fn test_function(
        &mut self,
        mut ntc: NativeTestCase,
    ) -> Result<(RunResult, Option<RunError>), RunError> {
        if self.nondeterministic {
            ntc.set_nondeterministic();
        }
        let family = alloc::sync::Arc::clone(ntc.family());
        family.set_reject_concurrent_machine(!self.nondeterministic);
        let tc_start = crate::sys::Instant::now();
        let run = self.execute(ntc).await?;
        let elapsed = tc_start.map_or(core::time::Duration::ZERO, |start| start.elapsed());
        if !self.nondeterministic && family.concurrent_machine() {
            self.nondeterministic = true;
            self.exec_cache.clear();
            self.kind_ledger.clear();
            self.consecutive_duplicates = 0;
            if self.settings.verbosity != Verbosity::Quiet && !self.settings.in_antithesis {
                self.settings.output.line(concurrent_machine_notice());
            }
        }
        let mismatch = self.record_run(&run, elapsed)?;
        Ok((run, mismatch))
    }

    /// Record one executed test case: the execution cache and kind ledger
    /// (via [`Self::record_execution`]), counters, test time, triviality,
    /// the targeting observations, the per-origin interesting map (with its
    /// incremental database save), and the bug-window markers.
    fn record_run(
        &mut self,
        run: &RunResult,
        elapsed: core::time::Duration,
    ) -> Result<Option<RunError>, InternalError> {
        let mismatch = if self.nondeterministic {
            None
        } else {
            self.record_execution(run)?
        };
        self.calls += 1;
        self.total_test_time += elapsed;
        if run.nodes.is_empty() && run.status >= Status::Invalid && !self.nondeterministic {
            self.test_is_trivial = true;
        }
        if run.status >= Status::Valid && !run.target_observations.is_empty() {
            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
            self.targeting.record(&choices, &run.target_observations);
        }
        if self.collect_statistics && matches!(run.status, Status::Valid | Status::Interesting) {
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
                let origin = run.origin.clone().unwrap_or_default();
                if !self.nondeterministic {
                    self.persister.record(&origin, &run.nodes)?;
                }
                update_interesting(&mut self.interesting, origin, run.nodes.clone());
            }
        }
        Ok(mismatch)
    }

    /// Feed one executed run to the detectors the tree used to be: the kind
    /// ledger, then — for conclusions; an overrun concluded nothing — the
    /// execution cache, whose digest hit both drives the duplicate-stop
    /// counter (generation-window cases only) and, on a verdict change,
    /// reports the flake the tree could never see. The returned error is
    /// `NonDeterministic` for kind drift and `Flaky` for a verdict change.
    fn record_execution(&mut self, run: &RunResult) -> Result<Option<RunError>, InternalError> {
        if let Some(msg) = self.kind_ledger.observe(&run.nodes)? {
            return Ok(Some(RunError::NonDeterministic(msg)));
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
        if self.collect_statistics {
            if recorded.duplicate {
                self.consecutive_duplicates += 1;
            } else {
                self.consecutive_duplicates = 0;
            }
        }
        Ok(recorded
            .verdict_mismatch
            .then(|| RunError::Flaky(flaky_diagnostic())))
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
    /// serves were ≈ exact repeats (experiment 010). While the run is
    /// nondeterministic nothing is served: identical choices need not
    /// produce identical outcomes, so every replay executes the body.
    async fn cached_test_function(
        &mut self,
        choices: &[ChoiceValue],
        nodes: Option<&[ChoiceNode]>,
        extend: usize,
    ) -> Result<RunResult, RunError> {
        if !self.nondeterministic {
            let key = hegel_internal_unwrap!(
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
/// the shrink, so the shrinker's executions record into the engine's cache
/// and counters like any other run.
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
            let matches = run.status == Status::Interesting
                && run.origin.as_deref() == Some(self.target_origin.as_str());
            Ok((matches, run.nodes, Spans::from(run.spans)))
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

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "../../tests/embedded/native/test_runner_tests.rs"]
mod tests;
