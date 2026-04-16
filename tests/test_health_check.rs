use hegel::HealthCheck;
use hegel::TestCase;
use hegel::generators as gs;

/// When assume() filters out almost all examples, FilterTooMuch should be
/// reported as a health check failure.
#[cfg(feature = "native")]
#[test]
fn native_filter_too_much_detected() {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            let x: u64 = tc.draw(hegel::generators::integers::<u64>()
                .min_value(0).max_value(1_000_000));
            tc.assume(x == 42); // almost always filtered out
        })
        .run();
    });
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("FilterTooMuch") || msg.contains("filter") || msg.contains("assume"),
        "expected FilterTooMuch health check error, got: {msg}"
    );
}

/// FilterTooMuch suppression must work even with extreme filtering.
#[cfg(feature = "native")]
#[test]
fn native_filter_too_much_suppressed() {
    // Should complete without panicking.
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let x: u64 = tc.draw(hegel::generators::integers::<u64>()
            .min_value(0).max_value(1_000_000));
        tc.assume(x == 42);
    })
    .settings(hegel::Settings::new().suppress_health_check([HealthCheck::FilterTooMuch]))
    .run();
}

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
