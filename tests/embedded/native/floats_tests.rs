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
