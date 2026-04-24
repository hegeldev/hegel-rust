mod common;

use hegel::HealthCheck;
use hegel::TestCase;
use hegel::generators as gs;

/// Suppresses FilterTooMuch with light filtering (most values pass).
#[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch])]
fn test_filter_too_much_suppressed(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers().min_value(0).max_value(100));
    tc.assume(n < 90);
}

/// Tests that the macro accepts multiple health checks in array syntax.
#[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch, HealthCheck::TooSlow])]
fn test_suppress_multiple(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers().min_value(0).max_value(100));
    tc.assume(n < 90);
}

/// Tests that `HealthCheck::all()` is accepted by the macro.
#[hegel::test(suppress_health_check = HealthCheck::all())]
fn test_suppress_all(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers().min_value(0).max_value(100));
    tc.assume(n < 90);
}

#[hegel::test(
    test_cases = 15,
    suppress_health_check = [HealthCheck::TestCasesTooLarge, HealthCheck::TooSlow, HealthCheck::LargeInitialTestCase]
)]
fn test_data_too_large_suppressed(tc: TestCase) {
    let do_big: bool = tc.draw(gs::booleans());
    if do_big {
        for _ in 0..100 {
            let _: i32 = tc.draw(gs::integers());
        }
    }
}

#[hegel::test(
    test_cases = 15,
    suppress_health_check = [HealthCheck::LargeInitialTestCase, HealthCheck::TestCasesTooLarge, HealthCheck::TooSlow]
)]
fn test_large_base_example_suppressed(tc: TestCase) {
    for _ in 0..10 {
        let _: Vec<i32> = tc.draw(gs::vecs(gs::integers()).min_size(50).max_size(50));
    }
}
