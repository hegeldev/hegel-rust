use super::*;

#[test]
fn sign_aware_lte_orders_negative_zero_below_positive_zero() {
    assert!(sign_aware_lte(-0.0, 0.0));
    assert!(!sign_aware_lte(0.0, -0.0));
}

#[test]
fn sign_aware_lte_is_reflexive_on_zero_pair() {
    assert!(sign_aware_lte(0.0, 0.0));
    assert!(sign_aware_lte(-0.0, -0.0));
}

#[test]
fn next_up_of_negative_zero_is_positive_zero() {
    let v = next_up(-0.0);
    assert_eq!(v, 0.0);
    assert!(v.is_sign_positive());
}

#[test]
fn next_down_of_positive_zero_is_negative_zero() {
    let v = next_down(0.0);
    assert_eq!(v, 0.0);
    assert!(v.is_sign_negative());
}

#[test]
fn next_up_of_negative_infinity_is_min_finite() {
    assert_eq!(next_up(f64::NEG_INFINITY), f64::MIN);
}

#[test]
fn next_down_of_positive_infinity_is_max_finite() {
    assert_eq!(next_down(f64::INFINITY), f64::MAX);
}

#[test]
fn next_up_fixed_points() {
    assert!(next_up(f64::NAN).is_nan());
    assert_eq!(next_up(f64::INFINITY), f64::INFINITY);
    assert_eq!(next_down(f64::NEG_INFINITY), f64::NEG_INFINITY);
}

#[test]
fn next_up_and_next_down_step_one_ulp() {
    assert_eq!(next_up(1.0), 1.0 + f64::EPSILON);
    assert_eq!(next_down(1.0 + f64::EPSILON), 1.0);
    assert_eq!(next_up(0.0), f64::from_bits(1));
    assert_eq!(next_down(-0.0), -f64::from_bits(1));
}

#[test]
fn sign_aware_lte_agrees_with_lte_for_non_zero_pairs() {
    let cases = [
        (1.0, 2.0),
        (2.0, 1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (f64::NEG_INFINITY, 0.0),
        (0.0, f64::INFINITY),
        (f64::NEG_INFINITY, f64::INFINITY),
        (-1e-300, 1e-300),
    ];
    for (x, y) in cases {
        assert_eq!(sign_aware_lte(x, y), x <= y, "{x} <= {y}");
    }
}
