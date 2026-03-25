use crate::common::utils::minimal;
use hegel::generators;

#[test]
fn test_shrinks_to_simple_float_above_1() {
    assert_eq!(
        minimal(generators::floats::<f64>(), |&x: &f64| x > 1.0),
        2.0
    );
}

#[test]
fn test_shrinks_to_simple_float_above_0() {
    assert_eq!(
        minimal(generators::floats::<f64>(), |&x: &f64| x > 0.0),
        1.0
    );
}

#[test]
fn test_can_shrink_in_variable_sized_context_1() {
    let n = 1;
    let x = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(n),
        |x: &Vec<f64>| x.iter().any(|&f| f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_shrink_in_variable_sized_context_2() {
    let n = 2;
    let x = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(n),
        |x: &Vec<f64>| x.iter().any(|&f| f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_shrink_in_variable_sized_context_3() {
    let n = 3;
    let x = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(n),
        |x: &Vec<f64>| x.iter().any(|&f| f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_shrink_in_variable_sized_context_8() {
    let n = 8;
    let x = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(n),
        |x: &Vec<f64>| x.iter().any(|&f| f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_shrink_in_variable_sized_context_10() {
    let n = 10;
    let x = minimal(
        generators::vecs(generators::floats::<f64>()).min_size(n),
        |x: &Vec<f64>| x.iter().any(|&f| f != 0.0),
    );
    assert_eq!(x.len(), n);
    assert_eq!(x.iter().filter(|&&f| f == 0.0).count(), n - 1);
    assert!(x.contains(&1.0));
}

#[test]
fn test_can_find_nan() {
    let x = minimal(generators::floats::<f64>(), |x: &f64| x.is_nan());
    assert!(x.is_nan());
}

#[test]
fn test_can_find_nans() {
    let x = minimal(
        generators::vecs(generators::floats::<f64>()),
        |x: &Vec<f64>| {
            let sum: f64 = x.iter().sum();
            sum.is_nan()
        },
    );
    if x.len() == 1 {
        assert!(x[0].is_nan());
    } else {
        assert!(x.len() >= 2 && x.len() <= 3);
    }
}
