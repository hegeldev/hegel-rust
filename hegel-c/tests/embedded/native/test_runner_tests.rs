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
use crate::native::core::choices::BooleanChoice;
use alloc::vec;

use crate::backend::{DataSource, Failure, TestCaseResult};
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::settings::{Mode, Phase};
use std::time::Duration;

/// A drawn boolean, or `Err(())` if the case overran / was aborted.
fn rbool(ds: &dyn DataSource) -> Result<bool, ()> {
    ds.generate_boolean(0.5, None).map_err(|_| ())
}

/// A drawn `i64` in `[min, max]`, or `Err(())` if the case overran.
fn rint(ds: &dyn DataSource, min: i64, max: i64) -> Result<i64, ()> {
    match ds.generate_integer(&BigInt::from(min), &BigInt::from(max)) {
        Ok(v) => Ok(v.to_i64().unwrap()),
        Err(_) => Err(()),
    }
}

/// A drawn `u64` over the full range, or `Err(())` if the case overran.
fn ru64(ds: &dyn DataSource) -> Result<u64, ()> {
    match ds.generate_integer(&BigInt::from(0u64), &BigInt::from(u64::MAX)) {
        Ok(v) => Ok(v.to_u64().unwrap()),
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

#[test]
fn too_slow_check_reports_when_under_threshold_and_unsuppressed() {
    let msg = too_slow_check(1, Duration::from_secs(60), Duration::from_secs(30), false);
    assert!(msg.is_some(), "expected too_slow_check to report a failure");
    assert!(msg.unwrap().contains("TooSlow"));
}

#[test]
fn too_slow_check_quiet_when_suppressed() {
    assert!(too_slow_check(1, Duration::from_secs(60), Duration::from_secs(30), true,).is_none());
}

#[test]
fn too_slow_check_quiet_when_under_threshold() {
    assert!(too_slow_check(1, Duration::from_secs(1), Duration::from_secs(30), false,).is_none());
}

#[test]
fn too_slow_check_quiet_when_enough_valid_cases() {
    assert!(
        too_slow_check(
            10_000,
            Duration::from_secs(60),
            Duration::from_secs(30),
            false,
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
    assert_eq!(invalid_thresholds(0.01, 0.99), (458, 100));
}

use std::cell::Cell;
use std::rc::Rc;

use crate::native::data_tree::{DataTreeNode, record_tree};

/// Build an [`Engine`] whose driver runs `body` (returning the test
/// case's outcome) and counts how many times the body actually executed,
/// then hand both to `after`, driving the whole interaction through a
/// [`CaseExchange`].
fn with_counting_ctx<T, B>(mut body: T, after: B)
where
    T: FnMut(&dyn DataSource) -> TestCaseResult,
    B: AsyncFnOnce(&mut Engine<'_>, &Rc<Cell<usize>>),
{
    let exec_count = Rc::new(Cell::new(0usize));
    let counter = exec_count.clone();
    let settings = Settings::new().database(None);
    let exchange = CaseExchange::new();
    let fut = async {
        let mut ctx = Engine::new(&settings, None, &exchange).unwrap();
        after(&mut ctx, &exec_count).await;
    };
    crate::exchange::drive(&exchange, fut, |ds| {
        counter.set(counter.get() + 1);
        let result = body(&*ds);
        ds.mark_complete(&result);
    });
}

/// Drive [`run_single_case`] to completion with a synchronous `run_case`
/// callback, the way the old pre-exchange entry point worked.
fn run_single_case_sync(
    settings: &Settings,
    key: Option<&str>,
    run_case: impl FnMut(Box<dyn DataSource + Send + Sync>),
) -> Option<Failure> {
    let exchange = CaseExchange::new();
    crate::exchange::drive(
        &exchange,
        run_single_case(settings, key, &exchange),
        run_case,
    )
    .unwrap()
}

/// Drive [`run_main`] to completion with a synchronous `run_case` callback,
/// preserving the old entry point's shape for threshold-injecting tests.
fn run_main_sync(
    settings: &Settings,
    key: Option<&str>,
    run_case: impl FnMut(Box<dyn DataSource + Send + Sync>),
    too_slow_threshold: Duration,
    shrink_budget: Duration,
) -> Result<Vec<Failure>, crate::backend::RunError> {
    let exchange = CaseExchange::new();
    crate::exchange::drive(
        &exchange,
        run_main(settings, key, &exchange, too_slow_threshold, shrink_budget),
        run_case,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

#[test]
fn cached_test_function_serves_tree_known_path_without_executing() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx, count| {
            record_tree(&mut ctx.tree_root, &[bool_node(false)], Status::Valid, &[]);

            let run = ctx
                .cached_test_function(
                    &[ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
                    None,
                    0,
                )
                .await
                .unwrap();
            assert_eq!(run.status, Status::Valid);
            assert_eq!(count.get(), 0, "tree-known path must not run the body");
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
        async |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];

            let first = ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(first.status, Status::Valid);
            assert_eq!(count.get(), 1);

            let second = ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(second.status, Status::Valid);
            assert_eq!(count.get(), 1, "exact repeat must be served from the tree");
        },
    );
}

#[test]
fn cached_test_function_serves_interesting_from_tree_with_origin_and_spans() {
    with_counting_ctx(
        |ds| {
            ds.start_span(7).unwrap();
            let b = rbool(ds);
            ds.stop_span(false).unwrap();
            match b {
                Ok(true) => boom("boom-on-true"),
                Ok(false) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];

            let first = ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(first.status, Status::Interesting);
            assert!(first.origin.is_some());
            assert_eq!(count.get(), 1);

            let second = ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(second.status, Status::Interesting);
            assert_eq!(
                count.get(),
                1,
                "interesting path must be served from the tree, not re-run"
            );
            assert_eq!(second.origin, first.origin);
            assert_eq!(second.spans.len(), 2, "outer span plus the per-draw span");
            assert_eq!(second.spans[0].label, "7");
            assert_eq!(second.spans[0].start, 0);
            assert_eq!(second.spans[0].end, 1);
            assert_eq!(second.spans[1].label, "28");
            assert_eq!(second.spans[1].parent, Some(0));
        },
    );
}

#[test]
fn overrun_during_draw_overrides_a_swallowed_valid_outcome() {
    with_counting_ctx(
        |ds| {
            let _ = rbool(ds);
            TestCaseResult::Valid
        },
        async |ctx, _| {
            let (run, _mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&[], None, None))
                .await
                .unwrap();
            assert_eq!(run.status, Status::EarlyStop);
        },
    );
}

#[test]
fn cached_test_function_probe_replays_prefix_then_draws_continuation() {
    with_counting_ctx(
        |ds| match (rbool(ds), rbool(ds)) {
            (Ok(_), Ok(_)) => TestCaseResult::Valid,
            _ => TestCaseResult::Overrun,
        },
        async |ctx, count| {
            let prefix = [ChoiceValue::Boolean(true)];
            let run = ctx.cached_test_function(&prefix, None, 1).await.unwrap();
            assert_eq!(run.status, Status::Valid);
            assert_eq!(count.get(), 1);
            assert_eq!(run.nodes.len(), 2);
            assert_eq!(run.nodes[0].value(), ChoiceValue::Boolean(true));
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
        async |ctx, count| {
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

            ctx.try_span_mutation(&nodes, &spans).await.unwrap();

            assert_eq!(count.get(), 1);
            assert_eq!(ctx.calls, 1);
            assert_eq!(ctx.valid_test_cases, 1);
            assert!(ctx.interesting.is_empty());
        },
    );
}

#[test]
fn span_mutation_returns_interesting_proposal() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(false) => boom("boom on false"),
            Ok(true) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx, count| {
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

            ctx.try_span_mutation(&nodes, &spans).await.unwrap();

            assert_eq!(count.get(), 1);
            assert_eq!(ctx.calls, 1);
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
        async |ctx, count| {
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

            ctx.valid_test_cases = 100;
            ctx.try_span_mutation(&nodes, &spans).await.unwrap();

            assert_eq!(count.get(), 0);
            assert_eq!(ctx.calls, 0);
            assert_eq!(ctx.valid_test_cases, 100);
        },
    );
}

#[test]
fn span_mutation_extends_diverged_proposals_with_random_draws() {
    with_counting_ctx(
        |ds| {
            for _ in 0..4 {
                if rbool(ds).is_err() {
                    return TestCaseResult::Overrun;
                }
            }
            TestCaseResult::Valid
        },
        async |ctx, count| {
            let nodes = vec![bool_node(false), bool_node(true)];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 2), span(1, 2)];

            ctx.try_span_mutation(&nodes, &spans).await.unwrap();

            assert!(count.get() >= 1);
            assert!(
                ctx.valid_test_cases >= 1,
                "a mutated sequence that runs out of data should be completed \
                 with fresh random draws instead of being discarded, got {} \
                 valid cases from {} calls",
                ctx.valid_test_cases,
                ctx.calls
            );
        },
    );
}

#[test]
fn create_rng_default_backend_is_prng() {
    let settings = Settings::new().seed(Some(123));
    assert!(matches!(
        create_rng(&settings, None),
        Ok(EngineRng::Prng(_))
    ));
}

#[cfg(unix)]
#[test]
fn create_rng_urandom_backend_reads_urandom() {
    let settings = Settings::new().backend(crate::settings::Backend::Urandom);
    assert!(matches!(
        create_rng(&settings, None),
        Ok(EngineRng::Urandom(_))
    ));
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
    let failure = run_single_case_sync(
        &Settings::new()
            .database(None)
            .mode(Mode::SingleTestCase)
            .verbosity(Verbosity::Quiet),
        None,
        |ds| {
            ds.mark_complete(&boom("single-case bug"));
        },
    )
    .unwrap();
    assert!(failure.origin.contains("single-case bug"), "{failure:?}");
}

#[test]
fn run_single_case_returns_none_for_a_passing_case() {
    let failure = run_single_case_sync(
        &Settings::new()
            .database(None)
            .mode(Mode::SingleTestCase)
            .verbosity(Verbosity::Quiet),
        None,
        |ds| {
            ds.mark_complete(&TestCaseResult::Valid);
        },
    );
    assert!(failure.is_none(), "{failure:?}");
}

#[test]
fn run_main_with_urandom_backend_generates_and_passes() {
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
    let exploration = run_main_sync(
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
    let exploration = run_main_sync(
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
    let body = |ds: &dyn DataSource| -> TestCaseResult {
        let mut collection = match ds.new_collection(0, None) {
            Ok(c) => c,
            Err(_) => return TestCaseResult::Overrun,
        };
        let mut len = 0usize;
        loop {
            match ds.collection_more(&mut collection) {
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
    let exploration = run_main_sync(
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
    let body = |ds: &dyn DataSource| match rbool(ds) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new().test_cases(100).database(None);
    let exploration = run_main_sync(
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

#[test]
fn too_large_check_reports_when_over_threshold_and_unsuppressed() {
    let msg = too_large_check(0, 20, false);
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

#[test]
fn large_initial_check_reports_on_overrun() {
    let msg = large_initial_check(true, Status::Invalid, 0, false);
    assert!(msg.unwrap().contains("LargeInitialTestCase"));
}

#[test]
fn large_initial_check_reports_on_large_valid_example() {
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
    assert!(large_initial_check(false, Status::Interesting, BUFFER_SIZE, false).is_none());
}

#[test]
fn genuine_overrun_is_early_stop_and_not_recorded_in_the_tree() {
    with_counting_ctx(
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx, _count| {
            let (run, _mismatch) = ctx
                .test_function(NativeTestCase::for_simplest(1).unwrap())
                .await
                .unwrap();
            assert_eq!(run.status, Status::EarlyStop);

            let mut tree = DataTreeNode::default();
            record_tree(&mut tree, &run.nodes, run.status, &[]);
            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value().clone()).collect();
            assert_eq!(crate::native::data_tree::simulate(&tree, &choices), None);
        },
    );
}

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
    let exploration = run_main_sync(
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
    assert!(!should_generate_more(
        false, 5, None, None, true, true, None
    ));
}

#[test]
fn shrink_verify_with_a_different_origin_is_flaky() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                boom("origin A")
            } else {
                boom("origin B")
            }
        },
    );
    match result {
        Err(crate::backend::RunError::Flaky(msg)) => {
            assert!(msg.contains("Flaky test detected"), "got: {msg}");
        }
        other => panic!("expected RunError::Flaky, got {other:?}"),
    }
}

#[test]
fn shrink_verify_surfaces_generator_nondeterminism() {
    use std::sync::atomic::{AtomicBool, Ordering};
    // The body draws a boolean and fails on true; after the first failure it
    // permanently switches the follow-up draw's kind. With
    // report_multiple_failures(false), generation stops at that first
    // failure, so the very next execution is the pre-shrink verification
    // replay — which must surface the kind mismatch as nondeterminism.
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .report_multiple_failures(false)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            let a = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            if !a {
                return TestCaseResult::Valid;
            }
            let follow_up = if seen_bug.swap(true, Ordering::SeqCst) {
                rint(ds, 0, 100).is_err()
            } else {
                rbool(ds).is_err()
            };
            if follow_up {
                return TestCaseResult::Overrun;
            }
            boom("stable origin")
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
fn reuse_detects_nondeterministic_generator_across_replays() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(false)]));

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
            run_single_case_sync(&settings, key, &mut run_case);
        }
        drawn
    };
    let a1 = draw_with_key(Some("test-a"));
    let a2 = draw_with_key(Some("test-a"));
    let b = draw_with_key(Some("test-b"));
    assert_eq!(a1, a2, "the same key must replay the same draws");
    assert_ne!(a1, b, "different keys must not share a derandomized stream");
}

#[test]
fn run_main_shrinks_a_cloned_stream_failure_to_the_minimal_tree() {
    let body = |ds: &dyn DataSource| -> TestCaseResult {
        let child = match ds.clone_stream() {
            Ok(c) => c,
            Err(_) => return TestCaseResult::Overrun,
        };
        if rint(ds, 0, 1000).is_err() {
            return TestCaseResult::Overrun;
        }
        match rint(&*child, 0, 1000) {
            Ok(v) if v >= 100 => boom("child too big"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        }
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new().test_cases(50).database(None).seed(Some(7));
    let exploration = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let result = complete_native(exploration).unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("child too big"));

    let blob = result.failures[0].reproduce_blob.as_ref().unwrap();
    let choices = crate::native::blob::decode_failure(blob).unwrap();
    assert_eq!(choices.len(), 2);
    let crate::native::core::ChoiceValue::Clone(record) = &choices[0] else {
        panic!("expected the shrunk sequence to keep the clone node: {choices:?}");
    };
    assert_eq!(
        record.owned_values(),
        vec![ChoiceValue::Integer(crate::native::bignum::BigInt::from(
            100
        ))]
    );
    assert_eq!(
        choices[1],
        ChoiceValue::Integer(crate::native::bignum::BigInt::from(0))
    );
}

/// Drive an engine future with up to `max_open_test_cases` cases open at
/// once: on each suspension, pull every queued case into a local pool and
/// run its body immediately, then mark exactly one pooled case complete —
/// the one `pick` chooses — before polling again. Single-threaded and fully
/// deterministic, this exercises every out-of-order completion path without
/// spawning a thread.
fn drive_out_of_order<F: core::future::Future>(
    exchange: &CaseExchange,
    fut: F,
    mut run_case: impl FnMut(&dyn DataSource) -> TestCaseResult,
    mut pick: impl FnMut(usize) -> usize,
) -> F::Output {
    let mut fut = core::pin::pin!(fut);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    let mut pool: Vec<(crate::exchange::BoxedDataSource, TestCaseResult)> = Vec::new();
    loop {
        match fut.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(out) => return out,
            core::task::Poll::Pending => {
                while let Some(ds) = exchange.try_take() {
                    let result = run_case(&*ds);
                    pool.push((ds, result));
                }
                assert!(!pool.is_empty(), "engine suspended with nothing to run");
                let idx = pick(pool.len());
                let (ds, result) = pool.remove(idx);
                ds.mark_complete(&result);
            }
        }
    }
}

/// Run `run_main` under [`drive_out_of_order`] with a window of
/// `max_open` and the given completion policy.
fn run_main_out_of_order<F>(
    settings: Settings,
    mut body: F,
    pick: impl FnMut(usize) -> usize,
) -> Result<Vec<Failure>, crate::backend::RunError>
where
    F: FnMut(&dyn DataSource) -> TestCaseResult,
{
    let exchange = CaseExchange::new();
    drive_out_of_order(
        &exchange,
        run_main(
            &settings,
            None,
            &exchange,
            Duration::from_secs(30),
            Duration::from_secs(300),
        ),
        &mut body,
        pick,
    )
}

/// A body failing on values >= 100 whose sequential convergent minimum is
/// the single choice `100`.
fn threshold_body(ds: &dyn DataSource) -> TestCaseResult {
    match rint(ds, 0, 1000) {
        Ok(v) if v >= 100 => boom("too big"),
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    }
}

fn assert_shrunk_to_100(failures: &[Failure]) {
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].origin.contains("too big"), "{failures:?}");
    let blob = failures[0].reproduce_blob.as_ref().unwrap();
    let choices = crate::native::blob::decode_failure(blob).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0], ChoiceValue::Integer(BigInt::from(100)));
}

#[test]
fn out_of_order_newest_first_completion_shrinks_to_the_sequential_minimum() {
    let failures = run_main_out_of_order(
        Settings::new()
            .database(None)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
        threshold_body,
        |n| n - 1,
    )
    .unwrap();
    assert_shrunk_to_100(&failures);
}

#[test]
fn out_of_order_oldest_first_completion_shrinks_to_the_sequential_minimum() {
    let failures = run_main_out_of_order(
        Settings::new()
            .database(None)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
        threshold_body,
        |_| 0,
    )
    .unwrap();
    assert_shrunk_to_100(&failures);
}

#[test]
fn out_of_order_seeded_random_completion_shrinks_to_the_sequential_minimum() {
    let mut state: u64 = 0x2545F4914F6CDD1D;
    let failures = run_main_out_of_order(
        Settings::new()
            .database(None)
            .max_open_test_cases(4)
            .verbosity(Verbosity::Quiet),
        threshold_body,
        move |n| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 33) as usize) % n
        },
    )
    .unwrap();
    assert_shrunk_to_100(&failures);
}

#[test]
fn pipelined_passing_run_executes_exactly_the_example_budget() {
    use std::cell::Cell;
    let executed = Cell::new(0u64);
    let failures = run_main_out_of_order(
        Settings::new()
            .database(None)
            .test_cases(50)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
        |ds| {
            executed.set(executed.get() + 1);
            match ru64(ds) {
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        |n| n - 1,
    )
    .unwrap();
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        executed.get(),
        50,
        "spawn gating must count in-flight cases against the budget"
    );
}

#[test]
fn pipelined_filter_too_much_fires_for_an_always_rejecting_body() {
    let result = run_main_out_of_order(
        Settings::new()
            .database(None)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
        |ds| match ru64(ds) {
            Ok(_) => TestCaseResult::Invalid,
            Err(()) => TestCaseResult::Overrun,
        },
        |n| n - 1,
    );
    match result {
        Err(crate::backend::RunError::HealthCheck(msg)) => {
            assert!(msg.contains("FilterTooMuch"), "{msg}");
        }
        other => panic!("expected RunError::HealthCheck, got {other:?}"),
    }
}

#[test]
fn pipelined_harvest_surfaces_nondeterminism() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let flip = AtomicUsize::new(0);
    let result = run_main_out_of_order(
        Settings::new()
            .database(None)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
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
        |n| n - 1,
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
fn pipelined_targeting_drains_and_optimises() {
    let failures = run_main_out_of_order(
        Settings::new()
            .database(None)
            .test_cases(60)
            .max_open_test_cases(3)
            .verbosity(Verbosity::Quiet),
        |ds| match rint(ds, 0, 1000) {
            Ok(v) => {
                ds.target_observation(v as f64, "score").unwrap();
                TestCaseResult::Valid
            }
            Err(()) => TestCaseResult::Overrun,
        },
        |n| n - 1,
    )
    .unwrap();
    assert!(failures.is_empty(), "{failures:?}");
}

/// The engine tolerates being resumed with cases open and nothing concluded
/// when the window allows several open cases: it just suspends again, and
/// the run still finishes once the driver resumes completing cases.
#[test]
fn resume_without_progress_is_benign_with_a_multi_case_window() {
    let settings = Settings::new()
        .database(None)
        .test_cases(10)
        .max_open_test_cases(3)
        .verbosity(Verbosity::Quiet);
    let exchange = CaseExchange::new();
    let fut = run_main(
        &settings,
        None,
        &exchange,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let mut fut = core::pin::pin!(fut);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    let mut pool: Vec<(crate::exchange::BoxedDataSource, TestCaseResult)> = Vec::new();
    let mut idle_polls = 0usize;
    let failures = loop {
        match fut.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(out) => break out.unwrap(),
            core::task::Poll::Pending => {
                while let Some(ds) = exchange.try_take() {
                    let result = match ru64(&*ds) {
                        Ok(_) => TestCaseResult::Valid,
                        Err(()) => TestCaseResult::Overrun,
                    };
                    pool.push((ds, result));
                }
                if idle_polls < 5 {
                    idle_polls += 1;
                    continue;
                }
                let (ds, result) = pool.pop().unwrap();
                ds.mark_complete(&result);
            }
        }
    };
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(idle_polls, 5, "the idle polls must actually have happened");
}

/// Under strict alternation (the default window of 1), resuming the engine
/// without completing the offered case is still the driving-contract
/// violation it always was.
#[test]
fn resuming_without_completion_at_window_one_is_a_usage_error() {
    let settings = Settings::new().database(None).verbosity(Verbosity::Quiet);
    let exchange = CaseExchange::new();
    let fut = run_main(
        &settings,
        None,
        &exchange,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let mut fut = core::pin::pin!(fut);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    let ds = exchange.take().unwrap();
    let result = match fut.as_mut().poll(&mut cx) {
        core::task::Poll::Ready(out) => out,
        core::task::Poll::Pending => panic!("expected the run to end in a usage error"),
    };
    drop(ds);
    match result {
        Err(crate::backend::RunError::UsageError(msg)) => {
            assert!(msg.contains("never marked complete"), "{msg}");
        }
        other => panic!("expected RunError::UsageError, got {other:?}"),
    }
}

#[test]
fn await_completions_with_nothing_in_flight_returns_empty() {
    with_counting_ctx(
        |_| TestCaseResult::Valid,
        async |ctx, count| {
            let runs = ctx.await_completions().await.unwrap();
            assert!(runs.is_empty());
            assert_eq!(count.get(), 0);
        },
    );
}

#[test]
fn drain_in_flight_surfaces_nondeterminism() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = AtomicUsize::new(0);
    with_counting_ctx(
        |ds| {
            let r = if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                rbool(ds).map(|_| ())
            } else {
                rint(ds, 0, 100).map(|_| ())
            };
            match r {
                Ok(()) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx, _| {
            let (run, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(
                    &[ChoiceValue::Boolean(true)],
                    None,
                    None,
                ))
                .await
                .unwrap();
            assert_eq!(run.status, Status::Valid);
            assert!(mismatch.is_none());

            ctx.spawn_case(NativeTestCase::for_choices(
                &[ChoiceValue::Boolean(true)],
                None,
                None,
            ));
            let err = ctx.drain_in_flight().await.unwrap_err();
            assert!(matches!(err, crate::backend::RunError::NonDeterministic(_)));
        },
    );
}

/// Same driving-contract violation as
/// [`resuming_without_completion_at_window_one_is_a_usage_error`], but on a
/// generation-loop case rather than the initial probe, so the pipelined
/// wait path reports it too.
#[test]
fn resuming_generation_without_completion_at_window_one_is_a_usage_error() {
    let settings = Settings::new().database(None).verbosity(Verbosity::Quiet);
    let exchange = CaseExchange::new();
    let fut = run_main(
        &settings,
        None,
        &exchange,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    let mut fut = core::pin::pin!(fut);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());

    assert!(fut.as_mut().poll(&mut cx).is_pending());
    let probe = exchange.take().unwrap();
    let result = match rbool(&*probe) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    probe.mark_complete(&result);

    assert!(fut.as_mut().poll(&mut cx).is_pending());
    let abandoned = exchange.take().unwrap();
    let result = match fut.as_mut().poll(&mut cx) {
        core::task::Poll::Ready(out) => out,
        core::task::Poll::Pending => panic!("expected the run to end in a usage error"),
    };
    drop(abandoned);
    match result {
        Err(crate::backend::RunError::UsageError(msg)) => {
            assert!(msg.contains("never marked complete"), "{msg}");
        }
        other => panic!("expected RunError::UsageError, got {other:?}"),
    }
}
