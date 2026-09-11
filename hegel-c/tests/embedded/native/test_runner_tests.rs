//! Embedded tests for `src/native/test_runner.rs`.
//!
//! These drive the engine directly — `run_main`, `Engine`,
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
use crate::settings::Phase;
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
/// concurrency bound is above 1. Since decision 70 this does not by itself
/// flip the run: only observed nondeterminism does.
fn concurrent_machine(ds: &dyn DataSource) -> Result<(), TestCaseResult> {
    match ds.new_state_machine(
        vec!["rule".to_string()],
        vec![0],
        alloc::vec::Vec::new(),
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

/// The flat cache keys on exact realized values, so a proposal longer than
/// a recorded conclusion is a miss and executes — the trailing-unread
/// serving the tree did is a deliberately accepted loss (experiment 010:
/// serves were ≈ exact repeats).
#[test]
fn cached_test_function_executes_a_proposal_longer_than_a_known_conclusion() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx, count| {
            let known = [ChoiceValue::Boolean(false)];
            ctx.cached_test_function(&known, None, 0).await.unwrap();
            assert_eq!(count.get(), 1);

            let run = ctx
                .cached_test_function(
                    &[ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
                    None,
                    0,
                )
                .await
                .unwrap();
            assert_eq!(run.status, Status::Valid);
            assert_eq!(count.get(), 2, "a longer proposal is not an exact repeat");
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
            assert_eq!(count.get(), 1, "exact repeat must be served from the cache");
        },
    );
}

/// The tree predicted an overrun for a truncated known path without running
/// the body; the flat cache does not record overruns, so the truncated
/// replay executes every time.
#[test]
fn cached_test_function_executes_a_truncated_known_path_to_overrun() {
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
            let overrun = ctx.cached_test_function(&truncated, None, 0).await.unwrap();
            assert_eq!(overrun.status, Status::EarlyStop);
            assert_eq!(count.get(), 2);

            let again = ctx.cached_test_function(&truncated, None, 0).await.unwrap();
            assert_eq!(again.status, Status::EarlyStop);
            assert_eq!(
                count.get(),
                3,
                "an overrun concluded nothing and is never served"
            );
        },
    );
}

#[test]
fn cached_test_function_probe_executes_a_truncated_prefix_with_continuation() {
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
fn cached_test_function_serves_interesting_from_cache_with_origin_and_spans() {
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
                "an interesting repeat must be served from the cache, not re-run"
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

/// Stateful bodies realize their draws through cloned streams; the cache
/// keys clones by child values, so an exact repeat of a clone-bearing
/// conclusion is served like any other. The tree declined these.
#[test]
fn a_repeated_stateful_probe_is_served() {
    with_counting_ctx(
        |ds| {
            let child = match ds.clone_stream() {
                Ok(c) => c,
                Err(_) => return TestCaseResult::Overrun,
            };
            match rint(&*child, 0, 1000) {
                Ok(v) if v >= 100 => boom("child too big"),
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx, count| {
            let clone = ChoiceValue::Clone(alloc::sync::Arc::new(
                crate::native::core::CloneRecord::from_values(vec![ChoiceValue::Integer(
                    BigInt::from(500),
                )]),
            ));
            let first = ctx
                .cached_test_function(std::slice::from_ref(&clone), None, 0)
                .await
                .unwrap();
            assert_eq!(first.status, Status::Interesting);
            assert_eq!(count.get(), 1);

            let second = ctx
                .cached_test_function(std::slice::from_ref(&clone), None, 0)
                .await
                .unwrap();
            assert_eq!(second.status, Status::Interesting);
            assert_eq!(count.get(), 1, "the clone-bearing repeat must be served");
        },
    );
}

/// The verdict-flip channel the tree never had: identical realized values
/// concluding differently flip the run under quiet strictness.
#[test]
fn a_reexecuted_fingerprint_with_a_different_outcome_flips_the_run() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_counting_ctx(
        move |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) == 0 {
                TestCaseResult::Valid
            } else {
                boom("flip")
            }
        },
        async |ctx, _count| {
            let choices = [ChoiceValue::Boolean(true)];
            let nodes = [bool_node(true)];
            let (run, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            assert_eq!(run.status, Status::Valid);
            assert!(mismatch.is_none());
            assert!(!ctx.nd_active);

            let (run, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            assert_eq!(run.status, Status::Interesting);
            assert!(
                mismatch.is_none(),
                "quiet strictness flips instead of aborting"
            );
            assert!(ctx.nd_active);
        },
    );
}

/// Under `error` strictness the same verdict flake is the flaky-test abort,
/// with decision 30's diagnostic verbatim.
#[test]
fn a_reexecuted_fingerprint_with_a_different_outcome_aborts_under_error_strictness() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        Settings::new()
            .database(None)
            .nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        move |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) == 0 {
                TestCaseResult::Valid
            } else {
                boom("flip")
            }
        },
        async |ctx| {
            let choices = [ChoiceValue::Boolean(true)];
            let nodes = [bool_node(true)];
            let (_, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            assert!(mismatch.is_none());
            let (_, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            match mismatch {
                Some(RunError::Flaky(msg)) => {
                    assert!(msg.contains("Flaky test detected"), "{msg}");
                    assert!(
                        msg.contains("The failure that did not reproduce was: Panic: flip"),
                        "the abort names the failure the cache recorded: {msg}"
                    );
                }
                other => panic!("expected the flaky abort, got {other:?}"),
            }
            assert!(
                !ctx.nd_active,
                "error strictness aborts instead of flipping"
            );
        },
    );
}

/// A verdict flip between two non-failing conclusions has no failure to
/// name, so the abort carries the bare diagnostic.
#[test]
fn a_valid_to_invalid_verdict_flip_aborts_with_the_bare_diagnostic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        Settings::new()
            .database(None)
            .nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        move |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) == 0 {
                TestCaseResult::Valid
            } else {
                TestCaseResult::Invalid
            }
        },
        async |ctx| {
            let choices = [ChoiceValue::Boolean(true)];
            let nodes = [bool_node(true)];
            let (_, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            assert!(mismatch.is_none());
            let (_, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            match mismatch {
                Some(RunError::Flaky(msg)) => {
                    assert!(msg.contains("Flaky test detected"), "{msg}");
                    assert!(
                        !msg.contains("did not reproduce"),
                        "no failure to name: {msg}"
                    );
                }
                other => panic!("expected the flaky abort, got {other:?}"),
            }
        },
    );
}

/// The flip drops everything the cache knew and stops serving: post-flip,
/// identical timelines need not conclude identically.
#[test]
fn the_execution_cache_is_flushed_and_serving_stops_at_the_flip() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(count.get(), 1, "served before the flip");

            ctx.nd_flip();
            assert!(
                ctx.exec_cache
                    .serve(&serialize_choices(&choices).unwrap())
                    .is_none(),
                "the flip flushes the cache"
            );
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(count.get(), 2, "nothing is served after the flip");
        },
    );
}

/// A generation-window duplicate advances the counter; a novel conclusion
/// resets it.
#[test]
fn duplicate_counter_resets_on_a_novel_case() {
    with_counting_ctx(
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx, _count| {
            ctx.collect_statistics = true;
            let run = |v: bool| NativeTestCase::for_choices(&[ChoiceValue::Boolean(v)], None, None);
            ctx.test_function(run(true)).await.unwrap();
            assert_eq!(ctx.consecutive_duplicates, 0);
            ctx.test_function(run(true)).await.unwrap();
            assert_eq!(ctx.consecutive_duplicates, 1);
            ctx.test_function(run(true)).await.unwrap();
            assert_eq!(ctx.consecutive_duplicates, 2);
            ctx.test_function(run(false)).await.unwrap();
            assert_eq!(
                ctx.consecutive_duplicates, 0,
                "a novel case resets the streak"
            );
        },
    );
}

fn tiny_invalid_run(
    settings: Settings,
    body_status: TestCaseResult,
) -> (
    Result<crate::backend::TestRunResult, crate::backend::RunError>,
    u64,
) {
    let execs = Cell::new(0u64);
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = match rbool(&*ds) {
            Ok(_) => body_status.clone(),
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let result = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    (result, execs.get())
}

/// A two-value space whose every case is filtered reaches ten consecutive
/// duplicates long before the 50-invalid threshold, and the exhausted-space
/// FilterTooMuch variant reports it.
#[test]
fn filter_too_much_fires_via_the_threshold_variant_on_an_exhausted_space() {
    let (result, execs) = tiny_invalid_run(
        Settings::new().database(None).test_cases(10_000),
        TestCaseResult::Invalid,
    );
    match result {
        Err(RunError::HealthCheck(msg)) => {
            assert!(
                msg.contains("every reachable input was filtered out"),
                "{msg}"
            );
        }
        other => panic!("expected the exhausted-space FilterTooMuch, got {other:?}"),
    }
    assert!(
        execs < 50,
        "the stop fires before the invalid threshold: {execs}"
    );
}

/// Suppressing FilterTooMuch must not send a tiny filtered space grinding
/// through the whole invalid budget: the duplicate stop stays active.
#[test]
fn duplicate_stop_stays_active_under_health_check_suppression() {
    let (result, execs) = tiny_invalid_run(
        Settings::new()
            .database(None)
            .test_cases(10_000)
            .suppress_health_check([HealthCheck::FilterTooMuch]),
        TestCaseResult::Invalid,
    );
    assert!(result.unwrap().failures.is_empty());
    assert!(
        execs < 50,
        "stopped by duplicates, not the invalid budget: {execs}"
    );
}

/// Under nondeterministic handling identical values need not repeat
/// identical outcomes, so the duplicate stop is suspended and the run
/// grinds to its invalid budget instead.
#[test]
fn duplicate_stop_is_disabled_under_nd_handling() {
    let mut settings = Settings::new()
        .database(None)
        .test_cases(10_000)
        .verbosity(Verbosity::Quiet)
        .suppress_health_check([HealthCheck::FilterTooMuch]);
    settings.nd_force = true;
    let (result, execs) = tiny_invalid_run(settings, TestCaseResult::Invalid);
    assert!(result.unwrap().failures.is_empty());
    assert!(execs > 100, "no duplicate stop under nd handling: {execs}");
}

/// A tiny space whose cases are valid runs to its test-case budget: the
/// duplicate stop only guards the all-invalid grind, and stopping a valid
/// space early would leave `one_of` alternatives unreached.
#[test]
fn a_tiny_passing_space_generates_to_its_test_case_budget() {
    let (result, execs) = tiny_invalid_run(
        Settings::new().database(None).test_cases(50),
        TestCaseResult::Valid,
    );
    assert!(result.unwrap().failures.is_empty());
    assert!(
        execs >= 50,
        "a valid space is budget-bounded, never duplicate-stopped: {execs}"
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

/// Span-mutation proposals are keyed as whole proposals, and the body here
/// realizes only one of the proposal's four values — so no attempt is an
/// exact repeat of a recorded conclusion and every one executes. The tree
/// served these; the loss is accepted (experiment 010).
#[test]
fn span_mutation_re_executes_proposals_that_are_not_exact_repeats() {
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

            assert_eq!(count.get(), 5);
            assert_eq!(ctx.calls, 5);
            assert_eq!(ctx.valid_test_cases, 5);
            assert!(!ctx.origins.any_live());
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
            let origins = ctx.origins.live_origins();
            assert_eq!(origins.len(), 1, "the first proposal should be Interesting");
            assert!(origins[0].contains("Panic"));
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
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(true)]).unwrap();
    let mut calls = 0u32;
    let mut stamped = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        stamped += u32::from(ds.should_capture());
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
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(false)]).unwrap();
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| match rbool(ds) {
        Ok(true) => boom("never"),
        Ok(false) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    })
    .unwrap();
    assert!(result.failures.is_empty());
}

#[test]
fn a_v1_blob_replays_with_the_continuation_budget() {
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(true)]).unwrap();
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        let first = match rbool(ds) {
            Ok(b) => b,
            Err(()) => return TestCaseResult::Overrun,
        };
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if first {
            boom("continuation reached")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: continuation reached");
}

#[test]
fn a_v1_blob_retries_up_to_its_budget() {
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(true)]).unwrap();
    let mut calls = 0u32;
    let mut stamped = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        stamped += u32::from(ds.should_capture());
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if calls == 3 {
            boom("third replay")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(calls, 3, "the replay loop retries past the early passes");
    assert_eq!(stamped, 3, "every replay is stamped for capture");
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic: third replay");
    assert!(result.failures[0].caveat.is_none());
}

#[test]
fn a_truly_stale_v1_blob_still_reports_stale_within_budget() {
    let blob = crate::native::blob::encode_failure(&[ChoiceValue::Boolean(true)]).unwrap();
    let mut calls = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        }
    })
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(calls, 4, "replays stop at the v1 budget");
}

/// A one-timeline ND blob whose single stored choice is a `true` boolean.
fn nd_blob() -> String {
    crate::native::blob::encode_nd_failure(&crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true)]],
        entropy: 7,
        extension: 4,
    })
    .unwrap()
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
        Some(
            "nondeterministic failure, reproduced from stored timelines: \
             failed 1 of 3 replays this run"
        )
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

/// Phase-15 cost guard: on a passing body the run's execution count is a
/// pure function of the seed, so replacing the tree's recording with the
/// flat cache must not change it at all.
#[test]
fn a_passing_run_executes_a_seed_pinned_count() {
    let execs = Cell::new(0u64);
    let body = |ds: &dyn DataSource| match rint(ds, I32_MIN, I32_MAX) {
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .test_cases(50)
        .database(None)
        .derandomize(true);
    let result = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(execs.get(), 50);
}

/// Phase-15 cost guard: the flat cache must keep serving shrink-phase
/// repeats, so a deterministic shrink-heavy run stays within ~1.1x the
/// tree-era execution count (010's 85% serve rate).
#[test]
fn a_deterministic_shrink_stays_within_the_tree_era_execution_budget() {
    let execs = Cell::new(0u64);
    let body = |ds: &dyn DataSource| {
        let mut sum: i64 = 0;
        for _ in 0..16 {
            match rint(ds, 0, 100) {
                Ok(v) => sum += v,
                Err(()) => return TestCaseResult::Overrun,
            }
        }
        if sum >= 200 {
            boom("large sum")
        } else {
            TestCaseResult::Valid
        }
    };
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .test_cases(50)
        .database(None)
        .derandomize(true);
    let result = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap();
    assert!(result.failures[0].origin.contains("large sum"));
    assert!(execs.get() <= 1661, "execs = {}", execs.get());
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
fn genuine_overrun_is_early_stop_and_not_cached() {
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
            let (run, _mismatch) = ctx
                .test_function(NativeTestCase::for_simplest(1).unwrap())
                .await
                .unwrap();
            assert_eq!(run.status, Status::EarlyStop);

            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value().clone()).collect();
            let replay = ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(replay.status, Status::EarlyStop);
            assert_eq!(count.get(), 2, "an overrun is never served");
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
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );

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
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]).unwrap(),
    );
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
    db.save(
        &secondary_key,
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]).unwrap(),
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
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]).unwrap(),
    );
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
    for n in [4242, 4243, 4244, 4245] {
        db.save(
            &secondary_key,
            &serialize_choices(&[ChoiceValue::Integer(BigInt::from(n))]).unwrap(),
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
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
    let stale = serialize_choices(&[ChoiceValue::Integer(BigInt::from(5))]).unwrap();
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
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(1111))]).unwrap(),
    );
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(2222))]).unwrap(),
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
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]).unwrap(),
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

/// A final replay that realizes past its proposal (the body demands a
/// second draw, overrunning) has a novel fingerprint, so no cache mismatch
/// fires. The status check catches the vanished failure and `Error`
/// strictness aborts with the flaky diagnostic. The body reproduces
/// through the discovery and the first check's replays, diverging only on
/// the final replay's execution.
#[test]
fn a_divergent_final_replay_vanish_aborts_under_error_strictness() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let failing = AtomicUsize::new(0);
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
            if failing.fetch_add(1, Ordering::SeqCst) as u64 > FIRST_CHECK_REPLAYS {
                if rbool(ds).is_err() {
                    return TestCaseResult::Overrun;
                }
                return TestCaseResult::Valid;
            }
            boom("stable origin")
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
fn a_flipped_reuse_run_persists_v2_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let seeded = serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap();
    db.save(b"k", &seeded);

    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("db origin")
    })
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
        "a flipped reuse run persists version-2 entries"
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(&secondary).contains(&seeded),
        "the stale v1 entry is demoted, not deleted"
    );
}

#[test]
fn a_concurrent_machine_alone_neither_flips_nor_notices() {
    use std::sync::{Arc, Mutex};
    for strictness in [
        NondeterminismStrictness::Quiet,
        NondeterminismStrictness::Warn,
        NondeterminismStrictness::Error,
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
        assert_eq!(
            notices, 0,
            "declared concurrency is not a detection (decision 70), \
             got a notice under {strictness:?}"
        );
    }
}

#[test]
fn reuse_detects_nondeterministic_generator_across_replays() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(false)]).unwrap(),
    );

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

/// Decision 9: a stored entry that no longer matches the generator is
/// staleness, never nondeterminism evidence — even under `error`
/// strictness. The ledger only ever compares executions within one run,
/// so the stored sequence's obsolete shape cannot contradict anything.
#[test]
fn a_stale_stored_entry_is_not_nondeterminism_evidence_under_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[
            ChoiceValue::Integer(BigInt::from(5)),
            ChoiceValue::Integer(BigInt::from(6)),
        ])
        .unwrap(),
    );
    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse, Phase::Generate])
            .test_cases(5)
            .nondeterminism_strictness(NondeterminismStrictness::Error)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert!(
        db.fetch(b"k").is_empty(),
        "the stale entry is deleted as staleness, not reported as nondeterminism"
    );
}

#[test]
fn nondeterministic_generator_contradicts_the_reuse_fed_kind_ledger_at_simplest_example() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );

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
        events: Vec::new(),
        divergence: None,
        live: Vec::new(),
    }
}

/// A valid [`RunResult`] realizing `nodes`, standing in for a raw
/// execution's outcome.
fn valid_at(nodes: Vec<ChoiceNode>) -> RunResult {
    RunResult {
        status: Status::Valid,
        nodes,
        spans: Vec::new(),
        origin: None,
        target_observations: crate::native::HashMap::default(),
        events: Vec::new(),
        divergence: None,
        live: Vec::new(),
    }
}

#[test]
fn history_records_raw_displacements_and_shrink_accepts() {
    with_engine(
        quiet_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true), bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at(
                    origin,
                    vec![bool_node(true), bool_node(true), bool_node(false)],
                ),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let history = ctx.origins.get(origin).unwrap().history();
            assert_eq!(history.entries().len(), 3);
            assert!(history.entries()[0].accept, "founding sighting");
            assert!(!history.entries()[1].accept, "non-displacing raw sighting");
            assert!(history.entries()[2].accept, "shortlex displacement");
        },
    );
}

#[test]
fn history_dedupes_repeated_timelines() {
    with_engine(
        quiet_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let origin = "Panic: bug";
            for _ in 0..3 {
                ctx.record_run(
                    &interesting_at(origin, vec![bool_node(true)]),
                    Duration::ZERO,
                    false,
                )
                .unwrap();
            }
            assert_eq!(
                ctx.origins.get(origin).unwrap().history().entries().len(),
                1
            );
        },
    );
}

#[test]
fn history_is_kept_only_while_deterministic() {
    with_engine(
        quiet_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.nd_flip();
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at("Panic: other", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let history = ctx.origins.get(origin).unwrap().history();
            assert_eq!(
                history.entries().len(),
                1,
                "post-flip executions must not enter history"
            );
            assert!(
                ctx.origins
                    .get("Panic: other")
                    .is_none_or(|c| c.history().is_empty())
            );
        },
    );
}

/// The interesting arm's measurement guard, pinned both ways: a plain
/// measurement run neither displaces, persists, nor enters history, while
/// a reuse-phase replay (`reuse_replays`) does all three.
#[test]
fn a_measurement_run_neither_displaces_nor_persists() {
    with_engine(
        quiet_settings(),
        Some("k"),
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                true,
            )
            .unwrap();
            assert_eq!(
                ctx.origins.incumbent(origin).unwrap(),
                &[bool_node(true)],
                "a measurement run must not displace the incumbent"
            );
            assert_eq!(
                ctx.origins.get(origin).unwrap().history().entries().len(),
                1
            );

            ctx.reuse_replays = true;
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                true,
            )
            .unwrap();
            ctx.reuse_replays = false;
            assert_eq!(
                ctx.origins.incumbent(origin).unwrap(),
                &[bool_node(false)],
                "a reuse-phase replay must displace like a raw run"
            );
            assert_eq!(
                ctx.origins.get(origin).unwrap().history().entries().len(),
                2
            );
        },
    );
}

#[test]
fn history_is_dropped_when_the_origin_confirms() {
    let bug = "deliberate";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert!(
                ctx.origins
                    .get(&origin)
                    .is_some_and(|c| !c.history().is_empty())
            );
            ctx.nd_flip();
            let output = Settings::new().output;
            ctx.nd_discovery_sweep(Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert!(!ctx.origins.needs_confirmation(&origin));
            assert!(
                ctx.origins
                    .get(&origin)
                    .is_none_or(|c| c.history().is_empty()),
                "confirmation must drop the origin's history"
            );
        },
    );
}

#[test]
fn a_first_check_outcome_miss_flips_the_run_before_displacement() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.first_check_sweep().await.unwrap();
            assert!(ctx.nd_active, "an outcome miss must flip the run");
            assert_eq!(
                ctx.origins.incumbent(origin).unwrap(),
                &[bool_node(true)],
                "the discovering sighting must still be the incumbent"
            );
            assert!(ctx.origins.get(origin).is_some_and(|c| c.first_checked()));
        },
    );
}

#[test]
fn a_first_check_realized_timeline_miss_flips_the_run() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(5)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.first_check_sweep().await.unwrap();
            assert!(
                ctx.nd_active,
                "a failing replay realizing different choices must flip the run"
            );
        },
    );
}

#[test]
fn first_check_evidence_seeds_the_origins_ledger() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(5)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.first_check_sweep().await.unwrap();
            let seed = ctx.origins.entry(&origin).take_seed().unwrap();
            assert_eq!(seed.fails(), 1, "the failing divergent replay is a fail");
            assert_eq!(seed.runs(), 1, "stop on first miss");
        },
    );
}

#[test]
fn the_discovery_bar_starts_from_the_first_check_seed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let bug = "bug";
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            let mut seed = nd::Evidence::default();
            for _ in 0..3 {
                seed.record(true);
            }
            ctx.origins.entry(&origin).seed_evidence(seed);
            ctx.nd_flip();
            let batch = ctx
                .nd_evidence_batch(&origin, &[ChoiceValue::Boolean(true)], None)
                .await
                .unwrap();
            assert!(batch.bar_accepted);
            assert_eq!(
                batch.evidence.runs(),
                nd::ANCHOR_SEED_RUNS,
                "the batch extends to the anchor-seed count including the seed"
            );
            assert_eq!(
                execs.load(Ordering::SeqCst) as u64,
                nd::ANCHOR_SEED_RUNS - 3,
                "seeded runs are not re-executed"
            );
        },
    );
}

#[test]
fn each_origin_gets_its_own_first_check() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            let b = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            boom(if b { "a" } else { "b" })
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: a", vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.first_check_sweep().await.unwrap();
            assert!(!ctx.nd_active);
            assert!(
                ctx.origins
                    .get("Panic: a")
                    .is_some_and(|c| c.first_checked())
            );
            assert!(
                ctx.origins
                    .get("Panic: b")
                    .is_some_and(|c| c.first_checked())
            );
            assert_eq!(execs.load(Ordering::SeqCst) as u64, 2 * FIRST_CHECK_REPLAYS);
        },
    );
}

#[test]
fn an_all_reproduce_first_check_keeps_the_run_deterministic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let bug = "bug";
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.first_check_sweep().await.unwrap();
            assert!(!ctx.nd_active);
            assert!(
                ctx.origins
                    .get(origin.as_str())
                    .is_some_and(|c| c.first_checked())
            );
            assert_eq!(execs.load(Ordering::SeqCst) as u64, FIRST_CHECK_REPLAYS);
            assert!(
                ctx.origins.entry(&origin).take_seed().is_none(),
                "an all-reproduce check seeds nothing"
            );
        },
    );
}

/// Seeds `origin`'s history and incumbent with `values`, oldest first,
/// via raw [`Engine::record_run`] calls — each entry displaces when
/// shortlex-smaller, mirroring a real generation phase.
fn seed_history(ctx: &mut Engine<'_>, origin: &str, values: &[i128]) {
    for &value in values {
        ctx.record_run(
            &interesting_at(origin, vec![int_node(value)]),
            Duration::ZERO,
            false,
        )
        .unwrap();
    }
}

#[test]
fn a_final_replay_miss_backtracks_to_the_reproduction_boundary() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 50 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 40]);
            assert_eq!(ctx.origins.incumbent(&origin).unwrap(), &[int_node(40)]);
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(ctx.nd_active);
            assert_eq!(
                ctx.origins.incumbent(&origin).unwrap(),
                &[int_node(90)],
                "the reproduction boundary is the incumbent again"
            );
            assert!(!ctx.origins.needs_confirmation(&origin));
            assert!(
                ctx.origins
                    .get(&origin)
                    .is_none_or(|c| c.history().is_empty()),
                "a restored origin's history is dropped"
            );
        },
    );
}

#[test]
fn the_backtrack_scan_probes_geometrically() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let bug = "bug";
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 90 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 85, 80, 75, 70, 65, 60, 55, 50]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(nodes, vec![int_node(90)]);
            assert_eq!(
                execs.load(Ordering::SeqCst) as u64,
                6 + nd::ANCHOR_SEED_RUNS,
                "geometric offsets 1/2/4/8 plus two refinement probes, then the bar"
            );
        },
    );
}

#[test]
fn the_scan_continues_past_a_bar_rejected_candidate() {
    let bug = "bug";
    let seen = Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));
    let body_seen = seen.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v == 90 || (v == 70 && body_seen.borrow_mut().insert(v)) {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 70, 40]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(
                nodes,
                vec![int_node(90)],
                "the bar-rejected one-shot candidate resumes the scan on the older side"
            );
        },
    );
}

#[test]
fn three_bar_rejections_exhaust_the_backtrack() {
    let bug = "bug";
    let seen = Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));
    let body_seen = seen.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 70 && body_seen.borrow_mut().insert(v) {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 80, 70, 60]);
            ctx.nd_flip();
            let Backtrack::Exhausted { evidence } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected exhaustion after BACKTRACK_BAR_ATTEMPTS rejections");
            };
            assert_eq!(
                evidence.0, 3,
                "each one-shot candidate failed its scan probe once"
            );
        },
    );
}

#[test]
fn a_spent_backtrack_budget_short_circuits_the_next_backtrack() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            seed_history(ctx, origin, &[90, 80, 70]);
            ctx.nd_flip();
            for _ in 0..nd::BACKTRACK_BAR_ATTEMPTS {
                assert!(ctx.origins.entry(origin).spend_backtrack_attempt());
            }
            let Backtrack::Exhausted { evidence } = ctx.backtrack(origin).await.unwrap() else {
                panic!("expected exhaustion");
            };
            assert_eq!(evidence, (0, 0));
            assert_eq!(
                execs.load(Ordering::SeqCst),
                0,
                "a spent budget skips the scan"
            );
        },
    );
}

#[test]
fn the_backtrack_stops_at_a_candidate_it_cannot_afford_to_bar() {
    let bug = "bug";
    let seen = Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));
    let body_seen = seen.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 80 && body_seen.borrow_mut().insert(v) {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 80]);
            ctx.nd_flip();
            for _ in 0..nd::BACKTRACK_BAR_ATTEMPTS - 1 {
                assert!(ctx.origins.entry(&origin).spend_backtrack_attempt());
            }
            let Backtrack::Exhausted { evidence } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected exhaustion");
            };
            assert_eq!(
                evidence,
                (2, 12),
                "two reproducing scan probes and one rejected batch, then the \
                 second candidate finds no attempt left"
            );
        },
    );
}

#[test]
fn the_sweep_rejects_an_origin_out_of_bar_attempts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            for _ in 0..nd::BAR_ATTEMPTS_PER_RUN {
                assert!(ctx.origins.entry(origin).spend_bar_attempt());
            }
            let output = Settings::new().output;
            ctx.nd_discovery_sweep(Verbosity::Debug, &output)
                .await
                .unwrap();
            assert!(
                ctx.origins.incumbent(origin).is_none(),
                "at the attempt cap the origin is evicted"
            );
            assert_eq!(execs.load(Ordering::SeqCst), 0, "no batch runs at the cap");
            assert_eq!(ctx.origins.unconfirmed().next(), Some(origin));
        },
    );
}

#[test]
fn shrink_admission_rejects_an_origin_out_of_bar_attempts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            let origin = "Panic: bug".to_string();
            ctx.record_run(
                &interesting_at(&origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.origins.entry(&origin).clear_history();
            for _ in 0..nd::BAR_ATTEMPTS_PER_RUN {
                assert!(ctx.origins.entry(&origin).spend_bar_attempt());
            }
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            let timed_out = ctx
                .shrink_origin(
                    origin.clone(),
                    vec![bool_node(true)],
                    Verbosity::Quiet,
                    &output,
                    None,
                    &mut shrunk,
                )
                .await
                .unwrap();
            assert!(!timed_out);
            assert!(shrunk.contains(&origin));
            assert!(ctx.origins.incumbent(&origin).is_none());
            assert_eq!(
                execs.load(Ordering::SeqCst),
                0,
                "no admission batch runs at the cap"
            );
        },
    );
}

#[test]
fn the_final_replay_review_confirms_through_the_bar() {
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
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert!(ctx.origins.needs_confirmation(origin));
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                !ctx.origins.needs_confirmation(origin),
                "the review's reproducing run passed the bar"
            );
            assert!(
                ctx.origins.incumbent(origin).is_some(),
                "the map incumbent stays"
            );
            let caveat = ctx.origins.caveat(origin).unwrap();
            assert!(caveat.contains("confirmed"), "{caveat}");
            assert!(
                caveat.contains("report time"),
                "the review replays land in the report counts: {caveat}"
            );
        },
    );
}

#[test]
fn a_bar_rejected_review_falls_through_to_eviction() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            let n = execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if n == 0 {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.origins.entry(origin).clear_history();
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(ctx.origins.needs_confirmation(origin));
            assert!(
                ctx.origins.incumbent(origin).is_none(),
                "a bar-rejected review evicts"
            );
            let caveat = ctx.origins.caveat(origin).unwrap();
            assert!(caveat.contains("below the confirmation bar"), "{caveat}");
        },
    );
}

#[test]
fn a_dry_review_folds_the_exhausted_backtracks_evidence() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            seed_history(ctx, origin, &[90, 80]);
            ctx.nd_flip();
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(ctx.origins.incumbent(origin).is_none());
            let caveat = ctx.origins.caveat(origin).unwrap();
            assert!(
                caveat.contains("failed 0 of 32 replays"),
                "the review's 29 trials and the scan's 3 both count: {caveat}"
            );
        },
    );
}

#[test]
fn an_out_of_attempts_review_evicts_without_a_batch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.origins.entry(origin).clear_history();
            for _ in 0..nd::BAR_ATTEMPTS_PER_RUN {
                assert!(ctx.origins.entry(origin).spend_bar_attempt());
            }
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                ctx.origins.incumbent(origin).is_none(),
                "a reproducing review at the attempt cap still evicts"
            );
            assert_eq!(
                execs.load(Ordering::SeqCst),
                1,
                "the reproduction runs, the bar batch does not"
            );
        },
    );
}

#[test]
fn an_expired_deadline_rejects_the_evidence_batch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("bug")
        },
        async |ctx| {
            let Some(now) = crate::sys::Instant::now() else {
                return;
            };
            let batch = ctx
                .nd_evidence_batch("Panic: bug", &[ChoiceValue::Boolean(true)], Some(now))
                .await
                .unwrap();
            assert!(!batch.bar_accepted, "a batch cut short proves nothing");
            assert_eq!(batch.evidence.runs(), 0);
            assert_eq!(execs.load(Ordering::SeqCst), 0);
        },
    );
}

#[test]
fn backtrack_pools_the_other_reproducing_entries() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 80 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 85, 40]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(nodes, vec![int_node(85)]);
            let pool = ctx.origins.get(&origin).unwrap().pool();
            assert_eq!(pool[0], vec![int_node(85).value()]);
            assert!(
                pool.contains(&vec![int_node(90).value()]),
                "the scan's other reproducing entry is pooled: {pool:?}"
            );
        },
    );
}

#[test]
fn a_backtracked_incumbent_anchors_from_its_bar_batch() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 50 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 40]);
            ctx.nd_flip();
            let Backtrack::Restored { .. } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            let (witness, anchor) = ctx.origins.entry(&origin).take_witness().unwrap();
            assert_eq!(witness.origin.as_deref(), Some(origin.as_str()));
            let mut expected = nd::Evidence::default();
            for _ in 0..nd::ANCHOR_SEED_RUNS {
                expected.record(true);
            }
            assert_eq!(anchor, expected.lower_bound());
        },
    );
}

#[test]
fn an_exhausted_backtrack_reports_caveat_only() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            seed_history(ctx, origin, &[90, 40]);
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                !ctx.origins.any_live(),
                "an exhausted backtrack rejects into the caveat-only path"
            );
            assert_eq!(ctx.origins.unconfirmed().collect::<Vec<_>>(), vec![origin]);
        },
    );
}

#[test]
fn backtrack_replays_are_capped() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            let values: Vec<i128> = (50..100).rev().collect();
            seed_history(ctx, origin, &values);
            ctx.nd_flip();
            let Backtrack::Exhausted { evidence } = ctx.backtrack(origin).await.unwrap() else {
                panic!("expected exhaustion");
            };
            assert_eq!(evidence, (0, BACKTRACK_SCAN_REPLAYS));
            assert_eq!(
                execs.load(Ordering::SeqCst) as u64,
                BACKTRACK_SCAN_REPLAYS,
                "the second pass stops at the scan cap"
            );
        },
    );
}

#[test]
fn a_displaced_incumbent_is_recoverable_after_a_late_flip() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 90 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 12]);
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            ctx.shrink_origin(
                origin.clone(),
                vec![int_node(12)],
                Verbosity::Quiet,
                &output,
                None,
                &mut shrunk,
            )
            .await
            .unwrap();
            assert_eq!(
                ctx.origins.incumbent(&origin).unwrap(),
                &[int_node(90)],
                "the verify miss backtracks to the displaced incumbent"
            );
            assert!(
                !shrunk.contains(&origin),
                "a restored origin requeues for a gauntleted pass"
            );
            assert!(!ctx.origins.needs_confirmation(&origin));
            ctx.shrink_origin(
                origin.clone(),
                vec![int_node(90)],
                Verbosity::Quiet,
                &output,
                None,
                &mut shrunk,
            )
            .await
            .unwrap();
            assert!(shrunk.contains(&origin));
            assert_eq!(ctx.origins.incumbent(&origin).unwrap(), &[int_node(90)]);
        },
    );
}

#[test]
fn backtrack_resumes_gauntleted_shrinking_under_remaining_budget() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 50 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 40]);
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, true)
                .await
                .unwrap();
            assert_eq!(
                ctx.origins.incumbent(&origin).unwrap(),
                &[int_node(50)],
                "the restored incumbent re-shrinks under the gauntlet"
            );
            assert!(!ctx.origins.needs_confirmation(&origin));
        },
    );
}

#[test]
fn a_flip_during_final_replay_reviews_already_replayed_origins() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v == 3 {
                boom("alpha")
            } else if v >= 50 {
                boom("zeta")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: alpha", vec![int_node(3)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            seed_history(ctx, "Panic: zeta", &[90, 40]);
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                !ctx.origins.needs_confirmation("Panic: alpha"),
                "the deterministically replayed origin re-entered the queue and confirmed"
            );
            assert!(!ctx.origins.needs_confirmation("Panic: zeta"));
            assert_eq!(
                ctx.origins.incumbent("Panic: zeta").unwrap(),
                &[int_node(90)]
            );
            assert!(ctx.origins.incumbent("Panic: alpha").is_some());
        },
    );
}

#[test]
fn a_single_entry_history_probes_its_founding_sighting() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 90 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(nodes, vec![int_node(90)]);
        },
    );
}

#[test]
fn the_refinement_narrows_to_the_newest_reproducing_entry() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 84 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[95, 90, 85, 80, 75, 70, 65, 60]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(
                nodes,
                vec![int_node(85)],
                "refinement lands on the boundary, not the geometric probe that found it"
            );
        },
    );
}

#[test]
fn a_raw_sighting_can_be_the_restored_incumbent() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 90 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[60, 90, 40]);
            let history = ctx.origins.get(&origin).unwrap().history();
            assert!(!history.entries()[1].accept, "90 does not displace 60");
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(
                nodes,
                vec![int_node(90)],
                "with no reproducing accept, a reproducing raw sighting is the candidate"
            );
        },
    );
}

#[test]
fn an_exhausted_backtrack_at_shrink_verify_keeps_the_caveat_path() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            seed_history(ctx, origin, &[90, 40]);
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            ctx.shrink_origin(
                origin.to_string(),
                vec![int_node(40)],
                Verbosity::Quiet,
                &output,
                None,
                &mut shrunk,
            )
            .await
            .unwrap();
            assert!(ctx.nd_active);
            assert!(
                shrunk.contains(origin),
                "an exhausted backtrack ends the origin's shrink pass"
            );
            assert!(ctx.origins.needs_confirmation(origin));
        },
    );
}

/// An origin first observed after the flip has no history (post-flip
/// executions never enter it), so its shrink admission is the bar,
/// unchanged from decision 24.
#[test]
fn a_post_flip_origin_faces_the_bar_at_shrink_time() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 50 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.nd_flip();
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(90)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert!(
                ctx.origins
                    .get(&origin)
                    .is_none_or(|c| c.history().is_empty())
            );
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            ctx.shrink_origin(
                origin.clone(),
                vec![int_node(90)],
                Verbosity::Quiet,
                &output,
                None,
                &mut shrunk,
            )
            .await
            .unwrap();
            assert!(!ctx.origins.needs_confirmation(&origin));
            assert!(shrunk.contains(&origin));
            assert_eq!(ctx.origins.incumbent(&origin).unwrap(), &[int_node(50)]);
        },
    );
}

#[test]
fn a_mid_shrink_flip_requeues_from_the_pre_shrink_nodes() {
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 50 {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = "Panic: bug";
            // Generation-window records keep digests only, so the shrink
            // probes that repeat these keys execute (nothing to serve) and
            // the contradicting verdicts are the observed mid-shrink flip.
            ctx.collect_statistics = true;
            ctx.record_run(
                &interesting_at(origin, vec![int_node(70)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            for v in 50..70 {
                ctx.record_run(&valid_at(vec![int_node(v)]), Duration::ZERO, false)
                    .unwrap();
            }
            ctx.collect_statistics = false;
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            ctx.shrink_origin(
                origin.to_string(),
                vec![int_node(70)],
                Verbosity::Quiet,
                &output,
                None,
                &mut shrunk,
            )
            .await
            .unwrap();
            assert!(
                ctx.nd_active,
                "a probe contradicting a generation verdict flips the run"
            );
            assert!(
                !shrunk.contains(origin),
                "a mid-shrink flip requeues instead of marking shrunk"
            );
            assert_eq!(
                ctx.origins.incumbent(origin).unwrap(),
                &[int_node(70)],
                "the requeue discards untrusted single-run progress"
            );
        },
    );
}

#[test]
fn an_exact_final_replay_vanish_aborts_under_error_strictness() {
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.record_run(
                &interesting_at("Panic: bug", vec![int_node(90)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let output = ctx.settings.output.clone();
            let err = ctx
                .final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap_err();
            assert!(
                matches!(err, RunError::Flaky(_)),
                "an exact-repeat vanish aborts through the cache mismatch: {err:?}"
            );
        },
    );
}

#[test]
fn a_backtrack_without_history_is_exhausted() {
    with_engine(
        quiet_settings(),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.nd_flip();
            let Backtrack::Exhausted { evidence } = ctx.backtrack("Panic: bug").await.unwrap()
            else {
                panic!("expected exhaustion");
            };
            assert_eq!(evidence, (0, 0));
        },
    );
}

#[test]
fn the_scan_budget_caps_the_first_pass() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            let mut values = vec![40i128];
            values.extend(50..95);
            seed_history(ctx, origin, &values);
            ctx.nd_flip();
            let Backtrack::Exhausted { evidence } = ctx.backtrack(origin).await.unwrap() else {
                panic!("expected exhaustion");
            };
            assert_eq!(evidence, (0, BACKTRACK_SCAN_REPLAYS));
            assert_eq!(
                execs.load(Ordering::SeqCst) as u64,
                BACKTRACK_SCAN_REPLAYS,
                "the raw-sighting sweep stops at the scan cap"
            );
        },
    );
}

#[test]
fn tied_raw_candidates_restore_the_shortlex_least() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if v >= 90 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[40, 95, 90]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(nodes, vec![int_node(90)]);
        },
    );
}

/// A failure that needs a warm-up execution per value reproduces only on
/// the second pass, which re-probes entries the first pass saw miss.
#[test]
fn a_second_pass_probe_can_find_the_candidate() {
    let bug = "bug";
    let counts = Rc::new(std::cell::RefCell::new(
        std::collections::HashMap::<i64, u32>::new(),
    ));
    let body_counts = counts.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            let mut counts = body_counts.borrow_mut();
            let seen = counts.entry(v).or_insert(0);
            *seen += 1;
            if v >= 90 && *seen >= 2 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            seed_history(ctx, &origin, &[90, 40]);
            ctx.nd_flip();
            let Backtrack::Restored { nodes } = ctx.backtrack(&origin).await.unwrap() else {
                panic!("expected a restore");
            };
            assert_eq!(nodes, vec![int_node(90)]);
        },
    );
}

#[test]
fn an_exact_shrink_verify_vanish_aborts_under_error_strictness() {
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![int_node(90)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            let err = ctx
                .shrink_origin(
                    origin.to_string(),
                    vec![int_node(90)],
                    Verbosity::Quiet,
                    &output,
                    None,
                    &mut shrunk,
                )
                .await
                .unwrap_err();
            assert!(
                matches!(err, RunError::Flaky(_)),
                "an exact-repeat vanish aborts through the cache mismatch: {err:?}"
            );
        },
    );
}

#[test]
fn a_divergent_shrink_verify_vanish_aborts_under_error_strictness() {
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() || rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![int_node(90)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let output = ctx.settings.output.clone();
            let mut shrunk = crate::native::HashSet::default();
            let err = ctx
                .shrink_origin(
                    origin.to_string(),
                    vec![int_node(90)],
                    Verbosity::Quiet,
                    &output,
                    None,
                    &mut shrunk,
                )
                .await
                .unwrap_err();
            assert!(
                matches!(err, RunError::Flaky(_)),
                "a divergent vanish misses the cache and aborts on the outcome: {err:?}"
            );
        },
    );
}

/// A kind change at a shared prefix aborts through the kind ledger with
/// the tree's message; the check's own diagnostic covers the shape the
/// ledger cannot see — a divergence in how many choices the body drew.
#[test]
fn a_first_check_structural_miss_aborts_under_error_strictness() {
    let bug = "bug";
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            boom(bug)
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(5), int_node(6)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            match ctx.first_check_sweep().await {
                Err(crate::backend::RunError::NonDeterministic(msg)) => {
                    assert!(msg.contains("position 1"), "got: {msg}");
                }
                other => panic!("expected RunError::NonDeterministic, got {other:?}"),
            }
        },
    );
}

#[test]
fn a_first_check_outcome_miss_aborts_under_error_strictness() {
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(true)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            match ctx.first_check_sweep().await {
                Err(crate::backend::RunError::Flaky(msg)) => {
                    assert!(msg.contains("Flaky test detected"), "got: {msg}");
                }
                other => panic!("expected RunError::Flaky, got {other:?}"),
            }
        },
    );
}

/// The check's cost pin: a deterministic always-failing body pays the
/// discovering execution, [`FIRST_CHECK_REPLAYS`] check replays, and the
/// report-time final replay — nothing else.
#[test]
fn a_deterministic_failing_run_pays_exactly_the_first_check_per_origin() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let result = reuse_run(
        Settings::new()
            .database(Some(path))
            .phases([Phase::Generate])
            .report_multiple_failures(false)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| {
            execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            boom("stable")
        },
    );
    assert!(result.is_ok());
    assert_eq!(
        execs.load(Ordering::SeqCst) as u64,
        1 + FIRST_CHECK_REPLAYS + 1
    );
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
            ctx.record_run(&interesting_at(origin, big), Duration::ZERO, false)
                .unwrap();
            assert_eq!(ctx.origins.incumbent(origin).unwrap().len(), 2);

            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert_eq!(
                ctx.origins.incumbent(origin).unwrap().len(),
                2,
                "a raw interesting run must not displace an occupied origin"
            );

            ctx.origins
                .entry(origin)
                .confirm(0.9, None, Vec::new(), (4, 4))
                .unwrap();
            ctx.record_run(
                &interesting_at(origin, vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert_eq!(ctx.origins.incumbent(origin).unwrap().len(), 2);
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
            )
            .unwrap();
            assert!(ctx.origins.incumbent("Panic: bug").is_some());
            assert!(ctx.origins.needs_confirmation("Panic: bug"));
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
            )
            .unwrap();
            let output = ctx.settings.output.clone();
            ctx.nd_discovery_sweep(Verbosity::Debug, &output)
                .await
                .unwrap();
            assert!(!ctx.origins.any_live());
            assert_eq!(
                ctx.origins.unconfirmed().collect::<Vec<_>>(),
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
            )
            .unwrap();
            let output = ctx.settings.output.clone();
            ctx.nd_discovery_sweep(Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert!(!ctx.origins.needs_confirmation("Panic: bug"));
            assert!(
                ctx.origins.get("Panic: bug").unwrap().pool().len() > 1,
                "confirmation replays realizing fresh continuations must be captured"
            );
            let (witness, anchor) = ctx.origins.entry("Panic: bug").take_witness().unwrap();
            assert_eq!(witness.origin.as_deref(), Some("Panic: bug"));
            assert!(anchor > 0.0);
            assert!(ctx.origins.incumbent("Panic: bug").is_some());
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
            )
            .unwrap();
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
            ctx.origins
                .entry("Panic: bug")
                .confirm(0.1, None, vec![vec![ChoiceValue::Boolean(false)]], (4, 9))
                .unwrap();
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
                incumbent_bounces: (0, 0),
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
fn nd_evidence_batch_restores_the_capture_flag() {
    with_engine(
        nd_settings(),
        None,
        |ds| match rbool(ds) {
            Ok(true) => boom("bug"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            ctx.capture_replays = true;
            ctx.nd_evidence_batch("Panic: bug", &[ChoiceValue::Boolean(true)], None)
                .await
                .unwrap();
            assert!(
                ctx.capture_replays,
                "a batch inside a capture window restores the flag"
            );
            ctx.capture_replays = false;
            ctx.nd_evidence_batch("Panic: bug", &[ChoiceValue::Boolean(true)], None)
                .await
                .unwrap();
            assert!(!ctx.capture_replays);
        },
    );
}

#[test]
fn anchor_seed_extension_reaches_the_reference_batch() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            let failing = match rbool(ds) {
                Ok(v) => v,
                Err(()) => return TestCaseResult::Overrun,
            };
            if failing {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let batch = ctx
                .nd_evidence_batch("Panic: bug", &[ChoiceValue::Boolean(true)], None)
                .await
                .unwrap();
            assert!(batch.bar_accepted);
            assert_eq!(
                batch.evidence.runs(),
                nd::ANCHOR_SEED_RUNS,
                "a bar accept extends to the reference batch before seeding the anchor"
            );
            assert!(batch.evidence.lower_bound() > nd::RETENTION_HIGH_WATER);
            assert!(batch.witness.is_some());
            let rejected = ctx
                .nd_evidence_batch("Panic: bug", &[ChoiceValue::Boolean(false)], None)
                .await
                .unwrap();
            assert!(!rejected.bar_accepted);
            assert_eq!(
                rejected.evidence.runs(),
                nd::GATE_RUNS,
                "a rejected batch stops at the bar"
            );
        },
    );
}

#[test]
fn nd_gauntlet_accept_tops_the_ledger_up_to_the_reference_batch() {
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
            let output = ctx.settings.output.clone();
            let mut probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: "Panic: bug".to_string(),
                verbosity: Verbosity::Quiet,
                output,
                gauntlet: true,
                ledger: HashMap::default(),
                raised: crate::native::HashSet::default(),
                anchor: 0.0,
                sweep: SweepMode::Fast,
                pending_accept: None,
                incumbent_bounces: (0, 0),
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(matched);
            let accept = probe.pending_accept.as_ref().unwrap();
            let ledger = probe.ledger.get(&accept.key).unwrap();
            assert_eq!(
                ledger.evidence.runs(),
                nd::ANCHOR_SEED_RUNS,
                "an accept tops the ledger up before it can raise the anchor"
            );
            assert_eq!(ledger.verdict, Some(true), "an accept latches");
            assert!(
                accept.lower_bound > nd::RETENTION_HIGH_WATER,
                "an always-failing candidate seeds a high-water anchor, got {}",
                accept.lower_bound
            );
        },
    );
}

#[test]
fn an_exhausted_alpha_budget_pins_new_candidates_at_the_ceiling() {
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
            let origin = "Panic: bug".to_string();
            let spend = &mut ctx.origins.entry(&origin).gauntlet_spend;
            while spend.charge(&nd::Evidence::default(), 0.0, true, None)
                != nd::GAUNTLET_MIN_FAILS_CEILING
            {}
            let output = ctx.settings.output.clone();
            let mut probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: origin,
                verbosity: Verbosity::Quiet,
                output,
                gauntlet: true,
                ledger: HashMap::default(),
                raised: crate::native::HashSet::default(),
                anchor: 0.0,
                sweep: SweepMode::Fast,
                pending_accept: None,
                incumbent_bounces: (0, 0),
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(
                matched,
                "an always-failing candidate still accepts at the ceiling"
            );
            let accept = probe.pending_accept.as_ref().unwrap();
            let ledger = probe.ledger.get(&accept.key).unwrap();
            assert_eq!(
                ledger.min_fails,
                nd::GAUNTLET_MIN_FAILS_CEILING,
                "the engine-held spend map outlives probe rebuilds"
            );
        },
    );
}

#[test]
fn a_rejected_candidate_latches_its_verdict() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_engine(
        nd_settings(),
        None,
        |ds| {
            let n = execs.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if n == 0 {
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
                anchor: 0.5,
                sweep: SweepMode::Fast,
                pending_accept: None,
                incumbent_bounces: (0, 0),
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(!matched, "the recruit's ledger drives to a reject");
            let key = serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap();
            assert_eq!(probe.ledger.get(&key).unwrap().verdict, Some(false));
            let replays = execs.load(Ordering::SeqCst);
            let (rematch, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(!rematch, "a bound verdict is final");
            assert_eq!(
                execs.load(Ordering::SeqCst),
                replays + 1,
                "a latched reject costs the proposal run and no drive"
            );
        },
    );
}

/// A per-execution die (splitmix64's finalizer over the execution index):
/// failure schedules that don't correlate with replay or shrink ordering.
fn scramble(i: usize) -> u64 {
    let mut x = i as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[test]
fn boost_skips_a_reliable_incumbent() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let execs = AtomicUsize::new(0);
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(10)
        .seed(Some(0xB005))
        .verbosity(Verbosity::Debug)
        .output(Output::callback(move |line| {
            sink.lock().unwrap().push(line.to_string());
        }));
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if scramble(execs.fetch_add(1, Ordering::SeqCst)) % 10 < 7 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(
        !lines.lock().unwrap().iter().any(|l| l.contains("nd boost")),
        "a 70%-reliable incumbent sits above the boost floor"
    );
}

#[test]
fn boost_runs_below_the_floor() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let execs = AtomicUsize::new(0);
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let mut settings = Settings::new()
        .database(None)
        .test_cases(20)
        .seed(Some(0xB006))
        .verbosity(Verbosity::Debug)
        .output(Output::callback(move |line| {
            sink.lock().unwrap().push(line.to_string());
        }));
    settings.nd_force = true;
    reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if scramble(execs.fetch_add(1, Ordering::SeqCst)) % 5 == 0 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("nd boost: origin=Panic: bug racing")),
        "a 20%-reliable incumbent sits below the boost floor"
    );
}

#[test]
fn shrink_does_not_drift_on_a_rising_landscape() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .test_cases(20)
        .seed(Some(0x2))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let n = match rint(ds, 0, 20) {
            Ok(v) => v,
            Err(()) => return TestCaseResult::Overrun,
        };
        if (scramble(execs.fetch_add(1, Ordering::SeqCst)) % 1000) < (100 + 40 * n) as u64 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let db = DirectoryTestCaseDatabase::new(&path);
    let entries = db.fetch(b"k");
    assert_eq!(entries.len(), 1);
    let state = crate::native::blob::decode_nd_state(&entries[0]).unwrap();
    let ChoiceValue::Integer(n_final) = &state.timelines[0][0] else {
        panic!("the incumbent must start with the drawn size");
    };
    let n_final = n_final.to_i64().unwrap();
    assert!(
        n_final >= 10,
        "shrinking to n = {n_final} (p = {}) trades failure probability away",
        (100 + 40 * n_final) as f64 / 1000.0
    );
}

#[test]
fn deterministic_core_is_retained() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)]).unwrap(),
    );
    let mut settings = Settings::new()
        .database(Some(path.clone()))
        .test_cases(10)
        .seed(Some(0xDE7))
        .verbosity(Verbosity::Quiet);
    settings.nd_force = true;
    let result = reuse_run(settings, "k", |ds| {
        let core = match rbool(ds) {
            Ok(v) => v,
            Err(()) => return TestCaseResult::Overrun,
        };
        if core || scramble(execs.fetch_add(1, Ordering::SeqCst)) % 10 < 7 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let state = db
        .fetch(b"k")
        .iter()
        .find_map(|e| crate::native::blob::decode_nd_state(e))
        .unwrap();
    assert_eq!(
        state.timelines[0],
        vec![ChoiceValue::Boolean(true)],
        "a 70%-reliable candidate must not displace a deterministic incumbent"
    );
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
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
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
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
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
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
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

/// The first-interesting check catches the post-discovery kind switch —
/// its replay realizes different choices — and warn strictness prints its
/// notice exactly once.
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
    assert_eq!(notices, 1, "the check's flip prints the warn notice once");
}

#[test]
fn reuse_kind_flip_under_quiet_completes_without_failures() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(false)]).unwrap(),
    );

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

/// Exec 0 draws over [0, 100] and fails; every later exec widens the range
/// to [0, 101] and passes. The pre-shrink verification replay then flips
/// twice — once for the verdict flake the cache reports (the replay
/// realizes the discovering case's exact values, now passing), once for
/// the vanished failure — and the second flip must be a no-op.
#[test]
fn a_double_flip_in_one_verify_prints_the_warn_notice_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let execs = AtomicUsize::new(0);
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
                events: Vec::new(),
                divergence: None,
                live: Vec::new(),
            };
            ctx.record_run(&valid, Duration::from_secs(1), true)
                .unwrap();
            ctx.record_run(
                &interesting_at("Panic: measured", vec![bool_node(true)]),
                Duration::ZERO,
                true,
            )
            .unwrap();
            assert_eq!(ctx.calls, 0);
            assert_eq!(ctx.valid_test_cases, 0);
            assert_eq!(ctx.total_test_time, Duration::ZERO);
            assert!(ctx.first_bug_at.is_none());
            assert!(ctx.last_bug_at.is_none());
            assert!(
                ctx.origins.incumbent("Panic: measured").is_some(),
                "a measurement run still admits a vacant origin"
            );
            assert!(ctx.origins.needs_confirmation("Panic: measured"));

            ctx.record_run(&valid, Duration::from_secs(1), false)
                .unwrap();
            assert_eq!(ctx.calls, 1);
            assert_eq!(ctx.valid_test_cases, 1);
            assert_eq!(ctx.total_test_time, Duration::from_secs(1));
        },
    );
}

#[test]
fn nd_reproduce_replays_the_counterexample_as_one_test_case_whichever_branch_the_test_takes() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let executions = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&executions);
    with_engine(
        nd_settings(),
        None,
        move |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if counter.fetch_add(1, Ordering::SeqCst) % 2 == 1 {
                match rint(ds, 0, 100) {
                    Ok(7) => boom("branch"),
                    Ok(_) => TestCaseResult::Valid,
                    Err(()) => TestCaseResult::Overrun,
                }
            } else {
                match rbool(ds) {
                    Ok(true) => boom("branch"),
                    Ok(false) => TestCaseResult::Valid,
                    Err(()) => TestCaseResult::Overrun,
                }
            }
        },
        async |ctx| {
            let stored = vec![
                vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
                vec![
                    ChoiceValue::Boolean(true),
                    ChoiceValue::Integer(BigInt::from(7)),
                ],
            ];
            for _ in 0..2 {
                let (run, evidence) = ctx
                    .nd_reproduce(Some("Panic: branch"), &stored, 1, 0, 0)
                    .await
                    .unwrap();
                let run = run.unwrap();
                assert_eq!(run.origin.as_deref(), Some("Panic: branch"));
                assert_eq!(
                    evidence.runs(),
                    1,
                    "one attempt reproduces whichever branch the test took (decision 74)"
                );
                assert_eq!(run.divergence, None, "a served branch is not a divergence");
            }
        },
    );
}

#[test]
fn a_replay_that_leaves_its_counterexample_is_named_at_debug_verbosity() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    with_engine(
        nd_settings()
            .verbosity(Verbosity::Debug)
            .output(Output::callback(move |line| {
                sink.lock().unwrap().push(line.to_string());
            })),
        None,
        |ds| match rint(ds, 0, 100) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            let stored = vec![vec![ChoiceValue::Boolean(true)]];
            let (run, _) = ctx.nd_reproduce(None, &stored, 1, 0, 0).await.unwrap();
            assert!(run.is_none());
        },
    );
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l == "replay left its counterexample at position 0 of stream []"),
        "{:?}",
        lines.lock().unwrap()
    );
}

#[test]
fn nd_reproduce_spends_its_attempts_on_diverged_clone_replays() {
    use crate::native::core::CloneRecord;
    use alloc::sync::Arc;
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            let child = match ds.clone_stream() {
                Ok(c) => c,
                Err(_) => return TestCaseResult::Overrun,
            };
            if rbool(&*child).is_err() {
                return TestCaseResult::Overrun;
            }
            match rint(&*child, 0, 100) {
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx| {
            let stored = vec![vec![
                ChoiceValue::Boolean(true),
                ChoiceValue::Clone(Arc::new(CloneRecord::from_values(vec![
                    ChoiceValue::Boolean(true),
                    ChoiceValue::Boolean(true),
                ]))),
            ]];
            let (run, evidence) = ctx.nd_reproduce(None, &stored, 3, 0, 0).await.unwrap();
            assert!(run.is_none());
            assert_eq!(
                evidence.runs(),
                3,
                "every replay is one budgeted trial, diverged or not (decision 71)"
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
                .nd_reproduce(Some("Panic: splice"), &stored, 2, 50, 0)
                .await
                .unwrap();
            let run = run.unwrap();
            assert_eq!(run.origin.as_deref(), Some("Panic: splice"));
            assert!(
                evidence.runs() > 2,
                "the counterexample faces its attempts before the splices"
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
                .nd_reproduce(Some("Panic: clone splice"), &stored, 2, 50, 0)
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
                .nd_reproduce(Some("Panic: fresh"), &stored, 2, 0, 0)
                .await
                .unwrap();
            assert!(run.is_none(), "the stored timeline never fails");
            let (run, _) = ctx
                .nd_reproduce(Some("Panic: fresh"), &stored, 2, 0, 40)
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
                incumbent_bounces: (0, 0),
            };
            let nodes = vec![bool_node(true)];
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(matched);
            probe.candidate_adopted().unwrap();
            let raised_anchor = probe.anchor;
            assert!(
                raised_anchor > 0.3,
                "the first adopted accept of a candidate raises the monotone anchor"
            );
            let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(matched);
            probe.candidate_adopted().unwrap();
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
fn nd_trusted_promotion_repersists_the_stored_pool() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let extra = vec![
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
    ];
    let state = crate::native::blob::NdReproState {
        timelines: vec![
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
            extra.clone(),
        ],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());

    let result = reuse_run(
        Settings::new()
            .database(Some(path))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rbool(ds) {
            Ok(true) => boom("bug"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let blob = result.failures[0].reproduce_blob.as_deref().unwrap();
    let Some(crate::native::blob::DecodedBlob::Nd(reported)) =
        crate::native::blob::decode_blob(blob)
    else {
        panic!("a promoted trusted origin emits replay state");
    };
    assert_eq!(
        reported.timelines,
        vec![vec![ChoiceValue::Boolean(true)]],
        "the stored extra timeline never served a replay, so the delete pass \
         removed it (decision 75); the shrunk incumbent is the whole counterexample"
    );
    assert!(!reported.timelines.contains(&extra));
}

#[test]
fn bounce_budget_is_zero_without_bounces_and_scales_with_the_rate() {
    assert_eq!(bounce_budget(0, 20), 0);
    assert_eq!(bounce_budget(3, 3), nd::GAUNTLET_CAP);
    assert_eq!(bounce_budget(10, 20), nd::GAUNTLET_CAP);
    assert_eq!(bounce_budget(1, 4), 10);
}

/// A body that alternates between two branches on successive executions
/// after a shared first boolean — David's `ps`/`pt`: odd executions draw
/// two more booleans and an integer (failing on true, true, 42), even ones
/// draw two integers (failing on 7, 9). Neither branch fails by luck under
/// a random continuation often enough to matter.
fn branching_body() -> impl FnMut(&dyn DataSource) -> TestCaseResult {
    let mut executions = 0usize;
    move |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        executions += 1;
        if executions % 2 == 0 {
            match (rint(ds, 0, 100), rint(ds, 0, 100)) {
                (Ok(7), Ok(9)) => boom("branch"),
                (Ok(_), Ok(_)) => TestCaseResult::Valid,
                _ => TestCaseResult::Overrun,
            }
        } else {
            match (rbool(ds), rbool(ds), rint(ds, 0, 100)) {
                (Ok(true), Ok(true), Ok(42)) => boom("branch"),
                (Ok(_), Ok(_), Ok(_)) => TestCaseResult::Valid,
                _ => TestCaseResult::Overrun,
            }
        }
    }
}

fn branch_s() -> Vec<ChoiceValue> {
    vec![
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(true),
        ChoiceValue::Integer(BigInt::from(42)),
    ]
}

fn branch_s_nodes() -> Vec<ChoiceNode> {
    vec![
        bool_node(true),
        bool_node(true),
        bool_node(true),
        int_node(42),
    ]
}

fn branch_t() -> Vec<ChoiceValue> {
    vec![
        ChoiceValue::Boolean(true),
        ChoiceValue::Integer(BigInt::from(7)),
        ChoiceValue::Integer(BigInt::from(9)),
    ]
}

#[test]
fn the_gauntlet_abandons_a_candidate_whose_rerun_leaves_it_when_the_incumbent_never_bounced() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    with_engine(nd_settings(), None, branching_body(), async |ctx| {
        let mut probe = EngineShrinkProbe {
            engine: &mut *ctx,
            target_origin: "Panic: branch".to_string(),
            verbosity: Verbosity::Debug,
            output: Output::callback(move |line| sink.lock().unwrap().push(line.to_string())),
            gauntlet: true,
            ledger: HashMap::default(),
            raised: crate::native::HashSet::default(),
            anchor: 0.3,
            sweep: SweepMode::Fast,
            pending_accept: None,
            incumbent_bounces: (0, 20),
        };
        let nodes = branch_s_nodes();
        let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
        assert!(!matched, "abandoned, not accepted");
        let ledger = probe.ledger.values().next().unwrap();
        assert_eq!(
            ledger.verdict, None,
            "no evidence either way: no verdict latched"
        );
        assert_eq!(ledger.bounces, 1);
        assert_eq!(
            (ledger.evidence.fails(), ledger.evidence.runs()),
            (1, 1),
            "only the recruiting run is evidence"
        );
        assert!(probe.pending_accept.is_none());
    });
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("gauntlet abandoned a candidate: 1 reruns left the timeline")),
        "{:?}",
        lines.lock().unwrap()
    );
}

#[test]
fn the_gauntlet_counts_only_on_timeline_reruns_within_the_bounce_budget() {
    with_engine(nd_settings(), None, branching_body(), async |ctx| {
        let mut probe = EngineShrinkProbe {
            engine: &mut *ctx,
            target_origin: "Panic: branch".to_string(),
            verbosity: Verbosity::Quiet,
            output: Output::callback(|_| {}),
            gauntlet: true,
            ledger: HashMap::default(),
            raised: crate::native::HashSet::default(),
            anchor: 0.3,
            sweep: SweepMode::Fast,
            pending_accept: None,
            incumbent_bounces: (10, 20),
        };
        let nodes = branch_s_nodes();
        let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
        assert!(
            matched,
            "every on-timeline rerun fails, so the candidate is accepted"
        );
        let ledger = probe.ledger.values().next().unwrap();
        assert_eq!(ledger.verdict, Some(true));
        assert_eq!(
            (ledger.evidence.fails(), ledger.evidence.runs()),
            (nd::ANCHOR_SEED_RUNS, nd::ANCHOR_SEED_RUNS),
            "off-timeline reruns are not evidence"
        );
        assert!(ledger.bounces >= nd::ANCHOR_SEED_RUNS - 1);
        let bounces = ledger.bounces;
        let accept = probe.pending_accept.as_ref().unwrap();
        assert_eq!(accept.bounces, accept_bounces(bounces));
        probe.candidate_adopted().unwrap();
        assert_eq!(probe.incumbent_bounces, accept_bounces(bounces));
    });
}

fn accept_bounces(bounces: u64) -> (u64, u64) {
    (bounces, nd::ANCHOR_SEED_RUNS + bounces)
}

#[test]
fn the_multiverse_passes_keep_both_branches_and_promote_the_smaller_one() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    with_engine(nd_settings(), None, branching_body(), async |ctx| {
        let origin = ctx.origins.entry("Panic: branch");
        origin.adopt(branch_s_nodes());
        origin
            .confirm(
                0.8,
                None,
                pooled_timelines(branch_s(), vec![branch_t()]),
                (20, 20),
            )
            .unwrap();
        assert_eq!(
            timeline_order(&branch_t(), &branch_s()),
            core::cmp::Ordering::Less,
            "a shorter timeline is the smaller one"
        );
        let output = Output::callback(move |line| sink.lock().unwrap().push(line.to_string()));
        ctx.nd_multiverse_shrink("Panic: branch", 0.8, None, Verbosity::Debug, &output)
            .await
            .unwrap();
        let counterexample = ctx.origins.get("Panic: branch").unwrap();
        assert_eq!(
            counterexample.timelines(),
            vec![branch_t(), branch_s()],
            "neither branch can be deleted — each reproduces only half the runs — \
             and the smaller one is promoted to the front"
        );
        assert_eq!(
            counterexample.incumbent_values().unwrap(),
            branch_t(),
            "the promoted component is installed from a witness that stayed on it"
        );
    });
    let lines = lines.lock().unwrap();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("nd multiverse delete") && l.ends_with("accepted=false"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("nd multiverse reorder") && l.ends_with("accepted=true"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("nd multiverse splice") && l.ends_with("accepted=false")),
        "{lines:?}"
    );
}

#[test]
fn the_multiverse_passes_stop_after_the_round_cap_and_on_the_deadline() {
    with_engine(
        nd_settings(),
        None,
        |ds| match rbool(ds) {
            Ok(true) => boom("bug"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            let dead: Vec<Vec<ChoiceValue>> = (1..=6)
                .map(|n| {
                    let mut timeline = vec![ChoiceValue::Boolean(true)];
                    timeline.extend(core::iter::repeat_n(ChoiceValue::Boolean(false), n));
                    timeline
                })
                .collect();
            let origin = ctx.origins.entry("Panic: bug");
            origin.adopt(vec![bool_node(true)]);
            origin
                .confirm(
                    0.5,
                    None,
                    pooled_timelines(vec![ChoiceValue::Boolean(true)], dead.clone()),
                    (20, 20),
                )
                .unwrap();
            assert_eq!(origin.timelines().len(), 7);
            let output = Output::callback(|_| {});
            ctx.nd_multiverse_shrink(
                "Panic: bug",
                0.5,
                crate::sys::Instant::now(),
                Verbosity::Quiet,
                &output,
            )
            .await
            .unwrap();
            assert_eq!(
                ctx.origins.entry("Panic: bug").timelines().len(),
                7,
                "an expired deadline stops before the first candidate"
            );
            ctx.nd_multiverse_shrink("Panic: bug", 0.5, None, Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert_eq!(
                ctx.origins.entry("Panic: bug").timelines().len(),
                7 - MULTIVERSE_ROUNDS,
                "one dead timeline goes per round, up to the round cap"
            );
            ctx.origins
                .entry("Panic: bug")
                .install_set(&[vec![ChoiceValue::Boolean(true)]], None);
            ctx.nd_multiverse_shrink("Panic: bug", 0.5, None, Verbosity::Quiet, &output)
                .await
                .unwrap();
            assert_eq!(ctx.origins.entry("Panic: bug").timelines().len(), 1);
        },
    );
}

#[test]
fn a_set_accept_that_needs_a_witness_on_its_first_timeline_is_refused_without_one() {
    with_engine(
        nd_settings(),
        None,
        |ds| match (rint(ds, 0, 100), rint(ds, 0, 100)) {
            (Ok(_), Ok(9)) => boom("second"),
            (Ok(_), Ok(_)) => TestCaseResult::Valid,
            _ => TestCaseResult::Overrun,
        },
        async |ctx| {
            ctx.origins
                .entry("Panic: second")
                .confirm(0.5, None, Vec::new(), (20, 20))
                .unwrap();
            let candidate = vec![
                vec![
                    ChoiceValue::Boolean(true),
                    ChoiceValue::Integer(BigInt::from(9)),
                ],
                vec![
                    ChoiceValue::Integer(BigInt::from(5)),
                    ChoiceValue::Integer(BigInt::from(9)),
                ],
            ];
            let verdict = ctx
                .nd_evaluate_set("Panic: second", &candidate, 0.5, true)
                .await
                .unwrap();
            assert!(
                !verdict.accepted,
                "every replay reproduces through the second timeline, so nothing can \
                 be installed as the first"
            );
            assert!(verdict.witness.is_none());
            let verdict = ctx
                .nd_evaluate_set("Panic: second", &candidate, 0.5, false)
                .await
                .unwrap();
            assert!(
                verdict.accepted,
                "the same set is a fine counterexample as it stands"
            );
        },
    );
}

#[test]
fn measurement_replays_are_counted_in_the_run_statistics() {
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
            )
            .unwrap();
            assert!(
                !ctx.statistics
                    .render()
                    .iter()
                    .any(|l| l.contains("measurement"))
            );
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                ctx.statistics
                    .render()
                    .iter()
                    .any(|l| l.contains("nondeterministic handling: measurement replays")),
                "the statistics block reports the measurement replays"
            );
        },
    );
}

#[test]
fn nd_trusted_zero_fail_shrink_batch_keeps_trusted_and_reports_honestly() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)]],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());

    let execs = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet)
            .report_multiple_failures(false),
        "k",
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
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(
        result.failures[0].reproduce_blob.is_some(),
        "a trusted origin is still reported with replay state"
    );
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.contains("reproduced from stored timelines earlier this run")
            && caveat.contains("not reproduced at report time"),
        "a zero-fail batch keeps the origin trusted and the caveat says so: {caveat:?}"
    );
}

#[test]
fn nd_trusted_weak_batch_promotes_and_shrinks_under_the_floor() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let state = crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)]],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());

    let execs = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet)
            .report_multiple_failures(false),
        "k",
        |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if execs.fetch_add(1, Ordering::SeqCst) < 2 {
                boom("bug")
            } else {
                TestCaseResult::Valid
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].reproduce_blob.is_some());
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.contains("confirmed") && !caveat.contains("stored timelines"),
        "one failing replay in the batch promotes the trusted origin: {caveat:?}"
    );
    assert!(
        caveat.contains("(failed 2 of "),
        "promotion folds the trusted reuse evidence into the confirmed counts: {caveat:?}"
    );
}

#[test]
fn nd_confirmed_dry_caveat_does_not_fold_report_replays_into_confirmation_counts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut settings = Settings::new()
        .database(None)
        .phases([Phase::Generate, Phase::Shrink])
        .verbosity(Verbosity::Quiet)
        .report_multiple_failures(false);
    settings.nd_force = true;
    let execs = AtomicUsize::new(0);
    let result = reuse_run(settings, "k", |ds| {
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        if execs.fetch_add(1, Ordering::SeqCst) <= 4 {
            boom("bug")
        } else {
            TestCaseResult::Valid
        }
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let caveat = result.failures[0].caveat.as_deref().unwrap();
    assert!(
        caveat.contains("(failed 4 of 20 replays)")
            && caveat.contains("not reproduced at report time"),
        "the dry wording quotes confirmation counts alone: {caveat:?}"
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
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());

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
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
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
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());

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
            let result = ctx.nd_reproduce(None, &stored, 0, 0, 4).await;
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
                incumbent_bounces: (0, 0),
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
            ctx.origins
                .entry("Panic: bug")
                .confirm(0.5, None, pool, (4, 6))
                .unwrap();
            let incumbent = vec![ChoiceValue::Boolean(false)];
            let state = ctx.nd_state_for("Panic: bug", incumbent.clone()).unwrap();
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
            )
            .unwrap();
            assert!(ctx.origins.incumbent("Panic: bug").is_some());
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                !ctx.origins.any_live(),
                "a dry unconfirmed origin leaves the interesting map"
            );
            assert_eq!(
                ctx.origins.unconfirmed().collect::<Vec<_>>(),
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
            )
            .unwrap();
            assert!(ctx.origins.needs_confirmation("Panic: bug"));
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                !ctx.origins.needs_confirmation("Panic: bug"),
                "a reproducing final replay confirms the origin"
            );
            assert!(ctx.origins.incumbent("Panic: bug").is_some());
            let report = ctx.build_report().unwrap();
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
            )
            .unwrap();
            ctx.origins
                .entry("Panic: a")
                .confirm(0.5, None, Vec::new(), (4, 6))
                .unwrap();
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let report = ctx.build_report().unwrap();
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
            ctx.origins.entry("Panic: a").replace(vec![bool_node(true)]);
            ctx.origins
                .entry("Panic: a")
                .confirm(0.5, None, Vec::new(), (4, 6))
                .unwrap();
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                ctx.origins.incumbent("Panic: b").is_some(),
                "the report-time measurement run admits the new origin"
            );
            let report = ctx.build_report().unwrap();
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
            )
            .unwrap();
            ctx.origins.entry("Panic: a").reject((0, 24));
            assert!(ctx.origins.needs_confirmation("Panic: a"));
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let report = ctx.build_report().unwrap();
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
            ctx.origins
                .entry("Panic: a")
                .replace(vec![bool_node(true), bool_node(true)]);
            ctx.origins
                .entry("Panic: a")
                .confirm(0.5, None, Vec::new(), (4, 6))
                .unwrap();
            ctx.record_run(
                &interesting_at("Panic: b", vec![bool_node(false)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            let report = ctx.build_report().unwrap();
            assert_eq!(report.failures.len(), 1);
            assert_eq!(
                report.failures[0].origin, "Panic: a",
                "an unconfirmed origin must not displace a confirmed one \
                 under single-failure reporting"
            );
        },
    );
}

/// Amends the decision-39 pin: observations record under ND handling too,
/// as selection-biased seed material for the measured race (decision 68).
#[test]
fn nd_runs_record_targeting_observations_as_race_seeds() {
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
            ctx.record_run(&observing_run(), Duration::ZERO, false)
                .unwrap();
            assert!(!ctx.targeting.is_empty());
        },
    );
    with_counting_ctx(
        |_ds| TestCaseResult::Valid,
        async |ctx, _execs| {
            ctx.record_run(&observing_run(), Duration::ZERO, false)
                .unwrap();
            assert!(!ctx.targeting.is_empty());
        },
    );
}

/// The full ND targeting loop on a rising landscape: the reference comes
/// from a fresh batch (not the recorded raw maximum, which is deliberately
/// cursed here), every race's adoption raises the reference, and the loop
/// spends exactly its race budget. The body goes invalid every tenth
/// execution so the invalid arms of the race, holdout, and scoring loops
/// are all exercised without blocking adoption.
#[test]
fn nd_targeting_races_adopt_improvements_onto_a_fresh_reference() {
    let settings = nd_settings().verbosity(Verbosity::Debug);
    let execs = Rc::new(Cell::new(0u64));
    let counter = execs.clone();
    with_engine(
        settings,
        None,
        move |ds| {
            counter.set(counter.get() + 1);
            if counter.get() % 10 == 0 {
                return TestCaseResult::Invalid;
            }
            let n = match rint(ds, 0, 100) {
                Ok(n) => n,
                Err(()) => return TestCaseResult::Overrun,
            };
            ds.target_observation(n as f64, "").unwrap();
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.targeting.record(
                &[ChoiceValue::Integer(BigInt::from(0))],
                &HashMap::from_iter([("".to_string(), 1e9)]),
            );
            ctx.optimise_targets_nd().await.unwrap();
            let target = ctx.targeting.nd_target("").unwrap();
            assert!(!target.dead);
            assert!(
                target.reference > 0.0,
                "adoption should have raised the reference above the seed's \
                 true score, got {}",
                target.reference
            );
            assert!(
                target.reference < 1e9,
                "the reference must come from measurement, not the recorded \
                 raw maximum"
            );
        },
    );
}

/// A label whose body never reports it: the reference batch observes no
/// score, the label is marked dead, and later firings spend nothing on it.
#[test]
fn nd_targeting_marks_an_unobserved_label_dead_and_skips_it() {
    let execs = Rc::new(Cell::new(0u64));
    let counter = execs.clone();
    with_engine(
        nd_settings(),
        None,
        move |_ds| {
            counter.set(counter.get() + 1);
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.targeting.record(
                &[ChoiceValue::Boolean(true)],
                &HashMap::from_iter([("s".to_string(), 1.0)]),
            );
            ctx.optimise_targets_nd().await.unwrap();
            assert!(ctx.targeting.nd_target("s").unwrap().dead);
            let after_first = execs.get();
            ctx.optimise_targets_nd().await.unwrap();
            assert_eq!(execs.get(), after_first);
        },
    );
}

/// A winner that never strictly beats the reference is not adopted: the
/// score is constant, so every holdout run ties and the sign test fails.
#[test]
fn nd_targeting_rejects_a_winner_that_never_beats_the_reference() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            if rint(ds, 0, 100).is_err() {
                return TestCaseResult::Overrun;
            }
            ds.target_observation(5.0, "").unwrap();
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.targeting.record(
                &[ChoiceValue::Integer(BigInt::from(0))],
                &HashMap::from_iter([("".to_string(), 5.0)]),
            );
            ctx.optimise_targets_nd().await.unwrap();
            assert_eq!(ctx.targeting.nd_target("").unwrap().reference, 5.0);
        },
    );
}

/// A discovery mid-race ends targeting: the failing region is reachable
/// only by the race's own probes, the discovered origin fills the
/// interesting map, and the second label's race never starts.
#[test]
fn nd_targeting_yields_to_a_discovery() {
    let execs = Rc::new(Cell::new(0u64));
    let counter = execs.clone();
    with_engine(
        nd_settings(),
        None,
        move |ds| {
            counter.set(counter.get() + 1);
            let n = match rint(ds, 0, 40) {
                Ok(n) => n,
                Err(()) => return TestCaseResult::Overrun,
            };
            if n >= 30 {
                return TestCaseResult::Interesting(Failure {
                    origin: "Panic: found by targeting".to_string(),
                    reproduce_blob: None,
                    caveat: None,
                });
            }
            ds.target_observation(n as f64, "a").unwrap();
            ds.target_observation(-(n as f64), "b").unwrap();
            TestCaseResult::Valid
        },
        async |ctx| {
            let seed = [ChoiceValue::Integer(BigInt::from(0))];
            ctx.targeting.record(
                &seed,
                &HashMap::from_iter([("a".to_string(), 0.0), ("b".to_string(), 0.0)]),
            );
            ctx.optimise_targets_nd().await.unwrap();
            assert!(ctx.origins.any_live());
            let spent = execs.get();
            ctx.optimise_targets_nd().await.unwrap();
            assert_eq!(execs.get(), spent);
        },
    );
}

/// A body with no draws leaves nothing to perturb: every probe realizes
/// the reference timeline itself, the candidate pool stays empty, and the
/// race adopts nothing.
#[test]
fn nd_targeting_race_with_no_distinct_candidates_adopts_nothing() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            ds.target_observation(1.0, "").unwrap();
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.targeting
                .record(&[], &HashMap::from_iter([("".to_string(), 1.0)]));
            ctx.optimise_targets_nd().await.unwrap();
            let target = ctx.targeting.nd_target("").unwrap();
            assert!(!target.dead);
            assert_eq!(target.reference, 1.0);
            assert!(target.timeline().is_empty());
        },
    );
}

/// A race whose candidates never observe the label produces a winner with
/// no observations, which is not adopted: only the seed value reports a
/// score, and every perturbation moves off it.
#[test]
fn nd_targeting_winner_with_zero_observations_is_not_adopted() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            let n = match rint(ds, 0, 1000) {
                Ok(n) => n,
                Err(()) => return TestCaseResult::Overrun,
            };
            if n == 0 {
                ds.target_observation(1.0, "").unwrap();
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            ctx.targeting.record(
                &[ChoiceValue::Integer(BigInt::from(0))],
                &HashMap::from_iter([("".to_string(), 1.0)]),
            );
            ctx.optimise_targets_nd().await.unwrap();
            assert_eq!(ctx.targeting.nd_target("").unwrap().reference, 1.0);
        },
    );
}

/// The adoption seam directly: a fresh batch that observes the label
/// re-estimates the reference and moves the node view, and one that
/// observes nothing abandons the adoption.
#[test]
fn nd_target_adopt_requires_an_observing_fresh_batch() {
    with_engine(
        nd_settings(),
        None,
        |ds| {
            let n = match rint(ds, 0, 100) {
                Ok(n) => n,
                Err(()) => return TestCaseResult::Overrun,
            };
            if n > 0 {
                ds.target_observation(n as f64, "").unwrap();
            }
            TestCaseResult::Valid
        },
        async |ctx| {
            let observing = [ChoiceValue::Integer(BigInt::from(9))];
            assert!(ctx.nd_target_adopt("", &observing, 0.0).await.unwrap());
            assert_eq!(ctx.targeting.nd_target("").unwrap().reference, 9.0);
            let silent = [ChoiceValue::Integer(BigInt::from(0))];
            assert!(!ctx.nd_target_adopt("", &silent, 0.0).await.unwrap());
            assert_eq!(ctx.targeting.nd_target("").unwrap().reference, 9.0);
        },
    );
}

/// The generation loop routes a flipped run's target phase to the measured
/// race: a full ND run with a targeting body spends far more executions
/// than its generation budget, and still reports no failure.
#[test]
fn nd_runs_fire_the_measured_race_from_the_generation_loop() {
    let settings = nd_settings();
    let execs = Rc::new(Cell::new(0u64));
    let counter = execs.clone();
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        counter.set(counter.get() + 1);
        let result = match rint(&*ds, 0, 100) {
            Ok(n) => {
                ds.target_observation(n as f64, "").unwrap();
                TestCaseResult::Valid
            }
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let result = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert!(
        execs.get() > 300,
        "the race's measurement replays should dwarf the generation \
         budget, saw {} executions",
        execs.get()
    );
}

/// An interesting run during the reference batch leaves the label unset —
/// not dead — so the next firing can retry once the failure machinery has
/// the origin.
#[test]
fn nd_targeting_reference_interrupted_by_a_discovery_is_not_marked_dead() {
    with_engine(
        nd_settings(),
        None,
        |_ds| {
            TestCaseResult::Interesting(Failure {
                origin: "Panic: immediate".to_string(),
                reproduce_blob: None,
                caveat: None,
            })
        },
        async |ctx| {
            ctx.targeting
                .record(&[], &HashMap::from_iter([("s".to_string(), 1.0)]));
            ctx.optimise_targets_nd().await.unwrap();
            assert!(ctx.targeting.nd_target("s").is_none());
            assert!(ctx.origins.any_live());
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
            ctx.origins
                .entry("Panic: bug")
                .confirm(
                    0.3,
                    Some(interesting_at("Panic: bug", vec![bool_node(true)])),
                    Vec::new(),
                    (4, 6),
                )
                .unwrap();
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
                    incumbent_bounces: (0, 0),
                };
                let nodes = vec![bool_node(true)];
                let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
                assert!(matched, "the gauntlet accepts the always-failing candidate");
                assert_eq!(
                    probe.anchor, 0.3,
                    "an unadopted accept must not move the probe anchor"
                );
            }
            let (_, anchor) = ctx.origins.entry("Panic: bug").take_witness().unwrap();
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
            ctx.origins
                .entry("Panic: bug")
                .confirm(0.3, None, Vec::new(), (4, 6))
                .unwrap();
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
                    incumbent_bounces: (0, 0),
                };
                let nodes = vec![bool_node(true)];
                let (matched, _, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
                assert!(matched);
                probe.candidate_adopted().unwrap();
                raised_anchor = probe.anchor;
                assert!(raised_anchor > 0.3, "adoption raises the anchor");
                probe.candidate_adopted().unwrap();
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
    let blob = crate::native::blob::encode_nd_failure(&state).unwrap();
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

/// A body whose generation kinds drift after the discovering case: the
/// first-interesting check's replay realizes the drifted kind, flipping
/// the run before shrinking, so the failure reports with its caveat and
/// persists as a v2 entry.
#[test]
fn a_kind_drift_after_discovery_flips_at_the_first_check() {
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
    assert!(caveat.contains("nondeterministic failure"), "got: {caveat}");
    let db = DirectoryTestCaseDatabase::new(&path);
    let primary = db.fetch(b"k");
    assert!(!primary.is_empty());
    for entry in &primary {
        assert!(
            crate::native::blob::decode_nd_state(entry).is_some(),
            "a flipped run persists v2 entries"
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
        caveat.contains("confirmed")
            && caveat.contains("of 20 replays at confirmation and 1 of 1 at report time"),
        "the requeued origin must face the full anchor-seeding batch before \
         the final replay's 1, not confirm on the final replay alone (the \
         first check's overrunning replay seeds the batch as one miss): {caveat:?}"
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
    })
    .unwrap();
    let pre_shrink = serialize_choices(&vec![ChoiceValue::Boolean(false); 20]).unwrap();
    assert!(
        shortlex(&v2_bytes, &pre_shrink) != core::cmp::Ordering::Greater,
        "the seeded entry must sit under the drain's shortlex break"
    );
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
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
    let v1_bytes = serialize_choices(&[ChoiceValue::Boolean(false)]).unwrap();
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
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
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
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

/// The drained entry replays to the same realized values the passing
/// simplest case concluded with, so the cache reports a verdict flake and
/// the run flips mid-drain.
#[test]
fn drain_stops_at_mid_drain_nd_flip() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let drained_first = serialize_choices(&vec![ChoiceValue::Boolean(false); 4]).unwrap();
    let survivor = serialize_choices(&vec![ChoiceValue::Boolean(true); 4]).unwrap();
    assert!(shortlex(&drained_first, &survivor) == core::cmp::Ordering::Less);
    let secondary_key = crate::native::database::sub_key(b"k", b"secondary");
    {
        let db = DirectoryTestCaseDatabase::new(&path);
        db.save(&secondary_key, &drained_first);
        db.save(&secondary_key, &survivor);
    }
    let execs = AtomicUsize::new(0);
    let result = reuse_run(one_bug_settings(&path).derandomize(true), "k", |ds| {
        let n = execs.fetch_add(1, Ordering::SeqCst);
        let mut saw_true = false;
        for _ in 0..4 {
            match rbool(ds) {
                Ok(v) => saw_true |= v,
                Err(()) => return TestCaseResult::Overrun,
            }
        }
        if n == 0 {
            return TestCaseResult::Valid;
        }
        if saw_true && rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("a")
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
        stamped.fetch_add(u32::from(ds.should_capture()), Ordering::SeqCst);
        if rbool(ds).is_err() {
            return TestCaseResult::Overrun;
        }
        boom("a")
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        u64::from(stamped.load(Ordering::SeqCst)),
        FIRST_CHECK_REPLAYS + 1,
        "the check replays and the final replay are stamped (decision 49, \
         amended): a stamped failing check replay of the same choices serves \
         as the captured discovery"
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
    let blob = crate::native::blob::encode_nd_failure(&state).unwrap();
    let mut calls = 0u32;
    let mut stamped = 0u32;
    let result = reproduce_blob_sync(&quiet_settings(), &blob, |ds| {
        calls += 1;
        stamped += u32::from(ds.should_capture());
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

#[derive(Clone, Debug, PartialEq)]
enum DbOp {
    Save(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>, Vec<u8>),
    Move(Vec<u8>, Vec<u8>, Vec<u8>),
}

/// In-memory [`TestCaseDatabase`] recording every mutating call, so tests
/// can assert on operation ordering as well as final contents.
#[derive(Clone, Default)]
struct LoggingDatabase(std::sync::Arc<LoggingState>);

#[derive(Default)]
struct LoggingState {
    ops: std::sync::Mutex<Vec<DbOp>>,
    entries: std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<Vec<u8>>>>,
}

impl LoggingDatabase {
    fn ops(&self) -> Vec<DbOp> {
        self.0.ops.lock().unwrap().clone()
    }
}

impl TestCaseDatabase for LoggingDatabase {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        self.0
            .entries
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        self.0
            .ops
            .lock()
            .unwrap()
            .push(DbOp::Save(key.to_vec(), value.to_vec()));
        let mut entries = self.0.entries.lock().unwrap();
        let values = entries.entry(key.to_vec()).or_default();
        if !values.contains(&value.to_vec()) {
            values.push(value.to_vec());
        }
    }

    fn delete(&self, key: &[u8], value: &[u8]) {
        self.0
            .ops
            .lock()
            .unwrap()
            .push(DbOp::Delete(key.to_vec(), value.to_vec()));
        if let Some(values) = self.0.entries.lock().unwrap().get_mut(key) {
            values.retain(|v| v != value);
        }
    }

    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        self.0
            .ops
            .lock()
            .unwrap()
            .push(DbOp::Move(src.to_vec(), dst.to_vec(), value.to_vec()));
        let mut entries = self.0.entries.lock().unwrap();
        if let Some(values) = entries.get_mut(src) {
            values.retain(|v| v != value);
        }
        let values = entries.entry(dst.to_vec()).or_default();
        if !values.contains(&value.to_vec()) {
            values.push(value.to_vec());
        }
    }
}

#[test]
fn persister_saves_new_bytes_before_deleting_superseded() {
    let db = LoggingDatabase::default();
    let mut persister = Persister::new(Some(Box::new(db.clone())), Some("k"));
    persister.record("Panic: bug", &[int_node(5)]).unwrap();
    persister.record("Panic: bug", &[int_node(3)]).unwrap();

    let old = serialize_choices(&[ChoiceValue::Integer(BigInt::from(5))]).unwrap();
    let new = serialize_choices(&[ChoiceValue::Integer(BigInt::from(3))]).unwrap();
    let ops = db.ops();
    let saved_new = ops
        .iter()
        .position(|op| *op == DbOp::Save(b"k".to_vec(), new.clone()))
        .unwrap();
    let removed_old = ops
        .iter()
        .position(|op| {
            matches!(op, DbOp::Delete(key, v) | DbOp::Move(key, _, v)
                if key.as_slice() == b"k" && *v == old)
        })
        .unwrap();
    assert!(
        saved_new < removed_old,
        "the superseding save must land before the superseded bytes leave the primary key"
    );
}

#[test]
fn persister_deletes_superseded_same_run_saves() {
    let db = LoggingDatabase::default();
    let mut persister = Persister::new(Some(Box::new(db.clone())), Some("k"));
    persister.record("Panic: bug", &[int_node(5)]).unwrap();
    persister.record("Panic: bug", &[int_node(3)]).unwrap();

    assert_eq!(
        db.fetch(b"k"),
        vec![serialize_choices(&[ChoiceValue::Integer(BigInt::from(3))]).unwrap()]
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(&secondary).is_empty(),
        "a superseded same-run save is deleted, not demoted"
    );
}

#[test]
fn end_of_run_reconciliation_demotes_only_the_run_start_primary() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let run_start = serialize_choices(&[
        ChoiceValue::Integer(BigInt::from(1005)),
        ChoiceValue::Boolean(true),
    ])
    .unwrap();
    db.save(b"k", &run_start);

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, i64::MIN, i64::MAX) {
            Ok(n) if n >= 1000 => boom("big bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        db.fetch(b"k"),
        vec![serialize_choices(&[ChoiceValue::Integer(BigInt::from(1000))]).unwrap()]
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert_eq!(
        db.fetch(&secondary),
        vec![run_start],
        "only the run-start primary entry demotes; same-run saves delete"
    );
}

#[test]
fn nd_secondary_corpus_stays_bounded_across_runs() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    let body = |execs: &AtomicUsize, ds: &dyn DataSource| match rint(ds, i64::MIN, i64::MAX) {
        Ok(n) if n >= 1000 && execs.fetch_add(1, Ordering::SeqCst) % 2 == 0 => boom("nd"),
        Ok(_) => TestCaseResult::Valid,
        Err(()) => TestCaseResult::Overrun,
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
    assert_eq!(result.failures.len(), 1);
    assert_eq!(db.fetch(b"k").len(), 1);
    assert!(
        db.fetch(&secondary).is_empty(),
        "a run with no run-start primary entry deposits nothing in the secondary corpus"
    );

    for run in 1..=3usize {
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
        assert_eq!(result.failures.len(), 1);
        assert_eq!(db.fetch(b"k").len(), 1);
        assert!(
            db.fetch(&secondary).len() <= run,
            "at most one secondary deposit per run"
        );
    }
}

#[test]
fn secondary_corpus_cap_evicts_shortlex_largest() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    let entry = |n: i64| serialize_choices(&[ChoiceValue::Integer(BigInt::from(n))]).unwrap();
    for n in 0..55 {
        db.save(&secondary, &entry(n));
    }

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Generate])
            .test_cases(5)
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rbool(ds) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert!(result.failures.is_empty());
    let mut kept = db.fetch(&secondary);
    kept.sort_by(|a, b| shortlex(a, b));
    let expected: Vec<Vec<u8>> = (0..50).map(entry).collect();
    assert_eq!(
        kept, expected,
        "eviction removes exactly the shortlex-largest overflow"
    );
}

#[test]
fn reuse_stops_sampling_secondary_once_the_primary_reproduces() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap(),
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    let sampled = serialize_choices(&[ChoiceValue::Boolean(false)]).unwrap();
    db.save(&secondary, &sampled);

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rbool(ds) {
            Ok(true) => boom("primary bug"),
            Ok(false) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        db.fetch(&secondary),
        vec![sampled],
        "sampled secondary entries are not replayed once primary reproduced"
    );
}

#[test]
fn shrink_drain_stops_at_secondary_entries_above_the_incumbent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[
            ChoiceValue::Integer(BigInt::from(1005)),
            ChoiceValue::Boolean(true),
        ])
        .unwrap(),
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    let above = serialize_choices(&[
        ChoiceValue::Integer(BigInt::from(2000)),
        ChoiceValue::Integer(BigInt::from(2000)),
    ])
    .unwrap();
    db.save(&secondary, &above);

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, i64::MIN, i64::MAX) {
            Ok(n) if n >= 1000 => boom("big bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(
        db.fetch(&secondary).contains(&above),
        "the drain stops before entries shortlex above the incumbent"
    );
}

#[test]
fn a_flip_during_a_successful_final_replay_keeps_the_failure() {
    let bug = "bug";
    let executions = Rc::new(Cell::new(0u32));
    let execs = executions.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(v) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            execs.set(execs.get() + 1);
            if v == 3 && execs.get() > 1 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            let ntc =
                NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(3))], None, None);
            let (run, mismatch) = ctx.test_function(ntc).await.unwrap();
            assert_eq!(run.status, Status::Valid);
            assert!(mismatch.is_none());
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(3)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            assert!(!ctx.nd_handling());
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, false)
                .await
                .unwrap();
            assert!(
                ctx.nd_handling(),
                "the verdict flip inside the successful replay entered nd handling"
            );
            assert!(
                !ctx.origins.needs_confirmation(&origin),
                "the reproduced origin re-entered the queue and confirmed"
            );
            assert!(ctx.origins.incumbent(&origin).is_some());
            let report = ctx.build_report().unwrap();
            assert_eq!(report.failures.len(), 1);
            assert!(report.failures[0].reproduce_blob.is_some());
        },
    );
}

#[test]
fn a_dry_pooled_review_backtracks_over_history_at_the_final_replay() {
    let bug = "bug";
    with_engine(
        quiet_settings(),
        None,
        |ds| {
            let Ok(a) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if a != 77 {
                return TestCaseResult::Valid;
            }
            let Ok(b) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            if b == 77 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(77), int_node(77)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at(&origin, vec![int_node(3)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.nd_flip();
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, true)
                .await
                .unwrap();
            assert!(
                !ctx.origins.needs_confirmation(&origin),
                "the dry review backtracked over history and confirmed"
            );
            assert_eq!(
                ctx.origins.incumbent(&origin).unwrap(),
                &[int_node(77), int_node(77)],
                "the restored incumbent displaced the fluke"
            );
        },
    );
}

/// The dry pooled review's backtrack finds nothing either: the history
/// timeline (v = 999_999, distinguishable from the incumbent and from any
/// plausible fresh draw) replays without failing, so the backtrack exhausts
/// and the origin is rejected into the caveat-only report with the batch's
/// and the backtrack's evidence combined.
#[test]
fn an_exhausted_backtrack_after_a_dry_pooled_review_rejects_the_origin() {
    let saw_history_value = Rc::new(Cell::new(false));
    let saw = saw_history_value.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| match rint(ds, 0, 1_000_000) {
            Ok(v) => {
                if v == 999_999 {
                    saw.set(true);
                }
                TestCaseResult::Valid
            }
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            let wide_node = |v: i128| {
                ChoiceNode::integer(
                    crate::native::core::choices::IntegerChoice {
                        min_value: BigInt::from(0),
                        max_value: BigInt::from(1_000_000),
                        shrink_towards: BigInt::from(0),
                    },
                    BigInt::from(v),
                    false,
                )
            };
            let origin = "Panic: bug";
            ctx.record_run(
                &interesting_at(origin, vec![wide_node(999_999)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.record_run(
                &interesting_at(origin, vec![wide_node(3)]),
                Duration::ZERO,
                false,
            )
            .unwrap();
            ctx.nd_flip();
            let output = ctx.settings.output.clone();
            ctx.final_replay(Verbosity::Quiet, &output, None, true)
                .await
                .unwrap();
            assert!(
                ctx.origins.incumbent(origin).is_none(),
                "an origin nothing reproduces is rejected into the caveat-only report"
            );
            let caveat = ctx.origins.caveat(origin).unwrap();
            assert!(
                caveat.starts_with("unconfirmed failure: failed 0 of"),
                "unexpected caveat: {caveat}"
            );
        },
    );
    assert!(
        saw_history_value.get(),
        "the dry pooled review must backtrack over the recorded history"
    );
}

#[test]
fn a_seeded_bar_quota_with_no_reproducing_replay_rejects_at_the_cap() {
    with_engine(
        quiet_settings(),
        None,
        |ds| match rint(ds, 0, 100) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            let origin = "Panic: bug";
            let mut seed = nd::Evidence::default();
            for _ in 0..nd::CONFIRM_MIN_FAILS {
                seed.record(true);
            }
            ctx.origins.entry(origin).seed_evidence(seed);
            ctx.nd_flip();
            let batch = ctx
                .nd_evidence_batch(origin, &[ChoiceValue::Integer(BigInt::from(3))], None)
                .await
                .unwrap();
            assert!(
                !batch.bar_accepted,
                "an accept needs an in-batch reproduction"
            );
            assert!(batch.witness.is_none());
            assert_eq!(batch.evidence.runs(), nd::CONFIRM_CAP);
        },
    );
}

#[test]
fn a_seeded_bar_quota_accepts_once_a_replay_reproduces() {
    let bug = "bug";
    let executions = Rc::new(Cell::new(0u32));
    let execs = executions.clone();
    with_engine(
        quiet_settings(),
        None,
        move |ds| {
            let Ok(_) = rint(ds, 0, 100) else {
                return TestCaseResult::Overrun;
            };
            execs.set(execs.get() + 1);
            if execs.get() >= 3 {
                boom(bug)
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            let origin = format!("Panic: {bug}");
            let mut seed = nd::Evidence::default();
            for _ in 0..nd::CONFIRM_MIN_FAILS {
                seed.record(true);
            }
            ctx.origins.entry(&origin).seed_evidence(seed);
            ctx.nd_flip();
            let batch = ctx
                .nd_evidence_batch(&origin, &[ChoiceValue::Integer(BigInt::from(3))], None)
                .await
                .unwrap();
            assert!(batch.bar_accepted);
            assert!(batch.witness.is_some());
        },
    );
}

#[test]
fn error_strictness_aborts_on_a_shrink_probe_verdict_flip() {
    let executions = Rc::new(Cell::new(0u32));
    let execs = executions.clone();
    with_engine(
        quiet_settings().nondeterminism_strictness(NondeterminismStrictness::Error),
        None,
        move |ds| {
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            execs.set(execs.get() + 1);
            if execs.get() > 1 {
                boom("flake")
            } else {
                TestCaseResult::Valid
            }
        },
        async |ctx| {
            ctx.collect_statistics = true;
            let first = ctx
                .cached_test_function(&[ChoiceValue::Boolean(true)], None, 0)
                .await
                .unwrap();
            assert_eq!(first.status, Status::Valid);
            ctx.collect_statistics = false;
            let err = match ctx
                .cached_test_function(&[ChoiceValue::Boolean(true)], None, 0)
                .await
            {
                Err(err) => err,
                Ok(_) => panic!("expected the probe verdict flip to abort"),
            };
            assert!(matches!(err, crate::backend::RunError::Flaky(_)));
        },
    );
}

#[test]
fn error_strictness_aborts_on_kind_drift_in_the_pre_shrink_drain() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[
            ChoiceValue::Integer(BigInt::from(9)),
            ChoiceValue::Integer(BigInt::from(3)),
        ])
        .unwrap(),
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    db.save(
        &secondary,
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]).unwrap(),
    );
    let executions = Rc::new(Cell::new(0u32));
    let execs = executions.clone();
    let mut run_case = move |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let range_max = if execs.get() == 2 { 50 } else { 100 };
        let result = match rint(&*ds, 0, range_max) {
            Ok(9) => boom("B"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .database(Some(path))
        .phases([Phase::Reuse, Phase::Shrink])
        .verbosity(Verbosity::Quiet)
        .nondeterminism_strictness(NondeterminismStrictness::Error);
    let err = run_main_sync(
        &settings,
        Some("k"),
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap_err();
    let crate::backend::RunError::NonDeterministic(msg) = err else {
        panic!("expected the drain's kind drift to abort, got {err:?}");
    };
    assert!(msg.contains("choice kind changed"));
}

#[test]
fn superseding_a_reused_run_start_entry_demotes_it_to_secondary() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let run_start = serialize_choices(&[ChoiceValue::Integer(BigInt::from(90))]).unwrap();
    let misaligned = serialize_choices(&[
        ChoiceValue::Integer(BigInt::from(95)),
        ChoiceValue::Integer(BigInt::from(3)),
    ])
    .unwrap();
    db.save(b"k", &run_start);
    db.save(b"k", &misaligned);
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        let result = match rint(&*ds, 0, 100) {
            Ok(v) if v >= 50 => boom("bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .database(Some(path))
        .phases([Phase::Reuse, Phase::Shrink])
        .verbosity(Verbosity::Quiet);
    let result = run_main_sync(
        &settings,
        Some("k"),
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let shrunk = serialize_choices(&[ChoiceValue::Integer(BigInt::from(50))]).unwrap();
    assert_eq!(db.fetch(b"k"), vec![shrunk]);
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(&secondary).contains(&run_start),
        "the superseded run-start entry demotes instead of deleting"
    );
}

#[test]
fn reconciliation_demotes_a_reproduced_run_start_entry_after_a_flip() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let run_start = serialize_choices(&[ChoiceValue::Integer(BigInt::from(90))]).unwrap();
    db.save(b"k", &run_start);
    let junk_v2 = crate::native::blob::encode_nd_state(&crate::native::blob::NdReproState {
        timelines: vec![vec![ChoiceValue::Integer(BigInt::from(7))]],
        entropy: 0,
        extension: 4,
    })
    .unwrap();
    db.save(b"k", &junk_v2);
    let executions = Rc::new(Cell::new(0u32));
    let execs = executions.clone();
    let mut run_case = move |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = match rint(&*ds, 0, 100) {
            Ok(90) if execs.get() == 1 => boom("bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let settings = Settings::new()
        .database(Some(path))
        .phases([Phase::Reuse, Phase::Shrink])
        .verbosity(Verbosity::Quiet);
    let result = run_main_sync(
        &settings,
        Some("k"),
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let primary = db.fetch(b"k");
    assert_eq!(primary.len(), 1);
    assert!(crate::native::blob::decode_nd_state(&primary[0]).is_some());
    assert!(
        db.fetch(&crate::native::database::sub_key(b"k", b"secondary"))
            .contains(&run_start),
        "the reproduced run-start entry demotes instead of deleting"
    );
}

#[test]
fn superseding_one_origin_keeps_a_byte_identical_entry_shared_with_another() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let settings = Settings::new()
        .database(Some(path))
        .verbosity(Verbosity::Quiet);
    let exchange = CaseExchange::new();
    let fut = async {
        let mut ctx = Engine::new(&settings, Some("k"), &exchange).unwrap();
        ctx.nd_flip();
        ctx.record_nd_incumbent("Panic: a", &[int_node(90)])
            .unwrap();
        ctx.record_nd_incumbent("Panic: b", &[int_node(90)])
            .unwrap();
        assert_eq!(db.fetch(b"k").len(), 1);
        ctx.record_nd_incumbent("Panic: a", &[int_node(50)])
            .unwrap();
        assert_eq!(
            db.fetch(b"k").len(),
            2,
            "the shared entry survives the other origin's supersession"
        );
    };
    crate::exchange::drive(&exchange, fut, |ds| {
        ds.mark_complete(&TestCaseResult::Valid);
    });
}

#[test]
fn the_caveat_only_fallback_honors_report_multiple_failures() {
    with_engine(
        quiet_settings().report_multiple_failures(false),
        None,
        |_ds| TestCaseResult::Valid,
        async |ctx| {
            ctx.nd_flip();
            ctx.origins.entry("Panic: a");
            ctx.origins.entry("Panic: b");
            let report = ctx.build_report().unwrap();
            assert_eq!(report.failures.len(), 1);
            assert_eq!(report.failures[0].origin, "Panic: a");
        },
    );
}

#[test]
fn a_fast_sweep_miss_cannot_reject_a_conclusively_accepted_timeline() {
    with_engine(
        quiet_settings(),
        None,
        |ds| match rint(ds, 0, 100) {
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
        async |ctx| {
            ctx.nd_flip();
            let output = ctx.settings.output.clone();
            let mut probe = EngineShrinkProbe {
                engine: &mut *ctx,
                target_origin: "Panic: bug".to_string(),
                verbosity: Verbosity::Quiet,
                output,
                gauntlet: true,
                ledger: HashMap::default(),
                anchor: 0.5,
                sweep: SweepMode::Fast,
                raised: crate::native::HashSet::default(),
                pending_accept: None,
                incumbent_bounces: (0, 0),
            };
            let key = serialize_choices(&[ChoiceValue::Integer(BigInt::from(9))]).unwrap();
            let mut evidence = nd::Evidence::default();
            for _ in 0..nd::ANCHOR_SEED_RUNS {
                evidence.record(true);
            }
            probe.ledger.insert(
                key,
                CandidateLedger {
                    evidence,
                    min_fails: nd::GAUNTLET_MIN_FAILS,
                    verdict: Some(true),
                    bounces: 0,
                },
            );
            let nodes = vec![int_node(9)];
            let (matched, actual, _) = probe.run(ShrinkRun::Full(&nodes)).await.unwrap();
            assert!(
                matched,
                "a conclusive ledger accept survives one dry replay"
            );
            assert!(probe.pending_accept.is_some());
            assert_eq!(actual, nodes);
        },
    );
}

/// A one-case budget skips the deterministic simplest-example probe (which
/// would otherwise consume the whole budget and pin every run to the
/// all-simplest case), so the single case is randomly generated.
#[test]
fn a_one_case_budget_generates_a_random_case() {
    let mut executions = 0;
    let mut drawn: Vec<u64> = Vec::new();
    let result = run_main_sync(
        &Settings::new()
            .test_cases(1)
            .database(None)
            .seed(Some(0))
            .verbosity(Verbosity::Quiet),
        None,
        |ds| {
            executions += 1;
            for _ in 0..4 {
                if let Ok(n) = ru64(&*ds) {
                    drawn.push(n);
                }
            }
            ds.mark_complete(&TestCaseResult::Valid);
        },
        Duration::from_secs(30),
        Duration::ZERO,
    );
    assert!(result.unwrap().failures.is_empty());
    assert_eq!(executions, 1, "one valid case is the whole budget");
    assert!(
        drawn.iter().any(|&n| n != 0),
        "the one case must be random, not the all-simplest probe: {drawn:?}"
    );
}

#[test]
fn derandomize_is_keyed_by_test_identity() {
    let settings = Settings::new()
        .test_cases(5)
        .database(None)
        .derandomize(true)
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
            run_main_sync(
                &settings,
                key,
                &mut run_case,
                Duration::from_secs(30),
                Duration::ZERO,
            )
            .unwrap();
        }
        drawn
    };
    let a1 = draw_with_key(Some("test-a"));
    let a2 = draw_with_key(Some("test-a"));
    let b = draw_with_key(Some("test-b"));
    assert_eq!(a1, a2, "the same key must replay the same draws");
    assert_ne!(a1, b, "different keys must not share a derandomized stream");
}

/// A run under nondeterministic handling trusts no repeat, so the duplicate
/// stop is off and an all-invalid two-value space grinds to its invalid
/// budget instead of stopping after ten duplicates.
#[test]
fn duplicate_stop_is_disabled_for_a_nondeterministic_run() {
    let execs = Cell::new(0u64);
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = match rbool(&*ds) {
            Ok(_) => TestCaseResult::Invalid,
            Err(()) => TestCaseResult::Overrun,
        };
        ds.mark_complete(&result);
    };
    let mut settings = Settings::new()
        .database(None)
        .test_cases(10_000)
        .verbosity(Verbosity::Quiet)
        .suppress_health_check([HealthCheck::FilterTooMuch]);
    settings.nd_force = true;
    let result = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    assert!(result.unwrap().failures.is_empty());
    assert!(
        execs.get() > 100,
        "no duplicate stop under nondeterministic handling: {}",
        execs.get()
    );
}

#[test]
fn reconciliation_deletes_a_same_run_leftover_absent_from_the_final_failures() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let settings = Settings::new().database(Some(path));
    let exchange = CaseExchange::new();
    let mut ctx = Engine::new(&settings, Some("k"), &exchange).unwrap();
    ctx.persister.record("Panic: bug", &[int_node(90)]).unwrap();
    ctx.origins.entry("Panic: bug").replace(vec![int_node(50)]);
    ctx.reconcile_database().unwrap();

    assert_eq!(
        db.fetch(b"k"),
        vec![serialize_choices(&[ChoiceValue::Integer(BigInt::from(50))]).unwrap()]
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert!(
        db.fetch(&secondary).is_empty(),
        "a same-run leftover is deleted, not demoted"
    );
}

/// Regression for issue #78: a test that rejects its input before making
/// any draw can never produce a valid case, so the run must report
/// Unsatisfiable after one call instead of passing.
#[test]
fn run_main_reports_unsatisfiable_for_trivial_always_invalid_test() {
    let mut calls = 0usize;
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        calls += 1;
        ds.mark_complete(&TestCaseResult::Invalid);
    };
    let settings = Settings::new().test_cases(100).database(None);
    let exploration = run_main_sync(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    match exploration {
        Err(crate::backend::RunError::Unsatisfiable(msg)) => {
            assert!(msg.contains("Unsatisfiable"), "unexpected message: {msg}");
            assert!(msg.contains("assume()"), "unexpected message: {msg}");
        }
        other => panic!("expected RunError::Unsatisfiable, got {other:?}"),
    }
    assert_eq!(calls, 1, "a trivial invalid test must stop after one call");
}

#[test]
fn shrink_phase_drain_stops_at_entries_above_the_largest_surviving_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let run_start = serialize_choices(&[
        ChoiceValue::Integer(BigInt::from(90)),
        ChoiceValue::Boolean(true),
    ])
    .unwrap();
    db.save(b"k", &run_start);
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    let small = serialize_choices(&[ChoiceValue::Integer(BigInt::from(10))]).unwrap();
    let large = serialize_choices(&[
        ChoiceValue::Integer(BigInt::from(80)),
        ChoiceValue::Integer(BigInt::from(4)),
    ])
    .unwrap();
    db.save(&secondary, &small);
    db.save(&secondary, &large);

    let result = reuse_run(
        Settings::new()
            .database(Some(path.clone()))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Quiet),
        "k",
        |ds| match rint(ds, 0, 100) {
            Ok(v) if v >= 50 => boom("bug"),
            Ok(_) => TestCaseResult::Valid,
            Err(()) => TestCaseResult::Overrun,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let shrunk = serialize_choices(&[ChoiceValue::Integer(BigInt::from(50))]).unwrap();
    assert_eq!(db.fetch(b"k"), vec![shrunk]);
    let kept = db.fetch(&secondary);
    assert!(
        !kept.contains(&small),
        "an entry at or below the surviving failure is replayed and drained"
    );
    assert!(
        kept.contains(&large),
        "an entry shortlex above the surviving failure survives the drain"
    );
    assert!(kept.contains(&run_start));
}

/// Antithesis is deterministic and does its own reproduction, so the
/// `warn` notice a flip would print does not apply there.
#[test]
fn the_warn_notice_is_suppressed_in_antithesis() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let seen_bug = AtomicBool::new(false);
    let result = reuse_run(
        Settings::for_env(false, true)
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
    let text = lines.lock().unwrap().join("\n");
    assert!(
        !text.contains("Nondeterministic test behavior detected"),
        "Antithesis is deterministic, so the notice does not apply:\n{text}"
    );
}

#[test]
fn a_reuse_replay_that_realizes_any_stored_timeline_is_aligned_and_skips_the_shrink() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let state = crate::native::blob::NdReproState {
        timelines: vec![branch_s(), branch_t()],
        entropy: 0,
        extension: 4,
    };
    db.save(b"k", &crate::native::blob::encode_nd_state(&state).unwrap());
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let executions = AtomicUsize::new(0);
    let result = reuse_run(
        Settings::new()
            .database(Some(path))
            .phases([Phase::Reuse, Phase::Shrink])
            .verbosity(Verbosity::Debug)
            .output(Output::callback(move |line| {
                sink.lock().unwrap().push(line.to_string());
            })),
        "k",
        |ds| {
            let n = executions.fetch_add(1, Ordering::SeqCst);
            if rbool(ds).is_err() {
                return TestCaseResult::Overrun;
            }
            if n % 2 == 0 {
                match (rint(ds, 0, 100), rint(ds, 0, 100)) {
                    (Ok(7), Ok(9)) => boom("branch"),
                    (Ok(_), Ok(_)) => TestCaseResult::Valid,
                    _ => TestCaseResult::Overrun,
                }
            } else {
                match (rbool(ds), rbool(ds), rint(ds, 0, 100)) {
                    (Ok(true), Ok(true), Ok(42)) => boom("branch"),
                    (Ok(_), Ok(_), Ok(_)) => TestCaseResult::Valid,
                    _ => TestCaseResult::Overrun,
                }
            }
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l == "Skipping shrink: reused aligned database replay"),
        "the first reuse replay took the second stored branch; realizing a stored \
         timeline other than the incumbent is still aligned"
    );
    assert!(
        executions.load(Ordering::SeqCst) < 60,
        "no confirmation batch and no shrink: {} executions",
        executions.load(Ordering::SeqCst)
    );
}
