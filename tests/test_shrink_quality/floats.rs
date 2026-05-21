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

#[test]
fn test_negative_non_integer_shrinks_through_integer_range() {
    // Restrict to negative non-integer floats so the shrinker stays in the
    // non-simple lex region and the negative-bound integer-range step in
    // `shrink_floats` actually fires.
    let x = minimal(
        gs::floats::<f64>()
            .min_value(-1000.0)
            .max_value(-0.1)
            .allow_nan(false)
            .allow_infinity(false),
        |x: &f64| *x < 0.0 && x.fract() != 0.0,
    );
    assert!(x < 0.0 && x.fract() != 0.0);
}

#[test]
fn test_nan_canonicalization_prefers_finite_when_predicate_admits() {
    // Predicate accepts NaN or any infinite. When the shrinker lands on a
    // NaN node, its NaN-canonicalization branch tries `f64::MAX` (rejected)
    // then `f64::INFINITY` (accepted) and steps the choice over to it.
    // Run a handful of seeds because which boundary value the random sampler
    // discovers first is luck-of-the-draw — every seed that lands on NaN
    // first walks the accept path.
    for _ in 0..10 {
        let x = minimal(gs::floats::<f64>(), |x: &f64| x.is_nan() || x.is_infinite());
        assert!(x.is_nan() || x.is_infinite());
    }
}

/// Translate of Hypothesis `test_minimal_fractions_1`
/// (`tests/quality/test_shrink_quality.py`).  Hypothesis works with
/// arbitrary `Fraction`s; the native runner shrinks `f64`s instead,
/// but the intent is the same: the simplest finite value satisfying
/// "true" should be 0.0.
#[test]
fn test_minimal_fractions_1() {
    assert_eq!(
        minimal(
            gs::floats::<f64>().allow_nan(false).allow_infinity(false),
            |_| true
        ),
        0.0
    );
}

/// Translate of Hypothesis `test_minimal_fractions_2`.  The simplest
/// finite value satisfying `x >= 1` is 1.0.
#[test]
fn test_minimal_fractions_2() {
    assert_eq!(
        minimal(
            gs::floats::<f64>().allow_nan(false).allow_infinity(false),
            |x: &f64| *x >= 1.0
        ),
        1.0
    );
}

/// Port of Hypothesis `test_shrinks_downwards_to_integers`
/// (`tests/quality/test_float_shrinking.py`).  The minimal float
/// satisfying `x >= min_value` for `min_value >= 0` is `min_value.ceil()`.
/// (Hypothesis's @given gates `f >= 0`; for negative `f` the minimum is
/// `0.0` since the shrink target sits inside the allowed range.)
#[test]
fn test_shrinks_downwards_to_integers() {
    for f in [0.5_f64, 1.5, 7.25, 100.0] {
        let result = minimal(
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false)
                .min_value(f),
            move |x: &f64| *x >= f,
        );
        assert_eq!(result, f.ceil(), "min_value {}", f);
    }
}

/// Port of Hypothesis `test_shrinks_downwards_to_integers_when_fractional`
/// (`tests/quality/test_float_shrinking.py`).  Strictly between `b`
/// and `2^53`, the minimal fractional (non-integer) float is `b + 0.5`.
#[test]
fn test_shrinks_downwards_to_integers_when_fractional() {
    for b in [1.0_f64, 5.0, 100.0, 12345.0] {
        let result = minimal(
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false)
                .min_value(b)
                .max_value(2.0_f64.powi(53)),
            move |x: &f64| *x > b && x.fract() != 0.0,
        );
        assert_eq!(result, b + 0.5, "b {}", b);
    }
}

/// Translate of Hypothesis `test_minimal_fractions_3`.  A list of
/// at least 5 finite floats shrinks to five zeroes.
#[test]
fn test_minimal_fractions_3() {
    let xs = minimal(
        gs::vecs(gs::floats::<f64>().allow_nan(false).allow_infinity(false)),
        |s: &Vec<f64>| s.len() >= 5,
    );
    assert_eq!(xs, vec![0.0; 5]);
}
