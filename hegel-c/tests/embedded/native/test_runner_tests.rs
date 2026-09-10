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

use crate::backend::{DataSource, DataSourceError, Failure, TestCaseResult};
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
    })
}

/// Create (and immediately drop) a one-rule state machine whose declared
/// concurrency bound is above 1. On the first such case of a run the
/// engine rejects the creation with an assume violation (`Err(Invalid)`
/// here): the case is discarded and the run flips into nondeterministic
/// mode, and later cases create the machine successfully.
fn concurrent_machine(ds: &dyn DataSource) -> Result<(), TestCaseResult> {
    match ds.new_state_machine(
        vec!["rule".to_string()],
        vec![0],
        alloc::vec::Vec::new(),
        alloc::vec::Vec::new(),
        2,
        2,
        50,
    ) {
        Ok(_) => Ok(()),
        Err(DataSourceError::Assume) => Err(TestCaseResult::Invalid),
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
/// concluding differently abort as a flaky test.
#[test]
fn a_reexecuted_fingerprint_with_a_different_outcome_is_flaky() {
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

            let (run, mismatch) = ctx
                .test_function(NativeTestCase::for_choices(&choices, Some(&nodes), None))
                .await
                .unwrap();
            assert_eq!(run.status, Status::Interesting);
            match mismatch {
                Some(crate::backend::RunError::Flaky(msg)) => {
                    assert!(msg.contains("Flaky test detected"), "{msg}");
                }
                other => panic!("expected the flaky abort, got {other:?}"),
            }
        },
    );
}

/// A generation-window repeat executes (the digest tier keeps no serving
/// entry), and a verdict flip on it aborts the replay chokepoint too.
#[test]
fn a_verdict_flip_on_a_generation_window_repeat_aborts_as_flaky() {
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
        async |ctx, count| {
            ctx.collect_statistics = true;
            let choices = [ChoiceValue::Boolean(true)];
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(count.get(), 1);

            let repeat = ctx.cached_test_function(&choices, None, 0).await;
            assert_eq!(count.get(), 2, "generation-window repeats execute");
            match repeat {
                Err(crate::backend::RunError::Flaky(msg)) => {
                    assert!(msg.contains("Flaky test detected"), "{msg}");
                }
                Err(other) => panic!("expected the flaky abort, got {other:?}"),
                Ok(_) => panic!("expected the flaky abort, got a run"),
            }
        },
    );
}

/// The flip drops everything the cache knew and stops serving: post-flip,
/// identical timelines need not conclude identically.
#[test]
fn the_execution_cache_is_flushed_and_serving_stops_at_the_flip() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let execs = AtomicUsize::new(0);
    with_counting_ctx(
        move |ds| {
            if execs.fetch_add(1, Ordering::SeqCst) >= 1 {
                if let Err(result) = concurrent_machine(ds) {
                    return result;
                }
            }
            match rbool(ds) {
                Ok(_) => TestCaseResult::Valid,
                Err(()) => TestCaseResult::Overrun,
            }
        },
        async |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(count.get(), 1, "served before the flip");

            let flipping = [ChoiceValue::Boolean(false)];
            ctx.cached_test_function(&flipping, None, 0).await.unwrap();
            assert_eq!(count.get(), 2);
            assert!(ctx.nondeterministic);
            assert!(
                ctx.exec_cache
                    .serve(&serialize_choices(&choices).unwrap())
                    .is_none(),
                "the flip flushes the cache"
            );

            ctx.cached_test_function(&choices, None, 0).await.unwrap();
            assert_eq!(count.get(), 3, "nothing is served after the flip");
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
fn filter_too_much_fires_via_the_duplicate_stop_on_an_exhausted_space() {
    let (result, execs) = tiny_invalid_run(
        Settings::new().database(None).test_cases(10_000),
        TestCaseResult::Invalid,
    );
    match result {
        Err(crate::backend::RunError::HealthCheck(msg)) => {
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

/// A nondeterministic run trusts no repeat, so the duplicate stop is
/// suspended at the flip and the run grinds to its invalid budget instead.
#[test]
fn duplicate_stop_is_disabled_for_a_nondeterministic_run() {
    let execs = Cell::new(0u64);
    let mut run_case = |ds: Box<dyn DataSource + Send + Sync>| {
        execs.set(execs.get() + 1);
        let result = if let Err(result) = concurrent_machine(&*ds) {
            result
        } else {
            match rbool(&*ds) {
                Ok(_) => TestCaseResult::Invalid,
                Err(()) => TestCaseResult::Overrun,
            }
        };
        ds.mark_complete(&result);
    };
    let result = run_main_sync(
        &Settings::new()
            .database(None)
            .test_cases(10_000)
            .verbosity(Verbosity::Quiet)
            .suppress_health_check([HealthCheck::FilterTooMuch]),
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    assert!(result.unwrap().failures.is_empty());
    assert!(
        execs.get() > 100,
        "no duplicate stop after the flip: {}",
        execs.get()
    );
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
    assert!(!result.nondeterministic);
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

/// Cost guard (experiment 010): on a passing body the run's execution count
/// is a pure function of the seed, so replacing the tree's recording with
/// the flat cache must not change it at all.
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

/// Cost guard (experiment 010): the flat cache must keep serving
/// shrink-phase repeats, so a deterministic shrink-heavy run stays within
/// ~1.1x the tree-era execution count (010's 85% serve rate).
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
    let settings = Settings::new()
        .test_cases(100)
        .database(None)
        .suppress_health_check([]);
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
fn nondeterministic_run_stops_at_first_bug_with_no_blob_and_no_verify() {
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
    assert!(result.failures[0].reproduce_blob.is_none());
    assert!(result.nondeterministic);
    assert!(
        seen_bug.load(Ordering::SeqCst),
        "the bug must have been discovered by generation"
    );
}

#[test]
fn nondeterministic_run_reports_a_bug_that_would_otherwise_be_flaky() {
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
}

#[test]
fn nondeterministic_run_discards_stale_entries_and_persists_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let seeded = serialize_choices(&[ChoiceValue::Boolean(true)]).unwrap();
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
    assert!(result.failures[0].reproduce_blob.is_none());
    assert!(
        db.fetch(b"k").is_empty(),
        "the stale replay is discarded like a failed assumption and deleted, \
         and the fresh failure is not persisted"
    );
    let secondary = crate::native::database::sub_key(b"k", b"secondary");
    assert!(db.fetch(&secondary).is_empty());
}

#[test]
fn a_concurrent_machine_prints_the_nondeterminism_notice_once() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let result = reuse_run(
        Settings::new()
            .database(None)
            .test_cases(5)
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
        .filter(|l| l.contains("Concurrent state machine detected"))
        .count();
    assert_eq!(notices, 1, "the notice is printed exactly once per run");
}

#[test]
fn a_concurrent_machine_prints_no_nondeterminism_notice_in_antithesis() {
    use std::sync::{Arc, Mutex};
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let result = reuse_run(
        Settings::for_env(false, true)
            .test_cases(5)
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
    let text = lines.lock().unwrap().join("\n");
    assert!(
        !text.contains("Concurrent state machine detected"),
        "Antithesis is deterministic, so the notice does not apply:\n{text}"
    );
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
fn superseding_one_origin_keeps_a_byte_identical_entry_shared_with_another() {
    let db = LoggingDatabase::default();
    let mut persister = Persister::new(Some(Box::new(db.clone())), Some("k"));
    persister.record("Panic: a", &[int_node(90)]).unwrap();
    persister.record("Panic: b", &[int_node(90)]).unwrap();
    assert_eq!(db.fetch(b"k").len(), 1);

    persister.record("Panic: a", &[int_node(50)]).unwrap();

    let shared = serialize_choices(&[ChoiceValue::Integer(BigInt::from(90))]).unwrap();
    let smaller = serialize_choices(&[ChoiceValue::Integer(BigInt::from(50))]).unwrap();
    let primary = db.fetch(b"k");
    assert!(
        primary.contains(&shared),
        "the shared entry survives the other origin's supersession"
    );
    assert!(primary.contains(&smaller));
    assert_eq!(primary.len(), 2);
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

#[test]
fn reconciliation_deletes_a_same_run_leftover_absent_from_the_final_failures() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let settings = Settings::new().database(Some(path));
    let exchange = CaseExchange::new();
    let mut ctx = Engine::new(&settings, Some("k"), &exchange).unwrap();
    ctx.persister.record("Panic: bug", &[int_node(90)]).unwrap();
    ctx.interesting
        .insert("Panic: bug".to_string(), vec![int_node(50)]);
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
