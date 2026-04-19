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
            let x: u64 = tc.draw(
                hegel::generators::integers::<u64>()
                    .min_value(0)
                    .max_value(1_000_000),
            );
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
        let x: u64 = tc.draw(
            hegel::generators::integers::<u64>()
                .min_value(0)
                .max_value(1_000_000),
        );
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

/// When cumulative test-case time exceeds the TooSlow threshold, the health
/// check fires. The test draws a value so the runner doesn't bail out on the
/// trivial-case path after the first iteration.
#[cfg(feature = "native")]
#[test]
fn native_too_slow_detected() {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            let _: bool = tc.draw(gs::booleans());
            std::thread::sleep(std::time::Duration::from_millis(300));
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
        msg.contains("TooSlow") || msg.contains("too long") || msg.contains("slow"),
        "expected TooSlow health check error, got: {msg}"
    );
}

/// TooSlow detection is suppressed when HealthCheck::TooSlow is in suppress_health_check.
#[cfg(feature = "native")]
#[test]
fn native_too_slow_suppressed() {
    // test_cases = 1 to avoid a 30-second test (300ms * 100 examples).
    hegel::Hegel::new(|_tc: hegel::TestCase| {
        std::thread::sleep(std::time::Duration::from_millis(300));
    })
    .settings(
        hegel::Settings::new()
            .test_cases(1)
            .suppress_health_check([HealthCheck::TooSlow]),
    )
    .run();
}

/// A single moderately-slow test case followed by fast cases must not trigger
/// TooSlow. Hypothesis's TooSlow check looks at cumulative draw time across
/// the run (with a 1s floor), not per-case wall-clock time, so a one-off slow
/// example shouldn't cause the run to fail.
#[cfg(feature = "native")]
#[test]
fn native_too_slow_single_slow_case_does_not_fire() {
    let mut count = 0;
    hegel::Hegel::new(move |_tc: hegel::TestCase| {
        if count == 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        count += 1;
    })
    .run();
}

/// Once a run has accumulated more than the upstream `max_valid_draws` (10)
/// valid examples, the TooSlow health check is disabled — mirroring
/// Hypothesis's `record_for_health_check`. Each test case here sleeps 50 ms,
/// so 50 valid examples take ~2.5 s of wall-clock time; without the cap the
/// check would fire after ~20 examples (1 s threshold), but in upstream
/// behaviour the check is silenced once the first 10 valid examples have
/// been recorded.
#[cfg(feature = "native")]
#[test]
fn native_too_slow_disabled_after_first_10_valid() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let _: bool = tc.draw(gs::booleans());
        std::thread::sleep(std::time::Duration::from_millis(50));
    })
    .settings(hegel::Settings::new().test_cases(50))
    .run();
}
