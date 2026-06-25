//! Embedded tests for `src/native/test_runner.rs`.
//!
//! These drive the engine directly — `run_main`, `run_single_case`, `Engine`,
//! the health-check helpers, and the database reuse phase. Test bodies draw
//! from the engine's own `DataSource` (the same interface the C ABI exposes)
//! and report their outcome by returning a `TestCaseResult`, rather than going
//! through the `hegeltest` frontend's `TestCase`/generators/`Hegel`, which live
//! in the other crate. A `boolean` draw is one weighted-0.5 choice and an
//! `integer` draw one `draw_integer` choice, so the realised choice sequences
//! match the equivalent `gs::booleans()` / `gs::integers()` draws.

use super::*;

use crate::backend::{DataSource, Failure, TestCaseResult};
use crate::settings::{Mode, Phase};
use ciborium::Value;
use std::time::Duration;

// ── raw DataSource draw helpers ─────────────────────────────────────────────

fn bool_schema() -> Value {
    Value::Map(vec![(
        Value::Text("type".into()),
        Value::Text("boolean".into()),
    )])
}

fn int_schema(min: i64, max: i64) -> Value {
    Value::Map(vec![
        (Value::Text("type".into()), Value::Text("integer".into())),
        (Value::Text("min_value".into()), Value::Integer(min.into())),
        (Value::Text("max_value".into()), Value::Integer(max.into())),
    ])
}

fn u64_schema() -> Value {
    Value::Map(vec![
        (Value::Text("type".into()), Value::Text("integer".into())),
        (Value::Text("min_value".into()), Value::Integer(0u64.into())),
        (
            Value::Text("max_value".into()),
            Value::Integer(u64::MAX.into()),
        ),
    ])
}

/// A drawn boolean, or `Err(())` if the case overran / was aborted.
fn rbool(ds: &dyn DataSource) -> Result<bool, ()> {
    match ds.generate(&bool_schema()) {
        Ok(Value::Bool(b)) => Ok(b),
        Ok(other) => panic!("expected boolean, got {other:?}"),
        Err(_) => Err(()),
    }
}

/// A drawn `i64` in `[min, max]`, or `Err(())` if the case overran.
fn rint(ds: &dyn DataSource, min: i64, max: i64) -> Result<i64, ()> {
    match ds.generate(&int_schema(min, max)) {
        Ok(Value::Integer(i)) => Ok(i128::from(i) as i64),
        Ok(other) => panic!("expected integer, got {other:?}"),
        Err(_) => Err(()),
    }
}

/// A drawn `u64` over the full range, or `Err(())` if the case overran.
fn ru64(ds: &dyn DataSource) -> Result<u64, ()> {
    match ds.generate(&u64_schema()) {
        Ok(Value::Integer(i)) => Ok(i128::from(i) as u64),
        Ok(other) => panic!("expected integer, got {other:?}"),
        Err(_) => Err(()),
    }
}

const I32_MIN: i64 = i32::MIN as i64;
const I32_MAX: i64 = i32::MAX as i64;

/// An INTERESTING result whose message and (stable, per-message) origin both
/// mention "Panic", standing in for a panicking test body.
fn boom(msg: &str) -> TestCaseResult {
    TestCaseResult::Interesting(Failure {
        origin: format!("Panic: {msg}"),
        reproduce_blob: None,
    })
}

// ── health-check helpers (pure) ─────────────────────────────────────────────

#[test]
fn too_slow_check_reports_when_under_threshold_and_unsuppressed() {
    let msg = too_slow_check(
        /* valid_test_cases */ 1,
        /* total_test_time */ Duration::from_secs(60),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ false,
    );
    assert!(msg.is_some(), "expected too_slow_check to report a failure");
    assert!(msg.unwrap().contains("TooSlow"));
}

#[test]
fn too_slow_check_quiet_when_suppressed() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ true,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_under_threshold() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(1),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_enough_valid_cases() {
    // Once enough valid cases have run, the health check is no longer
    // applied even if total_test_time exceeds the threshold.
    assert!(
        too_slow_check(
            /* valid_test_cases */ 10_000,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn flaky_diagnostic_mentions_flaky() {
    assert!(flaky_diagnostic().contains("Flaky test detected"));
}

#[test]
fn invalid_thresholds_match_hypothesis() {
    // Ported from Hypothesis's `_invalid_thresholds(r=0.01, c=0.99)`
    // (`engine.py`), which evaluates to `INVALID_THRESHOLD_BASE = 458` and
    // `INVALID_PER_VALID = 100`. Pin the port so an always-reject run gives up
    // after `458 + 1 = 459` cases, matching the core engine.
    assert_eq!(invalid_thresholds(0.01, 0.99), (458, 100));
}

// ── cached_run / span-mutation caching ──
//
// Span mutation proposes choice sequences whose paths are frequently
// already covered by generation. Pre-fix the native backend ran the test
// body for every proposal (`ctx.execute`), executing the test ~6× as often
// as Hypothesis, which routes mutations through `cached_test_function`.
// These tests pin the cache/tree short-circuits that close that gap.

use std::cell::Cell;
use std::rc::Rc;

use crate::native::core::ChoiceKind;
use crate::native::core::choices::BooleanChoice;
use crate::native::data_tree::{DataTreeNode, record_tree};

/// Build an [`Engine`] whose `run_case` runs `body` (returning the test
/// case's outcome) and counts how many times the body actually executed,
/// then hand both to `after`.
fn with_counting_ctx<T, B>(mut body: T, after: B)
where
    T: FnMut(&dyn DataSource) -> TestCaseResult,
    B: FnOnce(&mut Engine<'_>, &Rc<Cell<usize>>),
{
    let exec_count = Rc::new(Cell::new(0usize));
    let counter = exec_count.clone();
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        counter.set(counter.get() + 1);
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new().database(None);
    let mut ctx = Engine::new(&settings, None, &mut run_case);
    after(&mut ctx, &exec_count);
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

#[test]
fn cached_test_function_serves_tree_known_path_without_executing() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        |ctx, count| {
            // The tree already records a one-boolean Valid run; replaying that
            // path (plus an unread trailing choice, as a shape-changing shrink
            // candidate would produce) is served from the tree without running
            // the body. This is the single chokepoint both generation mutation
            // and shrinking go through.
            record_tree(&mut ctx.tree_root, &[bool_node(false)], Status::Valid, &[]);

            let run = ctx.cached_test_function(
                &[ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
                None,
                0,
            );
            assert_eq!(run.status, Status::Valid);
            assert_eq!(count.get(), 0, "tree-known path must not run the body");
            // Realised nodes are recovered from the tree walk (the trailing
            // choice was never read).
            assert_eq!(run.nodes.len(), 1);
        },
    );
}

#[test]
fn cached_test_function_executes_novel_then_serves_repeat() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];

            // Novel: executes and records the run into the tree.
            let first = ctx.cached_test_function(&choices, None, 0);
            assert_eq!(first.status, Status::Valid);
            assert_eq!(count.get(), 1);

            // Exact repeat: served from the tree, no re-run.
            let second = ctx.cached_test_function(&choices, None, 0);
            assert_eq!(second.status, Status::Valid);
            assert_eq!(count.get(), 1, "exact repeat must be served from the tree");
        },
    );
}

#[test]
fn cached_test_function_serves_interesting_from_tree_with_origin_and_spans() {
    // The lossless tree records the full outcome — status, origin, and the
    // spans (replayed from per-node events). So an interesting path is served
    // from the tree with its origin and spans intact and *without* re-running
    // the body; there is no separate result cache and no re-execution.
    with_counting_ctx(
        |ds| {
            // A span around the single draw, so the recorded path carries one
            // span to reconstruct. Interesting on `true`.
            ds.start_span(7).unwrap();
            let b = rbool(ds);
            ds.stop_span(false).unwrap();
            match b {
                Ok(true) => boom("boom-on-true"),
                Ok(false) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];

            // First call executes, finds the bug, and records it (origin +
            // spans + status) into the tree.
            let first = ctx.cached_test_function(&choices, None, 0);
            assert_eq!(first.status, Status::Interesting);
            assert!(first.origin.is_some());
            assert_eq!(count.get(), 1);

            // Second identical call: served entirely from the tree — no
            // execution — with the origin and spans reconstructed.
            let second = ctx.cached_test_function(&choices, None, 0);
            assert_eq!(second.status, Status::Interesting);
            assert_eq!(
                count.get(),
                1,
                "interesting path must be served from the tree, not re-run"
            );
            assert_eq!(second.origin, first.origin);
            assert_eq!(second.spans.len(), 1);
            assert_eq!(second.spans[0].label, "7");
            assert_eq!(second.spans[0].start, 0);
            assert_eq!(second.spans[0].end, 1);
        },
    );
}

#[test]
fn overrun_during_draw_overrides_a_swallowed_valid_outcome() {
    // A body that draws past the available choices overruns; if it swallows the
    // resulting error and reports VALID anyway, the engine must still treat the
    // case as EarlyStop. An overrun means the replayed prefix was too short —
    // not that the (incomplete) run passed — and recording it as a zero-length
    // VALID conclusion would poison the choice tree for every later candidate.
    with_counting_ctx(
        |ds| {
            // Empty prefix → `max_size` 0 → this draw overruns. The `Err` is
            // deliberately swallowed, mimicking a raw test body that doesn't
            // propagate the overrun.
            let _ = rbool(ds);
            TestCaseResult::Valid
        },
        |ctx, _| {
            let run = ctx.execute(NativeTestCase::for_choices(&[], None, None));
            assert_eq!(run.status, Status::EarlyStop);
        },
    );
}

#[test]
fn cached_test_function_probe_replays_prefix_then_draws_continuation() {
    // `extend > 0` replays the prefix and draws the remaining choices from the
    // engine RNG (the coarse / mutate_and_shrink probe path). The realised path
    // isn't known up front, so it always executes.
    with_counting_ctx(
        |ds| {
            // Two boolean draws: the first is the replayed prefix, the second
            // is drawn beyond it.
            match (rbool(ds), rbool(ds)) {
                (Ok(_), Ok(_)) => TestCaseResult::Valid,
                _ => TestCaseResult::Overrun,
            }
        },
        |ctx, count| {
            let prefix = [ChoiceValue::Boolean(true)];
            let run = ctx.cached_test_function(&prefix, None, 1);
            assert_eq!(run.status, Status::Valid);
            assert_eq!(count.get(), 1);
            // Prefix value replayed, continuation drawn → two realised nodes.
            assert_eq!(run.nodes.len(), 2);
            assert_eq!(run.nodes[0].value, ChoiceValue::Boolean(true));
        },
    );
}

#[test]
fn span_mutation_does_not_re_execute_identical_proposals() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        |ctx, count| {
            // Two spans of the same label, one nested in the other. Every
            // span-mutation attempt then proposes the *same* duplicated
            // choice sequence, so only the first proposal runs the body and
            // the rest are served from the cache.
            let nodes = vec![
                bool_node(false),
                bool_node(true),
                bool_node(false),
                bool_node(true),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 1);
            // The single executed probe was recorded: one call, one valid
            // example consumed from the budget, nothing interesting.
            assert_eq!(ctx.calls, 1);
            assert_eq!(ctx.valid_test_cases, 1);
            assert!(ctx.interesting.is_empty());
        },
    );
}

#[test]
fn span_mutation_returns_interesting_proposal() {
    with_counting_ctx(
        // INTERESTING on a `false` draw, so the all-`false` mutated proposal
        // is Interesting.
        |ds| match rbool(ds) {
            Ok(false) => boom("boom on false"),
            Ok(true) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        |ctx, count| {
            // Nested same-label spans → the deterministic proposal duplicates
            // the (false) prefix, and the body's single draw resolves to
            // `false` → Interesting on the first probe.
            let nodes = vec![
                bool_node(false),
                bool_node(false),
                bool_node(false),
                bool_node(false),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 1);
            assert_eq!(ctx.calls, 1);
            // An Interesting probe is not a valid example; budget untouched.
            assert_eq!(ctx.valid_test_cases, 0);
            let origin = ctx
                .interesting
                .keys()
                .next()
                .expect("the first proposal should be Interesting");
            assert!(origin.contains("Panic"));
        },
    );
}

#[test]
fn span_mutation_stops_when_example_budget_is_full() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        |ctx, count| {
            let nodes = vec![
                bool_node(false),
                bool_node(true),
                bool_node(false),
                bool_node(true),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            // Budget already full: no probe should run.
            ctx.valid_test_cases = 100;
            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 0);
            assert_eq!(ctx.calls, 0);
            assert_eq!(ctx.valid_test_cases, 100);
        },
    );
}

#[test]
fn create_rng_default_backend_is_prng() {
    let settings = Settings::new().seed(Some(123));
    assert!(matches!(create_rng(&settings, None), EngineRng::Prng(_)));
}

#[cfg(unix)]
#[test]
fn create_rng_urandom_backend_reads_urandom() {
    let settings = Settings::new().backend(crate::settings::Backend::Urandom);
    assert!(matches!(create_rng(&settings, None), EngineRng::Urandom(_)));
}

/// Wrap a `run_main` outcome into the aggregate
/// [`crate::backend::TestRunResult`], the way `embed::run_native` does —
/// convenient for tests that drive `run_main` directly to inject the
/// TooSlow / shrink-budget thresholds: the exploration failures wrapped up.
fn complete_native(
    exploration: Result<Vec<crate::backend::Failure>, crate::backend::RunError>,
) -> Result<crate::backend::TestRunResult, crate::backend::RunError> {
    Ok(crate::backend::TestRunResult {
        failures: exploration?,
    })
}

#[test]
fn run_single_case_returns_the_failure() {
    // `Mode::SingleTestCase` bypasses the TestRunner machinery: its one
    // test case runs as its own final, and the failure (if any) comes back
    // directly.
    let failure = run_single_case(
        &Settings::new()
            .database(None)
            .mode(Mode::SingleTestCase)
            .verbosity(Verbosity::Quiet),
        None,
        &mut |ds| {
            ds.mark_complete(&boom("single-case bug"));
        },
    )
    .unwrap();
    assert!(failure.origin.contains("single-case bug"), "{failure:?}");
}

#[test]
fn run_single_case_returns_none_for_a_passing_case() {
    let failure = run_single_case(
        &Settings::new()
            .database(None)
            .mode(Mode::SingleTestCase)
            .verbosity(Verbosity::Quiet),
        None,
        &mut |ds| {
            ds.mark_complete(&TestCaseResult::Valid);
        },
    );
    assert!(failure.is_none(), "{failure:?}");
}

#[test]
fn run_main_with_urandom_backend_generates_and_passes() {
    // End-to-end: the urandom backend drives the full engine (every draw
    // reads /dev/urandom) for a passing test. Exercises the urandom fill
    // path through the biased samplers.
    let body = |ds: &dyn DataSource| match rint(ds, I32_MIN, I32_MAX) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let exploration = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let result = complete_native(exploration).unwrap();
    assert!(result.failures.is_empty());
}

#[test]
fn run_main_with_urandom_backend_finds_counterexample() {
    // A test that always fails must still surface a failure under the
    // urandom backend, going through generation, shrinking (deterministic
    // concrete-choice replay), and final replay.
    let body = |ds: &dyn DataSource| match rint(ds, I32_MIN, I32_MAX) {
        Ok(_) => boom("always fails"),
        Err(()) => TestCaseResult::Overrun,
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let exploration = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let result = complete_native(exploration).unwrap();
    assert!(
        result.failures[0].origin.contains("always fails"),
        "{:?}",
        result.failures
    );
}

#[test]
fn slow_shrink_warning_mentions_shrinking() {
    let w = slow_shrink_warning();
    assert!(w.contains("Shrinking"), "{w}");
    assert!(w.contains("stopped"), "{w}");
}

#[test]
fn run_main_stops_shrinking_when_budget_is_exhausted() {
    // Drive `run_main` with a zero shrink budget so the wall-clock cutoff
    // fires deterministically instead of after five minutes. The run must
    // still surface the failure (with the best, un-shrunk example) rather
    // than hang, and the slow-shrink warning path is exercised.
    //
    // A collection of integers gives real shrinking work for the zero budget
    // to cut short.
    let body = |ds: &dyn DataSource| -> TestCaseResult {
        let cid = match ds.new_collection(0, None) {
            Ok(c) => c,
            Err(_) => return TestCaseResult::Overrun,
        };
        let mut len = 0usize;
        loop {
            match ds.collection_more(cid) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => return TestCaseResult::Overrun,
            }
            if rint(ds, I32_MIN, I32_MAX).is_err() {
                return TestCaseResult::Overrun;
            }
            len += 1;
        }
        if len > 0 {
            boom("non-empty vec")
        } else {
            TestCaseResult::Valid
        }
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .test_cases(200)
        .database(None)
        .derandomize(true);
    let exploration = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::ZERO,
    );
    let result = complete_native(exploration).unwrap();
    assert!(
        !result.failures.is_empty(),
        "the failure must still be reported"
    );
    assert!(
        result.failures[0].origin.contains("non-empty vec"),
        "{:?}",
        result.failures
    );
}

#[test]
fn run_main_reports_too_slow_at_call_site() {
    // Drive `run_main` with a zero TooSlow threshold so the (otherwise
    // 30s-gated) call-site early-return fires deterministically — instead of
    // relying on a test happening to exceed 30s of generation under coverage
    // instrumentation. The body draws a value so each case is non-trivial.
    let body = |ds: &dyn DataSource| match rbool(ds) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new().test_cases(100).database(None);
    let exploration = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::ZERO,
        Duration::from_secs(300),
    );
    let result = complete_native(exploration);
    match result {
        Err(crate::backend::RunError::HealthCheck(msg)) => {
            assert!(msg.contains("TooSlow"), "unexpected message: {msg}");
        }
        other => panic!("expected RunError::HealthCheck, got {other:?}"),
    }
}

// ── TestCasesTooLarge (too_large_check) ──

#[test]
fn too_large_check_reports_when_over_threshold_and_unsuppressed() {
    let msg = too_large_check(
        /* valid */ 0, /* overrun */ 20, /* suppressed */ false,
    );
    assert!(msg.is_some());
    assert!(msg.unwrap().contains("TestCasesTooLarge"));
}

#[test]
fn too_large_check_quiet_when_suppressed() {
    assert!(too_large_check(0, 20, true).is_none());
}

#[test]
fn too_large_check_quiet_when_under_threshold() {
    assert!(too_large_check(0, 19, false).is_none());
}

#[test]
fn too_large_check_quiet_when_enough_valid_cases() {
    assert!(too_large_check(10, 100, false).is_none());
}

// ── LargeInitialTestCase (large_initial_check) ──

#[test]
fn large_initial_check_reports_on_overrun() {
    let msg = large_initial_check(true, Status::Invalid, 0, false);
    assert!(msg.unwrap().contains("LargeInitialTestCase"));
}

#[test]
fn large_initial_check_reports_on_large_valid_example() {
    // node_count * 2 > BUFFER_SIZE.
    let msg = large_initial_check(false, Status::Valid, BUFFER_SIZE, false);
    assert!(msg.unwrap().contains("LargeInitialTestCase"));
}

#[test]
fn large_initial_check_quiet_for_small_valid_example() {
    assert!(large_initial_check(false, Status::Valid, 1, false).is_none());
}

#[test]
fn large_initial_check_quiet_when_suppressed() {
    assert!(large_initial_check(true, Status::Invalid, 0, true).is_none());
}

#[test]
fn large_initial_check_quiet_for_interesting() {
    // A bug found at the simplest example is reported as a failure, not a
    // health-check failure.
    assert!(large_initial_check(false, Status::Interesting, BUFFER_SIZE, false).is_none());
}

// ── overrun vs invalid distinction ──

#[test]
fn genuine_overrun_is_early_stop_and_not_recorded_in_the_tree() {
    // A genuine choice-budget overrun must be `Status::EarlyStop`, not
    // `Status::Invalid`. `record_tree` only records a conclusion for
    // `status >= Invalid`, so mislabelling an overrun would pin the path into
    // the data tree as a permanent dead-end.
    with_counting_ctx(
        |ds| {
            // Two draws against a one-choice budget: the second overruns.
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        |ctx, _count| {
            let (run, _mismatch) = ctx.test_function(NativeTestCase::for_simplest(1));
            assert_eq!(run.status, Status::EarlyStop);

            // The overrun path is therefore not concluded in the tree: a later
            // walk of the same prefix must re-execute (returns `None`) rather
            // than serve a cached dead-end.
            let mut tree = DataTreeNode::default();
            record_tree(&mut tree, &run.nodes, run.status, &[]);
            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value.clone()).collect();
            assert_eq!(crate::native::data_tree::simulate(&tree, &choices), None);
        },
    );
}

// ── database reuse semantics ──
//
// These drive the reuse phase via `run_main` (with the database configured)
// rather than the `Hegel` frontend, populating the on-disk corpus directly
// with `serialize_choices` so a precise stored prefix can be pinned.

/// A reuse-phase `run_main` over `path`/`key`, returning the aggregate result.
fn reuse_run<F>(
    settings: Settings,
    key: &str,
    mut body: F,
) -> Result<crate::backend::TestRunResult, crate::backend::RunError>
where
    F: FnMut(&dyn DataSource) -> TestCaseResult,
{
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let exploration = run_main(
        &settings,
        Some(key),
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    complete_native(exploration)
}

#[test]
fn reuse_replay_extends_past_stored_prefix() {
    // Hypothesis replays stored entries with extend="full": when the test now
    // draws more choices than the stored prefix holds, the replay continues
    // with fresh random draws instead of overrunning. The stored `[true]` is
    // one boolean short of what the test reads; it must still reproduce.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            let a = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            let _b = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            if a {
                boom("replayed bug")
            } else {
                TestCaseResult::Valid
            }
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "stored prefix one draw short must still reproduce via random extension"
    );
}

#[test]
fn reuse_consults_secondary_corpus_when_primary_fails_to_reproduce() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // The primary entry no longer fails; the still-failing example only
    // exists in the secondary (historical) corpus, which the reuse phase
    // samples when the primary corpus comes up short.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]),
    );
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    db.save(
        &secondary_key,
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]),
    );

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .test_cases(10)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, i64::MIN, i64::MAX) {
            Ok(4242) => boom("secondary bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "the secondary corpus entry must be replayed when primary finds nothing"
    );
}

#[test]
fn reuse_randomly_samples_secondary_corpus_when_it_overflows_the_shortfall() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // One primary entry that no longer reproduces, plus a secondary corpus
    // larger than the shortfall (desired_size 2 - 1 primary = 1).  That
    // drives the partial Fisher-Yates sampling path: more historical
    // entries exist than the reuse phase wants, so only a random subset is
    // replayed.  Every secondary entry reproduces the bug, so the run must
    // fail no matter which one the sample happens to keep.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]),
    );
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    for n in [4242, 4243, 4244, 4245] {
        db.save(
            &secondary_key,
            &serialize_choices(&[ChoiceValue::Integer(BigInt::from(n))]),
        );
    }

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .test_cases(2)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, i64::MIN, i64::MAX) {
            Ok(n) if n >= 4242 => boom("secondary bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "a sampled secondary entry must still reproduce the bug"
    );
}

#[test]
fn shrink_phase_drains_stale_secondary_corpus_entries() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    let stale = serialize_choices(&[ChoiceValue::Integer(BigInt::from(5))]);
    db.save(&secondary_key, &stale);

    // A failing run replays small secondary entries as shrink jump-starts
    // and deletes them either way (Hypothesis's clear_secondary_key) — the
    // secondary corpus must not grow without bound across runs.
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .test_cases(200)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, i64::MIN, i64::MAX) {
            Ok(n) if n >= 1000 => boom("big bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "the run should find the n >= 1000 bug"
    );
    assert!(
        !db.fetch(&secondary_key).contains(&stale),
        "the stale secondary entry must be drained"
    );
}

#[test]
fn should_generate_more_stops_ten_seconds_after_first_bug() {
    // Within the call-count window but past the 10-second wall-clock cutoff
    // (engine.py's first_bug_found_time): stop hunting for more origins.
    assert!(should_generate_more(
        false,
        20,
        Some(15),
        Some(15),
        true,
        true,
        Some(std::time::Duration::from_secs(9)),
    ));
    assert!(!should_generate_more(
        false,
        20,
        Some(15),
        Some(15),
        true,
        true,
        Some(std::time::Duration::from_secs(11)),
    ));
}

#[test]
fn reuse_stops_after_first_reproduced_bug_without_multiple_reporting() {
    use crate::native::bignum::BigInt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // Two stored entries that both still reproduce.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(1111))]),
    );
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(2222))]),
    );

    let calls = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .report_multiple_failures(false)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            calls.fetch_add(1, Ordering::SeqCst);
            match rint(ds, i64::MIN, i64::MAX) {
                Ok(n) if n >= 1000 => boom("stored bug"),
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "the stored bug should be reported"
    );
    // One reuse replay: the first entry reproduces, so the loop breaks.
    assert!(
        calls.load(Ordering::SeqCst) <= 2,
        "expected reuse to stop after the first reproduced bug, ran {} cases",
        calls.load(Ordering::SeqCst)
    );
}

#[test]
fn reuse_found_bug_skips_generation_entirely() {
    use crate::native::bignum::BigInt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]),
    );

    // Hypothesis skips generation when the database replay already
    // reproduced a failure ("we'd rather report that they're still failing
    // ASAP than take the time to look for new ones"). With reuse replays
    // now recorded like any other run, the bug-window heuristic alone would
    // let generation probe for several extra calls; the explicit skip must
    // keep the body-call count at exactly the one reuse replay.
    let calls = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .test_cases(200)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            calls.fetch_add(1, Ordering::SeqCst);
            match rint(ds, i64::MIN, i64::MAX) {
                Ok(4242) => boom("stored bug"),
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
    );
    assert!(
        result.map(|r| !r.failures.is_empty()).unwrap_or(false),
        "the stored bug should be reported"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "expected exactly one reuse replay and no generation or final replay"
    );
}

#[test]
fn should_generate_more_stops_without_bug_markers() {
    // Defensive arm: a non-empty interesting map with no bug-window markers
    // cannot arise from run_main any more (every interesting run passes
    // through record_run, which sets them), but the standalone function
    // must still answer sensibly.
    assert!(!should_generate_more(
        false, 5, None, None, true, true, None
    ));
}

#[test]
fn reuse_detects_nondeterministic_generator_across_replays() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // Two stored entries, so the reuse phase replays twice.
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(false)]));

    // The generator alternates the drawn kind per invocation: with reuse
    // replays now recorded into the choice tree, the second replay
    // contradicts the first and must surface the non-determinism diagnostic
    // instead of silently mispredicting.
    let flip = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            let r = if flip.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                rbool(ds).map(|_| ())
            } else {
                rint(ds, i64::MIN, i64::MAX).map(|_| ())
            };
            match r {
                Ok(()) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
    );
    match result {
        Err(crate::backend::RunError::NonDeterministic(msg)) => {
            assert!(
                msg.to_lowercase().contains("non-deterministic"),
                "got: {msg}"
            );
        }
        other => panic!("expected RunError::NonDeterministic, got {other:?}"),
    }
}

#[test]
fn nondeterministic_generator_contradicts_reuse_fed_tree_at_simplest_example() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // A single stored entry: the reuse phase replays it once (no second replay
    // to contradict, so the reuse-phase determinism check stays quiet) and
    // feeds the choice tree. Generation then runs its simplest-example probe,
    // whose draw the flipping generator makes contradict the reuse-fed tree —
    // exercising the post-reuse `for_simplest` non-determinism check rather
    // than the reuse-replay one.
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));

    let flip = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse, Phase::Generate])
            .test_cases(10)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            let r = if flip.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                rbool(ds).map(|_| ())
            } else {
                rint(ds, i64::MIN, i64::MAX).map(|_| ())
            };
            match r {
                Ok(()) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
    );
    match result {
        Err(crate::backend::RunError::NonDeterministic(msg)) => {
            assert!(
                msg.to_lowercase().contains("non-deterministic"),
                "got: {msg}"
            );
        }
        other => panic!("expected RunError::NonDeterministic, got {other:?}"),
    }
}

#[test]
fn run_single_case_derandomize_is_keyed_by_test_identity() {
    // Two different tests (database keys) running derandomized in
    // `Mode::SingleTestCase` must not draw identical streams; the same key
    // must replay the same stream.
    let settings = Settings::new()
        .database(None)
        .derandomize(true)
        .mode(Mode::SingleTestCase)
        .verbosity(Verbosity::Quiet);
    let draw_with_key = |key: Option<&str>| {
        let mut drawn: Vec<u64> = Vec::new();
        {
            let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
                for _ in 0..4 {
                    if let Ok(n) = ru64(&*ds) {
                        drawn.push(n);
                    }
                }
                ds.mark_complete(&TestCaseResult::Valid);
            };
            run_single_case(&settings, key, &mut run_case);
        }
        drawn
    };
    let a1 = draw_with_key(Some("test-a"));
    let a2 = draw_with_key(Some("test-a"));
    let b = draw_with_key(Some("test-b"));
    assert_eq!(a1, a2, "the same key must replay the same draws");
    assert_ne!(a1, b, "different keys must not share a derandomized stream");
}
