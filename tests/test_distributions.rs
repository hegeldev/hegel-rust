//! Distributional claims about the first-party generators: the value shapes
//! that find real bugs (range endpoints, zero, small magnitudes, NaN and
//! infinities, huge and tiny floats, empty and duplicated collections,
//! non-ASCII strings) must appear at healthy rates. Each test runs with a
//! fixed seed, so failures are deterministic, and every threshold sits well
//! below the observed rate (at most half of it).

use hegel::generators::{self as gs, Generator};
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

/// Draw a pair per test case: the value under test plus a deliberate second
/// draw. The second draw is part of every claim, not an accident: category
/// weights are chosen per test case and real properties draw more than one
/// value, so all rates are measured in the presence of another draw rather
/// than for a lone draw in an otherwise empty test case.
fn sample_pairs<T: std::fmt::Debug + Send + 'static>(
    seed: u64,
    first: impl Generator<T> + Send + Sync + 'static,
    second: impl Generator<T> + Send + Sync + 'static,
) -> Vec<(T, T)> {
    sample(20_000, seed, move |tc| {
        (tc.draw_silent(&first), tc.draw_silent(&second))
    })
}

/// Like [`sample_pairs`] for claims about a non-numeric value: the deliberate
/// second draw is a full-width `u64`, and only the value under test is kept.
fn sample_with_u64_companion<T: std::fmt::Debug + Send + 'static>(
    seed: u64,
    g: impl Generator<T> + Send + Sync + 'static,
) -> Vec<T> {
    sample(20_000, seed, move |tc| {
        let v = tc.draw_silent(&g);
        tc.draw(gs::integers::<u64>());
        v
    })
}

fn rate<T>(vs: &[T], pred: impl Fn(&T) -> bool) -> f64 {
    vs.iter().filter(|v| pred(v)).count() as f64 / vs.len() as f64
}

fn assert_min_rate<T>(vs: &[T], pred: impl Fn(&T) -> bool, min: f64, what: &str) {
    let r = rate(vs, pred);
    assert!(r > min, "{what} rate {r:.4}; expected > {min}");
}

fn assert_rate_between<T>(vs: &[T], pred: impl Fn(&T) -> bool, min: f64, max: f64, what: &str) {
    let r = rate(vs, pred);
    assert!(
        r > min && r < max,
        "{what} rate {r:.4}; expected in ({min}, {max})"
    );
}

mod integers {
    use super::*;

    trait ClaimInt: Copy + PartialEq + std::fmt::Debug + Send + 'static {
        const MIN_VALUE: Self;
        const MAX_VALUE: Self;
        fn magnitude(self) -> u128;
        fn distance_to_max(self) -> u128;
    }

    macro_rules! impl_claim_int {
        ($($t:ty),*) => {$(
            impl ClaimInt for $t {
                const MIN_VALUE: Self = <$t>::MIN;
                const MAX_VALUE: Self = <$t>::MAX;
                fn magnitude(self) -> u128 {
                    u128::from(self.unsigned_abs())
                }
                fn distance_to_max(self) -> u128 {
                    u128::from(<$t>::MAX.abs_diff(self))
                }
            }
        )*};
    }
    impl_claim_int!(i16, i32, i64, i128);

    macro_rules! impl_claim_uint {
        ($($t:ty),*) => {$(
            impl ClaimInt for $t {
                const MIN_VALUE: Self = <$t>::MIN;
                const MAX_VALUE: Self = <$t>::MAX;
                fn magnitude(self) -> u128 {
                    u128::from(self)
                }
                fn distance_to_max(self) -> u128 {
                    u128::from(<$t>::MAX - self)
                }
            }
        )*};
    }
    impl_claim_uint!(u8, u16, u64, u128);

    /// The generator's range minimum appears in the first draw of at least
    /// `min` of the pairs. Every threshold in this suite is individually
    /// calibrated to at most half the rate observed at the test's pinned
    /// seed.
    fn assert_min_endpoint<T: ClaimInt>(vs: &[(T, T)], range_min: T, min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a == range_min,
            min,
            &format!("{ty} min endpoint"),
        );
    }

    /// The type's `MAX`.
    fn assert_max_endpoint<T: ClaimInt>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a == T::MAX_VALUE,
            min,
            &format!("{ty} max endpoint"),
        );
    }

    /// Within the given distance of `MAX`.
    fn assert_near_max<T: ClaimInt>(vs: &[(T, T)], distance: u128, min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.distance_to_max() <= distance,
            min,
            &format!("{ty} near-MAX"),
        );
    }

    /// `MIN`, `MAX`, or a magnitude of at most 1.
    fn assert_boundary<T: ClaimInt>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a == T::MIN_VALUE || a == T::MAX_VALUE || a.magnitude() <= 1,
            min,
            &format!("{ty} boundary"),
        );
    }

    /// A magnitude of at most 8.
    fn assert_small<T: ClaimInt>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.magnitude() <= 8,
            min,
            &format!("{ty} small"),
        );
    }

    #[test]
    fn u64_full_width_hits_endpoints_and_small_values() {
        let vs = sample_pairs(0xD2, gs::integers::<u64>(), gs::integers::<u64>());
        assert_min_endpoint(&vs, 0, 0.003);
        assert_max_endpoint(&vs, 0.001);
        assert_near_max(&vs, 1, 0.003);
        assert_small(&vs, 0.02);
    }

    /// The swarm draws category weights once per test case, so several draws
    /// in one case go extreme together: pairs of full-width draws overflow
    /// `x + y` on over 1% of cases and land on boundaries simultaneously
    /// more often than independent per-draw rates would predict.
    #[test]
    fn i64_full_width_pairs_go_extreme_together() {
        let vs = sample_pairs(0xD6, gs::integers::<i64>(), gs::integers::<i64>());
        let boundary = |v: i64| {
            v == i64::MIN
                || v == i64::MAX
                || v == i64::MIN + 1
                || v == i64::MAX - 1
                || v.unsigned_abs() <= 1
        };
        assert_min_rate(&vs, |&(a, _)| boundary(a), 0.01, "i64 boundary");
        assert_min_rate(
            &vs,
            |&(a, b)| boundary(a) && boundary(b),
            0.0006,
            "i64 both draws boundary",
        );
        assert_min_rate(
            &vs,
            |&(a, b)| a.checked_add(b).is_none(),
            0.005,
            "i64 overflowing pair sum",
        );
    }

    #[test]
    fn i128_full_width_hits_boundary_and_small_values() {
        let vs = sample_pairs(0xD3, gs::integers::<i128>(), gs::integers::<i128>());
        assert_boundary(&vs, 0.006);
        assert_small(&vs, 0.015);
        assert_min_rate(&vs, |&(a, _)| a > 0, 0.2, "i128 positive");
        assert_min_rate(&vs, |&(a, _)| a < 0, 0.2, "i128 negative");
    }

    #[test]
    fn u128_full_width_hits_endpoints_and_small_values() {
        let vs = sample_pairs(0xD5, gs::integers::<u128>(), gs::integers::<u128>());
        assert_min_endpoint(&vs, 0, 0.003);
        assert_max_endpoint(&vs, 0.001);
        assert_small(&vs, 0.015);
    }

    #[test]
    fn u64_range_from_one_hits_low_endpoint() {
        let vs = sample_pairs(
            0xD4,
            gs::integers::<u64>().min_value(1),
            gs::integers::<u64>(),
        );
        assert_min_endpoint(&vs, 1, 0.003);
        assert_max_endpoint(&vs, 0.001);
        assert_small(&vs, 0.015);
    }

    #[test]
    fn i32_full_width_hits_endpoints_and_small_values() {
        let vs = sample_pairs(0xA5, gs::integers::<i32>(), gs::integers::<i32>());
        assert_min_endpoint(&vs, i32::MIN, 0.0025);
        assert_max_endpoint(&vs, 0.0025);
        assert_small(&vs, 0.015);
    }

    /// u16 arithmetic wraps at 65535/65536 in real bugs (length prefixes,
    /// font metrics), so near-MAX values and overflowing pair sums must be
    /// common.
    #[test]
    fn u16_hits_near_max_and_overflowing_sums() {
        let vs = sample_pairs(0xA3, gs::integers::<u16>(), gs::integers::<u16>());
        assert_min_endpoint(&vs, 0, 0.004);
        assert_max_endpoint(&vs, 0.005);
        assert_near_max(&vs, 2, 0.005);
        assert_min_rate(
            &vs,
            |&(a, b)| u32::from(a) + u32::from(b) > u32::from(u16::MAX),
            0.02,
            "u16 overflowing pair sum",
        );
    }

    #[test]
    fn i16_hits_endpoints_and_overflowing_differences() {
        let vs = sample_pairs(0xA4, gs::integers::<i16>(), gs::integers::<i16>());
        assert_min_endpoint(&vs, i16::MIN, 0.0025);
        assert_max_endpoint(&vs, 0.0025);
        assert_min_rate(
            &vs,
            |&(a, b)| i32::from(a) - i32::from(b) > i32::from(i16::MAX),
            0.002,
            "i16 overflowing difference",
        );
    }

    #[test]
    fn u8_hits_both_endpoints() {
        let vs = sample_pairs(0xA2, gs::integers::<u8>(), gs::integers::<u8>());
        assert_min_endpoint(&vs, 0, 0.005);
        assert_max_endpoint(&vs, 0.005);
    }
}

mod floats {
    use super::*;

    trait ClaimFloat: Copy + std::fmt::Debug + Send + 'static {
        const HALF_MAX: f64;
        fn widen(self) -> f64;
        fn subnormal(self) -> bool;
    }

    impl ClaimFloat for f64 {
        const HALF_MAX: f64 = f64::MAX / 2.0;
        fn widen(self) -> f64 {
            self
        }
        fn subnormal(self) -> bool {
            self.is_subnormal()
        }
    }

    impl ClaimFloat for f32 {
        const HALF_MAX: f64 = (f32::MAX / 2.0) as f64;
        fn widen(self) -> f64 {
            f64::from(self)
        }
        fn subnormal(self) -> bool {
            self.is_subnormal()
        }
    }

    /// NaN in the first draw of at least `min` of the pairs. Predicates on
    /// `f32` widen the value to `f64` first, which is exact; thresholds are
    /// calibrated like the integer ones.
    fn assert_nan_rate<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(vs, |&(a, _)| a.widen().is_nan(), min, &format!("{ty} NaN"));
    }

    fn assert_pos_inf<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen() == f64::INFINITY,
            min,
            &format!("{ty} +inf"),
        );
    }

    fn assert_neg_inf<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen() == f64::NEG_INFINITY,
            min,
            &format!("{ty} -inf"),
        );
    }

    /// A band: infinities must appear but not dominate.
    fn assert_infinite_band<T: ClaimFloat>(vs: &[(T, T)], min: f64, max: f64) {
        let ty = std::any::type_name::<T>();
        assert_rate_between(
            vs,
            |&(a, _)| a.widen().is_infinite(),
            min,
            max,
            &format!("{ty} infinite"),
        );
    }

    fn assert_pos_zero<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen() == 0.0 && a.widen().is_sign_positive(),
            min,
            &format!("{ty} +0.0"),
        );
    }

    fn assert_neg_zero<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen() == 0.0 && a.widen().is_sign_negative(),
            min,
            &format!("{ty} -0.0"),
        );
    }

    /// Finite, nonzero, with a zero fractional part.
    fn assert_integer_valued<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen().is_finite() && a.widen() != 0.0 && a.widen().fract() == 0.0,
            min,
            &format!("{ty} integer-valued"),
        );
    }

    /// A finite magnitude of at least 1e100.
    fn assert_huge<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen().is_finite() && a.widen().abs() >= 1e100,
            min,
            &format!("{ty} huge magnitude"),
        );
    }

    /// A finite magnitude above the type's `MAX / 2`.
    fn assert_near_max_magnitude<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen().is_finite() && a.widen().abs() > T::HALF_MAX,
            min,
            &format!("{ty} near-MAX magnitude"),
        );
    }

    /// A nonzero magnitude of at most 1e-100.
    fn assert_tiny<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(
            vs,
            |&(a, _)| a.widen() != 0.0 && a.widen().abs() <= 1e-100,
            min,
            &format!("{ty} tiny magnitude"),
        );
    }

    fn assert_subnormal<T: ClaimFloat>(vs: &[(T, T)], min: f64) {
        let ty = std::any::type_name::<T>();
        assert_min_rate(vs, |&(a, _)| a.subnormal(), min, &format!("{ty} subnormal"));
    }

    #[test]
    fn f64_unbounded_hits_special_values() {
        let vs = sample_pairs(0xF1, gs::floats::<f64>(), gs::floats::<f64>());
        assert_nan_rate(&vs, 0.005);
        assert_pos_inf(&vs, 0.005);
        assert_neg_inf(&vs, 0.005);
        assert_pos_zero(&vs, 0.002);
        assert_neg_zero(&vs, 0.001);
        assert_integer_valued(&vs, 0.2);
    }

    /// The bugs that need floats are overwhelmingly overflow (huge finite
    /// magnitudes: convex hulls, R-trees, lerp) and underflow (subnormals:
    /// robust predicates, normalization) — both shapes must be generated.
    #[test]
    fn f64_unbounded_hits_extreme_magnitudes() {
        let vs = sample_pairs(0xA6, gs::floats::<f64>(), gs::floats::<f64>());
        assert_huge(&vs, 0.03);
        assert_near_max_magnitude(&vs, 0.003);
        assert_tiny(&vs, 0.03);
        assert_subnormal(&vs, 0.001);
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
        let vs = sample_pairs(
            0xF2,
            gs::floats::<f64>().min_value(0.0).max_value(1.0),
            gs::floats::<f64>(),
        );
        assert_min_rate(&vs, |&(a, _)| a == 0.0, 0.005, "f64 in [0, 1] zero");
        assert_min_rate(&vs, |&(a, _)| a == 1.0, 0.01, "f64 in [0, 1] one");
        assert_min_rate(
            &vs,
            |&(a, _)| a > 0.0 && a < 1e-300,
            0.005,
            "f64 in [0, 1] tiny",
        );
    }

    /// Unbounded f32 draws must not collapse to infinity: large *finite*
    /// f32 values are the ones that trigger overflow bugs.
    #[test]
    fn f32_unbounded_is_mostly_finite_with_extreme_magnitudes() {
        let vs = sample_pairs(0xA7, gs::floats::<f32>(), gs::floats::<f32>());
        assert_nan_rate(&vs, 0.005);
        assert_infinite_band(&vs, 0.005, 0.2);
        assert_near_max_magnitude(&vs, 0.02);
        assert_subnormal(&vs, 0.001);
    }
}

mod booleans {
    use super::*;

    fn true_rate(seed: u64, g: impl Generator<bool> + Send + Sync + 'static) -> f64 {
        let vs = sample(2_000, seed, move |tc| {
            let mut trues = 0u32;
            for _ in 0..64 {
                if tc.draw_silent(&g) {
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
        let r = true_rate(0xB1, gs::booleans());
        assert!((0.4..=0.6).contains(&r), "true rate {r:.4}; expected ~0.5");
    }

    #[test]
    fn weighted_booleans_roughly_match_their_weight() {
        let r = true_rate(0xB2, gs::weighted_booleans(0.05));
        assert!((0.01..=0.15).contains(&r), "p=0.05 true rate {r:.4}");
        let r = true_rate(0xB3, gs::weighted_booleans(0.9));
        assert!((0.8..=0.97).contains(&r), "p=0.9 true rate {r:.4}");
        let r = true_rate(0xB4, gs::weighted_booleans(0.25));
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
        let vs = sample_with_u64_companion(
            0xC1,
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(255)),
        );
        assert_min_rate(&vs, |v: &Vec<i64>| v.is_empty(), 0.01, "vec empty");
        assert_min_rate(&vs, |v: &Vec<i64>| v.len() >= 10, 0.05, "vec len >= 10");
        assert_min_rate(&vs, |v: &Vec<i64>| has_duplicate(v), 0.2, "vec duplicate");
    }

    #[test]
    fn sized_vecs_hit_min_and_max_size() {
        let vs = sample_with_u64_companion(0xC2, gs::vecs(gs::booleans()).min_size(2).max_size(5));
        assert_min_rate(&vs, |v: &Vec<bool>| v.len() == 2, 0.05, "vec at min size");
        assert_min_rate(&vs, |v: &Vec<bool>| v.len() == 5, 0.05, "vec at max size");
    }

    #[test]
    fn small_range_vecs_repeat_elements() {
        let vs = sample_with_u64_companion(
            0xC3,
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(9))
                .min_size(2)
                .max_size(20),
        );
        assert_min_rate(&vs, |v: &Vec<i64>| has_duplicate(v), 0.5, "vec duplicate");
        assert_min_rate(
            &vs,
            |v: &Vec<i64>| v.len() >= 3 && v.iter().all(|&x| x == v[0]),
            0.0005,
            "vec all elements equal",
        );
    }

    #[test]
    fn nested_vecs_reach_real_nesting() {
        let vs = sample_with_u64_companion(0xA8, gs::vecs(gs::vecs(gs::booleans())));
        assert_min_rate(
            &vs,
            |v: &Vec<Vec<bool>>| v.iter().any(|inner| !inner.is_empty()),
            0.5,
            "some non-empty inner vec",
        );
        assert_min_rate(
            &vs,
            |v: &Vec<Vec<bool>>| v.iter().map(Vec::len).sum::<usize>() >= 8,
            0.3,
            "at least 8 leaves",
        );
        assert_min_rate(
            &vs,
            |v: &Vec<Vec<bool>>| v.len() >= 4 && v.iter().all(|inner| !inner.is_empty()),
            0.03,
            "4+ non-empty inner vecs",
        );
    }
}

mod recursive {
    use super::*;

    #[derive(Debug, Clone)]
    enum Tree {
        Leaf(#[allow(dead_code)] i32),
        Branch(Box<Tree>, Box<Tree>),
    }

    impl Tree {
        fn leaf_count(&self) -> usize {
            match self {
                Tree::Leaf(_) => 1,
                Tree::Branch(left, right) => left.leaf_count() + right.leaf_count(),
            }
        }
    }

    fn trees() -> gs::RecursiveGenerator<Tree> {
        gs::recursive(gs::integers::<i32>().map(Tree::Leaf), |subtrees| {
            hegel::tuples!(subtrees.clone(), subtrees)
                .map(|(left, right)| Tree::Branch(Box::new(left), Box::new(right)))
        })
    }

    /// Sizes must cover the whole range the default caps allow: bare leaves,
    /// mid-size trees, and trees close to the 100-leaf cap. Each draw
    /// steers toward a target size sampled across the whole budget, and
    /// the engine's novelty-seeking exploration concentrates on the large
    /// targets, so single-leaf values are uncommon — but they must never
    /// vanish.
    #[test]
    fn recursive_trees_have_diverse_sizes() {
        let vs = sample(4000, 0xE3, |tc| tc.draw_silent(trees()));
        assert_min_rate(&vs, |t| t.leaf_count() == 1, 0.015, "single leaf");
        assert_min_rate(&vs, |t| t.leaf_count() >= 25, 0.06, "25+ leaves");
        assert_min_rate(&vs, |t| t.leaf_count() >= 90, 0.004, "near the leaf cap");
    }

    #[derive(Debug, Clone)]
    enum Expr {
        Value,
        Negate(Box<Expr>),
        Add(Box<Expr>, Box<Expr>),
    }

    impl Expr {
        fn leaf_count(&self) -> usize {
            match self {
                Expr::Value => 1,
                Expr::Negate(e) => e.leaf_count(),
                Expr::Add(a, b) => a.leaf_count() + b.leaf_count(),
            }
        }

        fn depth(&self) -> usize {
            match self {
                Expr::Value => 0,
                Expr::Negate(e) => 1 + e.depth(),
                Expr::Add(a, b) => 1 + a.depth().max(b.depth()),
            }
        }
    }

    /// A grammar shaped like a real expression language — many unary
    /// operators per binary one, mean branch arity well below 2. The engine
    /// reprices the branch probability from the arities the branch function
    /// actually draws, so sizes must fill the leaf budget the way a purely
    /// binary grammar's do instead of collapsing to a handful of nodes.
    #[test]
    fn mixed_arity_trees_use_the_whole_size_range() {
        let vs = sample(4000, 0xE4, |tc| {
            tc.draw_silent(gs::recursive(gs::just(Expr::Value), |exprs| {
                hegel::compose!(|tc| {
                    if tc.draw(gs::integers::<u8>().max_value(23)) < 17 {
                        Expr::Negate(Box::new(tc.draw_silent(&exprs)))
                    } else {
                        Expr::Add(
                            Box::new(tc.draw_silent(&exprs)),
                            Box::new(tc.draw_silent(&exprs)),
                        )
                    }
                })
            }))
        });
        let mean = vs.iter().map(Expr::leaf_count).sum::<usize>() as f64 / vs.len() as f64;
        assert!(mean > 10.0, "mean leaf count {mean:.2}; expected > 10");
        assert_min_rate(&vs, |e| e.leaf_count() == 1, 0.1, "single leaf");
        assert_min_rate(&vs, |e| e.leaf_count() >= 25, 0.15, "25+ leaves");
        assert_min_rate(&vs, |e| e.leaf_count() >= 90, 0.003, "near the leaf cap");
    }

    /// A grammar with only unary branches can never grow past one leaf, so
    /// the leaf budget says nothing about it. The adaptive pricing pushes
    /// the branch probability up to its cap instead, spreading chain
    /// lengths from bare leaves up to the depth limit.
    #[test]
    fn chain_only_trees_spread_over_the_whole_depth_range() {
        let vs = sample(4000, 0xE5, |tc| {
            tc.draw_silent(gs::recursive(gs::just(Expr::Value), |exprs| {
                exprs.map(|e| Expr::Negate(Box::new(e)))
            }))
        });
        assert_min_rate(&vs, |e| e.depth() == 0, 0.005, "bare leaf");
        assert_min_rate(&vs, |e| e.depth() >= 10, 0.3, "chain of 10+");
        assert_min_rate(&vs, |e| e.depth() >= 25, 0.08, "chain of 25+");
    }

    /// Branch functions with more than two children per branch reprice
    /// downward as their branches close, so fewer attempts bust the leaf
    /// budget and accepted sizes still span it.
    #[test]
    fn ternary_trees_use_the_whole_size_range() {
        let vs = sample(4000, 0xE6, |tc| {
            tc.draw_silent(gs::recursive(gs::just(Expr::Value), |exprs| {
                hegel::compose!(|tc| {
                    Expr::Add(
                        Box::new(tc.draw_silent(&exprs)),
                        Box::new(Expr::Add(
                            Box::new(tc.draw_silent(&exprs)),
                            Box::new(tc.draw_silent(&exprs)),
                        )),
                    )
                })
            }))
        });
        assert_min_rate(&vs, |e| e.leaf_count() == 1, 0.01, "single leaf");
        assert_min_rate(&vs, |e| e.leaf_count() >= 25, 0.15, "25+ leaves");
        assert_min_rate(&vs, |e| e.leaf_count() >= 90, 0.008, "near the leaf cap");
    }

    /// The branch probability never depends on how much of the leaf budget
    /// an attempt has already spent (and for a fixed-arity branch function
    /// it never moves at all), so subtrees drawn first (on the left) must
    /// not be systematically bigger or branchier than their later siblings.
    #[test]
    fn recursive_trees_are_not_left_biased() {
        let vs = sample(4000, 0xE2, |tc| tc.draw_silent(trees().max_leaves(8)));
        let mut roots = 0usize;
        let mut left_leaves = 0usize;
        let mut right_leaves = 0usize;
        let mut left_branches = 0usize;
        let mut right_branches = 0usize;
        for t in &vs {
            if let Tree::Branch(left, right) = t {
                roots += 1;
                left_leaves += left.leaf_count();
                right_leaves += right.leaf_count();
                left_branches += usize::from(matches!(**left, Tree::Branch(_, _)));
                right_branches += usize::from(matches!(**right, Tree::Branch(_, _)));
            }
        }
        let branch_rate_gap = (left_branches as f64 - right_branches as f64).abs() / roots as f64;
        assert!(
            branch_rate_gap < 0.08,
            "left children branched at a rate {branch_rate_gap:.4} apart from right children"
        );
        let leaf_ratio = left_leaves as f64 / right_leaves as f64;
        assert!(
            (0.85..1.18).contains(&leaf_ratio),
            "left/right leaf ratio {leaf_ratio:.4}; expected close to 1"
        );
    }
}

mod strings {
    use super::*;

    #[test]
    fn default_text_covers_empty_and_non_ascii() {
        let vs = sample_with_u64_companion(0x51, gs::text());
        assert_min_rate(&vs, |s: &String| s.is_empty(), 0.02, "text empty");
        assert_min_rate(&vs, |s: &String| !s.is_ascii(), 0.2, "text non-ASCII");
        assert_min_rate(
            &vs,
            |s: &String| s.chars().any(|c| c as u32 > 0xFFFF),
            0.05,
            "text astral plane",
        );
        assert_min_rate(
            &vs,
            |s: &String| s.chars().next().is_some_and(|c| c.len_utf8() > 1),
            0.1,
            "text multi-byte first char",
        );
    }
}

mod combinators {
    use super::*;

    #[test]
    fn optional_generates_both_none_and_some() {
        let vs = sample_with_u64_companion(0x01, gs::optional(gs::integers::<u64>()));
        assert_min_rate(&vs, |v: &Option<u64>| v.is_none(), 0.1, "optional None");
        assert_min_rate(&vs, |v: &Option<u64>| v.is_some(), 0.3, "optional Some");
    }

    #[test]
    fn one_of_hits_every_branch() {
        let vs = sample_with_u64_companion(
            0x02,
            hegel::one_of!(
                gs::integers::<i64>().min_value(0).max_value(9),
                gs::integers::<i64>().min_value(100).max_value(109),
                gs::integers::<i64>().min_value(1000).max_value(1009),
            ),
        );
        assert_min_rate(&vs, |&v| v < 100, 0.1, "one_of first branch");
        assert_min_rate(
            &vs,
            |&v| (100..1000).contains(&v),
            0.1,
            "one_of second branch",
        );
        assert_min_rate(&vs, |&v| v >= 1000, 0.1, "one_of third branch");
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
