//! Ported from hypothesis-python/tests/quality/test_float_shrinking.py.
//!
//! The Python file stacks `@given` on top of `minimal(...)` bodies via
//! `suppress_health_check=[HealthCheck.nested_given]`. hegel-rust's
//! `minimal` helper runs derandomised (500 test cases per call), so an
//! outer `@given` loop of 100 draws multiplies the cost without adding
//! coverage beyond the explicit `@example` rows. Port the `@example` rows
//! as `#[test]`s and drop the outer `@given` — same approach used for
//! `test_zig_zagging.py`'s random pass. `HealthCheck.nested_given` has no
//! hegel-rust analogue (Hegel uses static dispatch, so `@given`-nesting
//! can't happen), so the suppression itself is also dropped.

use crate::common::utils::minimal;
use hegel::generators::{self as gs, Generator};

#[test]
fn test_shrinks_to_simple_floats() {
    assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x > 1.0), 2.0);
    assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x > 0.0), 1.0);
}

fn check_can_shrink_in_variable_sized_context(n: usize) {
    let x = minimal(
        gs::vecs(gs::floats::<f64>()).min_size(n),
        |xs: &Vec<f64>| xs.iter().any(|v| *v != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&v| v == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_shrink_in_variable_sized_context_1() {
    check_can_shrink_in_variable_sized_context(1);
}

#[test]
fn test_can_shrink_in_variable_sized_context_2() {
    check_can_shrink_in_variable_sized_context(2);
}

#[test]
fn test_can_shrink_in_variable_sized_context_3() {
    check_can_shrink_in_variable_sized_context(3);
}

#[test]
fn test_can_shrink_in_variable_sized_context_8() {
    check_can_shrink_in_variable_sized_context(8);
}

#[test]
fn test_can_shrink_in_variable_sized_context_10() {
    check_can_shrink_in_variable_sized_context(10);
}

#[test]
fn test_shrinks_downwards_to_integers_example_1_5() {
    let f: f64 = 1.5;
    assert_eq!(
        minimal(gs::floats::<f64>().min_value(f), |_: &f64| true),
        f.ceil()
    );
}

#[test]
fn test_shrinks_downwards_to_integers_example_max() {
    let f: f64 = 1.7976931348623157e308;
    assert_eq!(
        minimal(gs::floats::<f64>().min_value(f), |_: &f64| true),
        f.ceil()
    );
}

#[test]
fn test_shrinks_downwards_to_integers_when_fractional_example_1() {
    let b: f64 = 1.0;
    let max = (1u64 << 53) as f64;
    let g = minimal(
        gs::floats::<f64>()
            .min_value(b)
            .max_value(max)
            .exclude_min(true)
            .exclude_max(true)
            .filter(|x: &f64| x.trunc() != *x),
        |_: &f64| true,
    );
    assert_eq!(g, b + 0.5);
}
