mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators;
use hegel::{assume, hegel};

macro_rules! float_tests {
    ($t:ty) => {
        #[test]
        fn finite() {
            assert_all_examples(
                generators::floats::<$t>()
                    .allow_nan(false)
                    .allow_infinity(false),
                |&n| n.is_finite(),
            );
        }

        #[test]
        fn with_min() {
            hegel(|| {
                let min = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let n = hegel::draw(&generators::floats::<$t>().with_min(min));
                assert!(n >= min, "{n} should be >= {min}");
            });
        }

        #[test]
        fn with_max() {
            hegel(|| {
                let max = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let n = hegel::draw(&generators::floats::<$t>().with_max(max));
                assert!(n <= max, "{n} should be <= {max}");
            });
        }

        #[test]
        fn with_min_and_max() {
            hegel(|| {
                let a = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let b = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let min = a.min(b);
                let max = a.max(b);
                let n = hegel::draw(&generators::floats::<$t>().with_min(min).with_max(max));
                assert!(n >= min && n <= max, "{n} should be in [{min}, {max}]");
            });
        }

        #[test]
        fn exclude_min() {
            hegel(|| {
                let min = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                assume(min.next_up().is_finite());
                let n = hegel::draw(&generators::floats::<$t>().with_min(min).exclude_min());
                assert!(n > min, "{n} should be > {min}");
            });
        }

        #[test]
        fn exclude_max() {
            hegel(|| {
                let max = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                assume(max.next_down().is_finite());
                let n = hegel::draw(&generators::floats::<$t>().with_max(max).exclude_max());
                assert!(n < max, "{n} should be < {max}");
            });
        }

        #[test]
        fn exclude_min_and_max() {
            hegel(|| {
                let a = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let b = hegel::draw(
                    &generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let min = a.min(b);
                let max = a.max(b);
                assume(min.next_up() < max);
                let n = hegel::draw(
                    &generators::floats::<$t>()
                        .with_min(min)
                        .with_max(max)
                        .exclude_min()
                        .exclude_max(),
                );
                assert!(n > min && n < max, "{n} should be in ({min}, {max})");
            });
        }

        #[test]
        fn can_find_nan() {
            find_any(generators::floats::<$t>(), |n| n.is_nan());
        }

        #[test]
        fn can_find_inf() {
            find_any(generators::floats::<$t>(), |n| n.is_infinite());
        }

        #[test]
        fn can_find_positive() {
            find_any(generators::floats::<$t>(), |&n| n.is_finite() && n > 0.0);
        }

        #[test]
        fn can_find_negative() {
            find_any(generators::floats::<$t>(), |&n| n.is_finite() && n < 0.0);
        }

        #[test]
        fn fuzz_floats_bounds() {
            hegel(|| {
                let bound_gen = generators::optional(
                    generators::floats::<$t>()
                        .allow_nan(false)
                        .allow_infinity(false),
                );
                let mut low: Option<$t> = hegel::draw(&bound_gen);
                let mut high: Option<$t> = hegel::draw(&bound_gen);

                if let (Some(lo), Some(hi)) = (low, high) {
                    if lo > hi {
                        low = Some(hi);
                        high = Some(lo);
                    }
                }

                let exmin = low.is_some() && hegel::draw(&generators::booleans());
                let exmax = high.is_some() && hegel::draw(&generators::booleans());

                if let (Some(lo), Some(hi)) = (low, high) {
                    let effective_lo = if exmin { lo.next_up() } else { lo };
                    let effective_hi = if exmax { hi.next_down() } else { hi };
                    assume(effective_lo <= effective_hi);
                }

                let mut g = generators::floats::<$t>();
                if let Some(lo) = low {
                    g = g.with_min(lo);
                }
                if let Some(hi) = high {
                    g = g.with_max(hi);
                }
                if exmin {
                    g = g.exclude_min();
                }
                if exmax {
                    g = g.exclude_max();
                }

                let val = hegel::draw(&g);

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
            });
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
