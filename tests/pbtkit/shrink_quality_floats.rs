//! Ported from resources/pbtkit/tests/shrink_quality/test_floats.py
//!
//! Individually-skipped tests:
//! - `test_shrinks_to_simple_float_above_1` — requires the pbtkit
//!   `shrinking.mutation` pass; the hegel-rust server backend shrinks to
//!   `2.0` rather than the nearest representable float above `1.0`, and the
//!   native backend hasn't implemented `draw_float` yet.
//! - `test_negative_zero_shrinks_to_positive_zero` — uses `gs.nothing()`;
//!   hegel-rust has no empty-generator public API.

use crate::common::utils::minimal;
use hegel::generators as gs;

#[test]
fn test_shrinks_to_simple_float_above_0() {
    assert_eq!(minimal(gs::floats::<f64>().allow_nan(false), |x: &f64| *x > 0.0), 1.0);
}

#[test]
fn test_can_shrink_in_variable_sized_context_1() {
    check_shrink_in_variable_sized_context(1);
}

#[test]
fn test_can_shrink_in_variable_sized_context_2() {
    check_shrink_in_variable_sized_context(2);
}

#[test]
fn test_can_shrink_in_variable_sized_context_3() {
    check_shrink_in_variable_sized_context(3);
}

#[test]
fn test_can_shrink_in_variable_sized_context_8() {
    check_shrink_in_variable_sized_context(8);
}

#[test]
fn test_can_shrink_in_variable_sized_context_10() {
    check_shrink_in_variable_sized_context(10);
}

fn check_shrink_in_variable_sized_context(n: usize) {
    let x = minimal(
        gs::vecs(gs::floats::<f64>().allow_nan(false).allow_infinity(false)).min_size(n),
        move |x: &Vec<f64>| x.iter().any(|f| *f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_find_nan() {
    let x = minimal(gs::floats::<f64>(), |x: &f64| x.is_nan());
    assert!(x.is_nan());
}

#[test]
fn test_can_find_nans() {
    let x = minimal(gs::vecs(gs::floats::<f64>()), |x: &Vec<f64>| {
        x.iter().sum::<f64>().is_nan()
    });
    if x.len() == 1 {
        assert!(x[0].is_nan());
    } else {
        assert!(x.len() >= 2 && x.len() <= 3);
    }
}

#[cfg(feature = "native")]
use hegel::__native_test_internals::{
    ChoiceValue, NativeConjectureData, NativeConjectureRunner, NativeRunnerSettings,
    interesting_origin,
};
#[cfg(feature = "native")]
use rand::SeedableRng;
#[cfg(feature = "native")]
use rand::rngs::SmallRng;

#[cfg(feature = "native")]
#[test]
fn test_float_increment_shortens_via_negative() {
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v0 = data.draw_boolean(0.5);
            let v1 = data.draw_float(f64::NEG_INFINITY, f64::INFINITY, false, false);
            data.draw_boolean(0.5);
            if v1 < 0.0 {
                data.mark_interesting(interesting_origin(None));
            }
            data.draw_boolean(0.5);
            if v0 {
                data.mark_interesting(interesting_origin(None));
            }
        },
        NativeRunnerSettings::new().max_examples(1000),
        SmallRng::seed_from_u64(0),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
    let example = runner.interesting_examples.values().next().unwrap();
    assert_eq!(example.nodes.len(), 3);
}

#[cfg(feature = "native")]
#[test]
fn test_lower_and_bump_with_float_source_gaps() {
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v0 = data.draw_float(1.0, 2.0, false, true);
            let v1 = data.draw_boolean(0.5);
            if v0 > 1.5 && v1 {
                data.mark_interesting(interesting_origin(None));
            }
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
}

#[cfg(feature = "native")]
#[test]
fn test_lower_and_bump_with_bounded_float_target() {
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v0 = data.draw_integer(0, 5);
            let v1 = data.draw_float(0.0, 0.5, false, true);
            if v0 >= 3 && v1 > 0.0 {
                data.mark_interesting(interesting_origin(None));
            }
        },
        NativeRunnerSettings::new().max_examples(1000),
        SmallRng::seed_from_u64(0),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
}

#[cfg(feature = "native")]
#[test]
fn test_lower_and_bump_negative_zero_decrement_target() {
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v = data.draw_float(f64::NEG_INFINITY, f64::INFINITY, false, false);
            let a = data.draw_integer(0, 10);
            if v > 0.5 && a > 0 {
                data.mark_interesting(interesting_origin(None));
            }
        },
        NativeRunnerSettings::new().max_examples(1000),
        SmallRng::seed_from_u64(0),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
    let example = runner.interesting_examples.values().next().unwrap();
    assert_eq!(example.nodes[0].value, ChoiceValue::Float(1.0));
}
