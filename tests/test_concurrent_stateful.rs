mod common;

use common::utils::{assert_matches_regex, capture_hegel_output};
use hegel::generators as gs;
use hegel::stateful::{ConcurrentPool, concurrent_pool, run_concurrent};
use hegel::{HealthCheck, Hegel, Settings, TestCase, Verbosity};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

struct Counter {
    value: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl Counter {
    #[rule]
    fn increment(&self, _: TestCase) {
        self.value.fetch_add(1, Ordering::SeqCst);
    }

    #[rule]
    fn decrement(&self, tc: TestCase) {
        tc.assume(self.value.load(Ordering::SeqCst) > 0);
        self.value.fetch_sub(1, Ordering::SeqCst);
    }

    #[invariant]
    fn non_negative(&self, _: TestCase) {
        assert!(self.value.load(Ordering::SeqCst) >= 0);
    }
}

#[hegel::test(nondeterministic = true)]
fn test_concurrent_counter_passes(tc: TestCase) {
    let m = Counter {
        value: AtomicI64::new(0),
    };
    run_concurrent(m, tc, 1);
}

struct Grouped {
    log: Mutex<Vec<&'static str>>,
}

#[hegel::concurrent_state_machine]
impl Grouped {
    #[rule(group = "letters")]
    fn alpha(&self, _: TestCase) {
        self.log.lock().unwrap().push("alpha");
    }

    #[rule(group = "letters")]
    fn beta(&self, tc: TestCase) {
        let n: i8 = tc.draw(gs::integers());
        tc.assume(n != 0);
        self.log.lock().unwrap().push("beta");
    }

    #[rule(group = "numbers")]
    fn one(&self, _: TestCase) {
        self.log.lock().unwrap().push("one");
    }

    #[rule]
    fn anonymous(&self, _: TestCase) {
        self.log.lock().unwrap().push("anonymous");
    }

    #[invariant]
    fn log_is_bounded(&self, _: TestCase) {
        assert!(self.log.lock().unwrap().len() <= 100_000);
    }
}

#[hegel::test(nondeterministic = true)]
fn test_grouped_machine_passes(tc: TestCase) {
    let m = Grouped {
        log: Mutex::new(Vec::new()),
    };
    run_concurrent(m, tc, 3);
}

struct Boom;

#[hegel::concurrent_state_machine]
impl Boom {
    #[rule]
    fn boom(&self, tc: TestCase) {
        let x: i64 = tc.draw(gs::integers());
        panic!("concurrent boom {x}");
    }
}

#[test]
fn a_worker_panic_is_reported_with_its_real_origin_and_buffered_output() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(Boom, tc, 1))
            .settings(Settings::new().nondeterministic(true).database(None))
            .run();
    });
    let payload = result.expect_err("the failing machine must fail the run");
    assert_matches_regex(&panic_message(&payload), "concurrent boom");
    let text = lines.join("\n");
    assert!(
        text.contains("---------------- Round 1: group \"<anonymous>\" ----------------"),
        "the join points must note the round's concurrency group:\n{text}"
    );
    assert!(
        text.contains("[worker 0] Rule: boom"),
        "buffered rule notes must be tagged with the worker index:\n{text}"
    );
    assert!(
        text.contains("[worker 0]   let draw_1"),
        "buffered draw lines must be tagged with the worker index:\n{text}"
    );
    assert!(
        text.contains("test_concurrent_stateful.rs"),
        "the diagnostic must carry the worker's real panic location:\n{text}"
    );
    assert!(
        !text.contains("<unknown>"),
        "the ferried panic info must replace the cross-thread fallback:\n{text}"
    );
    assert!(
        !text.contains("To reproduce this failure"),
        "a nondeterministic failure must not print a reproducer line:\n{text}"
    );
}

#[test]
fn quiet_nondeterministic_runs_stay_quiet_but_still_fail() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(Boom, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let payload = result.expect_err("the failing machine must fail the run even when quiet");
    assert_matches_regex(&panic_message(&payload), "concurrent boom");
    assert!(lines.is_empty(), "quiet runs print nothing: {lines:?}");
}

#[test]
fn run_concurrent_requires_the_nondeterministic_declaration() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            let m = Counter {
                value: AtomicI64::new(0),
            };
            run_concurrent(m, tc, 1);
        })
        .settings(Settings::new().database(None))
        .run();
    });
    let payload = result.expect_err("run_concurrent must reject an undeclared run");
    assert_matches_regex(
        &panic_message(&payload),
        "requires the run to be declared nondeterministic",
    );
}

#[test]
fn reproduce_failure_is_rejected_on_a_nondeterministic_run() {
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|_tc| {})
            .settings(Settings::new().nondeterministic(true).database(None))
            .reproduce_failure("AAEC")
            .run();
    });
    let payload = result.expect_err("the blob must be rejected before any test case runs");
    assert_matches_regex(
        &panic_message(&payload),
        "reproduce_failure.* is not supported on a test declared\\s+nondeterministic",
    );
}

struct Exhaust;

#[hegel::concurrent_state_machine]
impl Exhaust {
    #[rule]
    fn exhaust(&self, tc: TestCase) {
        loop {
            let _: i64 = tc.draw_silent(gs::integers());
        }
    }
}

#[test]
fn an_overrunning_worker_classifies_the_case_as_an_overrun() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(Exhaust, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .suppress_health_check([HealthCheck::LargeInitialTestCase])
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let payload = result.expect_err("every case overruns, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "TestCasesTooLarge");
}

struct DeepSpans;

#[hegel::concurrent_state_machine]
impl DeepSpans {
    #[rule]
    fn nest(&self, tc: TestCase) {
        for _ in 0..101 {
            tc.start_span(1);
        }
        let _: bool = tc.draw_silent(gs::booleans());
    }
}

#[test]
fn an_engine_invalid_conclusion_classifies_the_case_as_invalid() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(DeepSpans, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let payload = result.expect_err("every case is invalid, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "FilterTooMuch");
}

struct NestAndBoom;

#[hegel::concurrent_state_machine]
impl NestAndBoom {
    #[rule]
    fn nest_and_boom(&self, tc: TestCase) {
        for _ in 0..101 {
            tc.start_span(1);
        }
        panic!("this panic must lose to the engine's invalid conclusion");
    }
}

#[test]
fn a_panic_that_loses_to_an_engine_side_conclusion_is_discarded() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(NestAndBoom, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .suppress_health_check([HealthCheck::FilterTooMuch]),
            )
            .run();
    });
    assert!(
        result.is_ok(),
        "the engine concluded every case invalid, so the run must pass"
    );
    let text = lines.join("\n");
    assert!(
        !text.contains("must lose"),
        "the losing panic's stash must be discarded, not printed:\n{text}"
    );
}

struct UsageError;

#[hegel::concurrent_state_machine]
impl UsageError {
    #[rule]
    fn bad_target(&self, tc: TestCase) {
        tc.target(f64::NAN);
    }
}

#[test]
fn a_workers_usage_error_aborts_the_run_verbatim() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(UsageError, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let payload = result.expect_err("an invalid-argument control payload aborts the run");
    assert_matches_regex(&panic_message(&payload), "finite score");
}

struct LateReject {
    checks: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl LateReject {
    #[rule]
    fn noop(&self, _: TestCase) {}

    #[invariant]
    fn only_the_initial_check_passes(&self, tc: TestCase) {
        tc.assume(self.checks.fetch_add(1, Ordering::SeqCst) == 0);
    }
}

#[test]
fn an_invariant_assumption_failure_at_a_join_point_invalidates_the_case() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            let m = LateReject {
                checks: AtomicI64::new(0),
            };
            run_concurrent(m, tc, 2);
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    let payload = result.expect_err("every case is invalid, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "FilterTooMuch");
}

#[hegel::test]
fn test_concurrent_pool_add_reuse_and_consume(tc: TestCase) {
    let pool: ConcurrentPool<i64> = concurrent_pool(&tc);
    assert!(pool.is_empty());
    pool.add(&tc, 10);
    pool.add(&tc, 20);
    assert_eq!(pool.len(), 2);
    let reused: i64 = tc.draw(pool.values_reusable());
    assert!(reused == 10 || reused == 20);
    assert_eq!(pool.len(), 2);
    let first: i64 = tc.draw(pool.values_consumed());
    assert_eq!(pool.len(), 1);
    let second: i64 = tc.draw(pool.values_consumed());
    assert_eq!(first + second, 30);
    assert!(pool.is_empty());
}

#[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch])]
fn test_concurrent_pool_empty_draws_reject(tc: TestCase) {
    let pool: ConcurrentPool<i64> = concurrent_pool(&tc);
    let consume: bool = tc.draw_silent(gs::booleans());
    if consume {
        let _: i64 = tc.draw_silent(pool.values_consumed());
    } else {
        let _: i64 = tc.draw_silent(pool.values_reusable());
    }
    unreachable!("an empty-pool draw must reject the test case");
}

struct PoolMachine {
    pool: ConcurrentPool<i64>,
    next: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl PoolMachine {
    #[rule]
    fn add(&self, tc: TestCase) {
        self.pool.add(&tc, self.next.fetch_add(1, Ordering::SeqCst));
    }

    #[rule]
    fn reuse(&self, tc: TestCase) {
        let value: i64 = tc.draw_silent(self.pool.values_reusable());
        assert!(value >= 0);
    }

    #[rule]
    fn consume(&self, tc: TestCase) {
        let value: i64 = tc.draw_silent(self.pool.values_consumed());
        assert!(value >= 0);
    }

    #[invariant]
    fn pool_is_bounded(&self, _: TestCase) {
        assert!(self.pool.len() <= 100_000);
    }
}

#[hegel::test(nondeterministic = true)]
fn test_concurrent_pool_across_workers(tc: TestCase) {
    let m = PoolMachine {
        pool: concurrent_pool(&tc),
        next: AtomicI64::new(0),
    };
    run_concurrent(m, tc, 3);
}

struct RacyCounter {
    value: AtomicI64,
    increments: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl RacyCounter {
    #[rule]
    fn racy_increment(&self, _: TestCase) {
        let value = self.value.load(Ordering::SeqCst);
        std::thread::yield_now();
        self.value.store(value + 1, Ordering::SeqCst);
        self.increments.fetch_add(1, Ordering::SeqCst);
    }

    #[invariant]
    fn no_lost_updates(&self, _: TestCase) {
        assert_eq!(
            self.value.load(Ordering::SeqCst),
            self.increments.load(Ordering::SeqCst)
        );
    }
}

/// A genuinely racy SUT: lost updates make the invariant fail on some runs
/// and not others. The smoke test accepts either outcome — the bug being
/// found is not asserted — but any reported failure must be the invariant's
/// own assertion, never a flakiness or nondeterminism complaint.
#[test]
fn racy_smoke_test_reports_only_genuine_failures() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            let m = RacyCounter {
                value: AtomicI64::new(0),
                increments: AtomicI64::new(0),
            };
            run_concurrent(m, tc, 4);
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .test_cases(20)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    if let Err(payload) = result {
        let message = panic_message(&payload);
        assert_matches_regex(&message, "left == right|assertion");
        assert!(
            !message.contains("Flaky"),
            "a nondeterministic run must not complain about flakiness: {message}"
        );
        assert!(
            !message.to_lowercase().contains("non-deterministic"),
            "a nondeterministic run must not report a generation mismatch: {message}"
        );
    }
}

struct NoRules;

impl hegel::stateful::ConcurrentStateMachine for NoRules {
    fn rules(&self) -> Vec<hegel::stateful::ConcurrentRule<Self>> {
        Vec::new()
    }
    fn invariants(&self) -> Vec<hegel::stateful::ConcurrentInvariant<Self>> {
        Vec::new()
    }
}

#[test]
fn a_machine_without_rules_is_a_usage_error() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| run_concurrent(NoRules, tc, 1))
            .settings(
                Settings::new()
                    .nondeterministic(true)
                    .database(None)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let payload = result.expect_err("a machine with no rules cannot run");
    assert_matches_regex(&panic_message(&payload), "no rules");
}

/// Exhaust the whole family draw budget on a clone stream, leaving the root
/// handle un-aborted so the next engine call on it is the one that observes
/// the exhaustion.
fn exhaust_budget_on_a_clone(tc: &TestCase) {
    let clone = tc.clone();
    let exhausted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        loop {
            let _: i64 = clone.draw_silent(gs::integers());
        }
    }));
    assert!(exhausted.is_err(), "the family budget is finite");
}

struct AddAfterExhaustion {
    pool: ConcurrentPool<i64>,
}

#[hegel::concurrent_state_machine]
impl AddAfterExhaustion {
    #[rule]
    fn add_after_exhaustion(&self, tc: TestCase) {
        let exhausted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                let _: i64 = tc.draw_silent(gs::integers());
            }
        }));
        assert!(exhausted.is_err(), "the family budget is finite");
        self.pool.add(&tc, 1);
        unreachable!("adding to a pool on an exhausted stream must overrun");
    }
}

#[test]
fn pool_add_on_an_exhausted_stream_is_an_overrun() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            let m = AddAfterExhaustion {
                pool: concurrent_pool(&tc),
            };
            run_concurrent(m, tc, 1);
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    let payload = result.expect_err("the first case overruns, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "LargeInitialTestCase");
}

#[test]
fn creating_a_pool_on_an_exhausted_stream_is_an_overrun() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc: TestCase| {
            let exhausted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    let _: i64 = tc.draw_silent(gs::integers());
                }
            }));
            assert!(exhausted.is_err(), "the family budget is finite");
            let _: ConcurrentPool<i64> = concurrent_pool(&tc);
            unreachable!("creating a pool on an exhausted stream must overrun");
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    let payload = result.expect_err("the first case overruns, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "LargeInitialTestCase");
}

#[test]
fn budget_exhaustion_during_the_concurrency_draw_is_an_overrun() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            exhaust_budget_on_a_clone(&tc);
            let m = Counter {
                value: AtomicI64::new(0),
            };
            run_concurrent(m, tc, 2);
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    let payload = result.expect_err("the first case overruns, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "LargeInitialTestCase");
}

#[test]
fn budget_exhaustion_during_the_main_threads_round_draws_is_an_overrun() {
    let (_, result) = capture_hegel_output(|| {
        Hegel::new(|tc| {
            exhaust_budget_on_a_clone(&tc);
            let m = Counter {
                value: AtomicI64::new(0),
            };
            run_concurrent(m, tc, 1);
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    });
    let payload = result.expect_err("the first case overruns, so the health check fires");
    assert_matches_regex(&panic_message(&payload), "LargeInitialTestCase");
}

#[test]
fn a_nondeterministic_run_prints_only_the_discovering_cases_output() {
    static CASES: AtomicI64 = AtomicI64::new(0);
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: TestCase| {
            let case = CASES.fetch_add(1, Ordering::SeqCst);
            let x: i64 = tc.draw(gs::integers());
            if case == 2 {
                panic!("boom on the third case with {x}");
            }
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .print_blob(true),
        )
        .run();
    });
    let payload = result.expect_err("the third case fails the run");
    assert_matches_regex(&panic_message(&payload), "boom on the third case");
    let draw_lines = lines.iter().filter(|l| l.contains("let ")).count();
    assert_eq!(
        draw_lines, 1,
        "only the discovering case's buffer is printed: {lines:?}"
    );
    let text = lines.join("\n");
    assert!(
        text.contains("panicked at"),
        "the diagnostic is printed after the buffer:\n{text}"
    );
    assert!(
        !text.contains("To reproduce this failure"),
        "no reproducer line is printed even with print_blob:\n{text}"
    );
}

#[test]
fn a_verbose_nondeterministic_run_streams_every_cases_output_live() {
    static CASES: AtomicI64 = AtomicI64::new(0);
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: TestCase| {
            let case = CASES.fetch_add(1, Ordering::SeqCst);
            let x: i64 = tc.draw(gs::integers());
            if case == 2 {
                panic!("boom on the third case with {x}");
            }
        })
        .settings(
            Settings::new()
                .nondeterministic(true)
                .database(None)
                .verbosity(Verbosity::Verbose),
        )
        .run();
    });
    let payload = result.expect_err("the third case fails the run");
    assert_matches_regex(&panic_message(&payload), "boom on the third case");
    let draw_lines = lines.iter().filter(|l| l.contains("let ")).count();
    assert_eq!(
        draw_lines, 4,
        "all three cases stream live and the failure report repeats the \
         discovering case's draw: {lines:?}"
    );
    let diagnostics = lines.iter().filter(|l| l.contains("panicked at")).count();
    assert_eq!(
        diagnostics, 2,
        "the diagnostic prints live at discovery and again in the failure \
         report: {lines:?}"
    );
}
