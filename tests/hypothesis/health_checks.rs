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

#[cfg(feature = "native")]
#[test]
fn test_suppressing_filtering_health_check() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Model Python's `unhealthy_filter`: reject the first batch of draws,
    // then start accepting. The upstream test's second half relies on the
    // filter eventually opening up so the test body runs (it raises
    // `ValueError`); a filter that rejects *everything* can't exercise
    // that half. The threshold is measured in filter calls rather than
    // test cases because `Filtered::do_draw` retries the predicate up to
    // 3 times per draw before giving up with `assume(false)`. With the
    // default `test_cases = 100` (→ up to 1000 calls → up to 3000 filter
    // calls), a threshold of 1500 rejects roughly the first 500 draws —
    // well past FilterTooMuch's 200-invalid bar — and then opens up so
    // the remaining budget can produce a valid value.
    let make_filter = || {
        let counter = Arc::new(AtomicUsize::new(0));
        move |_: &i64| counter.fetch_add(1, Ordering::Relaxed) >= 1500
    };

    // Part 1 (upstream `test1`): no suppression → FilterTooMuch fires
    // before the filter opens up, so the test body never runs.
    expect_panic(
        || {
            let filter = make_filter();
            Hegel::new(move |tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>().filter(filter.clone()));
                panic!("body-ran");
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FailedHealthCheck.*FilterTooMuch",
    );

    // Part 2 (upstream `test2`): with FilterTooMuch and TooSlow suppressed,
    // generation pushes past the 200-invalid bar until the filter opens up,
    // a valid value is drawn, and the test body runs and panics —
    // analogous to Python's `pytest.raises(ValueError)`.
    expect_panic(
        || {
            let filter = make_filter();
            Hegel::new(move |tc: TestCase| {
                let _: i64 = tc.draw(gs::integers::<i64>().filter(filter.clone()));
                panic!("body-ran");
            })
            .settings(
                Settings::new()
                    .database(None)
                    .suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow]),
            )
            .run();
        },
        "body-ran",
    );
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
    // invalid cases accumulate with no prior valid case, so we need the
    // range wide enough that a valid draw in the first 200 calls is
    // vanishingly unlikely. A `u16`-sized range would occasionally draw
    // `3` within the health-check window (≈0.2% of runs) and the check
    // would never fire; using the full `u64` range makes that impossible
    // (3 is not among the "nasty" boundary candidates, so uniform draws
    // dominate and P(3) ≈ 2^-64).
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let b: u64 = tc.draw(gs::integers::<u64>());
                tc.assume(b == 3);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FailedHealthCheck.*filter",
    );
}
