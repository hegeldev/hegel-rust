//! Embedded tests for `src/native/test_runner.rs` helpers.  Cover the
//! health-check diagnostics (TooSlow and the flaky-replay message) that the
//! runner folds into a failing `TestRunResult` instead of panicking, so no
//! panic crosses the FFI boundary into libhegel.

use super::*;
use std::time::Duration;

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
use crate::run_lifecycle::run_test_case;

/// Build an [`EngineCtx`] whose `run_case` runs `test_fn` and counts how many
/// times the test body actually executed, then hand both to `body`.
fn with_counting_ctx<T, B>(mut test_fn: T, body: B)
where
    T: FnMut(crate::TestCase),
    B: FnOnce(&mut EngineCtx<'_>, &Rc<Cell<usize>>),
{
    crate::run_lifecycle::init_panic_hook();
    let exec_count = Rc::new(Cell::new(0usize));
    let counter = exec_count.clone();
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        counter.set(counter.get() + 1);
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let mut ctx = EngineCtx::new(&mut run_case);
    body(&mut ctx, &exec_count);
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

#[test]
fn cached_run_skips_execution_when_tree_knows_the_path() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            // The tree already records a one-boolean run that concluded
            // Valid; replaying that path (plus an unread trailing choice,
            // as a duplicated span would produce) must not run the body.
            let mut tree = DataTreeNode::default();
            record_tree(&mut tree, &[bool_node(false)], Status::Valid, &[]);

            let (run, executed) = ctx.cached_run(
                &[ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
                &mut tree,
            );
            assert_eq!(run.status, Status::Valid);
            assert!(!executed);
            assert_eq!(count.get(), 0);
        },
    );
}

#[test]
fn cached_run_executes_novel_then_serves_repeat_from_cache() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            let mut tree = DataTreeNode::default();
            let choices = [ChoiceValue::Boolean(true)];

            let (first, executed_first) = ctx.cached_run(&choices, &mut tree);
            assert!(executed_first);
            assert_eq!(count.get(), 1);

            // A second identical replay is served without re-running.
            let (second, executed_second) = ctx.cached_run(&choices, &mut tree);
            assert!(!executed_second);
            assert_eq!(count.get(), 1);
            assert_eq!(first.status, second.status);
        },
    );
}

#[test]
fn cached_run_reexecutes_known_interesting_path_to_recover_payload() {
    // The tree can record that a path was Interesting but not the failure's
    // nodes/origin, so a cached_run on a tree-known Interesting path falls
    // through to a real execution to recover that payload.
    with_counting_ctx(
        |tc| {
            if tc.draw(crate::generators::booleans()) {
                panic!("boom");
            }
        },
        |ctx, count| {
            let mut tree = DataTreeNode::default();
            record_tree(&mut tree, &[bool_node(true)], Status::Interesting, &[]);

            let (run, executed) = ctx.cached_run(&[ChoiceValue::Boolean(true)], &mut tree);
            assert_eq!(run.status, Status::Interesting);
            assert!(executed);
            assert_eq!(count.get(), 1);
            assert!(run.origin.is_some());
        },
    );
}

#[test]
fn span_mutation_does_not_re_execute_identical_proposals() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
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

            let mut tree = DataTreeNode::default();
            let mut rng = EngineRng::seeded(0);
            let mut valid = 0u64;
            let (result, attempts) =
                try_span_mutation(&nodes, &spans, &mut rng, ctx, &mut tree, &mut valid, 100);

            assert!(result.is_none());
            assert_eq!(attempts, 1);
            assert_eq!(count.get(), 1);
            // The single valid execution consumed one unit of the budget.
            assert_eq!(valid, 1);
        },
    );
}

#[test]
fn span_mutation_returns_interesting_proposal() {
    with_counting_ctx(
        // Panics on a `false` draw, so the all-`false` mutated proposal is
        // Interesting.
        |tc| {
            if !tc.draw(crate::generators::booleans()) {
                panic!("boom on false");
            }
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

            let mut tree = DataTreeNode::default();
            let mut rng = EngineRng::seeded(0);
            let mut valid = 0u64;
            let (result, attempts) =
                try_span_mutation(&nodes, &spans, &mut rng, ctx, &mut tree, &mut valid, 100);

            let (_nodes, origin) = result.expect("the first proposal should be Interesting");
            assert!(origin.contains("Panic"));
            assert_eq!(attempts, 1);
            assert_eq!(count.get(), 1);
            // An Interesting probe is not a valid example; budget untouched.
            assert_eq!(valid, 0);
        },
    );
}

#[test]
fn span_mutation_stops_when_example_budget_is_full() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
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

            let mut tree = DataTreeNode::default();
            let mut rng = EngineRng::seeded(0);
            // Budget already full: no probe should run.
            let mut valid = 100u64;
            let (result, attempts) =
                try_span_mutation(&nodes, &spans, &mut rng, ctx, &mut tree, &mut valid, 100);

            assert!(result.is_none());
            assert_eq!(attempts, 0);
            assert_eq!(count.get(), 0);
            assert_eq!(valid, 100);
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

#[test]
fn run_main_with_urandom_backend_generates_and_passes() {
    // End-to-end: the urandom backend drives the full engine (every draw
    // reads /dev/urandom) for a passing test. Exercises the urandom fill
    // path through the biased samplers.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        let _: i32 = tc.draw(crate::generators::integers());
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let result = run_main(&settings, None, &mut run_case, Duration::from_secs(30));
    assert!(result.passed);
}

#[test]
fn run_main_with_urandom_backend_finds_counterexample() {
    // A test that always panics must still surface a failure under the
    // urandom backend, going through generation, shrinking (deterministic
    // concrete-choice replay), and final replay.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        let _: i32 = tc.draw(crate::generators::integers());
        panic!("always fails");
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let result = run_main(&settings, None, &mut run_case, Duration::from_secs(30));
    assert!(!result.passed);
    assert!(
        result.failures[0].panic_message.contains("always fails"),
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
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        tc.draw(crate::generators::booleans());
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new().test_cases(100).database(None);
    let result = run_main(&settings, None, &mut run_case, Duration::ZERO);
    assert!(!result.passed);
    assert!(
        result.failures[0].panic_message.contains("TooSlow"),
        "{:?}",
        result.failures
    );
}
