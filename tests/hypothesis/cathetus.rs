//! Ported from hypothesis-python/tests/cover/test_cathetus.py

#![cfg(feature = "native")]

use hegel::__native_test_internals::cathetus;
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[test]
fn test_cathetus_subnormal_underflow() {
    // Smallest positive subnormal (f64::MIN_POSITIVE * f64::EPSILON).
    let u = f64::from_bits(1);
    let h = 5.0 * u;
    let a = 4.0 * u;
    assert_eq!(cathetus(h, a), 3.0 * u);
}

#[test]
fn test_cathetus_simple_underflow() {
    let a = f64::MIN_POSITIVE;
    let h = a * 2.0_f64.sqrt();
    let b = cathetus(h, a);
    assert!(
        b > 0.0,
        "expecting positive cathetus({h:e}, {a:e}), got {b:e}"
    );
}

#[test]
fn test_cathetus_huge_no_overflow() {
    let h = f64::MAX;
    let a = h / 2.0_f64.sqrt();
    let b = cathetus(h, a);
    assert!(
        b.is_finite(),
        "expecting finite cathetus({h:e}, {a:e}), got {b:e}"
    );
}

#[test]
fn test_cathetus_large_no_overflow() {
    let h = f64::MAX / 3.0;
    let a = h / 2.0_f64.sqrt();
    let b = cathetus(h, a);
    assert!(
        b.is_finite(),
        "expecting finite cathetus({h:e}, {a:e}), got {b:e}"
    );
}

#[test]
fn test_cathetus_nan() {
    // (h, a) pairs that should all produce NaN.
    let cases: &[(f64, f64)] = &[
        // NaN hypot
        (f64::NAN, 3.0),
        (f64::NAN, 0.0),
        (f64::NAN, f64::INFINITY),
        (f64::NAN, f64::NAN),
        // Infeasible (h < a)
        (2.0, 3.0),
        (2.0, -3.0),
        (2.0, f64::INFINITY),
        (2.0, f64::NAN),
        // Surprisingly consistent with c99 hypot()
        (f64::INFINITY, f64::INFINITY),
    ];
    for &(h, a) in cases {
        let b = cathetus(h, a);
        assert!(b.is_nan(), "cathetus({h}, {a}) = {b}, expected NaN");
    }
}

#[test]
fn test_cathetus_infinite() {
    let cases: &[(f64, f64)] = &[
        (f64::INFINITY, 3.0),
        (f64::INFINITY, -3.0),
        (f64::INFINITY, 0.0),
        (f64::INFINITY, f64::NAN),
    ];
    for &(h, a) in cases {
        let b = cathetus(h, a);
        assert!(b.is_infinite(), "cathetus({h}, {a}) = {b}, expected inf");
    }
}

#[test]
fn test_cathetus_signs() {
    let cases: &[(f64, f64, f64)] = &[
        (-5.0, 4.0, 3.0),
        (5.0, -4.0, 3.0),
        (-5.0, -4.0, 3.0),
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 1.0),
    ];
    for &(h, a, expected) in cases {
        let got = cathetus(h, a);
        let tol = expected.abs() * f64::EPSILON;
        assert!(
            (got - expected).abs() <= tol,
            "cathetus({h}, {a}) = {got}, expected {expected} (tol {tol})"
        );
    }
}

#[test]
fn test_cathetus_always_leq_hypot() {
    let h_gen = gs::one_of(vec![
        gs::floats::<f64>().min_value(0.0).boxed(),
        gs::floats::<f64>()
            .min_value(1e308)
            .allow_infinity(false)
            .boxed(),
    ]);
    let a_gen = gs::one_of(vec![
        gs::floats::<f64>()
            .min_value(0.0)
            .allow_infinity(false)
            .boxed(),
        gs::floats::<f64>().min_value(0.0).max_value(1e250).boxed(),
    ]);
    Hegel::new(move |tc| {
        let h: f64 = tc.draw(&h_gen);
        let a: f64 = tc.draw(&a_gen);
        tc.assume(h >= a);
        let b = cathetus(h, a);
        assert!(0.0 <= b && b <= h, "cathetus({h:e}, {a:e}) = {b:e}");
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_pythagorean_triples() {
    let triples: &[(f64, f64, f64)] = &[
        (3.0, 4.0, 5.0),
        (5.0, 12.0, 13.0),
        (8.0, 15.0, 17.0),
        (7.0, 24.0, 25.0),
        (20.0, 21.0, 29.0),
        (12.0, 35.0, 37.0),
        (9.0, 40.0, 41.0),
        (28.0, 45.0, 53.0),
        (11.0, 60.0, 61.0),
        (16.0, 63.0, 65.0),
        (33.0, 56.0, 65.0),
        (48.0, 55.0, 73.0),
        (13.0, 84.0, 85.0),
        (36.0, 77.0, 85.0),
        (39.0, 80.0, 89.0),
        (65.0, 72.0, 97.0),
        (20.0, 99.0, 101.0),
        (60.0, 91.0, 109.0),
        (15.0, 112.0, 113.0),
        (44.0, 117.0, 125.0),
        (88.0, 105.0, 137.0),
        (17.0, 144.0, 145.0),
        (24.0, 143.0, 145.0),
        (51.0, 140.0, 149.0),
        (85.0, 132.0, 157.0),
        (119.0, 120.0, 169.0),
        (52.0, 165.0, 173.0),
        (19.0, 180.0, 181.0),
        (57.0, 176.0, 185.0),
        (104.0, 153.0, 185.0),
        (95.0, 168.0, 193.0),
        (28.0, 195.0, 197.0),
        (84.0, 187.0, 205.0),
        (133.0, 156.0, 205.0),
        (21.0, 220.0, 221.0),
        (140.0, 171.0, 221.0),
        (60.0, 221.0, 229.0),
        (105.0, 208.0, 233.0),
        (120.0, 209.0, 241.0),
        (32.0, 255.0, 257.0),
        (23.0, 264.0, 265.0),
        (96.0, 247.0, 265.0),
        (69.0, 260.0, 269.0),
        (115.0, 252.0, 277.0),
        (160.0, 231.0, 281.0),
        (161.0, 240.0, 289.0),
        (68.0, 285.0, 293.0),
    ];
    for &(a, b, h) in triples {
        let hypot = a.hypot(b);
        let h_tol = h.abs() * f64::EPSILON;
        assert!(
            (hypot - h).abs() <= h_tol,
            "hypot({a}, {b}) = {hypot}, expected {h} (tol {h_tol})"
        );
        let got_b = cathetus(h, a);
        let b_tol = b.abs() * f64::EPSILON;
        assert!(
            (got_b - b).abs() <= b_tol,
            "cathetus({h}, {a}) = {got_b}, expected {b} (tol {b_tol})"
        );
    }
}
