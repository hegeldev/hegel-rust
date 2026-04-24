//! Ported from hypothesis-python/tests/nocover/test_unusual_settings_configs.py.

use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings, TestCase, Verbosity};

#[test]
fn test_single_example() {
    Hegel::new(|tc: TestCase| {
        let _: i64 = tc.draw(gs::integers());
    })
    .settings(Settings::new().test_cases(1).database(None))
    .run();
}

#[test]
fn test_hard_to_find_single_example() {
    Hegel::new(|tc: TestCase| {
        let n: i64 = tc.draw(gs::integers());
        // Numbers are arbitrary, just deliberately unlikely to hit this too soon.
        tc.assume(n.rem_euclid(50) == 11);
    })
    .settings(
        Settings::new()
            .test_cases(1)
            .database(None)
            .suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow])
            .verbosity(Verbosity::Debug),
    )
    .run();
}
