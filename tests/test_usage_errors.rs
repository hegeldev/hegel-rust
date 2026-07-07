//! Tests for the unified invalid-argument (usage) error mechanism.
//!
//! A usage error — a generator configured with `max < min`, a float range that
//! contains no values, an empty `sampled_from`/`one_of`, an unsatisfiable
//! filter, a non-finite `tc.target()` score, ... — is a mistake in how the
//! test is *written*, not a property that failed on some input. The framework
//! must abort the run with the error message directly, rather than catching it
//! mid-draw and misreporting (and shrinking) it as a discovered counterexample
//! ("Property test failed: ...").
//!
//! Every site funnels through the `invalid_argument!` macro, so these tests
//! assert the shared contract: the message survives, it is *not* wrapped as a
//! property failure, and the internal sentinel never leaks.

mod common;

use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};
use std::time::Duration;

/// Run a property whose body (or a generator it draws) raises a usage error,
/// and return the message the run aborted with.
fn capture_run_panic(body: impl FnMut(hegel::TestCase) + std::panic::UnwindSafe) -> String {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        Hegel::new(body)
            .settings(Settings::new().test_cases(50).database(None))
            .run();
    }));
    let payload = result.expect_err("expected the run to abort with a usage error");
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default()
}

/// Assert `msg` is a clean usage error: it carries `expected`, it is not
/// dressed up as a property failure, and no internal marker leaked.
fn assert_clean_usage_error(msg: &str, expected: &str) {
    assert!(
        msg.contains(expected),
        "message {msg:?} is missing {expected:?}"
    );
    assert!(
        !msg.contains("Property test failed"),
        "a usage error must abort the run cleanly, not be reported as a property failure: {msg:?}"
    );
    assert!(
        !msg.contains("__HEGEL"),
        "internal sentinel leaked into the user-facing message: {msg:?}"
    );
}

#[test]
fn target_non_finite_score_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| tc.target(f64::NAN));
    assert_clean_usage_error(&msg, "finite");

    let msg = capture_run_panic(|tc| tc.target(f64::INFINITY));
    assert_clean_usage_error(&msg, "finite");
}

#[test]
fn target_duplicate_label_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        tc.target_labelled(1.0, "dup");
        tc.target_labelled(2.0, "dup");
    });
    assert_clean_usage_error(&msg, "at most once");
}

#[test]
fn float_range_with_no_values_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(f64::INFINITY)
                .exclude_min(true),
        );
    });
    assert_clean_usage_error(&msg, "InvalidArgument");
}

#[test]
fn text_with_inverted_codepoint_range_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: String = tc.draw(gs::text().min_codepoint(200).max_codepoint(100));
    });
    assert_clean_usage_error(&msg, "InvalidArgument");
}

#[test]
fn integer_max_below_min_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: i32 = tc.draw(gs::integers::<i32>().min_value(5).max_value(3));
    });
    assert_clean_usage_error(&msg, "Cannot have max_value < min_value");
}

#[test]
fn vec_max_size_below_min_size_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).min_size(5).max_size(2));
    });
    assert_clean_usage_error(&msg, "Cannot have max_size < min_size");
}

#[test]
fn duration_min_above_u64_nanos_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: Duration = tc.draw(gs::durations().min_value(Duration::from_secs(u64::MAX)));
    });
    assert_clean_usage_error(&msg, "largest generatable Duration");
}

#[test]
fn uuid_version_outside_1_to_5_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _ = tc.draw(gs::uuids().version(9));
    });
    assert_clean_usage_error(&msg, "version must be between 1 and 5");
}

#[test]
fn hashset_min_size_above_distinct_pool_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: std::collections::HashSet<i64> =
            tc.draw(gs::hashsets(gs::sampled_from(vec![1_i64, 2, 3])).min_size(5));
    });
    assert_clean_usage_error(
        &msg,
        "min_size 5 is larger than the 3 distinct values the element generator can produce",
    );
}

#[test]
fn sampled_from_empty_drawn_inline_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: i32 = tc.draw(gs::sampled_from(Vec::<i32>::new()));
    });
    assert_clean_usage_error(&msg, "cannot be empty");
}

#[test]
fn one_of_empty_drawn_inline_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: i32 = tc.draw(gs::one_of(
            Vec::<hegel::generators::BoxedGenerator<'_, i32>>::new(),
        ));
    });
    assert_clean_usage_error(&msg, "requires at least one generator");
}

#[test]
fn duration_max_below_min_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: Duration = tc.draw(
            gs::durations()
                .min_value(Duration::from_secs(10))
                .max_value(Duration::from_secs(1)),
        );
    });
    assert_clean_usage_error(&msg, "Cannot have max_value < min_value");
}

#[test]
fn unsatisfiable_filter_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        let _: i64 = tc.draw(gs::sampled_from(vec![0_i64, 1]).filter(|x: &i64| *x < 0));
    });
    assert_clean_usage_error(&msg, "Unsatisfiable filter");
}

#[test]
fn usage_error_inside_repeat_is_a_clean_usage_error() {
    let msg = capture_run_panic(|tc| {
        tc.repeat(|| {
            let _: i32 = tc.draw(gs::integers::<i32>().min_value(5).max_value(3));
        });
    });
    assert_clean_usage_error(&msg, "Cannot have max_value < min_value");
}
