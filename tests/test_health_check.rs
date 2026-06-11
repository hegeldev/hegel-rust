mod common;

use hegel::HealthCheck;
use hegel::TestCase;
use hegel::generators as gs;

/// Macro-form suppression: with `tc.assume(n == 42)` rejecting
/// ~99.99% of an `i32` range, FilterTooMuch *would* fire if not
/// suppressed (see `filter_too_much_detected` above for the
/// without-suppression failure shape).  This test pairs with that
/// one to confirm `#[hegel::test(suppress_health_check =
/// [HealthCheck::FilterTooMuch])]` actually applies the suppression
/// — the body's panic-free completion under heavy filtering is the
/// behavioural claim.
#[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch])]
fn test_filter_too_much_suppressed(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000));
    tc.assume(n == 42);
}

/// Macro-form suppression with the multi-element array syntax:
/// `[FilterTooMuch, TooSlow]`.  Heavy filtering + a sleep that pushes
/// per-case time near the TooSlow threshold confirms both
/// suppressions are applied (without either, the body would fail one
/// of the two checks).
#[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch, HealthCheck::TooSlow])]
fn test_suppress_multiple(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000));
    tc.assume(n == 42);
    std::thread::sleep(std::time::Duration::from_millis(10));
}

/// Macro-form suppression with the function-call syntax
/// `HealthCheck::all()`.  Same heavy filtering; same paired-test
/// reasoning: any active health check would fire, but `all()` covers
/// every variant so suppression carries the run through.
#[hegel::test(suppress_health_check = HealthCheck::all())]
fn test_suppress_all(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000));
    tc.assume(n == 42);
    std::thread::sleep(std::time::Duration::from_millis(10));
}

/// Once a bug has been found, health checks must stop firing: the post-bug
/// probing window keeps generating (here, a recurring bug among nothing but
/// `assume(false)` rejections, so invalid cases pile up far faster than
/// valid ones), and converting the already-found failure into FilterTooMuch
/// would mask the real bug. Mirrors Hypothesis's `record_for_health_check`,
/// which disables health checks at the first INTERESTING result.
#[hegel::test(
    derandomize = true,
    test_cases = 300u64,
    database = None,
    verbosity = hegel::Verbosity::Quiet
)]
#[should_panic(expected = "the real bug")]
fn test_health_checks_do_not_mask_a_found_bug(tc: TestCase) {
    let x: u64 = tc.draw(gs::integers::<u64>());
    if x % 2 == 1 {
        panic!("the real bug");
    }
    tc.assume(false);
}

mod health_checks {
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings, TestCase};

    #[test]
    fn test_default_health_check_can_weaken_specific() {
        Hegel::new(|tc: TestCase| {
            let _: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).min_size(1));
        })
        .settings(
            Settings::new()
                .test_cases(11)
                .database(None)
                .suppress_health_check(HealthCheck::all()),
        )
        .run();
    }

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Regression for the high-rejection-rate divergence Ethan reported: with
    /// FilterTooMuch suppressed, an always-rejecting test used to run
    /// `max_examples * 10` cases (1000 for the default 100), because the
    /// native generation loop capped on `calls < max_examples * 10` regardless
    /// of the rejection rate. Hypothesis instead stops once the invalid budget
    /// `INVALID_THRESHOLD_BASE + INVALID_PER_VALID * valid` is exceeded — for a
    /// run that never produces a valid example that is `458 + 0`, so the run
    /// gives up after exactly 459 cases. This pins the ported behaviour.
    #[test]
    fn always_reject_with_suppression_stops_at_invalid_budget() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        Hegel::new(move |tc: TestCase| {
            c.fetch_add(1, Ordering::SeqCst);
            // A wide domain so the choice tree never exhausts; every input is
            // rejected, so no valid example is ever produced.
            let _: i64 = tc.draw(gs::integers::<i64>());
            tc.reject();
        })
        .settings(
            Settings::new()
                .test_cases(100)
                .database(None)
                .derandomize(true)
                .suppress_health_check([HealthCheck::FilterTooMuch]),
        )
        .run();

        let n = count.load(Ordering::SeqCst);
        assert_eq!(
            n, 459,
            "always-reject with no valid examples should stop at the invalid \
             budget (459 cases), not the old max_examples*10 cap; got {n}"
        );
    }

    /// Regression for the FilterTooMuch threshold divergence: the native check
    /// used to require 200 *consecutive* rejects with *zero* valid examples,
    /// whereas Hypothesis trips at 50 *total* invalid draws while fewer than 10
    /// valid examples have been seen. With an always-rejecting test the check
    /// must now fire on the 50th invalid draw.
    #[test]
    fn always_reject_trips_filter_too_much_at_fifty() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(move |tc: TestCase| {
                c.fetch_add(1, Ordering::SeqCst);
                let _: i64 = tc.draw(gs::integers::<i64>());
                tc.reject();
            })
            .settings(
                Settings::new()
                    .test_cases(100)
                    .database(None)
                    .derandomize(true),
            )
            .run();
        }));

        let msg = result
            .expect_err("expected FilterTooMuch health-check panic")
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            msg.contains("FilterTooMuch"),
            "expected FilterTooMuch panic, got {msg:?}"
        );
        let n = count.load(Ordering::SeqCst);
        assert_eq!(
            n, 50,
            "FilterTooMuch should trip on the 50th invalid draw, got {n}"
        );
    }

    /// Regression for the `valid == 0` half of the FilterTooMuch divergence:
    /// the old native check could never fire once a single valid example had
    /// been produced (it reset its consecutive-reject counter on every
    /// non-invalid run and required `valid == 0`). Hypothesis keeps the check
    /// live until 10 valid examples accumulate. A test with a low-but-nonzero
    /// acceptance rate produces a handful of valid examples long before 50
    /// invalid draws pile up, so the check must still trip — under the old
    /// behaviour this run completed vacuously instead.
    #[test]
    fn low_acceptance_rate_trips_filter_too_much_despite_some_valid() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>());
                // ~6% acceptance: a few valid examples appear before 50
                // invalid draws accumulate, but valid stays well below 10.
                tc.assume(n.rem_euclid(16) == 0);
            })
            .settings(
                Settings::new()
                    .test_cases(100)
                    .database(None)
                    .derandomize(true),
            )
            .run();
        }));

        let msg = result
            .expect_err("expected FilterTooMuch health-check panic")
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            msg.contains("FilterTooMuch"),
            "expected FilterTooMuch panic even with some valid examples, got {msg:?}"
        );
    }
}

// Size-based health checks (TestCasesTooLarge / LargeInitialTestCase) are
// native-engine features, so these run only under `--features native`.
mod size_checks {
    use super::common::utils::expect_panic;
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings};

    // A generator whose elements alone overrun the choice buffer.
    fn oversized(tc: hegel::TestCase) {
        tc.draw(gs::vecs(gs::booleans()).min_size(20_000));
    }

    /// The smallest natural example already overruns the buffer, so
    /// LargeInitialTestCase fires.
    #[test]
    fn large_initial_test_case_fires() {
        expect_panic(
            || {
                Hegel::new(oversized)
                    .settings(Settings::new().test_cases(100).database(None))
                    .run();
            },
            "LargeInitialTestCase",
        );
    }

    /// With LargeInitialTestCase suppressed, the generation loop keeps
    /// overrunning, so TestCasesTooLarge fires instead.
    #[test]
    fn test_cases_too_large_fires() {
        expect_panic(
            || {
                Hegel::new(oversized)
                    .settings(
                        Settings::new()
                            .test_cases(100)
                            .database(None)
                            .suppress_health_check([HealthCheck::LargeInitialTestCase]),
                    )
                    .run();
            },
            "TestCasesTooLarge",
        );
    }

    /// Both suppressed: the run completes (no health-check panic) — it just
    /// keeps overrunning until the generation budget is spent.
    #[test]
    fn both_suppressed_does_not_fire() {
        Hegel::new(oversized)
            .settings(
                Settings::new()
                    .test_cases(5)
                    .database(None)
                    .suppress_health_check([
                        HealthCheck::LargeInitialTestCase,
                        HealthCheck::TestCasesTooLarge,
                    ]),
            )
            .run();
    }
}
