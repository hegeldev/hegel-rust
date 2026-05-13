use crate::common::utils::minimal;
use hegel::generators as gs;

#[test]
fn test_shrinks_to_simple_float_above_0() {
    assert_eq!(
        minimal(gs::floats::<f64>().allow_nan(false), |x: &f64| *x > 0.0),
        1.0
    );
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
