//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_health_checks.py.
//!
//! Individual upstream tests not ported (see SKIPPED.md):
//!
//! - `test_returning_non_none_is_forbidden`,
//!   `test_stateful_returnvalue_healthcheck` — check Hypothesis's `return_value`
//!   health check on `@given`/`@rule`/`@initialize`/`@invariant`-decorated
//!   functions. Rust closures have declared return types already; the concept
//!   is Python-specific and hegel-rust has no corresponding variant.
//! - `test_the_slow_test_health_check_can_be_disabled`,
//!   `test_the_slow_test_health_only_runs_if_health_checks_are_on` — use the
//!   `deadline=None` setting and `skipif_time_unpatched`, a pytest-specific
//!   time-freezing fixture. hegel-rust has no `deadline` setting.
//! - `test_differing_executors_fails_health_check` — tests the
//!   `differing_executors` health check on `@given`-decorated instance methods
//!   called with different `self` receivers. hegel-rust tests are closures
//!   passed to `Hegel::new(...).run()` with no class/instance dispatch.
//! - `test_it_is_an_error_to_suppress_non_iterables`,
//!   `test_it_is_an_error_to_suppress_non_healthchecks` — Python dynamic
//!   typing: pass a non-iterable or non-`HealthCheck` to
//!   `suppress_health_check`. Rust's type system prevents these at compile
//!   time (`impl IntoIterator<Item = HealthCheck>`).
//! - `test_nested_given_raises_healthcheck`,
//!   `test_triply_nested_given_raises_healthcheck`,
//!   `test_can_suppress_nested_given`,
//!   `test_cant_suppress_nested_given_on_inner`,
//!   `test_suppress_triply_nested_given` — all exercise
//!   `HealthCheck.nested_given`, which detects a `@given`-decorated function
//!   being called from inside another `@given` function. hegel-rust has no
//!   `nested_given` variant and no decorator-based test dispatch to nest.

#[cfg(feature = "native")]
use crate::common::utils::expect_panic;
use hegel::generators as gs;
#[cfg(feature = "native")]
use hegel::generators::Generator;
use hegel::{HealthCheck, Hegel, Settings, TestCase};

#[cfg(feature = "native")]
#[test]
fn test_slow_generation_fails_a_health_check() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>().map(|x| {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    x
                }));
            })
            .settings(Settings::new().test_cases(11).database(None))
            .run();
        },
        "FailedHealthCheck.*TooSlow",
    );
}

#[cfg(feature = "native")]
#[test]
fn test_slow_generation_inline_fails_a_health_check() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>());
                std::thread::sleep(std::time::Duration::from_millis(200));
            })
            .settings(Settings::new().test_cases(11).database(None))
            .run();
        },
        "FailedHealthCheck.*TooSlow",
    );
}

#[test]
fn test_default_health_check_can_weaken_specific() {
    Hegel::new(|tc: TestCase| {
        let xs: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).min_size(1));
        let _ = xs[0];
    })
    .settings(
        Settings::new()
            .test_cases(11)
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}

#[cfg(feature = "native")]
#[test]
fn test_suppressing_filtering_health_check() {
    // hegel-rust fires FilterTooMuch once 200 consecutive invalid calls
    // accumulate before any valid case (see `FILTER_TOO_MUCH_THRESHOLD` in
    // `src/native/runner.rs`). Use the default `test_cases = 100`, which
    // allows up to 1000 calls — enough headroom for the threshold to trip.
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>().filter(|_| false));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FailedHealthCheck.*FilterTooMuch",
    );

    // With FilterTooMuch (and TooSlow, as the Python original does) suppressed,
    // the same filter must not cause the run to panic with a health check
    // error. The filter still rejects every value, so the test body never
    // runs — analogous to Python raising no ValueError when the filter
    // successfully blocks all inputs.
    Hegel::new(|tc: TestCase| {
        let _: i64 = tc.draw(gs::integers::<i64>().filter(|_| false));
    })
    .settings(
        Settings::new()
            .database(None)
            .suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow]),
    )
    .run();
}

#[cfg(feature = "native")]
#[test]
fn test_filtering_everything_fails_a_health_check() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>().filter(|_| false));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FailedHealthCheck.*filter",
    );
}

#[cfg(feature = "native")]
#[test]
fn test_filtering_most_things_fails_a_health_check() {
    // The Python original draws 16 bits and `assume(b == 3)` — ~1/65536
    // acceptance. hegel-rust's FilterTooMuch fires when 200 consecutive
    // invalid cases accumulate with no prior valid case, so any range
    // wide enough to make valid draws vanishingly rare triggers it.
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let b: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(65535));
                tc.assume(b == 3);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FailedHealthCheck.*filter",
    );
}
