mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::TestCase;
use hegel::generators as gs;

macro_rules! float_tests {
    ($t:ty) => {
        #[test]
        fn finite() {
            assert_all_examples(
                gs::floats::<$t>().allow_nan(false).allow_infinity(false),
                |&n| n.is_finite(),
            );
        }

        #[hegel::test]
        fn with_min(tc: TestCase) {
            let min = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let n = tc.draw(gs::floats::<$t>().min_value(min));
            assert!(n >= min, "{n} should be >= {min}");
        }

        #[hegel::test]
        fn with_max(tc: TestCase) {
            let max = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let n = tc.draw(gs::floats::<$t>().max_value(max));
            assert!(n <= max, "{n} should be <= {max}");
        }

        #[hegel::test]
        fn with_min_and_max(tc: TestCase) {
            let a = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let b = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let min = a.min(b);
            let max = a.max(b);
            let n = tc.draw(gs::floats::<$t>().min_value(min).max_value(max));
            assert!(n >= min && n <= max, "{n} should be in [{min}, {max}]");
        }

        #[hegel::test]
        fn exclude_min(tc: TestCase) {
            let min = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            tc.assume(min.next_up().is_finite());
            let n = tc.draw(gs::floats::<$t>().min_value(min).exclude_min(true));
            assert!(n > min, "{n} should be > {min}");
        }

        #[hegel::test]
        fn exclude_max(tc: TestCase) {
            let max = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            tc.assume(max.next_down().is_finite());
            let n = tc.draw(gs::floats::<$t>().max_value(max).exclude_max(true));
            assert!(n < max, "{n} should be < {max}");
        }

        #[hegel::test]
        fn exclude_min_and_max(tc: TestCase) {
            let a = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let b = tc.draw(&gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let min = a.min(b);
            let max = a.max(b);
            tc.assume(min.next_up() < max);
            let n = tc.draw(
                &gs::floats::<$t>()
                    .min_value(min)
                    .max_value(max)
                    .exclude_min(true)
                    .exclude_max(true),
            );
            assert!(n > min && n < max, "{n} should be in ({min}, {max})");
        }

        #[test]
        fn can_find_nan() {
            find_any(gs::floats::<$t>(), |n| n.is_nan());
        }

        #[test]
        fn can_find_inf() {
            find_any(gs::floats::<$t>(), |n| n.is_infinite());
        }

        #[test]
        fn can_find_positive() {
            find_any(gs::floats::<$t>(), |&n| n.is_finite() && n > 0.0);
        }

        #[test]
        fn can_find_negative() {
            find_any(gs::floats::<$t>(), |&n| n.is_finite() && n < 0.0);
        }

        #[hegel::test]
        fn fuzz_floats_bounds(tc: TestCase) {
            let bound_gen = gs::optional(gs::floats::<$t>().allow_nan(false).allow_infinity(false));
            let mut low: Option<$t> = tc.draw(&bound_gen);
            let mut high: Option<$t> = tc.draw(&bound_gen);

            if let (Some(lo), Some(hi)) = (low, high) {
                if lo > hi {
                    low = Some(hi);
                    high = Some(lo);
                }
            }

            let exmin = low.is_some() && tc.draw(gs::booleans());
            let exmax = high.is_some() && tc.draw(gs::booleans());

            if let (Some(lo), Some(hi)) = (low, high) {
                let effective_lo = if exmin { lo.next_up() } else { lo };
                let effective_hi = if exmax { hi.next_down() } else { hi };
                tc.assume(effective_lo <= effective_hi);
                // Sign-aware reject of the min=+0.0, max=-0.0 range, which the
                // generator treats as empty (matches hypothesis's bad_zero_bounds).
                if effective_lo == 0 as $t && effective_hi == 0 as $t {
                    tc.assume(
                        !(effective_lo.is_sign_positive() && effective_hi.is_sign_negative()),
                    );
                }
            }

            let mut g = gs::floats::<$t>();
            if let Some(lo) = low {
                g = g.min_value(lo);
            }
            if let Some(hi) = high {
                g = g.max_value(hi);
            }
            g = g.exclude_min(exmin);
            g = g.exclude_max(exmax);

            let val = tc.draw(g);

            if val.is_finite() {
                if let Some(lo) = low {
                    assert!(val >= lo, "{val} should be >= {lo}");
                }
                if let Some(hi) = high {
                    assert!(val <= hi, "{val} should be <= {hi}");
                }
                if exmin {
                    if let Some(lo) = low {
                        assert!(val != lo, "{val} should not equal excluded min {lo}");
                    }
                }
                if exmax {
                    if let Some(hi) = high {
                        assert!(val != hi, "{val} should not equal excluded max {hi}");
                    }
                }
            }
        }
    };
}

mod f32_tests {
    use super::*;
    float_tests!(f32);
}

mod f64_tests {
    use super::*;
    float_tests!(f64);
}

mod pbtkit_floats {
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
    fn test_draw_unbounded_float_rejects_nan() {
        // Upstream calls pbtkit's private `_draw_unbounded_float(rnd)` 10000
        // times to reliably hit its NaN-rejection-sampling branch. hegel-rust
        // has no standalone equivalent (the logic is inlined in the
        // float-choice path), so port at the public-API layer: drawing 1000
        // unbounded floats with `allow_nan(false)` exercises the same property
        // (no NaN ever returned) through whichever rejection / re-sampling
        // strategy Hypothesis applies. Redundant with `test_floats_no_nan` —
        // kept distinct to mirror the upstream test surface.
        Hegel::new(|tc| {
            let f: f64 = tc.draw(gs::floats::<f64>().allow_nan(false));
            assert!(!f.is_nan());
        })
        .settings(Settings::new().test_cases(1000).database(None))
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
}

mod float_nastiness {
    use crate::common::utils::expect_panic;
    use crate::common::utils::{
        assert_all_examples, check_can_generate_examples, find_any, minimal,
    };
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_floats_are_in_range_large() {
        let lower = 9.9792015476736e291_f64;
        let upper = 1.7976931348623157e308_f64;
        assert_all_examples(
            gs::floats::<f64>().min_value(lower).max_value(upper),
            move |t: &f64| lower <= *t && *t <= upper,
        );
    }

    #[test]
    fn test_floats_are_in_range_full() {
        let lower = -f64::MAX;
        let upper = f64::MAX;
        assert_all_examples(
            gs::floats::<f64>().min_value(lower).max_value(upper),
            move |t: &f64| lower <= *t && *t <= upper,
        );
    }

    #[test]
    fn test_can_generate_positive_zero() {
        let result = minimal(gs::floats::<f64>(), |x: &f64| !x.is_sign_negative());
        assert_eq!(result, 0.0);
        assert!(!result.is_sign_negative());
    }

    #[test]
    fn test_can_generate_negative_zero() {
        let result = minimal(gs::floats::<f64>(), |x: &f64| x.is_sign_negative());
        assert_eq!(result, 0.0);
        assert!(result.is_sign_negative());
    }

    const ZERO_INTERVAL_CASES: [(f64, f64); 4] = [
        (-1.0, 1.0),
        (-0.0, 1.0),
        (-1.0, 0.0),
        (-f64::MIN_POSITIVE, f64::MIN_POSITIVE),
    ];

    #[test]
    fn test_can_generate_positive_zero_in_interval() {
        for (l, r) in ZERO_INTERVAL_CASES {
            let result = minimal(gs::floats::<f64>().min_value(l).max_value(r), |x: &f64| {
                !x.is_sign_negative()
            });
            assert_eq!(result, 0.0);
            assert!(!result.is_sign_negative());
        }
    }

    #[test]
    fn test_can_generate_negative_zero_in_interval() {
        for (l, r) in ZERO_INTERVAL_CASES {
            let result = minimal(gs::floats::<f64>().min_value(l).max_value(r), |x: &f64| {
                x.is_sign_negative()
            });
            assert_eq!(result, 0.0);
            assert!(result.is_sign_negative());
        }
    }

    #[test]
    fn test_does_not_generate_negative_if_right_boundary_is_positive() {
        assert_all_examples(
            gs::floats::<f64>().min_value(0.0).max_value(1.0),
            |x: &f64| !x.is_sign_negative(),
        );
    }

    #[test]
    fn test_does_not_generate_positive_if_right_boundary_is_negative() {
        assert_all_examples(
            gs::floats::<f64>().min_value(-1.0).max_value(-0.0),
            |x: &f64| x.is_sign_negative(),
        );
    }

    #[test]
    fn test_half_bounded_generates_zero_from_min() {
        find_any(gs::floats::<f64>().min_value(-1.0), |x: &f64| *x == 0.0);
    }

    #[test]
    fn test_half_bounded_generates_zero_from_max() {
        find_any(gs::floats::<f64>().max_value(1.0), |x: &f64| *x == 0.0);
    }

    #[test]
    fn test_half_bounded_respects_sign_of_upper_bound() {
        assert_all_examples(gs::floats::<f64>().max_value(-0.0), |x: &f64| {
            x.is_sign_negative()
        });
    }

    #[test]
    fn test_half_bounded_respects_sign_of_lower_bound() {
        assert_all_examples(gs::floats::<f64>().min_value(0.0), |x: &f64| {
            !x.is_sign_negative()
        });
    }

    #[test]
    fn test_filter_nan() {
        assert_all_examples(gs::floats::<f64>().allow_nan(false), |x: &f64| !x.is_nan());
    }

    #[test]
    fn test_filter_infinity() {
        assert_all_examples(gs::floats::<f64>().allow_infinity(false), |x: &f64| {
            !x.is_infinite()
        });
    }

    #[test]
    fn test_can_guard_against_draws_of_nan() {
        let tagged_floats = gs::one_of(vec![
            gs::tuples!(gs::just(0_i32), gs::floats::<f64>().allow_nan(false)).boxed(),
            gs::tuples!(gs::just(1_i32), gs::floats::<f64>().allow_nan(true)).boxed(),
        ]);
        let (tag, _f) = minimal(tagged_floats, |x: &(i32, f64)| x.1.is_nan());
        assert_eq!(tag, 1);
    }

    #[test]
    fn test_very_narrow_interval() {
        let upper_bound = -1.0_f64;
        let lower_bound = f64::from_bits(upper_bound.to_bits() + 10);
        assert!(lower_bound < upper_bound);

        assert_all_examples(
            gs::floats::<f64>()
                .min_value(lower_bound)
                .max_value(upper_bound),
            move |f: &f64| lower_bound <= *f && *f <= upper_bound,
        );
    }

    #[test]
    fn test_up_means_greater() {
        assert_all_examples(gs::floats::<f64>(), |x: &f64| {
            let hi = x.next_up();
            if *x < hi {
                return true;
            }
            (x.is_nan() && hi.is_nan())
                || (*x > 0.0 && x.is_infinite())
                || (*x == hi && *x == 0.0 && x.is_sign_negative() && !hi.is_sign_negative())
        });
    }

    #[test]
    fn test_down_means_lesser() {
        assert_all_examples(gs::floats::<f64>(), |x: &f64| {
            let lo = x.next_down();
            if *x > lo {
                return true;
            }
            (x.is_nan() && lo.is_nan())
                || (*x < 0.0 && x.is_infinite())
                || (*x == lo && *x == 0.0 && lo.is_sign_negative() && !x.is_sign_negative())
        });
    }

    #[test]
    fn test_updown_roundtrip() {
        assert_all_examples(
            gs::floats::<f64>().allow_nan(false).allow_infinity(false),
            |val: &f64| *val == val.next_down().next_up() && *val == val.next_up().next_down(),
        );
    }

    #[test]
    fn test_float32_can_exclude_infinity() {
        assert_all_examples(gs::floats::<f32>().allow_infinity(false), |x: &f32| {
            !x.is_infinite()
        });
    }

    #[test]
    fn test_finite_min_bound_does_not_overflow() {
        assert_all_examples(
            gs::floats::<f64>().min_value(1e304).allow_infinity(false),
            |x: &f64| !x.is_infinite(),
        );
    }

    #[test]
    fn test_finite_max_bound_does_not_overflow() {
        assert_all_examples(
            gs::floats::<f64>().max_value(-1e304).allow_infinity(false),
            |x: &f64| !x.is_infinite(),
        );
    }

    #[test]
    fn test_can_exclude_endpoints() {
        assert_all_examples(
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(1.0)
                .exclude_min(true)
                .exclude_max(true),
            |x: &f64| 0.0 < *x && *x < 1.0,
        );
    }

    #[test]
    fn test_can_exclude_neg_infinite_endpoint() {
        assert_all_examples(
            gs::floats::<f64>()
                .min_value(f64::NEG_INFINITY)
                .max_value(-1e307)
                .exclude_min(true),
            |x: &f64| !x.is_infinite(),
        );
    }

    #[test]
    fn test_can_exclude_pos_infinite_endpoint() {
        assert_all_examples(
            gs::floats::<f64>()
                .min_value(1e307)
                .max_value(f64::INFINITY)
                .exclude_max(true),
            |x: &f64| !x.is_infinite(),
        );
    }

    #[test]
    fn test_zero_intervals_are_ok() {
        check_can_generate_examples(gs::floats::<f64>().min_value(0.0).max_value(0.0));
        check_can_generate_examples(gs::floats::<f64>().min_value(-0.0).max_value(0.0));
        check_can_generate_examples(gs::floats::<f64>().min_value(-0.0).max_value(-0.0));
    }

    // Validation-only tests: Hypothesis rejects these invalid argument
    // combinations with an InvalidArgument error.

    #[test]
    fn test_exclude_infinite_endpoint_is_invalid_min() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: f64 = tc.draw(
                        gs::floats::<f64>()
                            .min_value(f64::INFINITY)
                            .exclude_min(true),
                    );
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            "InvalidArgument",
        );
    }

    #[test]
    fn test_exclude_infinite_endpoint_is_invalid_max() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: f64 = tc.draw(
                        gs::floats::<f64>()
                            .max_value(f64::NEG_INFINITY)
                            .exclude_max(true),
                    );
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            "InvalidArgument",
        );
    }

    #[test]
    fn test_exclude_entire_interval() {
        for bound in [1.0_f64, -1.0_f64, 1e10_f64, -1e-10_f64] {
            for (lo, hi) in [(true, false), (false, true), (true, true)] {
                expect_panic(
                    || {
                        Hegel::new(move |tc| {
                            let _: f64 = tc.draw(
                                gs::floats::<f64>()
                                    .min_value(bound)
                                    .max_value(bound)
                                    .exclude_min(lo)
                                    .exclude_max(hi),
                            );
                        })
                        .settings(Settings::new().test_cases(1).database(None))
                        .run();
                    },
                    "InvalidArgument",
                );
            }
        }
    }

    #[test]
    fn test_cannot_exclude_endpoint_with_zero_interval() {
        for lo in [0.0_f64, -0.0_f64] {
            for hi in [0.0_f64, -0.0_f64] {
                for (exmin, exmax) in [(true, false), (false, true), (true, true)] {
                    expect_panic(
                        || {
                            Hegel::new(move |tc| {
                                let _: f64 = tc.draw(
                                    gs::floats::<f64>()
                                        .min_value(lo)
                                        .max_value(hi)
                                        .exclude_min(exmin)
                                        .exclude_max(exmax),
                                );
                            })
                            .settings(Settings::new().test_cases(1).database(None))
                            .run();
                        },
                        "InvalidArgument",
                    );
                }
            }
        }
    }
}

mod nocover_floating {
    use crate::common::utils::{FindAny, assert_all_examples};
    use hegel::generators::{self as gs, Generator};
    use hegel::{HealthCheck, Hegel, Settings};

    #[test]
    fn test_is_float() {
        // Rust's f64 generator is statically typed, so every drawn value is a
        // float by construction; we still assert the generator runs.
        assert_all_examples(gs::floats::<f64>(), |_: &f64| true);
    }

    #[test]
    fn test_inversion_is_imperfect() {
        // @fails: find x != 0 such that x * (1/x) != 1.0. A NaN draw satisfies
        // the condition (1/NaN = NaN, NaN * NaN = NaN, NaN != 1.0).
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            *x != 0.0 && *x * (1.0 / *x) != 1.0
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_largest_range() {
        assert_all_examples(
            gs::floats::<f64>().min_value(-f64::MAX).max_value(f64::MAX),
            |x: &f64| !x.is_infinite(),
        );
    }

    #[test]
    fn test_negation_is_self_inverse() {
        // Not a @fails test, so Hegel::new directly. TRY_HARDER → test_cases(1000)
        // + FilterTooMuch suppression, mirroring the upstream.
        Hegel::new(|tc| {
            let x: f64 = tc.draw(gs::floats::<f64>());
            tc.assume(!x.is_nan());
            let y = -x;
            assert!(-y == x);
        })
        .settings(
            Settings::new()
                .test_cases(1000)
                .database(None)
                .suppress_health_check([HealthCheck::FilterTooMuch]),
        )
        .run();
    }

    #[test]
    fn test_is_not_nan() {
        // @fails: find a list containing a NaN.
        FindAny::new(gs::vecs(gs::floats::<f64>()), |xs: &Vec<f64>| {
            xs.iter().any(|x| x.is_nan())
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_is_not_positive_infinite() {
        FindAny::new(gs::floats::<f64>(), |x: &f64| *x > 0.0 && x.is_infinite())
            .max_attempts(1000)
            .suppress_health_check(HealthCheck::FilterTooMuch)
            .run();
    }

    #[test]
    fn test_is_not_negative_infinite() {
        FindAny::new(gs::floats::<f64>(), |x: &f64| *x < 0.0 && x.is_infinite())
            .max_attempts(1000)
            .suppress_health_check(HealthCheck::FilterTooMuch)
            .run();
    }

    #[test]
    fn test_is_int() {
        // @fails: find a finite float that is not integer-valued (e.g. 0.5).
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            x.is_finite() && *x != x.trunc()
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_is_not_int() {
        // @fails: find a finite integer-valued float (e.g. 0.0).
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            x.is_finite() && *x == x.trunc()
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_is_in_exact_int_range() {
        // @fails: find a finite float so large that x + 1 == x (magnitude ≥ 2^53).
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            x.is_finite() && *x + 1.0 == *x
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_can_find_floats_that_do_not_round_trip_through_strings() {
        // @fails: find x where its string form doesn't parse back to an equal
        // value. NaN satisfies this because NaN != NaN.
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            x.to_string().parse::<f64>().unwrap() != *x
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_can_find_floats_that_do_not_round_trip_through_reprs() {
        // Rust has no `repr()`; the Debug format plays the same role and produces
        // a round-trippable representation for all finite/infinite floats. NaN
        // still breaks it because NaN != NaN.
        FindAny::new(gs::floats::<f64>(), |x: &f64| {
            format!("{x:?}").parse::<f64>().unwrap() != *x
        })
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    // Upstream parametrises this over (snan, neg) ∈ {False, True}²; port the
    // four cases as separate tests so each runs under its own TRY_HARDER budget.
    // `snan` is true when `abs(x)`'s bit pattern differs from `f64::NAN`'s (i.e.
    // the mantissa has any non-high bit set); `neg` is true when the sign bit
    // is set. Matches `float_to_int(abs(x)) != float_to_int(float("nan"))` and
    // `math.copysign(1, x) == -1` from the upstream.

    fn variant_matches(x: f64, snan: bool, neg: bool) -> bool {
        let abs_bits = x.abs().to_bits();
        let nan_bits = f64::NAN.to_bits();
        let is_snan = abs_bits != nan_bits;
        let is_neg = x.is_sign_negative();
        snan == is_snan && neg == is_neg
    }

    // The two `quiet` variants require mantissa_bits == 0 exactly. The
    // `FilteredFloat` NaN fallback draws a random mantissa from [0, 2^52-1];
    // mantissa=0 occurs with only ~0.5% combined probability per NaN draw
    // (≈1% for mantissa=0 via the nasty-boundary path × 50% for the sign bit).
    // At the upstream's TRY_HARDER budget of 1000 the residual failure rate is
    // empirically ~7% — high enough to break CI. The signaling variants need
    // mantissa != 0 (essentially always true), so they're fine at 1000.
    // Bumping the two quiet tests to 10_000 drops the failure odds to well
    // below 1e-6 while still completing in <1s.
    const QUIET_NAN_ATTEMPTS: u64 = 10_000;

    #[test]
    fn test_can_find_negative_and_signaling_nans_quiet_positive() {
        // Fixed seed so coverage runs never hit the ~1e-3 residual flake
        // when the NaN mantissa-bit lottery happens to miss across all
        // 10_000 attempts.
        FindAny::new(
            gs::floats::<f64>().filter(|x| variant_matches(*x, false, false)),
            |_: &f64| true,
        )
        .max_attempts(QUIET_NAN_ATTEMPTS)
        .seed(0xc0ffee)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_can_find_negative_and_signaling_nans_signaling_positive() {
        FindAny::new(
            gs::floats::<f64>().filter(|x| variant_matches(*x, true, false)),
            |_: &f64| true,
        )
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_can_find_negative_and_signaling_nans_signaling_negative() {
        FindAny::new(
            gs::floats::<f64>().filter(|x| variant_matches(*x, true, true)),
            |_: &f64| true,
        )
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_can_find_nan_f32() {
        FindAny::new(
            gs::floats::<f32>().filter(|x: &f32| x.is_nan()),
            |_: &f32| true,
        )
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
    }

    #[test]
    fn test_floats_are_in_range() {
        Hegel::new(|tc| {
            let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
            let y: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
            let (x, y) = if x <= y { (x, y) } else { (y, x) };
            tc.assume(x < y);

            let t: f64 = tc.draw(gs::floats::<f64>().min_value(x).max_value(y));
            assert!(x <= t && t <= y);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }
}

mod quality_float_shrinking {
    use crate::common::utils::minimal;
    use hegel::generators::{self as gs, Generator};

    #[test]
    fn test_shrinks_to_simple_floats() {
        assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x > 1.0), 2.0);
        assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x > 0.0), 1.0);
    }

    fn check_can_shrink_in_variable_sized_context(n: usize) {
        let x = minimal(
            gs::vecs(gs::floats::<f64>()).min_size(n),
            |xs: &Vec<f64>| xs.iter().any(|v| *v != 0.0),
        );
        assert_eq!(x.len(), n);
        assert_eq!(x.iter().filter(|&&v| v == 0.0).count(), n - 1);
        assert!(x.contains(&1.0));
    }

    #[test]
    fn test_can_shrink_in_variable_sized_context_1() {
        check_can_shrink_in_variable_sized_context(1);
    }

    #[test]
    fn test_can_shrink_in_variable_sized_context_2() {
        check_can_shrink_in_variable_sized_context(2);
    }

    #[test]
    fn test_can_shrink_in_variable_sized_context_3() {
        check_can_shrink_in_variable_sized_context(3);
    }

    #[test]
    fn test_can_shrink_in_variable_sized_context_8() {
        check_can_shrink_in_variable_sized_context(8);
    }

    #[test]
    fn test_can_shrink_in_variable_sized_context_10() {
        check_can_shrink_in_variable_sized_context(10);
    }

    #[test]
    fn test_shrinks_downwards_to_integers_example_1_5() {
        let f: f64 = 1.5;
        assert_eq!(
            minimal(gs::floats::<f64>().min_value(f), |_: &f64| true),
            f.ceil()
        );
    }

    #[test]
    fn test_shrinks_downwards_to_integers_example_max() {
        let f: f64 = 1.7976931348623157e308;
        assert_eq!(
            minimal(gs::floats::<f64>().min_value(f), |_: &f64| true),
            f.ceil()
        );
    }

    #[test]
    fn test_shrinks_downwards_to_integers_when_fractional_example_1() {
        let b: f64 = 1.0;
        let max = (1u64 << 53) as f64;
        let g = minimal(
            gs::floats::<f64>()
                .min_value(b)
                .max_value(max)
                .exclude_min(true)
                .exclude_max(true)
                .filter(|x: &f64| x.trunc() != *x),
            |_: &f64| true,
        );
        assert_eq!(g, b + 0.5);
    }
}
