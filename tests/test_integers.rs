// clippy is rightfully complaining about a < n < b when that range is actually
// guaranteed by the types. Nevertheless I want these tests here as a foundational
// guardrail and for my sanity.
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(clippy::manual_range_contains)]

mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators as gs;

#[test]
fn test_i8() {
    assert_all_examples(gs::integers::<i8>(), |&n| n >= i8::MIN && n <= i8::MAX);
    find_any(gs::integers::<i8>(), |&n| n < i8::MIN / 2);
    find_any(gs::integers::<i8>(), |&n| n > i8::MAX / 2);
    find_any(gs::integers::<i8>(), |&n| n == i8::MIN);
    find_any(gs::integers::<i8>(), |&n| n == i8::MAX);
}

#[test]
fn test_i16() {
    assert_all_examples(gs::integers::<i16>(), |&n| n >= i16::MIN && n <= i16::MAX);
    find_any(gs::integers::<i16>(), |&n| n < i16::MIN / 2);
    find_any(gs::integers::<i16>(), |&n| n > i16::MAX / 2);
    find_any(gs::integers::<i16>(), |&n| n == i16::MIN);
    find_any(gs::integers::<i16>(), |&n| n == i16::MAX);
}

#[test]
fn test_i32() {
    assert_all_examples(gs::integers::<i32>(), |&n| n >= i32::MIN && n <= i32::MAX);
    find_any(gs::integers::<i32>(), |&n| n < i32::MIN / 2);
    find_any(gs::integers::<i32>(), |&n| n > i32::MAX / 2);
    find_any(gs::integers::<i32>(), |&n| n == i32::MIN);
    find_any(gs::integers::<i32>(), |&n| n == i32::MAX);
}

#[test]
fn test_i64() {
    assert_all_examples(gs::integers::<i64>(), |&n| n >= i64::MIN && n <= i64::MAX);
    find_any(gs::integers::<i64>(), |&n| n < i64::MIN / 2);
    find_any(gs::integers::<i64>(), |&n| n > i64::MAX / 2);
    find_any(gs::integers::<i64>(), |&n| n == i64::MIN);
    find_any(gs::integers::<i64>(), |&n| n == i64::MAX);
}

#[test]
fn test_u8() {
    assert_all_examples(gs::integers::<u8>(), |&n| n >= u8::MIN && n <= u8::MAX);
    find_any(gs::integers::<u8>(), |&n| n > u8::MAX / 2);
    find_any(gs::integers::<u8>(), |&n| n == u8::MIN);
    find_any(gs::integers::<u8>(), |&n| n == u8::MAX);
}

#[test]
fn test_u16() {
    assert_all_examples(gs::integers::<u16>(), |&n| n >= u16::MIN && n <= u16::MAX);
    find_any(gs::integers::<u16>(), |&n| n > u16::MAX / 2);
    find_any(gs::integers::<u16>(), |&n| n == u16::MIN);
    find_any(gs::integers::<u16>(), |&n| n == u16::MAX);
}

#[test]
fn test_u32() {
    assert_all_examples(gs::integers::<u32>(), |&n| n >= u32::MIN && n <= u32::MAX);
    find_any(gs::integers::<u32>(), |&n| n > u32::MAX / 2);
    find_any(gs::integers::<u32>(), |&n| n == u32::MIN);
    find_any(gs::integers::<u32>(), |&n| n == u32::MAX);
}

#[test]
fn test_u64() {
    assert_all_examples(gs::integers::<u64>(), |&n| n >= u64::MIN && n <= u64::MAX);
    find_any(gs::integers::<u64>(), |&n| n > u64::MAX / 2);
    find_any(gs::integers::<u64>(), |&n| n == u64::MIN);
    find_any(gs::integers::<u64>(), |&n| n == u64::MAX);
}

#[test]
fn test_i128() {
    assert_all_examples(gs::integers::<i128>(), |&n| {
        n >= i128::MIN && n <= i128::MAX
    });
    find_any(gs::integers::<i128>(), |&n| n < i128::MIN / 2);
    find_any(gs::integers::<i128>(), |&n| n > i128::MAX / 2);
    find_any(gs::integers::<i128>(), |&n| n == i128::MIN);
    find_any(gs::integers::<i128>(), |&n| n == i128::MAX);
}

#[test]
fn test_u128() {
    assert_all_examples(gs::integers::<u128>(), |&n| {
        n >= u128::MIN && n <= u128::MAX
    });
    find_any(gs::integers::<u128>(), |&n| n > u128::MAX / 2);
    find_any(gs::integers::<u128>(), |&n| n == u128::MIN);
    find_any(gs::integers::<u128>(), |&n| n == u128::MAX);
}

#[test]
fn test_isize() {
    assert_all_examples(gs::integers::<isize>(), |&n| {
        n >= isize::MIN && n <= isize::MAX
    });
    find_any(gs::integers::<isize>(), |&n| n < isize::MIN / 2);
    find_any(gs::integers::<isize>(), |&n| n > isize::MAX / 2);
    find_any(gs::integers::<isize>(), |&n| n == isize::MIN);
    find_any(gs::integers::<isize>(), |&n| n == isize::MAX);
}

#[test]
fn test_usize() {
    assert_all_examples(gs::integers::<usize>(), |&n| {
        n >= usize::MIN && n <= usize::MAX
    });
    find_any(gs::integers::<usize>(), |&n| n > usize::MAX / 2);
    find_any(gs::integers::<usize>(), |&n| n == usize::MIN);
    find_any(gs::integers::<usize>(), |&n| n == usize::MAX);
}

mod numerics {
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings};

    #[test]
    fn test_fuzz_floats_bounds() {
        Hegel::new(|tc| {
            let (mut low, mut high): (Option<f64>, Option<f64>) = tc.draw(gs::tuples!(
                gs::optional(gs::floats::<f64>().allow_nan(false)),
                gs::optional(gs::floats::<f64>().allow_nan(false)),
            ));
            if let (Some(l), Some(h)) = (low, high) {
                if l > h {
                    std::mem::swap(&mut low, &mut high);
                }
            }
            // See module docstring: hegel-rust's float generator can't express
            // `floats(min_value=inf, ...)` the way Hypothesis can.
            tc.assume(!low.is_some_and(|l| l.is_infinite()));
            tc.assume(!high.is_some_and(|h| h.is_infinite()));

            let exmin = low.is_some() && tc.draw(gs::booleans());
            let exmax = high.is_some() && tc.draw(gs::booleans());

            if let (Some(l), Some(h)) = (low, high) {
                let lo = if exmin { l.next_up() } else { l };
                let hi = if exmax { h.next_down() } else { h };
                tc.assume(lo <= hi);
                if lo == 0.0 && hi == 0.0 {
                    tc.assume(!exmin && !exmax && (1.0_f64).copysign(lo) <= (1.0_f64).copysign(hi));
                }
            }

            let mut s = gs::floats::<f64>();
            if let Some(l) = low {
                s = s.min_value(l);
            }
            if let Some(h) = high {
                s = s.max_value(h);
            }
            s = s.exclude_min(exmin).exclude_max(exmax);
            let val: f64 = tc.draw(s);
            tc.assume(val != 0.0); // positive/negative zero is an issue

            if let Some(l) = low {
                assert!(l <= val);
            }
            if let Some(h) = high {
                assert!(val <= h);
            }
            if exmin {
                assert_ne!(low.unwrap(), val);
            }
            if exmax {
                assert_ne!(high.unwrap(), val);
            }
        })
        .settings(
            Settings::new()
                .suppress_health_check(HealthCheck::all())
                .database(None),
        )
        .run();
    }
}

mod nocover_simple_numbers {
    use super::common::utils::{Minimal, minimal};
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_minimize_negative_int() {
        assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x < 0), -1);
        assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x < -1), -2);
    }

    #[test]
    fn test_positive_negative_int() {
        assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x > 0), 1);
        assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x > 1), 2);
    }

    fn boundaries() -> Vec<i64> {
        let mut bs: Vec<i64> = Vec::new();
        for i in 0..10 {
            bs.push(1i64 << i);
            bs.push((1i64 << i) - 1);
            bs.push((1i64 << i) + 1);
        }
        for i in 0..6 {
            bs.push(10i64.pow(i));
        }
        bs.sort();
        bs.dedup();
        bs
    }

    #[test]
    fn test_minimizes_int_down_to_boundary() {
        for boundary in boundaries() {
            assert_eq!(
                minimal(gs::integers::<i64>(), move |x: &i64| *x >= boundary),
                boundary,
                "boundary = {boundary}"
            );
        }
    }

    #[test]
    fn test_minimizes_int_up_to_boundary() {
        for boundary in boundaries() {
            assert_eq!(
                minimal(gs::integers::<i64>(), move |x: &i64| *x <= -boundary),
                -boundary,
                "boundary = {boundary}"
            );
        }
    }

    #[test]
    fn test_minimizes_ints_from_down_to_boundary() {
        for boundary in boundaries() {
            assert_eq!(
                minimal(
                    gs::integers::<i64>().min_value(boundary - 10),
                    move |x: &i64| {
                        assert!(*x >= boundary - 10);
                        *x >= boundary
                    }
                ),
                boundary,
                "boundary = {boundary}"
            );
            assert_eq!(
                minimal(gs::integers::<i64>().min_value(boundary), |_: &i64| true),
                boundary,
                "boundary = {boundary}"
            );
        }
    }

    #[test]
    fn test_minimizes_negative_integer_range_upwards() {
        assert_eq!(
            minimal(
                gs::integers::<i64>().min_value(-10).max_value(-1),
                |_: &i64| true
            ),
            -1
        );
    }

    #[test]
    fn test_minimizes_integer_range_to_boundary() {
        for boundary in boundaries() {
            assert_eq!(
                minimal(
                    gs::integers::<i64>()
                        .min_value(boundary)
                        .max_value(boundary + 100),
                    |_: &i64| true
                ),
                boundary,
                "boundary = {boundary}"
            );
        }
    }

    #[test]
    fn test_single_integer_range_is_range() {
        assert_eq!(
            minimal(
                gs::integers::<i64>().min_value(1).max_value(1),
                |_: &i64| true
            ),
            1
        );
    }

    #[test]
    fn test_minimal_small_number_in_large_range() {
        assert_eq!(
            minimal(
                gs::integers::<i64>()
                    .min_value(-(1i64 << 32))
                    .max_value(1i64 << 32),
                |x: &i64| *x >= 101
            ),
            101
        );
    }

    #[test]
    fn test_minimal_small_sum_float_list() {
        let xs = Minimal::new(gs::vecs(gs::floats::<f64>()).min_size(5), |x: &Vec<f64>| {
            x.iter().sum::<f64>() >= 1.0
        })
        .run();
        assert_eq!(xs, vec![0.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_minimals_boundary_floats() {
        assert_eq!(
            minimal(
                gs::floats::<f64>().min_value(-1.0).max_value(1.0),
                |_: &f64| true
            ),
            0.0
        );
    }

    #[test]
    fn test_minimal_non_boundary_float() {
        assert_eq!(
            minimal(
                gs::floats::<f64>().min_value(1.0).max_value(9.0),
                |x: &f64| *x > 2.0
            ),
            3.0
        );
    }

    #[test]
    fn test_minimal_float_is_zero() {
        assert_eq!(minimal(gs::floats::<f64>(), |_: &f64| true), 0.0);
    }

    #[test]
    fn test_minimal_asymetric_bounded_float() {
        assert_eq!(
            minimal(
                gs::floats::<f64>().min_value(1.1).max_value(1.6),
                |_: &f64| true
            ),
            1.5
        );
    }

    #[test]
    fn test_negative_floats_simplify_to_zero() {
        assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x <= -1.0), -1.0);
    }

    #[test]
    fn test_minimal_infinite_float_is_positive() {
        assert_eq!(
            minimal(gs::floats::<f64>(), |x: &f64| x.is_infinite()),
            f64::INFINITY
        );
    }

    #[test]
    fn test_can_minimal_infinite_negative_float() {
        let x = minimal(gs::floats::<f64>(), |x: &f64| *x < -f64::MAX);
        assert!(x < -f64::MAX);
    }

    #[test]
    fn test_can_minimal_float_on_boundary_of_representable() {
        minimal(gs::floats::<f64>(), |x: &f64| {
            *x + 1.0 == *x && !x.is_infinite()
        });
    }

    #[test]
    fn test_minimize_nan() {
        assert!(minimal(gs::floats::<f64>(), |x: &f64| x.is_nan()).is_nan());
    }

    #[test]
    fn test_minimize_very_large_float() {
        let t = f64::MAX / 2.0;
        assert_eq!(minimal(gs::floats::<f64>(), move |x: &f64| *x >= t), t);
    }

    fn is_integral(value: f64) -> bool {
        value.is_finite() && value == value.trunc()
    }

    #[test]
    fn test_can_minimal_float_far_from_integral() {
        minimal(gs::floats::<f64>(), |x: &f64| {
            x.is_finite() && !is_integral(*x * (1u64 << 32) as f64)
        });
    }

    #[test]
    fn test_list_of_fractional_float() {
        let xs = Minimal::new(gs::vecs(gs::floats::<f64>()).min_size(5), |x: &Vec<f64>| {
            x.iter().filter(|t| **t >= 1.5).count() >= 5
        })
        .run();
        let distinct: std::collections::HashSet<u64> = xs.iter().map(|v| v.to_bits()).collect();
        assert_eq!(distinct.len(), 1);
        assert_eq!(xs[0], 2.0);
    }

    #[test]
    fn test_minimal_fractional_float() {
        assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x >= 1.5), 2.0);
    }

    #[test]
    fn test_minimizes_lists_of_negative_ints_up_to_boundary() {
        let result = minimal(
            gs::vecs(gs::integers::<i64>()).min_size(10),
            |x: &Vec<i64>| x.iter().filter(|t| **t <= -1).count() >= 10,
        );
        assert_eq!(result, vec![-1; 10]);
    }

    fn check_floats_in_constrained_range(left: f64, right: f64) {
        Hegel::new(move |tc| {
            let r: f64 = tc.draw(gs::floats::<f64>().min_value(left).max_value(right));
            assert!(left <= r && r <= right);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_floats_in_constrained_range_zero_up() {
        check_floats_in_constrained_range(0.0, f64::from_bits(1));
    }

    #[test]
    fn test_floats_in_constrained_range_zero_down() {
        check_floats_in_constrained_range(-f64::from_bits(1), 0.0);
    }

    #[test]
    fn test_floats_in_constrained_range_straddle_zero() {
        check_floats_in_constrained_range(-f64::from_bits(1), f64::from_bits(1));
    }

    #[test]
    fn test_floats_in_constrained_range_subnormal_pair() {
        check_floats_in_constrained_range(f64::from_bits(1), f64::from_bits(2));
    }

    #[test]
    fn test_bounds_are_respected() {
        assert_eq!(
            minimal(gs::floats::<f64>().min_value(1.0), |_: &f64| true),
            1.0
        );
        assert_eq!(
            minimal(gs::floats::<f64>().max_value(-1.0), |_: &f64| true),
            -1.0
        );
    }

    #[test]
    fn test_floats_from_zero_have_reasonable_range() {
        for k in 0..10i32 {
            let n = 10f64.powi(k);
            assert_eq!(
                minimal(gs::floats::<f64>().min_value(0.0), move |x: &f64| *x >= n),
                n,
                "k = {k}"
            );
            assert_eq!(
                minimal(gs::floats::<f64>().max_value(0.0), move |x: &f64| *x <= -n),
                -n,
                "k = {k}"
            );
        }
    }

    #[test]
    fn test_explicit_allow_nan() {
        minimal(gs::floats::<f64>().allow_nan(true), |x: &f64| x.is_nan());
    }

    #[test]
    fn test_one_sided_contains_infinity() {
        minimal(gs::floats::<f64>().min_value(1.0), |x: &f64| {
            x.is_infinite()
        });
        minimal(gs::floats::<f64>().max_value(1.0), |x: &f64| {
            x.is_infinite()
        });
    }

    #[test]
    fn test_no_allow_infinity_upper() {
        Hegel::new(|tc| {
            let x: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).allow_infinity(false));
            assert!(!x.is_infinite());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_no_allow_infinity_lower() {
        Hegel::new(|tc| {
            let x: f64 = tc.draw(gs::floats::<f64>().max_value(0.0).allow_infinity(false));
            assert!(!x.is_infinite());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    // TestFloatsAreFloats: upstream asserts isinstance(arg, float). f64 is
    // statically typed in Rust, so these reduce to smoke tests.

    #[test]
    fn test_floats_are_floats_unbounded() {
        Hegel::new(|tc| {
            tc.draw(gs::floats::<f64>());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_floats_are_floats_int_float_bounds() {
        Hegel::new(|tc| {
            tc.draw(
                gs::floats::<f64>()
                    .min_value(0.0)
                    .max_value(u64::MAX as f64),
            );
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_floats_are_floats_float_float_bounds() {
        Hegel::new(|tc| {
            tc.draw(
                gs::floats::<f64>()
                    .min_value(0.0_f64)
                    .max_value(u64::MAX as f64),
            );
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }
}

mod nocover_boundary_exploration {
    use super::common::utils::Minimal;
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings};
    use rand::Rng;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn test_explore_arbitrary_function() {
        Hegel::new(|tc| {
            let seed: u64 = tc.draw(gs::integers::<u64>());
            let rng = RefCell::new(StdRng::seed_from_u64(seed));
            let cache: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());

            let predicate = move |x: &String| -> bool {
                if let Some(&v) = cache.borrow().get(x) {
                    return v;
                }
                let v = rng.borrow_mut().next_u64() & 1 == 0;
                cache.borrow_mut().insert(x.clone(), v);
                v
            };

            catch_unwind(AssertUnwindSafe(|| {
                Minimal::new(gs::text().min_size(5), predicate)
                    .test_cases(10)
                    .run();
            }))
            .ok();
        })
        .settings(
            Settings::new()
                .test_cases(10)
                .database(None)
                .suppress_health_check(HealthCheck::all()),
        )
        .run();
    }
}
