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
