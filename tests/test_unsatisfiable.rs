mod common;

use common::utils::expect_panic;
use hegel::{HealthCheck, Hegel, Settings, TestCase};

/// Regression for issue #78: a test whose only input is rejected by
/// `assume(false)` before any draw must error instead of passing
/// vacuously, as Hypothesis does with `Unsatisfiable`.
#[hegel::test(database = None)]
#[should_panic(expected = "Unsatisfiable")]
fn test_assume_false_without_draws_is_unsatisfiable(tc: TestCase) {
    tc.assume(false);
}

#[test]
fn reject_without_draws_is_unsatisfiable() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| tc.reject())
                .settings(Settings::new().database(None))
                .run();
        },
        "Unsatisfiable",
    );
}

/// Unlike FilterTooMuch, Unsatisfiable is not a health check: no amount
/// of retrying can produce a valid input, so suppression does not apply.
#[test]
fn unsatisfiable_is_not_suppressible() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| tc.assume(false))
                .settings(
                    Settings::new()
                        .database(None)
                        .suppress_health_check(HealthCheck::all()),
                )
                .run();
        },
        "Unsatisfiable",
    );
}

#[test]
fn trivial_passing_test_is_not_unsatisfiable() {
    Hegel::new(|_tc: TestCase| {})
        .settings(Settings::new().database(None))
        .run();
}
