//! Distributional claims about the first-party generators: the value shapes
//! that find real bugs (range endpoints, zero, small magnitudes, NaN and
//! infinities, huge and tiny floats, empty and duplicated collections,
//! non-ASCII strings) must appear at healthy rates. Each test runs with a
//! fixed seed, so failures are deterministic, and every threshold sits well
//! below the observed rate (at most half of it).

use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings, TestCase};
use std::sync::{Arc, Mutex};

fn sample<T: Send + 'static>(
    n: u64,
    seed: u64,
    draw: impl Fn(&TestCase) -> T + Send + Sync + 'static,
) -> Vec<T> {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&out);
    Hegel::new(move |tc| {
        let v = draw(&tc);
        sink.lock().unwrap().push(v);
    })
    .settings(
        Settings::new()
            .test_cases(n)
            .seed(Some(seed))
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
    let vs = Arc::try_unwrap(out).ok().unwrap().into_inner().unwrap();
    assert!(
        vs.len() as u64 >= n / 2,
        "only {} samples collected",
        vs.len()
    );
    vs
}

fn rate<T>(vs: &[T], pred: impl Fn(&T) -> bool) -> f64 {
    vs.iter().filter(|v| pred(v)).count() as f64 / vs.len() as f64
}

macro_rules! assert_rate {
    ($vs:expr, $pred:expr, $min:expr, $what:literal) => {
        let r = rate(&$vs, $pred);
        assert!(
            r > $min,
            concat!($what, " rate {:.4}; expected > {}"),
            r,
            $min
        );
    };
    ($vs:expr, $pred:expr, $min:expr, $max:expr, $what:literal) => {
        let r = rate(&$vs, $pred);
        assert!(
            r > $min && r < $max,
            concat!($what, " rate {:.4}; expected in ({}, {})"),
            r,
            $min,
            $max
        );
    };
}

mod integers {
    use super::*;

    #[test]
    fn u64_full_width_hits_endpoints_and_small_values() {
        let vs = sample(20_000, 0xD2, |tc| {
            let a: u64 = tc.draw(gs::integers::<u64>());
            let _b: u64 = tc.draw(gs::integers::<u64>());
            a
        });
        assert_rate!(vs, |&v| v == 0, 0.005, "zero");
        assert_rate!(vs, |&v| v == u64::MAX, 0.005, "u64::MAX");
        assert_rate!(vs, |&v| v >= u64::MAX - 1, 0.005, "near-MAX");
        assert_rate!(vs, |&v| v <= 8, 0.05, "small");
    }

    #[test]
    fn i128_full_width_hits_boundary_and_small_values() {
        let vs = sample(20_000, 0xD3, |tc| {
            let a: i128 = tc.draw(gs::integers::<i128>());
            let _b: i128 = tc.draw(gs::integers::<i128>());
            a
        });
        assert_rate!(
            vs,
            |&v| v == i128::MIN || v == i128::MAX || v.unsigned_abs() <= 1,
            0.015,
            "boundary"
        );
        assert_rate!(vs, |&v| v.unsigned_abs() <= 8, 0.03, "small");
        assert_rate!(vs, |&v| v > 0, 0.2, "positive");
        assert_rate!(vs, |&v| v < 0, 0.2, "negative");
    }

    #[test]
    fn u128_full_width_hits_endpoints_and_small_values() {
        let vs = sample(20_000, 0xD5, |tc| {
            let a: u128 = tc.draw(gs::integers::<u128>());
            let _b: u128 = tc.draw(gs::integers::<u128>());
            a
        });
        assert_rate!(vs, |&v| v == 0, 0.005, "zero");
        assert_rate!(vs, |&v| v == u128::MAX, 0.005, "u128::MAX");
        assert_rate!(vs, |&v| v <= 8, 0.05, "small");
    }

    #[test]
    fn u64_range_from_one_hits_low_endpoint() {
        let vs = sample(20_000, 0xD4, |tc| {
            let a: u64 = tc.draw(gs::integers::<u64>().min_value(1));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            a
        });
        assert_rate!(vs, |&v| v == 1, 0.005, "one (min endpoint)");
        assert_rate!(vs, |&v| v <= 8, 0.05, "small");
        assert_rate!(vs, |&v| v == u64::MAX, 0.005, "u64::MAX");
    }

    #[test]
    fn i32_full_width_hits_endpoints_and_small_values() {
        let vs = sample(20_000, 0xA5, |tc| {
            let a: i32 = tc.draw(gs::integers::<i32>());
            let _b: i32 = tc.draw(gs::integers::<i32>());
            a
        });
        assert_rate!(vs, |&v| v == i32::MIN, 0.0025, "i32::MIN");
        assert_rate!(vs, |&v| v == i32::MAX, 0.0025, "i32::MAX");
        assert_rate!(vs, |&v| v.unsigned_abs() <= 8, 0.03, "small");
    }

    /// u16 arithmetic wraps at 65535/65536 in real bugs (length prefixes,
    /// font metrics), so near-MAX values and overflowing pair sums must be
    /// common.
    #[test]
    fn u16_hits_near_max_and_overflowing_sums() {
        let vs = sample(20_000, 0xA3, |tc| {
            let a: u16 = tc.draw(gs::integers::<u16>());
            let b: u16 = tc.draw(gs::integers::<u16>());
            (a, b)
        });
        assert_rate!(vs, |&(a, _)| a == 0, 0.005, "zero");
        assert_rate!(vs, |&(a, _)| a == u16::MAX, 0.005, "u16::MAX");
        assert_rate!(vs, |&(a, _)| a >= u16::MAX - 2, 0.005, "near-MAX");
        assert_rate!(
            vs,
            |&(a, b)| u32::from(a) + u32::from(b) > u32::from(u16::MAX),
            0.02,
            "overflowing sum"
        );
    }

    #[test]
    fn i16_hits_endpoints_and_overflowing_differences() {
        let vs = sample(20_000, 0xA4, |tc| {
            let a: i16 = tc.draw(gs::integers::<i16>());
            let b: i16 = tc.draw(gs::integers::<i16>());
            (a, b)
        });
        assert_rate!(vs, |&(a, _)| a == i16::MIN, 0.005, "i16::MIN");
        assert_rate!(vs, |&(a, _)| a == i16::MAX, 0.005, "i16::MAX");
        assert_rate!(
            vs,
            |&(a, b)| i32::from(a) - i32::from(b) > i32::from(i16::MAX),
            0.002,
            "overflowing difference"
        );
    }

    #[test]
    fn u8_hits_both_endpoints() {
        let vs = sample(20_000, 0xA2, |tc| {
            let a: u8 = tc.draw(gs::integers::<u8>());
            let _b: u8 = tc.draw(gs::integers::<u8>());
            a
        });
        assert_rate!(vs, |&v| v == 0, 0.005, "zero");
        assert_rate!(vs, |&v| v == u8::MAX, 0.005, "u8::MAX");
    }
}

mod floats {
    use super::*;

    #[test]
    fn f64_unbounded_hits_special_values() {
        let vs = sample(20_000, 0xF1, |tc| {
            let a: f64 = tc.draw(gs::floats::<f64>());
            let _b: f64 = tc.draw(gs::floats::<f64>());
            a
        });
        assert_rate!(vs, |v: &f64| v.is_nan(), 0.01, "NaN");
        assert_rate!(vs, |&v| v == f64::INFINITY, 0.005, "+inf");
        assert_rate!(vs, |&v| v == f64::NEG_INFINITY, 0.005, "-inf");
        assert_rate!(vs, |&v| v == 0.0 && v.is_sign_positive(), 0.002, "+0.0");
        assert_rate!(vs, |&v| v == 0.0 && v.is_sign_negative(), 0.001, "-0.0");
        assert_rate!(
            vs,
            |&v| v.is_finite() && v != 0.0 && v.fract() == 0.0,
            0.2,
            "integer-valued"
        );
    }

    /// The bugs that need floats are overwhelmingly overflow (huge finite
    /// magnitudes: convex hulls, R-trees, lerp) and underflow (subnormals:
    /// robust predicates, normalization) — both shapes must be generated.
    #[test]
    fn f64_unbounded_hits_extreme_magnitudes() {
        let vs = sample(20_000, 0xA6, |tc| {
            let a: f64 = tc.draw(gs::floats::<f64>());
            let b: f64 = tc.draw(gs::floats::<f64>());
            (a, b)
        });
        assert_rate!(
            vs,
            |&(a, _): &(f64, f64)| a.is_finite() && a.abs() >= 1e100,
            0.03,
            "huge magnitude"
        );
        assert_rate!(
            vs,
            |&(a, _): &(f64, f64)| a.is_finite() && a.abs() > f64::MAX / 2.0,
            0.003,
            "near-MAX magnitude"
        );
        assert_rate!(
            vs,
            |&(a, _): &(f64, f64)| a != 0.0 && a.abs() <= 1e-100,
            0.03,
            "tiny magnitude"
        );
        assert_rate!(
            vs,
            |&(a, _): &(f64, f64)| a.is_subnormal(),
            0.001,
            "subnormal"
        );
        let overflowing_sums = vs
            .iter()
            .filter(|(a, b)| a.is_finite() && b.is_finite() && (a + b).is_infinite())
            .count();
        assert!(
            overflowing_sums > 0,
            "no pair of finite draws had an overflowing sum"
        );
    }

    #[test]
    fn f64_unit_interval_hits_endpoints_and_tiny_values() {
        let vs = sample(20_000, 0xF2, |tc| {
            let a: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
            let _b: f64 = tc.draw(gs::floats::<f64>());
            a
        });
        assert_rate!(vs, |&v| v == 0.0, 0.005, "zero");
        assert_rate!(vs, |&v| v == 1.0, 0.01, "one");
        assert_rate!(vs, |&v| v > 0.0 && v < 1e-300, 0.005, "tiny");
    }

    /// Unbounded f32 draws must not collapse to infinity: large *finite*
    /// f32 values are the ones that trigger overflow bugs.
    #[test]
    fn f32_unbounded_is_mostly_finite_with_extreme_magnitudes() {
        let vs = sample(20_000, 0xA7, |tc| {
            let a: f32 = tc.draw(gs::floats::<f32>());
            let _b: f32 = tc.draw(gs::floats::<f32>());
            a
        });
        assert_rate!(vs, |v: &f32| v.is_nan(), 0.005, "NaN");
        assert_rate!(vs, |v: &f32| v.is_infinite(), 0.005, 0.2, "infinite");
        assert_rate!(
            vs,
            |&v| v.is_finite() && v.abs() > f32::MAX / 2.0,
            0.02,
            "near-MAX magnitude"
        );
        assert_rate!(
            vs,
            |&v| v != 0.0 && v.abs() < f32::MIN_POSITIVE,
            0.001,
            "subnormal"
        );
    }
}

mod booleans {
    use super::*;

    fn true_rate(seed: u64, p: f64) -> f64 {
        let vs = sample(2_000, seed, move |tc| {
            let mut trues = 0u32;
            for _ in 0..64 {
                if tc.draw(gs::weighted_booleans(p)) {
                    trues += 1;
                }
            }
            trues
        });
        let total: u64 = vs.iter().map(|&t| u64::from(t)).sum();
        total as f64 / (vs.len() as u64 * 64) as f64
    }

    #[test]
    fn unweighted_booleans_are_roughly_fair() {
        let vs = sample(2_000, 0xB1, |tc| {
            let mut trues = 0u32;
            for _ in 0..64 {
                if tc.draw(gs::booleans()) {
                    trues += 1;
                }
            }
            trues
        });
        let total: u64 = vs.iter().map(|&t| u64::from(t)).sum();
        let r = total as f64 / (vs.len() as u64 * 64) as f64;
        assert!((0.4..=0.6).contains(&r), "true rate {r:.4}; expected ~0.5");
    }

    #[test]
    fn weighted_booleans_roughly_match_their_weight() {
        let r = true_rate(0xB2, 0.05);
        assert!((0.01..=0.15).contains(&r), "p=0.05 true rate {r:.4}");
        let r = true_rate(0xB3, 0.9);
        assert!((0.8..=0.97).contains(&r), "p=0.9 true rate {r:.4}");
        let r = true_rate(0xB4, 0.25);
        assert!((0.15..=0.35).contains(&r), "p=0.25 true rate {r:.4}");
    }
}

mod collections {
    use super::*;

    fn has_duplicate(v: &[i64]) -> bool {
        let mut s = v.to_vec();
        s.sort_unstable();
        s.windows(2).any(|w| w[0] == w[1])
    }

    #[test]
    fn default_vecs_cover_empty_long_and_duplicated() {
        let vs = sample(20_000, 0xC1, |tc| {
            let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(255)));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v
        });
        assert_rate!(vs, |v: &Vec<i64>| v.is_empty(), 0.01, "empty");
        assert_rate!(vs, |v: &Vec<i64>| v.len() >= 10, 0.05, "len >= 10");
        assert_rate!(vs, |v: &Vec<i64>| has_duplicate(v), 0.2, "duplicate");
    }

    #[test]
    fn sized_vecs_hit_min_and_max_size() {
        let vs = sample(20_000, 0xC2, |tc| {
            let v: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).min_size(2).max_size(5));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v.len()
        });
        assert_rate!(vs, |&l| l == 2, 0.05, "at min size");
        assert_rate!(vs, |&l| l == 5, 0.05, "at max size");
    }

    #[test]
    fn small_range_vecs_repeat_elements() {
        let vs = sample(20_000, 0xC3, |tc| {
            let v: Vec<i64> = tc.draw(
                gs::vecs(gs::integers::<i64>().min_value(0).max_value(9))
                    .min_size(2)
                    .max_size(20),
            );
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v
        });
        assert_rate!(vs, |v: &Vec<i64>| has_duplicate(v), 0.5, "duplicate");
        assert_rate!(
            vs,
            |v: &Vec<i64>| v.len() >= 3 && v.iter().all(|&x| x == v[0]),
            0.0005,
            "all elements equal"
        );
    }

    #[test]
    fn nested_vecs_reach_real_nesting() {
        let vs = sample(20_000, 0xA8, |tc| {
            let v: Vec<Vec<bool>> = tc.draw(gs::vecs(gs::vecs(gs::booleans())));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v
        });
        assert_rate!(
            vs,
            |v: &Vec<Vec<bool>>| v.iter().any(|inner| !inner.is_empty()),
            0.5,
            "some non-empty inner vec"
        );
        assert_rate!(
            vs,
            |v: &Vec<Vec<bool>>| v.iter().map(Vec::len).sum::<usize>() >= 8,
            0.3,
            "at least 8 leaves"
        );
        assert_rate!(
            vs,
            |v: &Vec<Vec<bool>>| v.len() >= 4 && v.iter().all(|inner| !inner.is_empty()),
            0.03,
            "4+ non-empty inner vecs"
        );
    }
}

mod strings {
    use super::*;

    #[test]
    fn default_text_covers_empty_and_non_ascii() {
        let vs = sample(20_000, 0x51, |tc| {
            let s: String = tc.draw(gs::text());
            let _b: u64 = tc.draw(gs::integers::<u64>());
            s
        });
        assert_rate!(vs, |s: &String| s.is_empty(), 0.02, "empty");
        assert_rate!(vs, |s: &String| !s.is_ascii(), 0.2, "non-ASCII");
        assert_rate!(
            vs,
            |s: &String| s.chars().any(|c| c as u32 > 0xFFFF),
            0.05,
            "astral plane"
        );
        assert_rate!(
            vs,
            |s: &String| s.chars().next().is_some_and(|c| c.len_utf8() > 1),
            0.1,
            "multi-byte first char"
        );
    }
}

mod combinators {
    use super::*;

    #[test]
    fn optional_generates_both_none_and_some() {
        let vs = sample(20_000, 0x01, |tc| {
            let v: Option<u64> = tc.draw(gs::optional(gs::integers::<u64>()));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v.is_none()
        });
        assert_rate!(vs, |&none| none, 0.1, "None");
        assert_rate!(vs, |&none| !none, 0.3, "Some");
    }

    #[test]
    fn one_of_hits_every_branch() {
        let vs = sample(20_000, 0x02, |tc| {
            let v: i64 = tc.draw(hegel::one_of!(
                gs::integers::<i64>().min_value(0).max_value(9),
                gs::integers::<i64>().min_value(100).max_value(109),
                gs::integers::<i64>().min_value(1000).max_value(1009),
            ));
            let _b: u64 = tc.draw(gs::integers::<u64>());
            v
        });
        assert_rate!(vs, |&v| v < 100, 0.1, "first branch");
        assert_rate!(vs, |&v| (100..1000).contains(&v), 0.1, "second branch");
        assert_rate!(vs, |&v| v >= 1000, 0.1, "third branch");
    }

    #[test]
    fn sampled_from_is_roughly_uniform() {
        let vs = sample(20_000, 0xA1, |tc| {
            let v: u8 = tc.draw(gs::sampled_from(&[10u8, 20, 30, 40, 50][..]));
            for _ in 0..16 {
                tc.draw(gs::booleans());
            }
            v
        });
        for k in [10u8, 20, 30, 40, 50] {
            let r = rate(&vs, |&v| v == k);
            assert!(
                (0.08..=0.4).contains(&r),
                "element {k} rate {r:.4}; expected roughly uniform"
            );
        }
    }
}
