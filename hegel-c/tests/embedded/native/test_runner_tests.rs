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
        caveat: None,
    })
}

/// Create (and immediately drop) a one-rule state machine whose declared
/// concurrency bound is above 1, flipping the run into nondeterministic
/// handling at the end of the executing case.
fn concurrent_machine(ds: &dyn DataSource) -> Result<(), TestCaseResult> {
    match ds.new_state_machine(
        vec!["rule".to_string()],
        vec![0],
        alloc::vec::Vec::new(),
        2,
        2,
    ) {
        Ok(_) => Ok(()),
        Err(_) => Err(TestCaseResult::Overrun),
    }
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
) -> Result<crate::backend::TestRunResult, crate::backend::RunError> {
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

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        crate::native::core::choices::IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
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
fn cached_test_function_predicts_overrun_for_truncated_known_path() {
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
        async |ctx, count| {
            let full = [ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)];
            let first = ctx.cached_test_function(&full, None, 0).await.unwrap();
            assert_eq!(first.status, Status::Valid);
            assert_eq!(count.get(), 1);

            let truncated = [ChoiceValue::Boolean(false)];
            let predicted = ctx.cached_test_function(&truncated, None, 0).await.unwrap();
            assert_eq!(predicted.status, Status::EarlyStop);
            assert_eq!(count.get(), 1, "a predicted overrun must not run the body");
            assert_eq!(predicted.nodes.len(), 1);

            let again = ctx.cached_test_function(&truncated, None, 0).await.unwrap();
            assert_eq!(again.status, Status::EarlyStop);
            assert_eq!(count.get(), 1);
        },
    );
}

#[test]
fn cached_test_function_probe_executes_past_a_predicted_overrun() {
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
        async |ctx, count| {
            let full = [ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)];
            ctx.cached_test_function(&full, None, 0).await.unwrap();
            assert_eq!(count.get(), 1);

            let prefix = [ChoiceValue::Boolean(false)];
            let run = ctx.cached_test_function(&prefix, None, 1).await.unwrap();
            assert_eq!(run.status, Status::Valid);
            assert_eq!(
                count.get(),
                2,
                "a probe must execute the body to draw its continuation"
            );
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
            let run = ctx
                .execute(NativeTestCase::for_choices(&[], None, None))
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

/// Drive [`reproduce_blob`] to completion with a synchronous body.
fn reproduce_blob_sync(
    settings: &Settings,
    blob: &str,
    mut body: impl FnMut(&dyn DataSource) -> TestCaseResult,
) -> Result<crate::backend::TestRunResult, crate::backend::RunError> {
    let exchange = CaseExchange::new();
    crate::exchange::drive(&exchange, reproduce_blob(settings, blob, &exchange), |ds| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    })
}

fn quiet_settings() -> Settings {
    Settings::new().database(None).verbosity(Verbosity::Quiet)
}

#[test]
fn reproduce_blob_rejects_an_undecodable_blob_as_the_runs_error() {
    let err = reproduce_blob_sync(&quiet_settings(), "!!! junk !!!", |_| TestCaseResult::Valid)
        .unwrap_err();
    let crate::backend::RunError::UsageError(msg) = err else {
        panic!("expected a usage error, got {err:?}");
    };
    assert!(msg.contains("could not be decoded"), "{msg}");
}

#[test]
fn reproduce_blob_replays_a_deterministic_blob_exactly_once() {
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(true)]);
    let mut calls = 0u32;
    let mut stamped = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        stamped += u32::from(ds.is_nondeterministic());
        match rbool(ds) {
            Ok(true) => boom("deterministic replay"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        }
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(stamped, 1, "the replay is stamped for capture");
    assert_eq!(result.failures.len(), 1);
    let failure = &result.failures[0];
    assert_eq!(failure.origin, "Panic: deterministic replay");
    assert!(failure.reproduce_blob.is_none());
    assert!(failure.caveat.is_none());
}

#[test]
fn reproduce_blob_reports_a_stale_deterministic_blob_as_passed() {
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(false)]);
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| match rbool(ds) {
        Ok(true) => boom("never"),
        Ok(false) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    })
    .unwrap();
    assert!(result.failures.is_empty());
}

/// A one-timeline ND blob whose single stored choice is a `true` boolean.
fn nd_blob() -> String {
    crate::native::blob::encode_nd_failure(&crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 7,
        extension: 4,
    })
}

#[test]
fn reproduce_blob_replays_an_nd_blob_until_a_replay_fails() {
    let mut calls = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &nd_blob(), |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        calls += 1;
        if calls == 3 {
            boom("third replay")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(calls, 3, "the replay loop retries past the early misses");
    assert_eq!(result.failures.len(), 1);
    let failure = &result.failures[0];
    assert_eq!(failure.origin, "Panic: third replay");
    assert!(failure.reproduce_blob.is_none());
    assert_eq!(
        failure.caveat.as_deref(),
        Some("nondeterministic failure: reproduced from the stored entry this run")
    );
}

#[test]
fn reproduce_blob_reports_an_exhausted_nd_blob_as_passed() {
    let mut calls = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &nd_blob(), |ds| {
        calls += 1;
        match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        }
    })
    .unwrap();
    assert!(result.failures.is_empty());
    assert!(calls > 1, "a stale ND blob is retried before giving up");
}

#[test]
fn reproduce_blob_replays_an_nd_blob_under_error_strictness() {
    let mut settings = quiet_settings();
    settings.nondeterminism_strictness = NondeterminismStrictness::Error;
    let result = reproduce_blob_sync(&settings, &nd_blob(), |ds| match rbool(ds) {
        Ok(true) => boom("error strictness"),
        Ok(false) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: error strictness");
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
    let result = exploration.unwrap();
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
    let result = exploration.unwrap();
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
    let result = exploration.unwrap();
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
    let result = exploration;
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
    run_main_sync(
        &settings,
        Some(key),
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
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
        2,
        "expected exactly one reuse replay plus the final replay, no generation"
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
            .nondeterminism_strictness(NondeterminismStrictness::Error)
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
            .nondeterminism_strictness(NondeterminismStrictness::Error)
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

/// Same kind-switch body as [`shrink_verify_surfaces_generator_nondeterminism`],
/// but with no shrink phase the execution after the first failure is the
/// report-time final replay — its kind mismatch must abort under `Error`
/// strictness too.
#[test]
fn final_replay_surfaces_generator_nondeterminism_under_error_strictness() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate])
            .report_multiple_failures(false)
            .nondeterminism_strictness(NondeterminismStrictness::Error)
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
fn a_concurrent_run_shrinks_and_reports_a_caveated_blob() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            if let Err(result) = concurrent_machine(ds) {
                return result;
            }
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
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("stable origin"));
    assert!(result.failures[0].reproduce_blob.is_some());
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(caveat.starts_with("nondeterministic failure"), "{caveat}");
    assert!(
        seen_bug.load(Ordering::SeqCst),
        "the bug must have been discovered by generation"
    );
}

#[test]
fn a_concurrent_one_shot_bug_is_reported_unconfirmed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let failed_once = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            if let Err(result) = concurrent_machine(ds) {
                return result;
            }
            match rbool(ds) {
                Ok(true) if !failed_once.swap(true, Ordering::SeqCst) => boom("racy origin"),
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("racy origin"));
    assert!(result.failures[0].reproduce_blob.is_none());
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.starts_with("unconfirmed failure: failed 0 of"),
        "{caveat}"
    );
}

#[test]
fn a_concurrent_reuse_run_persists_v2_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let seeded = serialize_choices(&[ChoiceValue::Boolean(true)]);
    db.save(b"k", &seeded);

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if let Err(result) = concurrent_machine(ds) {
                return result;
            }
            boom("db origin")
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].reproduce_blob.is_some());
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(caveat.starts_with("nondeterministic failure"), "{caveat}");
    let primary = db.fetch(b"k");
    assert!(!primary.is_empty());
    assert!(
        primary
            .iter()
            .all(|e| crate::native::blob::decode_nd_state(e).is_some()),
        "a concurrent run persists version-2 entries"
    );
    let secondary = crate::native::data_tree::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(&secondary).contains(&seeded),
        "the stale v1 entry is demoted, not deleted"
    );
}

#[test]
fn a_concurrent_machine_notices_only_under_warn_strictness() {
    use std::sync::{Arc, Mutex};
    for (strictness, expected) in [
        (NondeterminismStrictness::Quiet, 0),
        (NondeterminismStrictness::Warn, 1),
    ] {
        let lines: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&lines);
        let result = reuse_run(
            Settings::new()
                .database(None)
                .test_cases(5)
                .nondeterminism_strictness(strictness)
                .output(Output::callback(move |line| {
                    sink.lock().unwrap().push(line.to_string());
                })),
            "k",
            |ds| {
                if let Err(result) = concurrent_machine(ds) {
                    return result;
                }
                match rbool(ds) {
                    Ok(_) => TestCaseResult::Valid,
                    Err(()) => TestCaseResult::Overrun,
                }
            },
        )
        .unwrap();
        assert!(result.failures.is_empty());
        let notices = lines
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.contains("Nondeterministic test behavior detected"))
            .count();
        assert_eq!(notices, expected, "under {expected} expected notices");
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
            .nondeterminism_strictness(NondeterminismStrictness::Error)
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
            .nondeterminism_strictness(NondeterminismStrictness::Error)
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
    let result = exploration.unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("child too big"));

    let blob = result.failures[0].reproduce_blob.as_ref().unwrap();
    let crate::native::blob::DecodedBlob::Choices(choices) =
        crate::native::blob::decode_blob(blob).unwrap()
    else {
        panic!("expected a deterministic blob");
    };
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

/// Settings entering ND handling directly, for lifecycle tests.
fn nd_settings() -> Settings {
    let mut settings = Settings::new().database(None).verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    settings
}

/// Like [`with_counting_ctx`] but with caller-supplied settings and database
/// key, for tests that drive `Engine` internals under ND handling.
fn with_engine<T, B>(settings: Settings, key: Option<&str>, mut body: T, after: B)
where
    T: FnMut(&dyn DataSource) -> TestCaseResult,
    B: AsyncFnOnce(&mut Engine<'_>),
{
    let exchange = CaseExchange::new();
    let fut = async {
        let mut ctx = Engine::new(&settings, key, &exchange).unwrap();
        after(&mut ctx).await;
    };
    crate::exchange::drive(&exchange, fut, |ds| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    });
}

/// An interesting [`RunResult`] at `origin` realizing `nodes`, standing in
/// for a raw execution's outcome.
fn interesting_at(origin: &str, nodes: Vec<ChoiceNode>) -> RunResult {
    RunResult {
        status: Status::Interesting,
        nodes,
        spans: Vec::new(),
        origin: Some(origin.to_string()),
        target_observations: crate::native::HashMap::default(),
        span_events: Vec::new(),
        events: Vec::new(),
    }
}

#[test]
fn nd_raw_interesting_never_displaces_an_occupied_origin() {
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let origin = "Panic: bug";
            let big = vec![bool_node(true), bool_node(true)];
            ctx.record_run(&interesting_at(origin, big), Duration::ZERO, false);
            assert_eq!(ctx.interesting.get(origin).unwrap().len(), 2);

            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            );
            assert_eq!(
                ctx.interesting.get(origin).unwrap().len(),
                2,
                "a raw interesting run must not displace an occupied origin"
            );

            ctx.nd_origins
                .confirm(origin, 0.9, None, Vec::new(), (4, 4));
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            );
            assert_eq!(ctx.interesting.get(origin).unwrap().len(), 2);
        },
    );
}

#[test]
fn nd_admission_leaves_the_origin_unconfirmed() {
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            assert!(ctx.interesting.contains_key("Panic: bug"));
            assert!(ctx.nd_origins.needs_confirmation("Panic: bug"));
        },
    );
}

#[test]
fn nd_discovery_sweep_evicts_an_unconfirmable_origin() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let settings = nd_settings().output(Output::callback(move |line| {
        sink.lock().unwrap().push(line.to_string());
    }));
    with_engine(
        settings,
        None,
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: fluke", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            let output = ctx.settings.output.clone();
            ctx.nd_discovery_sweep(Verbosity::Debug, &output)
                .await
                .unwrap();
            assert!(ctx.interesting.is_empty());
            assert_eq!(
                ctx.nd_origins.unconfirmed().collect::<Vec<_>>(),
                vec!["Panic: fluke"]
            );
        },
    );
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("nd discovery confirm"))
    );
}

#[test]
fn nd_discovery_sweep_confirms_a_real_failure() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() || rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            let output = ctx.settings.output.clone();
            ctx.nd_discovery_sweep(Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert!(!ctx.nd_origins.needs_confirmation("Panic: bug"));
            assert!(
                ctx.nd_origins.pool("Panic: bug").len() > 1,
                "confirmation replays realizing fresh continuations must be captured"
            );
            let (witness, anchor) = ctx.nd_origins.take_witness("Panic: bug").unwrap();
            assert_eq!(witness.origin.as_deref(), Some("Panic: bug"));
            assert!(anchor > 0.0);
            assert!(ctx.interesting.contains_key("Panic: bug"));
        },
    );
}

#[test]
fn nd_persists_only_validated_incumbents() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    with_engine(
        settings,
        Some("k"),
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            assert!(
                db.fetch(b"k").is_empty(),
                "raw interesting must not persist before confirmation"
            );
            let output = ctx.settings.output.clone();
            ctx.nd_discovery_sweep(Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert!(
                !db.fetch(b"k").is_empty(),
                "confirmation commits the validated incumbent"
            );
        },
    );
}

#[test]
fn nd_boost_raises_the_anchor_or_declines() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let mut settings = Settings::new()
        .database(None)
        .verbosity(Verbosity::Debug)
        .output(Output::callback(move |line| {
            sink.lock().unwrap().push(line.to_string());
        }));
    settings.nd_force = true;
    with_engine(
        settings,
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            let incumbent = vec![ChoiceValue::Boolean(true)];
            ctx.nd_origins.confirm(
                "Panic: bug",
                0.1,
                None,
                vec![vec![ChoiceValue::Boolean(false)]],
                (4, 9),
            );
            let (witness, lcb) = ctx
                .nd_boost("Panic: bug", &incumbent, 0.0)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(witness.origin.as_deref(), Some("Panic: bug"));
            assert!(lcb > 0.5);
            assert!(
                ctx.nd_boost("Panic: bug", &incumbent, 0.99)
                    .await
                    .unwrap()
                    .is_none()
            );
        },
    );
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("nd boost: origin=Panic: bug anchor 0.000 -> ")),
        "an accepted boost reports the anchor move at Debug verbosity"
    );
}

#[test]
fn nd_gauntlet_probe_rejects_a_candidate_that_stops_reproducing() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) == 0 {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let output = ctx.settings.output.clone();
            let mut probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: "Panic: bug".to_string(),
                verbosity: Verbosity::Quiet,
                output,
                gauntlet: true,
                ledger: HashMap::default(),
                raised: crate::native::HashSet::default(),
                anchor: 0.99,
                sweep: SweepMode::Fast,
                pending_accept: None,
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(
                !matched,
                "the evidence upper bound must fall below the anchor-derived threshold"
            );
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(!matched, "a non-matching first run is a single-run reject");
        },
    );
}

#[test]
fn nd_gauntlet_run_shrinks_a_deterministic_core_end_to_end() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(20)
        .verbosity(Verbosity::Debug)
        .output(Output::callback(move |line| {
            sink.lock().unwrap().push(line.to_string());
        }));
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("always")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("Panic: always"));
    assert!(result.failures[0].reproduce_blob.is_some());
    assert!(
        !lines.lock().unwrap().iter().any(|l| l.contains("nd boost")),
        "an always-failing origin confirms above the reliability floor, so boost is skipped"
    );
}

#[test]
fn nd_gauntlet_run_holds_a_flaky_failure_end_to_end() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(20)
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if execs.fetch_add(1, Ordering::SeqCst) % 5 != 0 {
            boom("flaky")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("Panic: flaky"));
    assert!(
        !result.failures[0].origin.contains("[unconfirmed"),
        "an 80%-failure bug must confirm"
    );
    assert!(result.failures[0].reproduce_blob.is_some());
}

#[test]
fn nd_reports_each_origin_with_its_own_caveat_and_blob() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(30)
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let first = match rbool(ds) {
            Ok(v) => v,
            Err(()) => return TestCaseResult::Overrun,
        };
        if execs.fetch_add(1, Ordering::SeqCst) % 5 == 4 {
            return TestCaseResult::Valid;
        }
        if first { boom("one") } else { boom("two") }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 2);
    let mut origins: Vec<&str> = result.failures.iter().map(|f| f.origin.as_str()).collect();
    origins.sort_unstable();
    assert_eq!(origins, ["Panic: one", "Panic: two"]);
    let blobs: Vec<&str> = result
        .failures
        .iter()
        .map(|f| f.reproduce_blob.as_deref().unwrap())
        .collect();
    assert_ne!(blobs[0], blobs[1], "each origin encodes its own incumbent");
    for f in &result.failures {
        let caveat = f.caveat.as_deref().unwrap();
        assert!(
            caveat.starts_with("nondeterministic failure"),
            "each origin carries its own confirmed caveat: {caveat}"
        );
    }
}

#[test]
fn nd_unconfirmed_failure_is_reported_with_a_caveat() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(10)
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if execs.fetch_add(1, Ordering::SeqCst) == 1 {
            boom("once")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: once");
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.starts_with("unconfirmed failure: failed 0 of "),
        "zero reproductions must weight the environment hypothesis: {caveat}"
    );
    assert!(caveat.contains("environment"));
    assert!(result.failures[0].reproduce_blob.is_none());
}

#[test]
fn nd_reuse_retries_a_stored_flaky_timeline_until_it_reproduces() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .phases([Phase::Reuse])
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if execs.fetch_add(1, Ordering::SeqCst) % 3 == 2 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("Panic: bug"));
    assert!(
        !db.fetch(b"k").is_empty(),
        "a reproduced entry stays in the primary corpus"
    );
}

#[test]
fn nd_misaligned_trusted_reuse_confirms_at_shrink_and_persists_its_pool() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let n = execs.fetch_add(1, Ordering::SeqCst);
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if n % 2 == 0 && rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if n == 4 {
            boom("a fluke discovered mid-shrink")
        } else if n % 3 == 2 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(
        result.failures.len(),
        1,
        "the mid-shrink fluke must face the full bar and be evicted"
    );
    assert!(result.failures[0].origin.contains("Panic: bug"));
    assert!(
        !result.failures[0].origin.contains("[unconfirmed"),
        "a trusted origin confirmed at shrink time reports as a plain failure"
    );
    assert!(!db.fetch(b"k").is_empty());
}

#[test]
fn nd_trusted_failure_that_stops_reproducing_is_still_reported() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    let execs = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let first = match rbool(ds) {
            Ok(v) => v,
            Err(()) => return TestCaseResult::Overrun,
        };
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        let n = execs.fetch_add(1, Ordering::SeqCst);
        if n == 2 && first {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("Panic: bug"));
    assert!(
        !result.failures[0].origin.contains("[unconfirmed"),
        "a database-trusted origin is never demoted to a caveat"
    );
}

/// Draws a boolean, fails on `true`, and permanently switches the follow-up
/// draw's kind after the first failure, so the pre-shrink verification replay
/// hits a kind mismatch — the schedule
/// `shrink_verify_surfaces_generator_nondeterminism` pins the `Error` abort
/// on.
fn kind_switch_body(
    seen_bug: &std::sync::atomic::AtomicBool,
    ds: &dyn DataSource,
) -> TestCaseResult {
    use std::sync::atomic::Ordering;
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
}

#[test]
fn structure_flip_under_quiet_recovers_the_failure_without_a_notice() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .report_multiple_failures(false)
            .output(Output::callback(move |line| {
                sink.lock().unwrap().push(line.to_string());
            })),
        "k",
        |ds| kind_switch_body(&seen_bug, ds),
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: stable origin");
    assert!(result.failures[0].reproduce_blob.is_some());
    assert!(
        !lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("Nondeterministic test behavior detected")),
        "quiet strictness prints no notice"
    );
}

#[test]
fn structure_flip_under_warn_prints_the_notice_once() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .report_multiple_failures(false)
            .nondeterminism_strictness(NondeterminismStrictness::Warn)
            .output(Output::callback(move |line| {
                sink.lock().unwrap().push(line.to_string());
            })),
        "k",
        |ds| kind_switch_body(&seen_bug, ds),
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: stable origin");
    let notices = lines
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("Nondeterministic test behavior detected"))
        .count();
    assert_eq!(notices, 1, "the notice is printed exactly once per run");
}

#[test]
fn reuse_kind_flip_under_quiet_completes_without_failures() {
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
    )
    .unwrap();
    assert!(result.failures.is_empty());
}

#[test]
fn outcome_flake_under_quiet_confirms_and_shrinks_the_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    // Valid at exec 0 (generation) and exec 2 (the pre-shrink verification
    // replay, whose non-failure triggers the flip); failing everywhere else,
    // so confirmation accepts and the shrink runs the gauntlet.
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
            let n = execs.fetch_add(1, Ordering::SeqCst);
            if n == 0 || n == 2 {
                TestCaseResult::Valid
            } else {
                boom("outcome")
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: outcome");
    assert!(result.failures[0].reproduce_blob.is_some());
}

#[test]
fn outcome_flake_under_quiet_that_never_reproduces_reports_a_caveat() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
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
            if execs.fetch_add(1, Ordering::SeqCst) == 1 {
                boom("outcome")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: outcome");
    assert!(
        result.failures[0]
            .caveat
            .as_deref()
            .unwrap()
            .starts_with("unconfirmed failure: failed 0 of ")
    );
    assert!(result.failures[0].reproduce_blob.is_none());
}

#[test]
fn a_double_flip_in_one_verify_prints_the_warn_notice_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let execs = AtomicUsize::new(0);
    // Exec 0 draws over [0, 100] and fails; every later exec widens the range
    // to [0, 101] and passes. The pre-shrink verification replay then flips
    // twice — once for the constraint mismatch against the recorded tree,
    // once for the vanished failure — and the second flip must be a no-op.
    let result = reuse_run(
        Settings::new()
            .database(None)
            .phases([Phase::Generate, Phase::Shrink])
            .report_multiple_failures(false)
            .nondeterminism_strictness(NondeterminismStrictness::Warn)
            .output(Output::callback(move |line| {
                sink.lock().unwrap().push(line.to_string());
            })),
        "k",
        |ds| {
            let n = execs.fetch_add(1, Ordering::SeqCst);
            let hi = if n == 0 { 100 } else { 101 };
            if rint(ds, 0, hi).is_err() {
                return TestCaseResult::Overrun;
            }
            if n == 0 {
                boom("shift")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: shift");
    assert!(result.failures[0].caveat.is_some());
    let notices = lines
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("Nondeterministic test behavior detected"))
        .count();
    assert_eq!(notices, 1, "the notice is printed exactly once per run");
}

#[test]
fn nd_handling_confirms_and_shrinks_a_clone_bearing_body() {
    let result = reuse_run(
        nd_settings()
            .phases([Phase::Generate, Phase::Shrink])
            .test_cases(200),
        "k",
        |ds| {
            let a = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            let clone = match ds.clone_stream() {
                Ok(c) => c,
                Err(_) => return TestCaseResult::Overrun,
            };
            let b = match rbool(&*clone) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            if a && b {
                boom("clone bug")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: clone bug");
    assert!(result.failures[0].reproduce_blob.is_some());
}

#[test]
fn measurement_runs_move_no_counters_but_still_admit_origins() {
    with_engine(
        Settings::new().database(None).verbosity(Verbosity::Quiet),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.nd_active = true;
            let valid = RunResult {
                status: Status::Valid,
                nodes: vec![bool_node(true)],
                spans: Vec::new(),
                origin: None,
                target_observations: crate::native::HashMap::default(),
                span_events: Vec::new(),
                events: Vec::new(),
            };
            ctx.record_run(&valid, Duration::from_secs(1), true);
            ctx.record_run(
                &interesting_at("Panic: measured", vec![bool_node(true)]),
                Duration::ZERO,
                true,
            );
            assert_eq!(ctx.calls, 0);
            assert_eq!(ctx.valid_test_cases, 0);
            assert_eq!(ctx.total_test_time, Duration::ZERO);
            assert!(ctx.first_bug_at.is_none());
            assert!(ctx.last_bug_at.is_none());
            assert!(
                ctx.interesting.contains_key("Panic: measured"),
                "a measurement run still admits a vacant origin"
            );
            assert!(ctx.nd_origins.needs_confirmation("Panic: measured"));

            ctx.record_run(&valid, Duration::from_secs(1), false);
            assert_eq!(ctx.calls, 1);
            assert_eq!(ctx.valid_test_cases, 1);
            assert_eq!(ctx.total_test_time, Duration::from_secs(1));
        },
    );
}

#[test]
fn a_diverged_replay_miss_carries_the_verbatim_watermark_weight() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let diverge = AtomicBool::new(false);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            let second = if diverge.load(Ordering::SeqCst) {
                rint(ds, 0, 100).map(|_| ())
            } else {
                rbool(ds).map(|_| ())
            };
            match second {
                Ok(()) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx| {
            let stored = vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)];
            let replay = ctx.nd_replay_once(&stored, None).await.unwrap();
            assert!(!replay.failed);
            assert_eq!(replay.weight, 1.0, "an aligned miss weighs in full");
            diverge.store(true, Ordering::SeqCst);
            let replay = ctx.nd_replay_once(&stored, None).await.unwrap();
            assert!(!replay.failed);
            assert_eq!(
                replay.weight, 0.5,
                "a replay that diverged after tracking half the timeline weighs half"
            );
        },
    );
}

#[test]
fn nd_reproduce_rescues_a_pool_miss_with_a_positional_splice() {
    with_engine(
        nd_settings().seed(Some(3)),
        None,
        |ds| {
            let (a, b) = match (rbool(ds), rbool(ds)) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return TestCaseResult::Overrun,
            };
            if a && b {
                boom("splice")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let stored = vec![
                vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(false)],
                vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
            ];
            let (run, evidence) = ctx
                .nd_reproduce(Some("Panic: splice"), &stored, 2.0, 50, 0)
                .await
                .unwrap();
            let run = run.unwrap();
            assert_eq!(run.origin.as_deref(), Some("Panic: splice"));
            assert!(
                evidence.runs() > 4,
                "both timelines face the first-fit tier before the splices"
            );
        },
    );
}

#[test]
fn a_positional_splice_carries_whole_clone_records_across_intact() {
    use crate::native::core::CloneRecord;
    use alloc::sync::Arc;
    let clone_of = |v: i64| {
        ChoiceValue::Clone(Arc::new(CloneRecord::from_values(vec![
            ChoiceValue::Integer(BigInt::from(v)),
        ])))
    };
    with_engine(
        nd_settings().seed(Some(3)),
        None,
        |ds| {
            let armed = match rbool(ds) {
                Ok(b) => b,
                Err(()) => return TestCaseResult::Overrun,
            };
            let child = match ds.clone_stream() {
                Ok(c) => c,
                Err(_) => return TestCaseResult::Overrun,
            };
            match rint(&*child, 0, 1000) {
                Ok(x) if armed && x >= 500 => boom("clone splice"),
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx| {
            let stored = vec![
                vec![ChoiceValue::Boolean(true), clone_of(0)],
                vec![ChoiceValue::Boolean(false), clone_of(600)],
            ];
            let (run, _) = ctx
                .nd_reproduce(Some("Panic: clone splice"), &stored, 2.0, 50, 0)
                .await
                .unwrap();
            let run = run.unwrap();
            assert_eq!(run.origin.as_deref(), Some("Panic: clone splice"));
            let ChoiceValue::Clone(record) = run.nodes[1].value() else {
                panic!("the clone position survives the splice: {:?}", run.nodes);
            };
            assert_eq!(
                record.owned_values(),
                vec![ChoiceValue::Integer(BigInt::from(600))],
                "the spliced timeline replays the clone record verbatim"
            );
        },
    );
}

#[test]
fn nd_reproduce_falls_back_to_fresh_generation_and_reports_a_dry_pool() {
    with_engine(
        nd_settings().seed(Some(1)),
        None,
        |ds| match rbool(ds) {
            Ok(true) => boom("fresh"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            let stored = vec![vec![ChoiceValue::Boolean(false)]];
            let (run, _) = ctx
                .nd_reproduce(Some("Panic: fresh"), &stored, 2.0, 0, 0)
                .await
                .unwrap();
            assert!(run.is_none(), "the stored timeline never fails");
            let (run, _) = ctx
                .nd_reproduce(Some("Panic: fresh"), &stored, 2.0, 0, 40)
                .await
                .unwrap();
            assert!(
                run.is_some(),
                "fresh generation past a dry pool still finds the failure"
            );
        },
    );
}

#[test]
fn gauntlet_reruns_are_measurement_runs_and_a_reaccept_never_raises_the_anchor_again() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("boom")
        },
        async |ctx| {
            let mut probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: "Panic: boom".to_string(),
                verbosity: Verbosity::Quiet,
                output: Output::callback(|_| {}),
                gauntlet: true,
                ledger: HashMap::default(),
                raised: crate::native::HashSet::default(),
                anchor: 0.3,
                sweep: SweepMode::Fast,
                pending_accept: None,
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(matched);
            probe.candidate_adopted();
            let raised_anchor = probe.anchor;
            assert!(
                raised_anchor > 0.3,
                "the first adopted accept of a candidate raises the monotone anchor"
            );
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(matched);
            probe.candidate_adopted();
            assert_eq!(
                probe.anchor, raised_anchor,
                "re-accepting the same timeline draws on replay evidence and \
                 must not keep raising the anchor"
            );
            assert_eq!(
                ctx.calls, 2,
                "the two candidate proposals count; their gauntlet reruns are \
                 measurement runs and do not"
            );
        },
    );
}

#[test]
fn nd_failures_persist_v2_state_and_reproduce_across_runs() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let execs = AtomicUsize::new(0);
    let result = reuse_run(
        {
            let mut s = Settings::new()
                .database(Some(path.clone()))
                .phases([Phase::Generate, Phase::Shrink])
                .verbosity(Verbosity::Quiet);
            s.nd_force = true;
            s
        },
        "k",
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                boom("nd")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let blob = result.failures[0].reproduce_blob.as_deref().unwrap();
    assert!(
        matches!(
            crate::native::blob::decode_blob(blob),
            Some(crate::native::blob::DecodedBlob::Nd(_))
        ),
        "a nondeterministic failure's blob carries replay state"
    );

    let db = DirectoryTestCaseDatabase::new(&path);
    let entries = db.fetch(b"k");
    assert_eq!(entries.len(), 1, "one v2 entry per confirmed origin");
    assert!(
        deserialize_choices(&entries[0]).is_none(),
        "a v1 reader rejects the v2 entry instead of misreading it"
    );
    let state = crate::native::blob::decode_nd_state(&entries[0]).unwrap();
    assert!(!state.timelines.is_empty());

    let execs = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                boom("nd")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(
        result.failures.len(),
        1,
        "a later run reproduces the flaky failure from its v2 entry alone"
    );
    assert_eq!(result.failures[0].origin, "Panic: nd");
    let blob = result.failures[0].reproduce_blob.as_deref().unwrap();
    assert!(matches!(
        crate::native::blob::decode_blob(blob),
        Some(crate::native::blob::DecodedBlob::Nd(_))
    ));
    assert_eq!(
        db.fetch(b"k").len(),
        1,
        "an aligned reuse hit re-persists the stored state"
    );
}

#[test]
fn stale_nd_entries_demote_to_secondary_then_delete() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state));

    let settings = || {
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .verbosity(Verbosity::Quiet)
    };
    let body = |ds: &dyn DataSource| match rbool(ds) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    let result = reuse_run(settings(), "k", body).unwrap();
    assert!(result.failures.is_empty());
    let secondary = crate::native::data_tree::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(b"k").is_empty(),
        "a primary miss demotes the entry instead of deleting it"
    );
    assert_eq!(db.fetch(&secondary).len(), 1);

    let result = reuse_run(settings(), "k", body).unwrap();
    assert!(result.failures.is_empty());
    assert!(db.fetch(b"k").is_empty());
    assert!(
        db.fetch(&secondary).is_empty(),
        "a secondary miss deletes the entry"
    );
}

#[test]
fn nd_reproduction_holds_across_runs_for_a_family_of_flaky_bodies() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for period in [3usize, 4, 5] {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap().to_string();
        let body = |execs: &AtomicUsize, ds: &dyn DataSource| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) % period == period - 1 {
                boom("periodic")
            } else {
                TestCaseResult::Valid
            }
        };

        let execs = AtomicUsize::new(0);
        let result = reuse_run(
            {
                let mut s = Settings::new()
                    .database(Some(path.clone()))
                    .phases([Phase::Generate, Phase::Shrink])
                    .verbosity(Verbosity::Quiet);
                s.nd_force = true;
                s
            },
            "k",
            |ds| body(&execs, ds),
        )
        .unwrap();
        assert_eq!(
            result.failures.len(),
            1,
            "period {period}: first run finds the bug"
        );

        let execs = AtomicUsize::new(0);
        let result = reuse_run(
            Settings::new()
                .database(Some(path.clone()))
                .phases([Phase::Reuse])
                .verbosity(Verbosity::Quiet),
            "k",
            |ds| body(&execs, ds),
        )
        .unwrap();
        assert_eq!(
            result.failures.len(),
            1,
            "period {period}: the second run reproduces from the v2 entry"
        );
        assert_eq!(result.failures[0].origin, "Panic: periodic");
    }
}

#[test]
fn a_v2_entry_replay_that_detects_nondeterminism_under_error_strictness_aborts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state));

    let execs = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .nondeterminism_strictness(NondeterminismStrictness::Error)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            let r = if execs.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                rbool(ds).map(|_| ())
            } else {
                rint(ds, 0, 100).map(|_| ())
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
fn a_fresh_tier_replay_that_detects_nondeterminism_under_error_strictness_aborts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        Settings::new()
            .database(None)
            .nondeterminism_strictness(NondeterminismStrictness::Error)
            .verbosity(Verbosity::Quiet),
        None,
        |ds| {
            let r = if execs.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                rbool(ds).map(|_| ())
            } else {
                rint(ds, 0, 100).map(|_| ())
            };
            match r {
                Ok(()) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx| {
            let stored = vec![vec![ChoiceValue::Boolean(true)]];
            let result = ctx.nd_reproduce(None, &stored, 0.0, 0, 4).await;
            match result {
                Err(crate::backend::RunError::NonDeterministic(msg)) => {
                    assert!(
                        msg.to_lowercase().contains("non-deterministic"),
                        "got: {msg}"
                    );
                }
                Err(other) => panic!("expected RunError::NonDeterministic, got {other:?}"),
                Ok(_) => panic!("expected RunError::NonDeterministic, got a result"),
            }
        },
    );
}

#[test]
fn nd_shrinking_never_lowers_the_failure_probability_at_the_noise_floor() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let bug_ticks = AtomicUsize::new(0);
    let noise_ticks = AtomicUsize::new(0);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(30)
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let Ok(n) = rint(ds, 0, 5) else {
            return TestCaseResult::Overrun;
        };
        let mut bug = false;
        for _ in 0..n {
            match rint(ds, 0, 20) {
                Ok(v) => bug |= v >= 10,
                Err(()) => return TestCaseResult::Overrun,
            }
        }
        if bug {
            if bug_ticks.fetch_add(1, Ordering::SeqCst) % 10 != 9 {
                return boom("bug");
            }
        } else if noise_ticks.fetch_add(1, Ordering::SeqCst) % 50 == 0 {
            return boom("bug");
        }
        TestCaseResult::Valid
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].origin.contains("Panic: bug"));
    let blob = result.failures[0].reproduce_blob.as_ref().unwrap();
    let crate::native::blob::DecodedBlob::Nd(state) =
        crate::native::blob::decode_blob(blob).unwrap()
    else {
        panic!("expected an nd blob");
    };
    let incumbent = state.incumbent();
    let atoms: Vec<i128> = incumbent[1..]
        .iter()
        .map(|v| match v {
            ChoiceValue::Integer(n) => i128::try_from(n).unwrap(),
            other => panic!("expected integer atoms, got {other:?}"),
        })
        .collect();
    assert!(
        atoms.iter().any(|&v| v >= 10),
        "the final example must keep the p = 0.9 bug rather than drift to the \
         p = 0.02 noise floor: {atoms:?}"
    );
    assert_eq!(
        atoms,
        vec![10],
        "the L4 standard: one atom, minimized to the bug boundary"
    );
}

#[test]
fn gauntlet_depth_charges_the_deadline_not_the_logical_counters() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let bug_runs = AtomicUsize::new(0);
    let total_runs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            total_runs.fetch_add(1, Ordering::SeqCst);
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 10 && bug_runs.fetch_add(1, Ordering::SeqCst) % 3 != 0 {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let output = ctx.settings.output.clone();
            let probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: "Panic: bug".to_string(),
                verbosity: Verbosity::Quiet,
                output,
                gauntlet: true,
                ledger: HashMap::default(),
                raised: crate::native::HashSet::default(),
                anchor: 0.3,
                sweep: SweepMode::Fast,
                pending_accept: None,
            };
            let mut shrinker =
                Shrinker::with_probe(Box::new(probe), vec![int_node(47)], Spans::new());
            shrinker.shrink().await.unwrap();
            assert_eq!(shrinker.current_nodes.len(), 1);
            assert_eq!(
                shrinker.current_nodes[0].value(),
                ChoiceValue::Integer(BigInt::from(10)),
                "the confirmed-dry stop must land on the true boundary"
            );
            assert!(!shrinker.timed_out);
            assert!(
                shrinker.calls < total_runs.load(Ordering::SeqCst),
                "gauntlet reruns are physical only: {} logical calls, {} executions",
                shrinker.calls,
                total_runs.load(Ordering::SeqCst)
            );
            assert!(
                bug_runs.load(Ordering::SeqCst) > 0,
                "accepts must have been gauntleted"
            );
        },
    );
}

#[test]
fn pooled_timelines_caps_at_pool_cap_and_dedupes() {
    let incumbent = vec![ChoiceValue::Boolean(false)];
    let mut rest: Vec<Vec<ChoiceValue>> = (0..nd::POOL_CAP + 3)
        .map(|i| vec![ChoiceValue::Boolean(true); i + 1])
        .collect();
    rest.insert(0, incumbent.clone());
    rest.insert(2, vec![ChoiceValue::Boolean(true)]);
    let timelines = pooled_timelines(incumbent.clone(), rest);
    assert_eq!(timelines.len(), nd::POOL_CAP);
    assert_eq!(timelines[0], incumbent);
    for (i, t) in timelines.iter().enumerate() {
        assert!(!timelines[i + 1..].contains(t));
    }
}

#[test]
fn nd_state_for_caps_stored_timelines_at_pool_cap() {
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let pool: Vec<Vec<ChoiceValue>> = (0..nd::POOL_CAP)
                .map(|i| vec![ChoiceValue::Boolean(true); i + 1])
                .collect();
            ctx.nd_origins
                .confirm("Panic: bug", 0.5, None, pool, (4, 6));
            let incumbent = vec![ChoiceValue::Boolean(false)];
            let state = ctx.nd_state_for("Panic: bug", incumbent.clone());
            assert_eq!(state.timelines.len(), nd::POOL_CAP);
            assert_eq!(state.timelines[0], incumbent);
        },
    );
}

#[test]
fn final_replay_evicts_a_dry_unconfirmed_origin() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            assert!(ctx.interesting.contains_key("Panic: bug"));
            ctx.final_replay().await.unwrap();
            assert!(
                ctx.interesting.is_empty(),
                "a dry unconfirmed origin leaves the interesting map"
            );
            assert_eq!(
                ctx.nd_origins.unconfirmed().collect::<Vec<_>>(),
                vec!["Panic: bug"]
            );
        },
    );
}

#[test]
fn a_reproducing_final_replay_confirms_an_unconfirmed_origin() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            assert!(ctx.nd_origins.needs_confirmation("Panic: bug"));
            ctx.final_replay().await.unwrap();
            assert!(
                !ctx.nd_origins.needs_confirmation("Panic: bug"),
                "a reproducing final replay confirms the origin"
            );
            assert!(ctx.interesting.contains_key("Panic: bug"));
            let report = ctx.build_report();
            assert_eq!(report.failures.len(), 1);
            assert!(report.failures[0].reproduce_blob.is_some());
        },
    );
}

#[test]
fn report_blobs_only_confirmed_origins() {
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: a", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            ctx.nd_origins
                .confirm("Panic: a", 0.5, None, Vec::new(), (4, 6));
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            );
            let report = ctx.build_report();
            assert_eq!(report.failures.len(), 1);
            assert_eq!(report.failures[0].origin, "Panic: a");
            assert!(report.failures[0].reproduce_blob.is_some());
            assert!(
                report.failures[0]
                    .caveat
                    .as_deref()
                    .unwrap()
                    .contains("confirmed")
            );
        },
    );
}

#[test]
fn an_origin_admitted_during_the_final_replay_is_not_blobbed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) == 0 {
                boom("b")
            } else {
                boom("a")
            }
        },
        async |ctx| {
            ctx.interesting
                .insert("Panic: a".to_string(), vec![bool_node(true)]);
            ctx.nd_origins
                .confirm("Panic: a", 0.5, None, Vec::new(), (4, 6));
            ctx.final_replay().await.unwrap();
            assert!(
                ctx.interesting.contains_key("Panic: b"),
                "the report-time measurement run admits the new origin"
            );
            let report = ctx.build_report();
            assert_eq!(report.failures.len(), 1);
            assert_eq!(report.failures[0].origin, "Panic: a");
            assert!(report.failures[0].reproduce_blob.is_some());
        },
    );
}

#[test]
fn unconfirmed_origins_report_caveat_only_when_nothing_confirmed() {
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: a", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            );
            assert!(ctx.nd_origins.reject("Panic: a", (0, 24)));
            ctx.interesting.remove("Panic: a");
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            );
            let report = ctx.build_report();
            assert_eq!(report.failures.len(), 2);
            for failure in &report.failures {
                assert!(failure.reproduce_blob.is_none());
            }
            assert!(
                report.failures[0]
                    .caveat
                    .as_deref()
                    .unwrap()
                    .contains("failed 0 of 24 replays")
            );
            assert!(
                report.failures[1]
                    .caveat
                    .as_deref()
                    .unwrap()
                    .contains("observed once, never replayed")
            );
        },
    );
}

#[test]
fn report_multiple_false_truncates_after_the_confirmed_filter() {
    with_engine(
        nd_settings().report_multiple_failures(false),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.interesting.insert(
                "Panic: a".to_string(),
                vec![bool_node(true), bool_node(true)],
            );
            ctx.nd_origins
                .confirm("Panic: a", 0.5, None, Vec::new(), (4, 6));
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            );
            let report = ctx.build_report();
            assert_eq!(report.failures.len(), 1);
            assert_eq!(
                report.failures[0].origin, "Panic: a",
                "an unconfirmed origin must not displace a confirmed one \
                 under single-failure reporting"
            );
        },
    );
}

#[test]
fn nd_runs_record_no_targeting_observations() {
    let observing_run = || {
        let mut run = interesting_at("Panic: unused", vec![bool_node(true)]);
        run.status = Status::Valid;
        run.origin = None;
        run.target_observations.insert("score".to_string(), 1.0);
        run
    };
    with_engine(
        nd_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.record_run(&observing_run(), Duration::ZERO, false);
            assert!(ctx.targeting.is_empty());
        },
    );
    with_counting_ctx(
        |_ds| TestCaseResult::Valid,
        async |ctx, _execs| {
            ctx.record_run(&observing_run(), Duration::ZERO, false);
            assert!(!ctx.targeting.is_empty());
        },
    );
}

#[test]
fn gauntlet_accept_without_adoption_moves_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let mut settings = Settings::new()
        .database(Some(path))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    with_engine(
        settings,
        Some("k"),
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            ctx.nd_origins.confirm(
                "Panic: bug",
                0.3,
                Some(interesting_at("Panic: bug", vec![bool_node(true)])),
                Vec::new(),
                (4, 6),
            );
            {
                let mut probe = EngineShrinkProbe {
                    engine: &mut *ctx,
                    target_origin: "Panic: bug".to_string(),
                    verbosity: Verbosity::Quiet,
                    output: Output::callback(|_| {}),
                    gauntlet: true,
                    ledger: HashMap::default(),
                    anchor: 0.3,
                    sweep: SweepMode::Fast,
                    raised: crate::native::HashSet::default(),
                    pending_accept: None,
                };
                let nodes = vec![bool_node(true)];
                let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
                assert!(matched, "the gauntlet accepts the always-failing candidate");
                assert_eq!(
                    probe.anchor, 0.3,
                    "an unadopted accept must not move the probe anchor"
                );
            }
            let (_, anchor) = ctx.nd_origins.take_witness("Panic: bug").unwrap();
            assert_eq!(anchor, 0.3, "the stored anchor is untouched");
            assert!(
                ctx.db().unwrap().fetch(b"k").is_empty(),
                "an unadopted accept must not persist an incumbent"
            );
        },
    );
}

#[test]
fn anchor_raises_only_on_adoption_and_once_per_timeline() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let mut settings = Settings::new()
        .database(Some(path))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    with_engine(
        settings,
        Some("k"),
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            ctx.nd_origins
                .confirm("Panic: bug", 0.3, None, Vec::new(), (4, 6));
            let raised_anchor;
            {
                let mut probe = EngineShrinkProbe {
                    engine: &mut *ctx,
                    target_origin: "Panic: bug".to_string(),
                    verbosity: Verbosity::Quiet,
                    output: Output::callback(|_| {}),
                    gauntlet: true,
                    ledger: HashMap::default(),
                    anchor: 0.3,
                    sweep: SweepMode::Fast,
                    raised: crate::native::HashSet::default(),
                    pending_accept: None,
                };
                let nodes = vec![bool_node(true)];
                let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
                assert!(matched);
                probe.candidate_adopted();
                raised_anchor = probe.anchor;
                assert!(raised_anchor > 0.3, "adoption raises the anchor");
                probe.candidate_adopted();
                assert_eq!(
                    probe.anchor, raised_anchor,
                    "adoption without a fresh accept is a no-op"
                );
            }
            assert!(
                !ctx.db().unwrap().fetch(b"k").is_empty(),
                "adoption persists the incumbent"
            );
        },
    );
}

#[test]
fn reproduce_blob_never_runs_a_fresh_generation() {
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Integer(BigInt::from(0))]],
        entropy: 0,
        extension: 4,
    };
    let blob = crate::native::blob::encode_nd_failure(&state);
    let draws = std::cell::RefCell::new(Vec::new());
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        match rint(ds, 0, 1_000_000) {
            Ok(v) => {
                draws.borrow_mut().push(v);
                if v != 0 {
                    boom("nonzero")
                } else {
                    TestCaseResult::Valid
                }
            }
            Err(()) => TestCaseResult::Overrun,
        }
    })
    .unwrap();
    assert!(
        result.failures.is_empty(),
        "a fresh generation would draw a nonzero value and fail"
    );
    let draws = draws.borrow();
    assert!(!draws.is_empty());
    assert!(draws.iter().all(|&v| v == 0));
}

/// Settings for the flip-routing and drain tests: single-failure reporting
/// stops generation at the first bug, so execution indices are exact.
fn one_bug_settings(path: &str) -> Settings {
    Settings::new()
        .database(Some(path.to_string()))
        .phases([Phase::Generate, Phase::Shrink])
        .verbosity(Verbosity::Quiet)
        .report_multiple_failures(false)
}

#[test]
fn a_flip_during_the_shrink_verify_routes_the_origin_through_the_bar() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let execs = AtomicUsize::new(0);
    let result = reuse_run(one_bug_settings(&path), "k", |ds| {
        if execs.fetch_add(1, Ordering::SeqCst) == 0 {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
        } else if rint(ds, 0, 100).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.contains("confirmed") && caveat.contains("failed 5 of 5"),
        "the flipped verify must route the origin through the full bar \
         (4 fails) before the final replay's 1, not confirm on the final \
         replay alone: {caveat:?}"
    );
    let db = DirectoryTestCaseDatabase::new(&path);
    let primary = db.fetch(b"k");
    assert!(!primary.is_empty());
    for entry in &primary {
        assert!(
            crate::native::blob::decode_nd_state(entry).is_some(),
            "only bar-validated v2 entries may reach the primary key"
        );
    }
}

#[test]
fn a_flip_during_shrink_probes_requeues_the_origin_for_a_gauntleted_shrink() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let execs = AtomicUsize::new(0);
    let result = reuse_run(one_bug_settings(&path), "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if execs.fetch_add(1, Ordering::SeqCst) >= 2 {
            if let Err(result) = concurrent_machine(ds) {
                return result;
            }
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.contains("confirmed") && caveat.contains("failed 5 of 5"),
        "the requeued origin must face the full bar (4 fails) before the \
         final replay's 1, not confirm on the final replay alone: {caveat:?}"
    );
}

#[test]
fn shrink_drain_retains_v2_secondary_entries_unreplayed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let v2_bytes = crate::native::blob::encode_nd_state(&crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 7,
        extension: 4,
    });
    let pre_shrink = serialize_choices(&vec![ChoiceValue::Boolean(false); 20]);
    assert!(
        shortlex(&v2_bytes, &pre_shrink) != core::cmp::Ordering::Greater,
        "the seeded entry must sit under the drain's shortlex break"
    );
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    {
        let db = DirectoryTestCaseDatabase::new(&path);
        db.save(&secondary_key, &v2_bytes);
    }
    let result = reuse_run(one_bug_settings(&path), "k", |ds| {
        for _ in 0..20 {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let db = DirectoryTestCaseDatabase::new(&path);
    assert!(
        db.fetch(&secondary_key).contains(&v2_bytes),
        "the drain must not delete a v2 entry it cannot replay"
    );
}

#[test]
fn nd_run_skips_the_pre_shrink_secondary_drain() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let v1_bytes = serialize_choices(&[ChoiceValue::Boolean(false)]);
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    {
        let db = DirectoryTestCaseDatabase::new(&path);
        db.save(&secondary_key, &v1_bytes);
    }
    let mut settings = one_bug_settings(&path);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let db = DirectoryTestCaseDatabase::new(&path);
    assert!(
        db.fetch(&secondary_key).contains(&v1_bytes),
        "under nondeterministic handling no entry class is drained"
    );
}

#[test]
fn shrink_drain_deletes_undecodable_secondary_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x07];
    assert!(crate::native::blob::decode_nd_state(&garbage).is_none());
    assert!(deserialize_choices(&garbage).is_none());
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    {
        let db = DirectoryTestCaseDatabase::new(&path);
        db.save(&secondary_key, &garbage);
    }
    let result = reuse_run(one_bug_settings(&path), "k", |ds| {
        for _ in 0..5 {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let db = DirectoryTestCaseDatabase::new(&path);
    assert!(
        !db.fetch(&secondary_key).contains(&garbage),
        "an entry neither format can decode is deleted"
    );
}

#[test]
fn drain_stops_at_mid_drain_nd_flip() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let drained_first =
        serialize_choices(&[ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)]);
    let survivor = serialize_choices(&[ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)]);
    assert!(shortlex(&drained_first, &survivor) == core::cmp::Ordering::Less);
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    {
        let db = DirectoryTestCaseDatabase::new(&path);
        db.save(&secondary_key, &drained_first);
        db.save(&secondary_key, &survivor);
    }
    let execs = AtomicUsize::new(0);
    let result = reuse_run(one_bug_settings(&path), "k", |ds| {
        if execs.fetch_add(1, Ordering::SeqCst) == 0 {
            for _ in 0..4 {
                if rbool(ds).is_err() {
                    return TestCaseResult::Overrun;
                }
            }
            boom("a")
        } else {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let db = DirectoryTestCaseDatabase::new(&path);
    let secondary = db.fetch(&secondary_key);
    assert!(
        !secondary.contains(&drained_first),
        "the first entry got its replay and its delete"
    );
    assert!(
        secondary.contains(&survivor),
        "a mid-drain flip stops the single-replay deletes for the rest"
    );
}

#[test]
fn deterministic_final_replay_is_stamped() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let stamped = AtomicU32::new(0);
    let execs = AtomicU32::new(0);
    let settings = Settings::new()
        .database(None)
        .verbosity(Verbosity::Quiet)
        .report_multiple_failures(false);
    let result = reuse_run(settings, "k", |ds| {
        execs.fetch_add(1, Ordering::SeqCst);
        stamped.fetch_add(u32::from(ds.is_nondeterministic()), Ordering::SeqCst);
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        stamped.load(Ordering::SeqCst),
        1,
        "exactly the final replay is stamped on a deterministic run"
    );
    assert!(execs.load(Ordering::SeqCst) > 1);
}

#[test]
fn nd_blob_replay_cases_are_stamped() {
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 0,
        extension: 4,
    };
    let blob = crate::native::blob::encode_nd_failure(&state);
    let mut calls = 0u32;
    let mut stamped = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        stamped += u32::from(ds.is_nondeterministic());
        match rbool(ds) {
            Ok(true) => boom("a"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(calls >= 1);
    assert_eq!(stamped, calls, "every ND blob replay execution is stamped");
}
