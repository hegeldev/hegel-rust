use std::collections::HashSet;

use crate::common::utils::{find_any, minimal};
use hegel::generators;

// From test_discovery_ability.py

#[test]
fn test_can_produce_positive_infinity() {
    find_any(generators::floats::<f64>(), |&x| x == f64::INFINITY);
}

#[test]
fn test_can_produce_negative_infinity() {
    find_any(generators::floats::<f64>(), |&x| x == f64::NEG_INFINITY);
}

#[test]
fn test_can_produce_nan() {
    find_any(generators::floats::<f64>(), |x| x.is_nan());
}

#[test]
fn test_can_produce_floats_near_left() {
    find_any(
        generators::floats::<f64>().min_value(0.0).max_value(1.0),
        |&t| t < 0.2,
    );
}

#[test]
fn test_can_produce_floats_near_right() {
    find_any(
        generators::floats::<f64>().min_value(0.0).max_value(1.0),
        |&t| t > 0.8,
    );
}

#[test]
fn test_can_produce_floats_in_middle() {
    find_any(
        generators::floats::<f64>().min_value(0.0).max_value(1.0),
        |&t| (0.2..=0.8).contains(&t),
    );
}

#[test]
fn test_mostly_sensible_floats() {
    find_any(generators::floats::<f64>(), |&t| t + 1.0 > t);
}

#[test]
fn test_mostly_largish_floats() {
    find_any(generators::floats::<f64>(), |&x| x > 0.0 && x + 1.0 > 1.0);
}

// From test_floating.py

#[test]
fn test_inversion_is_imperfect() {
    find_any(generators::floats::<f64>(), |&x| {
        x != 0.0 && !x.is_nan() && !x.is_infinite() && x * (1.0 / x) != 1.0
    });
}

#[test]
fn test_can_find_floats_that_do_not_round_trip_through_strings() {
    find_any(generators::floats::<f64>(), |x| {
        let s = format!("{}", x);
        match s.parse::<f64>() {
            Ok(y) => *x != y || (x.is_nan() != y.is_nan()),
            Err(_) => true,
        }
    });
}

// From test_simple_numbers.py — float shrinking/findability

#[test]
fn test_minimal_float_is_zero() {
    assert_eq!(minimal(generators::floats::<f64>(), |_| true), 0.0);
}

#[test]
fn test_minimals_boundary_floats() {
    assert_eq!(
        minimal(
            generators::floats::<f64>().min_value(-1.0).max_value(1.0),
            |_| true
        ),
        0.0
    );
}

#[test]
fn test_minimal_non_boundary_float() {
    let x = minimal(
        generators::floats::<f64>().min_value(1.0).max_value(9.0),
        |&x| x > 2.0,
    );
    assert_eq!(x, 3.0);
}

#[test]
fn test_minimal_asymmetric_bounded_float() {
    assert_eq!(
        minimal(
            generators::floats::<f64>().min_value(1.1).max_value(1.6),
            |_| true
        ),
        1.5
    );
}

#[test]
fn test_negative_floats_simplify_to_zero() {
    assert_eq!(minimal(generators::floats::<f64>(), |&x| x <= -1.0), -1.0);
}

#[test]
fn test_minimal_infinite_float_is_positive() {
    assert_eq!(
        minimal(generators::floats::<f64>(), |x| x.is_infinite()),
        f64::INFINITY
    );
}

#[test]
fn test_can_minimal_infinite_negative_float() {
    let x = minimal(generators::floats::<f64>(), |&x| x < -f64::MAX);
    assert!(x.is_infinite() && x < 0.0);
}

#[test]
fn test_can_minimal_float_on_boundary_of_representable() {
    let x = minimal(generators::floats::<f64>(), |&x| {
        x + 1.0 == x && !x.is_infinite()
    });
    assert!(x.is_finite());
    assert_eq!(x + 1.0, x);
}

#[test]
fn test_minimize_nan() {
    let x = minimal(generators::floats::<f64>(), |x| x.is_nan());
    assert!(x.is_nan());
}

#[test]
fn test_minimize_very_large_float() {
    let t = f64::MAX / 2.0;
    assert_eq!(minimal(generators::floats::<f64>(), move |&x| x >= t), t);
}

#[test]
fn test_minimal_fractional_float() {
    assert_eq!(minimal(generators::floats::<f64>(), |&x| x >= 1.5), 2.0);
}

#[test]
fn test_minimal_small_sum_float_list() {
    let xs = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(5),
        |x: &Vec<f64>| x.iter().sum::<f64>() >= 1.0,
    );
    assert_eq!(xs, vec![0.0, 0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn test_list_of_fractional_float() {
    let result = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(5),
        |x: &Vec<f64>| x.iter().filter(|&&t| t >= 1.5).count() >= 5,
    );
    let unique: HashSet<u64> = result.iter().map(|&f| f.to_bits()).collect();
    assert_eq!(unique, HashSet::from([2.0f64.to_bits()]));
}

#[test]
fn test_bounds_are_respected() {
    assert_eq!(
        minimal(generators::floats::<f64>().min_value(1.0), |_| true),
        1.0
    );
    assert_eq!(
        minimal(generators::floats::<f64>().max_value(-1.0), |_| true),
        -1.0
    );
}

#[test]
fn test_floats_from_zero_have_reasonable_range_0() {
    assert_eq!(
        minimal(generators::floats::<f64>().min_value(0.0), |&x| x >= 1.0),
        1.0
    );
}

#[test]
fn test_floats_from_zero_have_reasonable_range_3() {
    assert_eq!(
        minimal(generators::floats::<f64>().min_value(0.0), |&x| x >= 1000.0),
        1000.0
    );
}

#[test]
fn test_floats_from_zero_have_reasonable_range_6() {
    assert_eq!(
        minimal(generators::floats::<f64>().min_value(0.0), |&x| x
            >= 1_000_000.0),
        1_000_000.0
    );
}

#[test]
fn test_explicit_allow_nan() {
    let x = minimal(generators::floats::<f64>(), |x| x.is_nan());
    assert!(x.is_nan());
}

#[test]
fn test_one_sided_contains_infinity() {
    let x = minimal(generators::floats::<f64>().min_value(1.0), |x| {
        x.is_infinite()
    });
    assert!(x.is_infinite());
}

#[test]
fn test_can_minimal_float_far_from_integral() {
    let x = minimal(generators::floats::<f64>(), |&x| {
        x.is_finite() && (x * (1u64 << 32) as f64).fract() != 0.0
    });
    assert!(x.is_finite());
}
