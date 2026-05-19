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
}
