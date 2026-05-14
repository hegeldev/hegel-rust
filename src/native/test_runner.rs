//! Native [`TestRunner`] implementation.
//!
//! `NativeTestRunner` plugs into the same [`crate::run_lifecycle::drive`]
//! pipeline as the server backend's `ServerTestRunner`.  The trait
//! method [`TestRunner::run`] is the engine driver: it owns the
//! database replay, generation, shrinking, and final-replay phases,
//! and uses the supplied `run_case` callback to actually execute each
//! test body.
//!
//! Inside, [`EngineCtx`] wraps the `run_case` callback together with
//! a shrink-result cache, exposing `run` / `run_shrink_with_origin` /
//! `run_probe_with_origin` / `run_final` so the surrounding shrinker
//! and span-mutation passes can drive replays.

use std::collections::{HashMap, hash_map::Entry};

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use crate::backend::{DataSource, Failure, TestCaseResult, TestRunResult, TestRunner};
use crate::native::core::{
    BUFFER_SIZE, ChoiceNode, ChoiceValue, NativeTestCase, Span, Status, sort_key,
};
use crate::native::data_source::NativeDataSource;
use crate::native::database::{
    DirectoryTestCaseDatabase, TestCaseDatabase, deserialize_choices, serialize_choices,
};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use crate::runner::{Database, HealthCheck, Mode, Phase, Settings, Verbosity};

/// One run's worth of results: status, the realised choice nodes and
/// spans, and (for `Status::Interesting`) the captured failure carrying
/// the rendered diagnostic and the opaque origin string identifying
/// *where* the panic happened.  The origin is supplied by
/// [`crate::run_lifecycle::run_test_case`] from the captured panic
/// `file:line:col`; per-origin shrinking and database storage key on it.
#[derive(Clone)]
pub struct RunResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub spans: Vec<Span>,
    pub origin: Option<String>,
    pub failure: Option<Failure>,
}

const RANDOM_GENERATION_BATCH: u64 = 10;
const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Maximum number of consecutive filtered (assume()-failed) test cases before
/// FilterTooMuch is reported. Mirrors Hypothesis's `max_invalid_draws`,
/// scaled up slightly to be less sensitive to mild filtering.
const FILTER_TOO_MUCH_THRESHOLD: u64 = 200;

/// Cumulative wall-clock threshold across the generation phase before
/// TooSlow fires.
const TOO_SLOW_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(1);

/// Health checks (TooSlow / FilterTooMuch) are evaluated only while the run
/// has fewer than this many valid examples on record.
const HEALTH_CHECK_MAX_VALID: u64 = 10;

/// Native backend's [`TestRunner`] implementation.
pub struct NativeTestRunner;

impl TestRunner for NativeTestRunner {
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool),
    ) -> TestRunResult {
        if settings.mode == Mode::SingleTestCase {
            return run_single(settings, run_case);
        }
        run_main(settings, database_key, run_case)
    }
}

/// Run a single test case (used by `Mode::SingleTestCase`).
fn run_single(
    settings: &Settings,
    run_case: &mut dyn FnMut(Box<dyn DataSource>, bool),
) -> TestRunResult {
    // Honour `settings.seed` / `settings.derandomize` here for the same
    // reason `run_main` does: callers (Antithesis runs especially) pass
    // a deterministic seed expecting `Mode::SingleTestCase` to replay
    // the same draws on every invocation. Without this, a `seed(Some(42))`
    // is silently ignored and each call produces fresh OS-random draws.
    let mut rng = create_rng(settings, None);
    let ntc = NativeTestCase::new_random(SmallRng::from_rng(&mut rng));
    let (data_source, handle) = NativeDataSource::new(ntc);
    run_case(Box::new(data_source), true);
    match NativeDataSource::take_outcome(&handle) {
        TestCaseResult::Interesting(failure) => TestRunResult {
            passed: false,
            failures: vec![failure],
        },
        _ => TestRunResult {
            passed: true,
            failures: Vec::new(),
        },
    }
}

/// The full multi-test-case engine: database replay, generation, shrinking,
/// final replay.
fn run_main(
    settings: &Settings,
    database_key: Option<&str>,
    run_case: &mut dyn FnMut(Box<dyn DataSource>, bool),
) -> TestRunResult {
    let mut rng = create_rng(settings, database_key);
    let max_examples = settings.test_cases;
    let verbosity = settings.verbosity;

    // `Database::Unset` is the non-CI default (set by `Settings::new` in
    // `src/runner.rs`); it means "the user didn't pick, so use the
    // sensible default." For parity with the server backend (which
    // forwards `Unset` and lets the server pick its own default) and
    // with upstream Hypothesis (whose `DirectoryBasedExampleDatabase`
    // defaults to `.hypothesis/examples` relative to cwd), the native
    // default is `.hegel/examples` relative to cwd. `Disabled` is the
    // explicit opt-out; `Path(p)` is the explicit choice.
    let db: Option<Box<dyn TestCaseDatabase>> = match &settings.database {
        Database::Path(p) => Some(Box::new(DirectoryTestCaseDatabase::new(p))),
        Database::Unset => Some(Box::new(DirectoryTestCaseDatabase::new(".hegel/examples"))),
        Database::Disabled => None,
    };

    let mut ctx = EngineCtx::new(run_case);

    // Local data tree used by the generation phase to drive `for_probe`
    // toward unexplored prefixes (mirrors `NativeConjectureRunner`'s
    // `DataTreeNode`).
    let mut tree_root = crate::native::data_tree::DataTreeNode::default();

    // Per-origin tracking: each distinct panic site (file:line:col captured
    // by [`crate::run_lifecycle::run_test_case`]) gets its own shrunk
    // counterexample. Mirrors Hypothesis's `interesting_examples` map and
    // is what makes a single test that fails with several distinct bugs
    // surface each one.
    let mut interesting: HashMap<String, Vec<ChoiceNode>> = HashMap::new();
    let mut valid_test_cases: u64 = 0;
    let mut calls: u64 = 0;
    let mut test_is_trivial = false;
    let mut invalid_calls: u64 = 0;
    let mut total_test_time = std::time::Duration::ZERO;
    let mut replay_aligned = false;

    // --- Database replay phase ---
    //
    // Mirrors `engine.py`'s reuse step: every stored value is replayed,
    // not just the first interesting one.  A test that previously
    // discovered N distinct bugs has N stored choice sequences in the
    // DB; each must be replayed so each bug's shrunk counterexample
    // re-surfaces in `interesting`.  Pre-A23 the loop broke on the
    // first interesting result, silently losing the rest.
    //
    // `replay_aligned` tracks whether *every* interesting replay's
    // realised choice sequence matches the stored prefix length —
    // when true the runner can skip the shrink phase because each
    // stored value is already minimal.  Any single divergence flips
    // it to false so the shrinker re-runs over the full set.
    if settings.phases.contains(&Phase::Reuse) {
        if let (Some(db_ref), Some(key)) = (&db, database_key) {
            let key_bytes = key.as_bytes();
            let mut values = db_ref.fetch(key_bytes);
            values.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            replay_aligned = !values.is_empty();
            for raw in values {
                let Some(stored_choices) = deserialize_choices(&raw) else {
                    db_ref.delete(key_bytes, &raw);
                    continue;
                };
                let ntc = NativeTestCase::for_choices(&stored_choices, None, None);
                let run = ctx.run(ntc);
                if run.status == Status::Interesting {
                    let origin = run.origin.unwrap_or_default();
                    if run.nodes.len() != stored_choices.len() {
                        replay_aligned = false;
                    }
                    update_interesting(&mut interesting, origin, run.nodes);
                } else {
                    // Non-interesting (or invalid) replay: the stored
                    // value no longer reproduces the bug, drop it.
                    db_ref.delete(key_bytes, &raw);
                }
            }
            if interesting.is_empty() {
                // No replay survived — fall back to the pre-replay
                // alignment state so the shrink phase decides based on
                // generation results instead.
                replay_aligned = false;
            }
        }
    }

    // --- Generation phase ---
    //
    // Mirrors `engine.py::should_generate_more`: pre-bug we run until
    // either the `max_examples` budget or the choice tree is exhausted;
    // post-bug we keep running for a bounded extra window so that a test
    // with multiple distinct failure origins surfaces all of them, not
    // just the first one to fire.
    let mut first_bug_at: Option<u64> = None;
    let mut last_bug_at: Option<u64> = None;
    let shrink_enabled = settings.phases.contains(&Phase::Shrink);
    while settings.phases.contains(&Phase::Generate)
        && !test_is_trivial
        && valid_test_cases < max_examples
        && calls < max_examples * 10
        && !tree_root.is_exhausted
        && should_generate_more(
            interesting.is_empty(),
            calls,
            first_bug_at,
            last_bug_at,
            shrink_enabled,
        )
    {
        for _ in 0..RANDOM_GENERATION_BATCH {
            if test_is_trivial
                || valid_test_cases >= max_examples
                || calls >= max_examples * 10
                || tree_root.is_exhausted
                || !should_generate_more(
                    interesting.is_empty(),
                    calls,
                    first_bug_at,
                    last_bug_at,
                    shrink_enabled,
                )
            {
                break;
            }

            let batch_rng = SmallRng::from_rng(&mut rng);
            let prefix = crate::native::data_tree::generate_novel_prefix(&tree_root, &mut rng);
            let ntc = if prefix.is_empty() {
                NativeTestCase::new_random(batch_rng)
            } else {
                NativeTestCase::for_probe(&prefix, batch_rng, BUFFER_SIZE)
            };
            if verbosity == Verbosity::Verbose {
                eprintln!("Running test case");
            }

            let tc_start = std::time::Instant::now();
            let run = ctx.run(ntc);
            crate::native::data_tree::record_tree(&mut tree_root, &run.nodes, run.status, &[]);
            let elapsed = tc_start.elapsed();
            calls += 1;

            if verbosity == Verbosity::Debug {
                eprintln!(
                    "test case #{calls}: status = {:?}, choices = {}",
                    run.status,
                    run.nodes.len()
                );
            }

            if run.status != Status::Invalid {
                total_test_time += elapsed;
            }
            if run.nodes.is_empty() && run.status >= Status::Invalid {
                test_is_trivial = true;
            }
            if run.status >= Status::Valid {
                valid_test_cases += 1;
            }

            if run.status == Status::Invalid {
                invalid_calls += 1;
                if invalid_calls >= FILTER_TOO_MUCH_THRESHOLD
                    && valid_test_cases == 0
                    && !settings
                        .suppress_health_check
                        .contains(&HealthCheck::FilterTooMuch)
                {
                    panic!(
                        "FailedHealthCheck: FilterTooMuch — it looks like this \
                         test is filtering out too many inputs. \
                         {invalid_calls} inputs were filtered out by assume() \
                         before any valid input was generated. \
                         If this is expected, suppress the check with \
                         suppress_health_check = [HealthCheck::FilterTooMuch]."
                    );
                }
            } else {
                invalid_calls = 0;
            }

            // The TooSlow guard short-circuits on the 1 s wall-clock
            // threshold, which the in-process test harness rarely
            // reaches; covering the `suppress_health_check` tail
            // requires deliberately stalling the test body.
            // nocov start
            if valid_test_cases < HEALTH_CHECK_MAX_VALID
                && total_test_time > TOO_SLOW_THRESHOLD
                && !settings
                    .suppress_health_check
                    .contains(&HealthCheck::TooSlow)
            {
                panic!(
                    "FailedHealthCheck: TooSlow — input generation is slow: \
                     only {valid_test_cases} valid inputs after {:?} (threshold \
                     {:?}). Slow generation makes property testing much less \
                     effective. If this is expected, suppress the check with \
                     suppress_health_check = [HealthCheck::TooSlow].",
                    total_test_time, TOO_SLOW_THRESHOLD
                );
            }
            // nocov end

            if run.status == Status::Interesting {
                let origin = run.origin.clone().unwrap_or_default();
                if first_bug_at.is_none() {
                    first_bug_at = Some(calls);
                }
                last_bug_at = Some(calls);
                update_interesting(&mut interesting, origin, run.nodes.clone());
            } else if run.status == Status::Valid {
                // Bump `calls` by the *actual* number of probes
                // `try_span_mutation` ran, not the maximum: when no labels
                // have ≥2 occurrences (or when the first probe fires
                // Interesting) the closure short-circuits below
                // `SPAN_MUTATION_ATTEMPTS`.
                let (mutation_result, mutation_attempts) =
                    try_span_mutation(&run.nodes, &run.spans, &mut rng, &mut ctx);
                calls += mutation_attempts as u64;
                if let Some((mut_nodes, origin)) = mutation_result {
                    if first_bug_at.is_none() {
                        first_bug_at = Some(calls);
                    }
                    last_bug_at = Some(calls);
                    update_interesting(&mut interesting, origin, mut_nodes);
                }
            }
        }
    }

    // Tree-exhaustion fallback: a small choice domain (e.g. integer in
    // [0, 10] = 11 children) can exhaust the tree well before
    // FILTER_TOO_MUCH_THRESHOLD invalid calls; re-fire the check here.
    if tree_root.is_exhausted
        && valid_test_cases == 0
        && interesting.is_empty()
        && !test_is_trivial
        && !settings
            .suppress_health_check
            .contains(&HealthCheck::FilterTooMuch)
        && invalid_calls > 0
    {
        panic!(
            "FailedHealthCheck: FilterTooMuch — every reachable input was \
             filtered out by assume() before any valid input was generated. \
             {invalid_calls} inputs were filtered out across the full search \
             space. If this is expected, suppress the check with \
             suppress_health_check = [HealthCheck::FilterTooMuch]."
        );
    }

    // --- Shrinking phase ---
    if !interesting.is_empty() && !replay_aligned && settings.phases.contains(&Phase::Shrink) {
        if verbosity == Verbosity::Debug {
            let total: usize = interesting.values().map(|n| n.len()).sum();
            eprintln!(
                "Shrinking: {} origin(s), initial total length = {}",
                interesting.len(),
                total
            );
        }
        let origins: Vec<String> = interesting.keys().cloned().collect();
        for origin in origins {
            let initial = interesting.get(&origin).cloned().unwrap_or_default();

            // Re-validate that this origin's example still fails. If not,
            // the test is flaky.
            let choices: Vec<ChoiceValue> = initial.iter().map(|n| n.value.clone()).collect();
            let verify_ntc = NativeTestCase::for_choices(&choices, Some(&initial), None);
            let verify = ctx.run(verify_ntc);
            if verify.status != Status::Interesting {
                panic!(
                    "Flaky test detected: Your test produced different outcomes \
                     when run with the same generated data — it failed when it \
                     previously succeeded, or succeeded when it previously failed. \
                     This usually means your test depends on external state such as \
                     global variables, system time, or external random number generators."
                );
            }

            let target_origin = origin.clone();
            let shrunk = {
                let mut shrinker = Shrinker::with_probe(
                    Box::new(|req: ShrinkRun| {
                        if verbosity == Verbosity::Verbose {
                            eprintln!("Running test case");
                        }
                        let result = match req {
                            ShrinkRun::Full(nodes) => {
                                ctx.run_shrink_with_origin(nodes, &target_origin)
                            }
                            ShrinkRun::Probe {
                                prefix,
                                seed,
                                max_size,
                            } => ctx.run_probe_with_origin(prefix, seed, max_size, &target_origin),
                        };
                        calls += 1;
                        result
                    }),
                    verify.nodes,
                );
                shrinker.shrink();
                shrinker.current_nodes
            };
            interesting.insert(origin, shrunk);
        }

        if verbosity == Verbosity::Debug {
            let total: usize = interesting.values().map(|n| n.len()).sum();
            eprintln!(
                "Shrinking complete: {} origin(s), final total length = {}",
                interesting.len(),
                total
            );
        }
    } else if interesting.is_empty() && verbosity == Verbosity::Debug {
        // No bug found — nothing to shrink; left for symmetry with the
        // ported `Test done.` line below.
    } else if replay_aligned && verbosity == Verbosity::Debug {
        eprintln!("Skipping shrink: reused aligned database replay");
    }

    // --- Save to database ---
    //
    // For each interesting origin, save the shrunk counterexample to
    // primary.  Any *displaced* primary entry — present at start of
    // run but no longer in `interesting` — moves to the
    // `<key>.secondary` sub-corpus rather than disappearing, mirroring
    // upstream's `engine.py::downgrade_choices` (lines 899-902).  The
    // secondary key is the historical fallback corpus the next reuse
    // pass consults if primary doesn't have enough entries.
    if let (Some(db_ref), Some(key)) = (&db, database_key) {
        let key_bytes = key.as_bytes();
        let secondary_key = crate::native::data_tree::sub_key(key_bytes, b"secondary");
        let new_entries: std::collections::HashSet<Vec<u8>> = interesting
            .values()
            .map(|nodes| {
                let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
                serialize_choices(&choices)
            })
            .collect();
        let primary_now = db_ref.fetch(key_bytes);
        for old in primary_now {
            if !new_entries.contains(&old) {
                db_ref.move_value(key_bytes, &secondary_key, &old);
            }
        }
        for new_bytes in &new_entries {
            db_ref.save(key_bytes, new_bytes);
        }
    }

    if verbosity == Verbosity::Debug {
        eprintln!("Test done. interesting_test_cases={}", interesting.len());
    }

    // --- Final replay ---
    //
    // Replay each origin's shrunk counterexample with `is_final = true` so
    // every distinct bug fires its panic through the user's test body in
    // its proper context (and side effects like `*shrunk.lock().unwrap() =
    // Some(...)` get captured per origin). Replay in shortlex-descending
    // order: the smallest counterexample is the one observed *last*, so a
    // user-side `Mutex<Option<…>>` that overwrites on each panic ends up
    // holding the simplest example. Each replay's `Failure` is appended to
    // the returned `TestRunResult::failures`, which `drive` then turns into
    // either the single-failure or multi-failure outer panic.
    let mut origins_sorted: Vec<(String, Vec<ChoiceNode>)> = interesting.into_iter().collect();
    origins_sorted.sort_by_key(|origin| std::cmp::Reverse(sort_key(&origin.1)));

    // Mirror Hypothesis's `report_multiple_bugs` setting: when `false`, drop
    // all but the smallest origin (the one observed *last* under the
    // shortlex-descending sort above), so the runner surfaces a single
    // failure rather than every distinct bug Hegel found.
    if !settings.report_multiple_failures {
        if let Some(last) = origins_sorted.pop() {
            origins_sorted.clear();
            origins_sorted.push(last);
        }
    }

    let mut failures: Vec<crate::backend::Failure> = Vec::new();
    for (_origin, nodes) in origins_sorted {
        let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        let ntc = NativeTestCase::for_choices(&choices, Some(&nodes), None);
        let run = ctx.run_final(ntc);

        match (run.status, run.failure) {
            (Status::Interesting, Some(failure)) => {
                failures.push(failure);
            }
            _ => flaky_final_replay_panic(), // nocov
        }
    }

    TestRunResult {
        passed: failures.is_empty(),
        failures,
    }
}

/// Mirrors `engine.py::should_generate_more`: pre-bug we always keep
/// generating; post-bug we keep going just long enough to surface other
/// distinct origins. The window comes from Hypothesis:
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

// nocov start
#[cold]
fn flaky_final_replay_panic() -> ! {
    panic!(
        "Flaky test detected: Your test produced different outcomes \
         when run with the same generated data — it failed when it \
         previously succeeded, or succeeded when it previously failed. \
         This usually means your test depends on external state such as \
         global variables, system time, or external random number generators."
    );
}
// nocov end

fn should_generate_more(
    no_bug_yet: bool,
    calls: u64,
    first_bug_at: Option<u64>,
    last_bug_at: Option<u64>,
    shrink_enabled: bool,
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
    if !shrink_enabled {
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

/// Wraps the cross-backend `run_case` callback together with the
/// non-determinism trie and the shrink-result cache, exposing the
/// `NativeRunner` surface the surrounding shrinker, span-mutation, and
/// targeting code expect.
///
/// `Settings::mode` does not need to be stored here: it is captured in
/// the `run_case` closure built by `run_lifecycle::drive` (which calls
/// `run_test_case(_, _, _, mode, _)` per invocation), so by the time
/// `run_case` reaches us the mode is already plumbed.
pub(crate) struct EngineCtx<'a> {
    run_case: &'a mut dyn FnMut(Box<dyn DataSource>, bool),
    cache: HashMap<Vec<ChoiceValue>, RunResult>,
}

impl<'a> EngineCtx<'a> {
    fn new(run_case: &'a mut dyn FnMut(Box<dyn DataSource>, bool)) -> Self {
        EngineCtx {
            run_case,
            cache: HashMap::new(),
        }
    }

    /// Execute one test case via `run_case`, recording the trie and
    /// returning a [`RunResult`] populated from the outcome reported by the
    /// data source's `mark_complete` plus the [`NativeTestCase`]'s realized
    /// choice nodes.
    fn execute(&mut self, ntc: NativeTestCase, is_final: bool) -> RunResult {
        let (data_source, handle) = NativeDataSource::new(ntc);
        (self.run_case)(Box::new(data_source), is_final);
        let nodes = NativeDataSource::take_nodes(&handle);
        let spans = NativeDataSource::take_spans(&handle);
        let tc_result = NativeDataSource::take_outcome(&handle);

        let (status, failure) = match tc_result {
            TestCaseResult::Valid => (Status::Valid, None),
            TestCaseResult::Invalid => (Status::Invalid, None),
            TestCaseResult::Overrun => (Status::Invalid, None),
            TestCaseResult::Interesting(f) => (Status::Interesting, Some(f)),
        };
        let origin = failure.as_ref().map(|f| f.origin.clone());

        RunResult {
            status,
            nodes,
            spans,
            origin,
            failure,
        }
    }

    /// Cache-aware shrink-time replay restricted to a specific origin: the
    /// shrinker probes a candidate, but a slip into a different bug's
    /// counterexample is rejected so each origin's shrunk minimum is its
    /// own bug, not whichever was found first.
    fn run_shrink_with_origin(
        &mut self,
        candidate_nodes: &[ChoiceNode],
        target_origin: &str,
    ) -> (bool, Vec<ChoiceNode>) {
        let key: Vec<ChoiceValue> = candidate_nodes.iter().map(|n| n.value.clone()).collect();
        if let Some(cached) = self.cache.get(&key) {
            let matches = cached.status == Status::Interesting
                && cached.origin.as_deref() == Some(target_origin);
            return (matches, cached.nodes.clone());
        }

        let ntc = NativeTestCase::for_choices(&key, Some(candidate_nodes), None);
        let run = self.execute(ntc, false);
        let matches =
            run.status == Status::Interesting && run.origin.as_deref() == Some(target_origin);
        self.cache.insert(key, run.clone());
        (matches, run.nodes)
    }

    fn run_probe_with_origin(
        &mut self,
        prefix: &[ChoiceValue],
        seed: u64,
        max_size: usize,
        target_origin: &str,
    ) -> (bool, Vec<ChoiceNode>) {
        let rng = SmallRng::seed_from_u64(seed);
        let ntc = NativeTestCase::for_probe(prefix, rng, max_size);
        let run = self.execute(ntc, false);
        let matches =
            run.status == Status::Interesting && run.origin.as_deref() == Some(target_origin);
        let key: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value.clone()).collect();
        self.cache.insert(key, run.clone());
        (matches, run.nodes)
    }

    /// Replay the shrunk counterexample one last time with `is_final = true`,
    /// so the panic hook prints location/backtrace/message and the surrounding
    /// driver can re-raise.
    fn run_final(&mut self, ntc: NativeTestCase) -> RunResult {
        self.execute(ntc, true)
    }

    /// Run one test case as part of the search loop (not a final replay).
    fn run(&mut self, ntc: NativeTestCase) -> RunResult {
        self.execute(ntc, false)
    }
}

/// Try span mutation: find two spans with the same label and either duplicate
/// the parent's prefix (when one contains the other, e.g. recursive tree
/// structures) or replace both with identical choices from one donor.
///
/// Port of Hypothesis's `generate_mutations_from`. Returns the mutated
/// shrunk nodes plus the panic origin if the attempt produced an
/// interesting result.
/// Runs up to [`SPAN_MUTATION_ATTEMPTS`] span-mutation probes via `ctx`.
/// Returns the mutated counterexample (if one was found) plus the number of
/// `ctx.execute` calls that actually ran — N3 fix: pre-N3 the caller
/// unconditionally added `SPAN_MUTATION_ATTEMPTS` to its `calls` counter,
/// even when no labels with ≥2 occurrences exist (multi is empty → 0 probes
/// run) or when a probe fired Interesting on attempt 1 (only 1 probe ran).
/// The accounting now reflects what actually happened.
fn try_span_mutation(
    nodes: &[ChoiceNode],
    spans: &[Span],
    rng: &mut SmallRng,
    ctx: &mut EngineCtx<'_>,
) -> (Option<(Vec<ChoiceNode>, String)>, usize) {
    use std::collections::HashSet;

    let mut by_label: HashMap<&str, HashSet<(usize, usize)>> = HashMap::new();
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
        return (None, 0);
    }

    let values: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();

    let mut attempts: usize = 0;
    for _ in 0..SPAN_MUTATION_ATTEMPTS {
        let group = &multi[rng.random_range(0..multi.len())];
        let i_a = rng.random_range(0..group.len());
        let mut i_b = rng.random_range(0..group.len() - 1);
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
            let (donor_start, donor_end) = if rng.random::<bool>() {
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

        let ntc = NativeTestCase::for_choices(&attempt, None, None);
        let run = ctx.execute(ntc, false);
        attempts += 1;
        if run.status == Status::Interesting {
            let origin = run.origin.unwrap_or_default();
            return (Some((run.nodes, origin)), attempts);
        }
    }
    (None, attempts)
}

fn create_rng(settings: &Settings, database_key: Option<&str>) -> SmallRng {
    if let Some(seed) = settings.seed {
        SmallRng::seed_from_u64(seed)
    } else if settings.derandomize {
        let key = database_key.unwrap_or("unnamed-test");
        let hash = hash_string(key);
        SmallRng::seed_from_u64(hash)
    } else {
        SmallRng::from_rng(&mut rand::rng())
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
