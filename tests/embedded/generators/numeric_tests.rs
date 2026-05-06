use super::*;

#[test]
fn sign_aware_lte_neg_zero_lt_pos_zero() {
    assert!(sign_aware_lte(-0.0_f64, 0.0_f64));
}

#[test]
fn sign_aware_lte_pos_zero_not_lte_neg_zero() {
    assert!(!sign_aware_lte(0.0_f64, -0.0_f64));
}

#[test]
fn sign_aware_lte_pos_zero_lte_pos_zero() {
    assert!(sign_aware_lte(0.0_f64, 0.0_f64));
}

#[test]
fn sign_aware_lte_neg_zero_lte_neg_zero() {
    assert!(sign_aware_lte(-0.0_f64, -0.0_f64));
}

#[test]
fn sign_aware_lte_falls_back_to_lte_for_nonzero() {
    assert!(sign_aware_lte(1.0_f64, 2.0_f64));
    assert!(!sign_aware_lte(2.0_f64, 1.0_f64));
    assert!(sign_aware_lte(-1.0_f64, 0.0_f64));
    assert!(sign_aware_lte(-1.0_f64, -0.0_f64));
    assert!(!sign_aware_lte(0.0_f64, -1.0_f64));
}

#[test]
fn sign_aware_lte_works_for_f32() {
    assert!(sign_aware_lte(-0.0_f32, 0.0_f32));
    assert!(!sign_aware_lte(0.0_f32, -0.0_f32));
    assert!(sign_aware_lte(1.0_f32, 2.0_f32));
}
