mod common;

use common::utils::expect_panic;
use hegel::generators as gs;
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

/// A test that draws data and rejects every input is Unsatisfiable even
/// with every health check suppressed: suppression lets generation spend
/// its whole invalid budget, but a run with no valid input must not pass.
#[test]
fn always_rejecting_test_with_draws_is_unsatisfiable() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                tc.draw(gs::booleans());
                tc.reject();
            })
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

/// A test accepting one of seven equally likely values keeps generating
/// until it has found it, then stops once the seven values are exhausted:
/// it never ends early as passed, nor as Unsatisfiable, on a run of
/// rejections.
#[test]
fn rare_valid_input_is_found_under_suppression() {
    for seed in 0..50 {
        let valid = std::cell::Cell::new(0u64);
        Hegel::new(|tc: TestCase| {
            let value = tc.draw(gs::integers::<i32>().min_value(0).max_value(6));
            tc.assume(value == 6);
            valid.set(valid.get() + 1);
        })
        .settings(
            Settings::new()
                .database(None)
                .seed(Some(seed))
                .test_cases(5)
                .suppress_health_check(HealthCheck::all()),
        )
        .run();
        assert!(
            (1..=5).contains(&valid.get()),
            "seed {seed}: {}",
            valid.get()
        );
    }
}

/// With every health check suppressed and a large budget, a test that draws
/// an integer and rejects every input ends Unsatisfiable, naming the
/// rejections neutrally: the engine cannot tell assume() from a filter.
#[test]
fn always_rejecting_integer_test_is_unsatisfiable_with_all_checks_suppressed() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                tc.draw(gs::integers::<i64>());
                tc.reject();
            })
            .settings(
                Settings::new()
                    .database(None)
                    .test_cases(1000)
                    .suppress_health_check(HealthCheck::all()),
            )
            .run();
        },
        "Unsatisfiable: unable to satisfy the test's assumptions. No valid input was \
         generated: 459 inputs were rejected by assume\\(\\) or a filter",
    );
}

/// Rejecting both values of a boolean exhausts the space: the run is
/// Unsatisfiable, saying so, rather than a FilterTooMuch health check.
#[test]
fn rejecting_every_input_of_a_finite_space_is_unsatisfiable() {
    let calls = std::sync::atomic::AtomicU64::new(0);
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tc.draw(gs::booleans());
                tc.reject();
            })
            .settings(Settings::new().database(None).test_cases(200))
            .run();
        },
        "Unsatisfiable: unable to satisfy the test's assumptions. The test tried \
         every possible input, and all",
    );
    let calls = calls.load(std::sync::atomic::Ordering::SeqCst);
    assert!(calls < 50, "{calls}");
}

/// A test over a finite space stops once every input has been tried,
/// short of its test-case budget.
#[test]
fn a_finite_space_stops_once_exhausted() {
    let seen = std::cell::RefCell::new(Vec::new());
    Hegel::new(|tc: TestCase| {
        seen.borrow_mut().push(tc.draw(gs::booleans()));
    })
    .settings(Settings::new().database(None).test_cases(200))
    .run();
    let seen = seen.borrow();
    assert!(seen.len() < 200, "{seen:?}");
    assert!(seen.contains(&false) && seen.contains(&true), "{seen:?}");
}

#[test]
fn trivial_passing_test_is_not_unsatisfiable() {
    Hegel::new(|_tc: TestCase| {})
        .settings(Settings::new().database(None))
        .run();
}
