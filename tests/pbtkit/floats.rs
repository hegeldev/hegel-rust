//! Ported from resources/pbtkit/tests/test_floats.py.
//!
//! Individually-skipped tests:
//! - `test_floats_database_round_trip` — asserts pbtkit's `count == prev + 2`
//!   replay invariant on `DirectoryDB`; hegel-rust's replay-loop call-count
//!   shape isn't guaranteed to match (same reason as the
//!   `test_core.py::test_reuses_results_from_the_database` whole-file skip).
//! - `test_floats_deserialize_truncated` — feeds pbtkit's
//!   `SerializationTag.FLOAT` byte layout directly to its `DirectoryDB`;
//!   hegel-rust's `NativeDatabase` uses `serialize_choices` with a different
//!   on-disk layout, so these exact byte patterns have no analog (same
//!   reason as the `test_core.py` byte-format-specific skips).
//! - `test_float_sort_key_type_mismatch` — Python dynamic-typing
//!   `sort_key("hello")`; Rust's `sort_key(f64)` signature makes the
//!   non-float case unrepresentable (same pattern as the already-skipped
//!   `test_string_sort_key_type_mismatch`, `test_bytes_sort_key_type_mismatch`,
//!   `test_core.py::test_sort_key_type_mismatch`).
//! - `test_draw_unbounded_float_rejects_nan` — exercises pbtkit's private
//!   `_draw_unbounded_float` helper directly; the Rust equivalent is not
//!   exposed through any public or native-test surface.
//!
//! `test_mantissa_reduction_search` is ported as the embedded shrinker test
//! `shrink_floats_mantissa_reduction_converges` in
//! `tests/embedded/native/shrinker_tests.rs`.

use crate::common::utils::{assert_all_examples, find_any, minimal};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_floats_bounded() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(1.0)
                .allow_nan(false),
        );
        assert!((0.0..=1.0).contains(&f));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_floats_unbounded() {
    // The upstream `monkeypatch.setattr(..., "NAN_DRAW_PROBABILITY", 0.5)` was
    // purely a pbtkit-coverage boost for `_draw_nan`; drop it here. The
    // portable part is that unbounded float draws complete without panicking.
    Hegel::new(|tc| {
        tc.draw(gs::floats::<f64>());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_floats_shrinks_to_zero() {
    let f = minimal(gs::floats::<f64>().allow_nan(false), |f: &f64| *f != 0.0);
    assert_ne!(f, 0.0);
}

#[test]
fn test_floats_bounded_shrinks() {
    let f = minimal(
        gs::floats::<f64>()
            .min_value(1.0)
            .max_value(10.0)
            .allow_nan(false),
        |f: &f64| *f >= 5.0,
    );
    assert!((5.0..=10.0).contains(&f));
}

#[test]
fn test_floats_no_nan() {
    assert_all_examples(gs::floats::<f64>().allow_nan(false), |f: &f64| !f.is_nan());
}

#[test]
fn test_floats_no_infinity() {
    assert_all_examples(
        gs::floats::<f64>().allow_infinity(false).allow_nan(false),
        |f: &f64| f.is_finite(),
    );
}

#[test]
fn test_floats_negative_range() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(-10.0)
                .max_value(-1.0)
                .allow_nan(false),
        );
        assert!((-10.0..=-1.0).contains(&f));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_floats_shrinks_negative() {
    let f = minimal(
        gs::floats::<f64>()
            .min_value(-10.0)
            .max_value(-1.0)
            .allow_nan(false),
        |f: &f64| *f <= -5.0,
    );
    assert!((-10.0..=-5.0).contains(&f));
}

#[test]
fn test_floats_shrinks_truncates() {
    let f = minimal(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(100.0)
            .allow_nan(false),
        |f: &f64| *f > 1.0,
    );
    assert!(f > 1.0 && f <= 100.0);
}

#[test]
fn test_floats_half_bounded() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(0.0)
                .allow_nan(false)
                .allow_infinity(false),
        );
        assert!(f >= 0.0);
        assert!(f.is_finite());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();

    Hegel::new(|tc| {
        let f: f64 = tc.draw(
            gs::floats::<f64>()
                .max_value(0.0)
                .allow_nan(false)
                .allow_infinity(false),
        );
        assert!(f <= 0.0);
        assert!(f.is_finite());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_floats_shrinks_large_or_nan() {
    find_any(gs::floats::<f64>(), |f: &f64| {
        f.is_nan() || f.abs() >= 1e300
    });
}

#[test]
fn test_floats_shrinks_scientific() {
    find_any(gs::floats::<f64>().allow_nan(false), |f: &f64| {
        f.abs() >= 1e10
    });
}

#[test]
fn test_floats_shrinks_negative_exponent() {
    let f = minimal(gs::floats::<f64>().allow_nan(false), |f: &f64| {
        *f > 0.0 && *f < 1e-100
    });
    assert!(f > 0.0 && f < 1e-100);
}

#[test]
fn test_floats_half_bounded_min() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).allow_infinity(false));
        assert!(f >= 0.0);
        assert!(f.is_finite());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_floats_half_bounded_max() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(gs::floats::<f64>().max_value(0.0).allow_infinity(false));
        assert!(f <= 0.0);
        assert!(f.is_finite());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_floats_half_bounded_with_infinity() {
    find_any(gs::floats::<f64>().min_value(0.0), |f: &f64| {
        f.is_infinite()
    });
}

#[test]
fn test_floats_shrinks_non_canonical() {
    find_any(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(10.0)
            .allow_nan(false),
        |f: &f64| *f != 0.0,
    );
}

#[test]
fn test_floats_shrinks_nan_only() {
    let f = minimal(gs::floats::<f64>(), |f: &f64| f.is_nan());
    assert!(f.is_nan());
}

#[test]
fn test_floats_shrinks_nan_to_simpler() {
    let f = minimal(gs::floats::<f64>(), |f: &f64| f.is_nan() || f.is_infinite());
    assert_eq!(f, f64::INFINITY);
}

#[test]
fn test_floats_shrinks_neg_inf() {
    let f = minimal(gs::floats::<f64>(), |f: &f64| f.is_infinite());
    assert_eq!(f, f64::INFINITY);
}

#[test]
fn test_floats_shrinks_neg_inf_to_finite() {
    let f = minimal(gs::floats::<f64>().allow_nan(false), |f: &f64| {
        f.abs() > 1e300
    });
    assert!(f.is_finite() && f.abs() > 1e300);
}

#[test]
fn test_floats_shrinks_inf_to_finite() {
    let f = minimal(gs::floats::<f64>().allow_nan(false), |f: &f64| *f > 1e300);
    assert!(f.is_finite() && f > 1e300);
}

#[test]
fn test_floats_shrinks_large_exponent() {
    let f = minimal(gs::floats::<f64>().allow_nan(false), |f: &f64| *f >= 1e15);
    assert!(f >= 1e15);
}

#[test]
fn test_floats_shrinks_small_positive() {
    let f = minimal(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(1.0)
            .allow_nan(false),
        |f: &f64| 0.01 < *f && *f < 0.5,
    );
    assert!(0.01 < f && f < 0.5);
}

#[test]
fn test_shrinks_float_with_large_fractional() {
    let f = minimal(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(0.5)
            .allow_nan(false),
        |f: &f64| 0.001 < *f && *f < 0.5,
    );
    assert!(0.001 < f && f < 0.5);
}

#[test]
fn test_float_shrinks_across_exponent_boundary() {
    // The shrinker must cross exponent boundaries to reach values just below
    // -2.0 — pbtkit shrinks to `-2.0 - 1 ULP`; hegel-rust's shrinker stops at
    // the simpler `-3.0` (exponent 1024, mantissa 2^51). Both satisfy
    // `-3.0 <= v < -2.0`; the test guards against the pre-regression shrinker
    // getting stuck at larger magnitudes like `-4.0`.
    let f = minimal(
        gs::floats::<f64>().allow_nan(false).allow_infinity(false),
        |v: &f64| *v < -2.0,
    );
    assert!((-3.0..-2.0).contains(&f));
}

// ── FloatChoice internals (native-gated) ────────────────────────────────────

#[cfg(feature = "native")]
mod float_choice_internals {
    use hegel::__native_test_internals::{BigUint, FloatChoice};

    fn fc(min_value: f64, max_value: f64, allow_nan: bool, allow_infinity: bool) -> FloatChoice {
        FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
        }
    }

    /// Largest finite-float index in the global ordering used by
    /// `FloatChoice::from_index` on an unbounded allow-all `FloatChoice`.
    /// The max-index finite float in Hypothesis's lex ordering is the
    /// subnormal `f64::from_bits((1<<52)-1)` (biased_exp=0 with every
    /// mantissa bit set — after the bit-reversal `update_mantissa` applies,
    /// its encoded mantissa is also all-ones). Its lex index is
    /// `(1<<63) | (2046<<52) | ((1<<52)-1)`. `float_global_rank` then packs
    /// sign into the low bit, so the last finite rank is the negative
    /// variant: `max_lex * 2 + 1`.
    fn max_finite_index() -> BigUint {
        let max_lex: u64 = (1u64 << 63) | (2046u64 << 52) | ((1u64 << 52) - 1);
        BigUint::from(max_lex) * BigUint::from(2u32) + BigUint::from(1u32)
    }

    #[test]
    fn test_floats_simplest_positive_range() {
        assert_eq!(fc(1.0, 10.0, false, true).simplest(), 1.0);
        assert_eq!(fc(-10.0, -1.0, false, true).simplest(), -1.0);
        assert_eq!(fc(-1.0, 1.0, false, true).simplest(), 0.0);
    }

    #[test]
    fn test_floats_validate_edge_cases() {
        let kind = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
        assert!(kind.validate(f64::NAN));
        assert!(kind.validate(f64::INFINITY));
        assert!(kind.validate(f64::NEG_INFINITY));
        assert!(kind.validate(0.0));

        let no_nan = fc(f64::NEG_INFINITY, f64::INFINITY, false, true);
        assert!(!no_nan.validate(f64::NAN));

        let no_inf = fc(f64::NEG_INFINITY, f64::INFINITY, true, false);
        assert!(!no_inf.validate(f64::INFINITY));
        assert!(!no_inf.validate(f64::NEG_INFINITY));

        let bounded = fc(0.0, 1.0, false, false);
        assert!(!bounded.validate(2.0));
        assert!(bounded.validate(0.5));
    }

    #[test]
    fn test_floats_sort_key_ordering() {
        let kind = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
        // Finite < +inf < -inf < NaN
        assert!(kind.sort_key(0.0) < kind.sort_key(f64::INFINITY));
        assert!(kind.sort_key(f64::INFINITY) < kind.sort_key(f64::NEG_INFINITY));
        assert!(kind.sort_key(f64::NEG_INFINITY) < kind.sort_key(f64::NAN));
        // Simpler finite values first.
        assert!(kind.sort_key(1.0) < kind.sort_key(2.0));
        assert!(kind.sort_key(1.0) < kind.sort_key(1.5));
        assert!(kind.sort_key(1.0) < kind.sort_key(-1.0));
    }

    #[test]
    fn test_float_index_subnormals() {
        // 5e-324 is the smallest positive subnormal; under Hypothesis's lex
        // ordering it ranks after every normal finite value.
        let kind = fc(f64::NEG_INFINITY, f64::INFINITY, false, false);
        let idx = kind.to_index(5e-324);
        assert!(idx > kind.to_index(1e300));
        let back = kind.from_index(idx).unwrap();
        assert_eq!(back, 5e-324);
    }

    #[test]
    fn test_float_index_bounded_simplest() {
        // Range where simplest is found via the power-of-2 search.
        let kind = fc(0.5, 2.0, false, false);
        assert_eq!(kind.simplest(), 1.0);
        assert_eq!(kind.to_index(1.0), BigUint::from(0u32));
    }

    #[test]
    fn test_float_from_index_inf() {
        let kind = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
        let max = max_finite_index();
        assert_eq!(
            kind.from_index(&max + BigUint::from(1u32)),
            Some(f64::INFINITY)
        );
        assert_eq!(
            kind.from_index(&max + BigUint::from(2u32)),
            Some(f64::NEG_INFINITY)
        );
        let v = kind.from_index(&max + BigUint::from(3u32)).unwrap();
        assert!(v.is_nan());
    }

    #[test]
    fn test_float_from_index_past_max() {
        let kind = fc(0.0, 1.0, false, false);
        let huge = BigUint::from(10u32).pow(20);
        assert_eq!(kind.from_index(huge), None);
        // Bounded range where base + index exceeds the max finite index.
        let kind2 = fc(1e300, 2e300, false, false);
        assert_eq!(kind2.from_index(max_finite_index()), None);
    }

    #[test]
    fn test_float_from_index_out_of_bounded_range() {
        // Index 1 on a positive-only range falls on a value outside the range
        // (e.g. -0.0 or -1.0 in the global ordering), so validate rejects.
        let kind = fc(1.0, 2.0, false, false);
        assert_eq!(kind.from_index(BigUint::from(1u32)), None);
    }

    #[test]
    fn test_float_from_index_none_paths() {
        let kind = fc(f64::NEG_INFINITY, f64::INFINITY, false, false);
        let max = max_finite_index();
        // offset == 1, allow_infinity=false -> None (not +inf).
        assert_eq!(kind.from_index(&max + BigUint::from(1u32)), None);
        // Well past all infs/NaNs, allow_nan=false -> None.
        let huge = BigUint::from(10u32).pow(20);
        assert_eq!(kind.from_index(huge), None);
        // Bounded range where base + index exceeds the max finite index.
        let kind2 = fc(1e300, 2e300, false, false);
        assert_eq!(kind2.from_index(max_finite_index()), None);
    }

    #[test]
    fn test_float_simplest_with_inf_bounds() {
        assert_eq!(
            fc(f64::NEG_INFINITY, f64::INFINITY, false, false).simplest(),
            0.0
        );
        assert_eq!(fc(1.0, f64::INFINITY, false, false).simplest(), 1.0);
        assert_eq!(fc(f64::NEG_INFINITY, -1.0, false, false).simplest(), -1.0);
    }

    #[test]
    fn test_float_simplest_tiny_range() {
        // No power of 2 in range; simplest falls back to the lower boundary.
        assert_eq!(fc(1.5, 1.75, false, false).simplest(), 1.5);
    }

    #[test]
    fn test_float_simplest_subnormal_range() {
        // pbtkit's ordering picks `1e-323` (smaller mantissa under its raw
        // `(exp_rank, mantissa, sign)` ordering). hegel-rust uses Hypothesis's
        // lex ordering, which bit-reverses subnormal mantissas; that makes
        // `2e-323` (mantissa 4 → reversed 1<<49) simpler than `1e-323`
        // (mantissa 2 → reversed 1<<50). The test still pins that simplest
        // exhausts the power-of-2 search and falls back to a range boundary.
        assert_eq!(fc(1e-323, 2e-323, false, false).simplest(), 2e-323);
    }

    #[test]
    fn test_float_simplest_finds_power_of_two() {
        // Power-of-2 search finds 1.0 = 2^0 inside [0.5, 2.0].
        assert_eq!(fc(0.5, 2.0, false, false).simplest(), 1.0);
    }

    #[test]
    fn test_float_negative_zero_simplest() {
        // Range contains 0.0 (0.0 >= -1.0 && 0.0 <= 0.0), so simplest is 0.0.
        assert_eq!(fc(-1.0, 0.0, false, false).simplest(), 0.0);
    }

    #[test]
    fn test_float_choice_unit() {
        // pbtkit's `(exp_rank, mantissa, sign)` ordering puts `-0.0` at
        // index 1 (next to `0.0`). hegel-rust uses Hypothesis's lex
        // ordering, in which the index immediately after `0.0` is `1.0`
        // (integer encoding), so unit here is `1.0`.
        assert_eq!(fc(-10.0, 10.0, false, false).unit(), 1.0);
        // Negative-only range: simplest is the boundary closest to zero.
        let fc_neg = fc(-10.0, -5.0, false, false);
        assert_eq!(fc_neg.simplest(), -5.0);
        // Single-value range: unit falls back to simplest.
        assert_eq!(fc(5.0, 5.0, false, false).unit(), 5.0);
    }
}
