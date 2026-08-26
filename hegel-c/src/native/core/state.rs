use crate::native::{HashMap, HashSet};
use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicUsize, Ordering};

use once_cell::race::OnceBox;

use rand::{Rng, RngExt};

use crate::native::rng::EngineRng;

use super::MAX_CLONE_DEPTH;
use super::choices::{
    BooleanChoice, BytesChoice, ChoiceNode, ChoiceTemplate, ChoiceTemplateKind, ChoiceValue,
    EngineError, FloatChoice, IntegerChoice, InterestingOrigin, RealizedStream, Status,
    StringChoice,
};
use super::float_index::index_to_float;
use super::{
    BOUNDARY_PROBABILITY, BUFFER_SIZE, CURATED_MIN_WIDTH, DIRICHLET_ALPHA_DIFFUSE,
    DIRICHLET_ALPHA_ENDPOINT, DIRICHLET_ALPHA_INTERESTING, DIRICHLET_ALPHA_MIDDLE,
};
use crate::control::{
    InternalError, hegel_internal_assert, hegel_internal_debug_assert, hegel_internal_unwrap,
};
use crate::native::bignum::{BigInt, BigUint, ToPrimitive, Zero};
use crate::native::floats::{next_down, next_up};
use crate::native::intervalsets::IntervalSet;
use crate::native::statistics::{
    Distribution, LogStudentTDistribution, PiecewiseDistribution, UniformDistribution,
};
use crate::sys::sync::{Lazy, Mutex};

/// State for a variable-length collection.
pub struct ManyState {
    pub min_size: usize,
    pub max_size: f64,
    pub p_continue: f64,
    pub count: usize,
    pub rejections: usize,
    pub force_stop: bool,
}

impl ManyState {
    pub fn new(min_size: usize, max_size: Option<usize>) -> Self {
        ManyState {
            min_size,
            max_size: max_size.map_or(f64::INFINITY, |n| n as f64),
            p_continue: length_p_continue(min_size, max_size),
            count: 0,
            rejections: 0,
            force_stop: false,
        }
    }
}

/// Probability of extending a length draw beyond its current size. Length
/// clusters around an `average_size` derived from
/// `min(max(min_size * 2, min_size + 5), 0.5 * (min_size + max_size))`.
pub(crate) fn length_p_continue(min_size: usize, max_size: Option<usize>) -> f64 {
    let max_f = max_size.map_or(f64::INFINITY, |n| n as f64);
    let min_f = min_size as f64;
    let average = f64::min(f64::max(min_f * 2.0, min_f + 5.0), 0.5 * (min_f + max_f));
    let desired_extra = average - min_f;
    let max_extra = max_f - min_f;

    if desired_extra >= max_extra {
        0.99
    } else if max_f.is_infinite() {
        1.0 - 1.0 / (1.0 + desired_extra)
    } else {
        1.0 - 1.0 / (2.0 + desired_extra)
    }
}

/// Interesting integer constants: powers of 2 (2^16..2^65), powers of 10
/// (10^5..10^19), factorials (9!..20!), primorials — plus their ±1
/// neighbours and negations.
static GLOBAL_CONSTANTS_INTEGERS: Lazy<Vec<i128>> = Lazy::new(|| {
    let mut base: Vec<i128> = Vec::new();
    for n in 16u32..66 {
        base.push(1i128 << n);
    }
    let mut p10 = 100_000i128;
    for _ in 5..20u32 {
        base.push(p10);
        p10 *= 10;
    }
    let mut f = 362_880i128;
    base.push(f);
    for i in 10u32..=20 {
        f *= i as i128;
        base.push(f);
    }
    base.extend_from_slice(&[
        510_510i128,
        6_469_693_230,
        304_250_263_527_210,
        32_589_158_477_190_044_730,
    ]);
    let n_base = base.len();
    for i in 0..n_base {
        base.push(base[i] - 1);
        base.push(base[i] + 1);
    }
    let n_half = base.len();
    for i in 0..n_half {
        base.push(-base[i]);
    }
    base.sort_unstable();
    base.dedup();
    base
});

/// Geometric-distribution length draw for variable-length collections.
///
/// Drawing length uniformly from `[min_size, max_size]` produces huge
/// values when `max_size` is large; instead, the size follows a geometric
/// variate with stop probability derived from [`length_p_continue`].
fn many_draw_length(
    rng: &mut EngineRng,
    min_size: usize,
    max_size: usize,
) -> Result<usize, InternalError> {
    if min_size == max_size {
        return Ok(min_size);
    }
    let p_continue = length_p_continue(min_size, Some(max_size));
    let u: f64 = rng.random();
    let extra = libm::floor(libm::log(u) / libm::log(p_continue));
    hegel_internal_assert!(extra >= 0.0);
    Ok(min_size.saturating_add(extra as usize).min(max_size))
}

/// The shared integer distribution used by [`biased_integer_sample`] as
/// the non-nasty fallback. A piecewise distribution composed of:
///
///   * uniform on `[-256, 256]` for the central core, and
///   * a log-Student's-t (scale_bits = 13, df = 2) for the heavy outer
///     tails — so magnitudes spread smoothly across many orders without
///     the prior bucketed-bit-size cliffs.
///
/// Statically constructed because the constructor evaluates `Γ` and CDF
/// integrals at the switchover; recomputing it per draw would dominate
/// runtime.
static INTEGERS_DISTRIBUTION: Lazy<
    Result<PiecewiseDistribution<UniformDistribution, LogStudentTDistribution>, InternalError>,
> = Lazy::new(|| {
    PiecewiseDistribution::new(
        UniformDistribution::new(256.0),
        LogStudentTDistribution::new(13.0, 2),
        256.0,
    )
});

/// Draw an integer in `[min_value, max_value]` from
/// [`INTEGERS_DISTRIBUTION`] restricted to that range.
///
/// Falls back to a plain uniform draw when the CDF window across the
/// requested range is too narrow for inverse-CDF sampling to be stable.
/// Callers must ensure `min_value < max_value`; the `min == max` early
/// return is handled at the [`biased_integer_sample`] call site.
fn integer_sample_from_distribution(
    min_value: i128,
    max_value: i128,
    rng: &mut EngineRng,
) -> Result<i128, InternalError> {
    let dist = INTEGERS_DISTRIBUTION.as_ref().map_err(Clone::clone)?;
    let lo = dist.cdf(min_value as f64 - 0.5)?;
    let hi = dist.cdf(max_value as f64 + 0.5)?;
    if hi - lo < 1e-13 {
        return Ok(rng.random_range(min_value..=max_value));
    }
    let p = (lo + rng.random::<f64>() * (hi - lo)).max(f64::MIN_POSITIVE);
    Ok((libm::round(dist.inverse_cdf(p)?) as i128).clamp(min_value, max_value))
}

/// Hand-picked "interesting" boundary values: the small magnitudes `0..=±8`,
/// the powers of two and their neighbours, plus the `i{16,32,64}::{MIN,MAX}`
/// boundaries. Merged into [`SORTED_NASTY_POOL`] at startup.
///
/// The small magnitudes are deliberately *contiguous* through ±8 (including
/// ±3..=±6, which are neither `2^k` nor `2^k−1`): off-by-small-`n` bugs are as
/// common as power-of-two boundary bugs, so every small magnitude earns a place
/// in the curated set even though it breaks the `2^k`/`2^k−1` pattern of the
/// larger entries.
static INTERESTING_INTEGERS: &[i128] = &[
    0,
    1,
    -1,
    2,
    -2,
    3,
    -3,
    4,
    -4,
    5,
    -5,
    6,
    -6,
    7,
    -7,
    8,
    -8,
    15,
    -15,
    16,
    -16,
    31,
    -31,
    32,
    -32,
    63,
    -63,
    64,
    -64,
    127,
    -127,
    128,
    -128,
    255,
    -255,
    256,
    -256,
    511,
    -511,
    512,
    -512,
    1023,
    -1023,
    1024,
    -1024,
    2047,
    -2047,
    2048,
    -2048,
    4095,
    -4095,
    4096,
    -4096,
    8191,
    -8191,
    8192,
    -8192,
    i16::MAX as i128,
    i16::MIN as i128,
    i32::MAX as i128,
    i32::MIN as i128,
    i64::MAX as i128,
    i64::MIN as i128,
];

/// [`INTERESTING_INTEGERS`] sorted and deduped, so the curated tier can find
/// its in-range slice with `partition_point`.
static SORTED_INTERESTING: Lazy<Vec<i128>> = Lazy::new(|| {
    let mut v = INTERESTING_INTEGERS.to_vec();
    v.sort_unstable();
    v.dedup();
    v
});

/// Sorted, deduped union of [`INTERESTING_INTEGERS`] and
/// [`GLOBAL_CONSTANTS_INTEGERS`]. Used by [`narrow_nasty_sample`] to find
/// the in-range boundary candidates via two `partition_point` calls instead
/// of an O(n²) per-call dedup loop.
static SORTED_NASTY_POOL: Lazy<Vec<i128>> = Lazy::new(|| {
    let mut all: Vec<i128> = INTERESTING_INTEGERS
        .iter()
        .copied()
        .chain(GLOBAL_CONSTANTS_INTEGERS.iter().copied())
        .collect();
    all.sort_unstable();
    all.dedup();
    all
});

/// Draw a standard normal (mean 0, variance 1) via the Box–Muller transform.
fn standard_normal(rng: &mut EngineRng) -> f64 {
    // `1.0 - random` lands in `(0, 1]`, keeping `ln` finite.
    let u1 = 1.0 - rng.random::<f64>();
    let u2 = rng.random::<f64>();
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(core::f64::consts::TAU * u2)
}

/// Draw a Gamma(`shape`, scale 1) variate. Marsaglia–Tsang's method for
/// `shape >= 1`, with Ahrens–Dieter's boost `Gamma(a) = Gamma(a+1) · U^(1/a)`
/// for `shape < 1` (the regime the small Dirichlet concentrations use). Always
/// returns a strictly non-negative, finite value.
fn sample_gamma(shape: f64, rng: &mut EngineRng) -> Result<f64, InternalError> {
    hegel_internal_debug_assert!(shape > 0.0, "gamma shape must be positive");
    if shape < 1.0 {
        let g = sample_gamma(shape + 1.0, rng)?;
        // `random` may be 0; clamp so `pow` stays finite.
        let u = rng.random::<f64>().max(f64::MIN_POSITIVE);
        return Ok(g * libm::pow(u, 1.0 / shape));
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / libm::sqrt(9.0 * d);
    loop {
        let x = standard_normal(rng);
        let t = 1.0 + c * x;
        let v = t * t * t;
        if v <= 0.0 {
            continue;
        }
        let u = rng.random::<f64>();
        let x2 = x * x;
        if u < 1.0 - 0.033_1 * x2 * x2 {
            return Ok(d * v);
        }
        if libm::log(u) < 0.5 * x2 + d * (1.0 - v + libm::log(v)) {
            return Ok(d * v);
        }
    }
}

/// Draw a point on the 4-simplex from a Dirichlet with the given concentrations
/// (via normalised independent Gamma variates). Returns weights that sum to 1.
fn sample_dirichlet4(alphas: [f64; 4], rng: &mut EngineRng) -> Result<[f64; 4], InternalError> {
    let mut g = [0.0f64; 4];
    for (slot, &alpha) in g.iter_mut().zip(alphas.iter()) {
        *slot = sample_gamma(alpha, rng)?;
    }
    Ok(normalize_to_simplex(g))
}

/// Normalise non-negative weights so they sum to 1. If every weight is zero
/// (all Gammas underflowed — only possible when every concentration is below 1,
/// which the caller's do not do), falls back to an even split rather than
/// dividing by zero.
fn normalize_to_simplex(mut g: [f64; 4]) -> [f64; 4] {
    let sum: f64 = g.iter().sum();
    if sum <= 0.0 {
        return [0.25; 4];
    }
    for slot in &mut g {
        *slot /= sum;
    }
    g
}

/// Per-test-case *swarm* parameters: the mixture weights of the four value
/// categories a wide-range integer draw chooses between — endpoints, curated
/// interesting values, the diffuse large-constant pool, and the ordinary middle
/// distribution. Drawn once at the start of each generated test case (see
/// [`Self::draw`]) and held constant across every draw and every clone-stream of
/// that case. The middle weight is implicit: `1 - endpoint - interesting -
/// diffuse`.
///
/// They only ever change *how likely* each category is, never *which* values are
/// reachable. Because hegel records typed choice *values* and the samplers are
/// consulted only for fresh draws (replay and shrinking read the recorded values
/// directly), these parameters are a pure generation-time reweighting: they are
/// never written into the choice sequence and never affect shrinking.
///
/// The weights come from a Dirichlet (see the `DIRICHLET_ALPHA_*` constants), so
/// most cases are middle-dominated ("normal") while a thin lumpy tail
/// concentrates on one special category. An endpoint-heavy case draws both
/// operands of `x + y` from `{min, max, …}`, so their sum overflows about half
/// the time — the correlation a fixed per-value probability can't produce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenerationParameters {
    /// Probability a wide-range draw returns a range endpoint (`min`, `max`,
    /// `min + 1`, `max - 1`).
    pub endpoint_probability: f64,
    /// Probability a wide-range draw returns a curated `INTERESTING_INTEGERS`
    /// value in range (zero, ±1, small magnitudes, powers of two, type limits).
    pub interesting_probability: f64,
    /// Probability a wide-range draw returns a diffuse large constant.
    pub diffuse_probability: f64,
}

impl GenerationParameters {
    /// Draw a fresh set of parameters for one test case from the Dirichlet over
    /// the four value categories.
    pub fn draw(rng: &mut EngineRng) -> Result<Self, InternalError> {
        let alphas = [
            DIRICHLET_ALPHA_ENDPOINT,
            DIRICHLET_ALPHA_INTERESTING,
            DIRICHLET_ALPHA_DIFFUSE,
            DIRICHLET_ALPHA_MIDDLE,
        ];
        let [endpoint, interesting, diffuse, _middle] = sample_dirichlet4(alphas, rng)?;
        Ok(GenerationParameters {
            endpoint_probability: endpoint,
            interesting_probability: interesting,
            diffuse_probability: diffuse,
        })
    }
}

impl Default for GenerationParameters {
    /// A fixed fallback used only when no test-case parameters have been drawn
    /// (a replay-only test case never samples, so it never consults these). The
    /// values are the mean of [`Self::draw`] — each category's Dirichlet
    /// concentration over their total — so any accidental use still produces a
    /// reasonable distribution rather than a degenerate one.
    fn default() -> Self {
        let total = DIRICHLET_ALPHA_ENDPOINT
            + DIRICHLET_ALPHA_INTERESTING
            + DIRICHLET_ALPHA_DIFFUSE
            + DIRICHLET_ALPHA_MIDDLE;
        GenerationParameters {
            endpoint_probability: DIRICHLET_ALPHA_ENDPOINT / total,
            interesting_probability: DIRICHLET_ALPHA_INTERESTING / total,
            diffuse_probability: DIRICHLET_ALPHA_DIFFUSE / total,
        }
    }
}

/// Boundary-biased sample for a type-erased integer choice.
///
/// Implements the "nasty value" boost used by both the
/// [`NativeTestCase::draw_integer`] code path and the data-tree novel-prefix
/// walk, keeping the two random-generation routes consistent.
///
/// When the choice's span fits `i128` (the overwhelmingly common case) this
/// runs the native [`biased_i128_sample`] — nasty pool plus heavy-tailed
/// distribution — and re-widens the result into the choice's concrete type.
/// Otherwise (a `BigInt` choice, or a `u128` range past `i128::MAX`) it falls
/// back to [`biguint_sample_in_range`].
pub(crate) fn biased_integer_sample(
    ic: &IntegerChoice,
    rng: &mut EngineRng,
    params: GenerationParameters,
) -> Result<BigInt, InternalError> {
    Ok(match (ic.min_value.to_i128(), ic.max_value.to_i128()) {
        (Some(min_i), Some(max_i)) => BigInt::from(biased_i128_sample(min_i, max_i, rng, params)?),
        _ => biguint_sample_in_range(&ic.min_value, &ic.max_value, rng, params),
    })
}

/// Narrow-range sampler: the original nasty-pool + distribution behaviour,
/// unchanged, so ranges below [`CURATED_MIN_WIDTH`] keep their exact previous
/// generation and shrink behaviour.
fn narrow_nasty_sample(
    min_value: i128,
    max_value: i128,
    rng: &mut EngineRng,
) -> Result<i128, InternalError> {
    let pool = &*SORTED_NASTY_POOL;
    let lo = pool.partition_point(|&v| v < min_value);
    let hi = pool.partition_point(|&v| v <= max_value);
    let static_slice = &pool[lo..hi];
    let need_min = static_slice.first() != Some(&min_value);
    let need_max = static_slice.last() != Some(&max_value);
    let count = static_slice.len() + (need_min as usize) + (need_max as usize);
    let threshold = (count as f64 * BOUNDARY_PROBABILITY).min(0.5);
    if rng.random::<f64>() < threshold {
        let idx = rng.random_range(0..count);
        Ok(if need_min && idx == 0 {
            min_value
        } else if need_max && idx == count - 1 {
            max_value
        } else {
            static_slice[idx - need_min as usize]
        })
    } else {
        integer_sample_from_distribution(min_value, max_value, rng)
    }
}

/// Boundary-biased sample in `[min_value, max_value]`.
///
/// Narrow ranges use [`narrow_nasty_sample`]. Wide ranges (width at least
/// [`CURATED_MIN_WIDTH`], or wider than `i128`) choose one of four value
/// categories from a single decision draw `u`, weighted by this case's
/// [`GenerationParameters`]:
///
///   * *endpoints* — `{min, max, min + 1, max - 1}`;
///   * *interesting* — `INTERESTING_INTEGERS` in range;
///   * *diffuse* — the large `GLOBAL_CONSTANTS_INTEGERS` pool in range;
///   * *middle* — the ordinary distribution (the remaining mass).
///
/// An empty special category's mass falls through to the next, and finally to
/// the middle.
fn biased_i128_sample(
    min_value: i128,
    max_value: i128,
    rng: &mut EngineRng,
    params: GenerationParameters,
) -> Result<i128, InternalError> {
    if min_value == max_value {
        return Ok(min_value);
    }

    // `checked_sub` returning `None` means the width overflows `i128`, so the
    // range is unambiguously wide.
    let is_wide = max_value
        .checked_sub(min_value)
        .is_none_or(|w| w as u128 >= CURATED_MIN_WIDTH);
    if !is_wide {
        return narrow_nasty_sample(min_value, max_value, rng);
    }

    // Endpoint category: the range edges and their inner neighbours (up to four
    // distinct values on the stack, so this hot path allocates nothing).
    let mut endpoints = [0i128; 4];
    let mut n_end = 0usize;
    for v in [
        min_value,
        max_value,
        min_value.saturating_add(1),
        max_value.saturating_sub(1),
    ] {
        if v >= min_value && v <= max_value && !endpoints[..n_end].contains(&v) {
            endpoints[n_end] = v;
            n_end += 1;
        }
    }

    // Interesting category: the curated interesting-value slice in range. May
    // overlap the endpoints in value (e.g. `i64::MAX`); the two are still
    // distinct mixture components.
    let interesting = &*SORTED_INTERESTING;
    let lo = interesting.partition_point(|&v| v < min_value);
    let hi = interesting.partition_point(|&v| v <= max_value);
    let interesting_slice = &interesting[lo..hi];

    // Diffuse category: the large-constant pool restricted to range.
    let diffuse = &*GLOBAL_CONSTANTS_INTEGERS;
    let dlo = diffuse.partition_point(|&v| v < min_value);
    let dhi = diffuse.partition_point(|&v| v <= max_value);
    let diffuse_slice = &diffuse[dlo..dhi];

    // One shared draw selects the category by cumulative weight; an empty
    // category is skipped so its mass flows to the next (finally the middle).
    let u = rng.random::<f64>();
    let mut acc = params.endpoint_probability;
    if u < acc && n_end > 0 {
        return Ok(endpoints[rng.random_range(0..n_end)]);
    }
    acc += params.interesting_probability;
    if u < acc && !interesting_slice.is_empty() {
        return Ok(interesting_slice[rng.random_range(0..interesting_slice.len())]);
    }
    acc += params.diffuse_probability;
    if u < acc && !diffuse_slice.is_empty() {
        return Ok(diffuse_slice[rng.random_range(0..diffuse_slice.len())]);
    }
    integer_sample_from_distribution(min_value, max_value, rng)
}

/// Boundary-biased sample for an integer range too wide for `i128` (a `BigInt`
/// choice, or a `u128` range past `i128::MAX`). Uses the same four-category
/// mixture as [`biased_i128_sample`] (endpoints, interesting, diffuse, middle),
/// weighted by this case's [`GenerationParameters`]; the middle draws a
/// roughly-uniform value via rejection sampling over the span's bit length.
fn biguint_sample_in_range(
    min: &BigInt,
    max: &BigInt,
    rng: &mut EngineRng,
    params: GenerationParameters,
) -> BigInt {
    if min == max {
        return min.clone();
    }
    let span: BigUint = (max - min).magnitude();
    let bits = span.bits();

    // Endpoint category: the range edges and their inner neighbours.
    let mut endpoints: Vec<BigInt> = Vec::with_capacity(4);
    for v in [
        min.clone(),
        max.clone(),
        min + BigInt::from(1),
        max - BigInt::from(1),
    ] {
        if &v >= min && &v <= max && !endpoints.contains(&v) {
            endpoints.push(v);
        }
    }

    // Diffuse category: powers of two spanning the range magnitude.
    let mut diffuse: Vec<BigInt> = Vec::new();
    {
        let mut push_diffuse = |v: BigInt| {
            if &v >= min && &v <= max {
                diffuse.push(v);
            }
        };
        for k in 1..=bits.min(128) {
            let p2 = BigInt::from(BigUint::from(1u32) << (k as usize));
            push_diffuse(-p2.clone());
            push_diffuse(p2);
        }
    }
    diffuse.sort();
    diffuse.dedup();

    // One shared draw selects the category by cumulative weight; an empty
    // category is skipped so its mass flows to the next (finally the middle). A
    // range beyond `i128` always has non-empty endpoints.
    let u = rng.random::<f64>();
    let mut acc = params.endpoint_probability;
    if u < acc && !endpoints.is_empty() {
        return endpoints[rng.random_range(0..endpoints.len())].clone();
    }
    acc += params.interesting_probability;
    if u < acc {
        // Interesting category (built only when selected): the interesting set
        // restricted to range.
        let mut interesting: Vec<BigInt> = Vec::new();
        for &iv in &*SORTED_INTERESTING {
            let v = BigInt::from(iv);
            if &v >= min && &v <= max {
                interesting.push(v);
            }
        }
        if !interesting.is_empty() {
            return interesting[rng.random_range(0..interesting.len())].clone();
        }
    }
    acc += params.diffuse_probability;
    if u < acc && !diffuse.is_empty() {
        return diffuse[rng.random_range(0..diffuse.len())].clone();
    }

    min + BigInt::from(sample_biguint_at_most(&span, rng))
}

/// Uniformly draw a [`BigUint`] in `[0, span]` by rejection sampling masked
/// `span.bits()`-bit values. The acceptance probability is at least 1/2 per
/// attempt (the mask bounds candidates to `[0, 2^bits - 1]` and `span >=
/// 2^(bits-1)`), so this terminates quickly. Callers (only
/// [`biguint_sample_in_range`], past its `min == max` early return) always pass
/// a strictly positive span, so `bits >= 1`.
fn sample_biguint_at_most(span: &BigUint, rng: &mut EngineRng) -> BigUint {
    let bits = span.bits();
    if bits == 0 {
        unreachable!("sample_biguint_at_most requires a positive span");
    }
    let n_bytes = bits.div_ceil(8) as usize;
    let top_bits = (bits % 8) as u32;
    loop {
        let mut bytes: Vec<u8> = (0..n_bytes).map(|_| rng.random::<u8>()).collect();
        if top_bits != 0 {
            let mask = (1u8 << top_bits) - 1;
            let last = bytes.len() - 1;
            bytes[last] &= mask;
        }
        let candidate = BigUint::from_bytes_le(&bytes);
        if &candidate <= span {
            return candidate;
        }
    }
}

/// Float counterpart of [`biased_integer_sample`]: draws boundary / "nasty"
/// values (`0.0`, `-0.0`, `±1.0`, `±MAX`, `±INFINITY`, `MIN_POSITIVE`, NaN,
/// plus the user's `min_value`/`max_value`) with probability proportional to
/// `BOUNDARY_PROBABILITY × |nasty|`, falling back to a uniform-ish lex draw
/// otherwise. Shared with the data-tree walk so novel-prefix exploration
/// hits the same boundary distribution as fresh draws.
pub(crate) fn biased_float_sample(
    fc: &FloatChoice,
    rng: &mut EngineRng,
) -> Result<f64, InternalError> {
    const SIGNALING_NAN: f64 = f64::from_bits(0x7FF0_0000_0000_0001);
    let candidates = [
        fc.min_value,
        fc.max_value,
        next_up(fc.min_value),
        fc.min_value + 1.0,
        fc.max_value - 1.0,
        next_down(fc.max_value),
        0.0,
        -0.0_f64,
        1.0,
        -1.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        SIGNALING_NAN,
        -SIGNALING_NAN,
        f64::MIN_POSITIVE,
        fc.smallest_nonzero_magnitude,
        -fc.smallest_nonzero_magnitude,
        f64::MAX,
        -f64::MAX,
    ];
    let valid_count = candidates.iter().filter(|&&v| fc.validate(v)).count();
    let nasty_threshold = (valid_count as f64 * BOUNDARY_PROBABILITY).min(0.5);

    if rng.random::<f64>() < nasty_threshold {
        let idx = rng.random_range(0..valid_count);
        let picked = candidates
            .iter()
            .copied()
            .filter(|&v| fc.validate(v))
            .nth(idx);
        return Ok(hegel_internal_unwrap!(
            picked,
            "the second validate pass found fewer candidates than valid_count"
        ));
    }
    let mag = index_to_float(rng.random::<u64>());
    let raw = if rng.random::<u64>() & 1 == 1 {
        -mag
    } else {
        mag
    };
    let f = if fc.validate(raw) {
        raw
    } else {
        float_clamp(fc, raw)
    };
    if fc.validate(f) { Ok(f) } else { fc.simplest() }
}

/// Port of Hypothesis's `make_float_clamper`: remap an out-of-range draw
/// into `[min_value, max_value]`, using its mantissa bits as a fraction of
/// the range so that distinct raw draws keep producing distinct in-range
/// values, and re-routing around the `smallest_nonzero_magnitude` band.
pub(crate) fn float_clamp(fc: &FloatChoice, raw: f64) -> f64 {
    let (min_value, max_value) = (fc.min_value.max(-f64::MAX), fc.max_value.min(f64::MAX));
    const MANTISSA_MASK: u64 = (1u64 << 52) - 1;
    let range_size = (max_value - min_value).min(f64::MAX);
    let mant = raw.abs().to_bits() & MANTISSA_MASK;
    let mut f = min_value + range_size * (mant as f64 / MANTISSA_MASK as f64);
    if f != 0.0 && f.abs() < fc.smallest_nonzero_magnitude {
        f = fc.smallest_nonzero_magnitude;
        if fc.smallest_nonzero_magnitude > max_value {
            f = -f;
        }
    }
    f.max(min_value).min(max_value)
}

/// Boundary-biased sample for bytes. Draws the simplest (`min_size` zeros),
/// the all-zeros minimum-plus-one length, or a single-`0xff` byte with
/// probability proportional to `BOUNDARY_PROBABILITY × |nasty|`, falling
/// back to a length drawn from [`many_draw_length`] with uniformly random
/// byte values.
pub(crate) fn biased_bytes_sample(
    bc: &BytesChoice,
    rng: &mut EngineRng,
) -> Result<Vec<u8>, InternalError> {
    let want_zero = bc.min_size == 0 && bc.max_size > 0;
    let want_ff = bc.min_size <= 1 && bc.max_size >= 1;
    let count = 1 + want_zero as usize + want_ff as usize;
    let nasty_threshold = count as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < nasty_threshold {
        let mut slot = rng.random_range(0..count);
        if slot == 0 {
            return Ok(bc.simplest());
        }
        slot -= 1;
        if want_zero {
            if slot == 0 {
                return Ok(vec![0u8]);
            }
            slot -= 1;
        }
        hegel_internal_debug_assert!(want_ff && slot == 0);
        return Ok(vec![0xffu8]);
    }
    let len = many_draw_length(rng, bc.min_size, bc.max_size)?;
    Ok((0..len).map(|_| rng.random::<u8>()).collect())
}

/// Sample a boolean that is `true` with probability `p`, spending exactly one
/// byte of entropy regardless of `p`.
///
/// Port of Hypothesis's `BytestringProvider.draw_boolean`: draw a single byte
/// `n ∈ [0, 256)` and return `n >= falsey`, where
/// `falsey = min(255, max(1, floor(256 * (1 - p))))`. The `min(255, …)` keeps
/// at least `n == 255` truthy for tiny `p` (for `p ≤ ~2⁻⁵⁴`, `1.0 - p` rounds
/// to exactly `1.0` and the floor alone would make `true` unreachable), and
/// the `max(1, …)` keeps at least `n == 0` falsey for `p` near one. A
/// probability needing more than 8 bits is approximated, but only slightly.
///
/// Callers must pass `0.0 < p < 1.0`; the `p <= 0` / `p >= 1` cases are forced
/// without consuming entropy by [`NativeTestCase::weighted`].
///
/// Using a single byte (rather than a full `f64`, which would burn 8 bytes per
/// boolean) matters for the urandom backend, where every byte is
/// fuzzer-controlled entropy and a one-bit decision should cost one byte.
pub(crate) fn weighted_boolean_sample(p: f64, rng: &mut EngineRng) -> bool {
    let falsey = (libm::floor(256.0 * (1.0 - p)).max(1.0) as u32).min(255);
    let mut byte = [0u8; 1];
    rng.fill_bytes(&mut byte);
    u32::from(byte[0]) >= falsey
}

/// Full-precision weighted boolean: `true` with probability `p`, faithful to
/// probabilities far below [`weighted_boolean_sample`]'s 1/256 quantization
/// floor (which would turn e.g. a stateful stop signal's `p = 2^-16` into
/// `1/256`).
///
/// Delegates to [`RngExt::random_bool`], which scales `p` to a 64-bit
/// threshold and compares it against a fresh `u64` — spending 8 bytes of
/// entropy rather than the one byte [`weighted_boolean_sample`] uses, so it is
/// reserved for draws whose probability needs the precision (routed via
/// [`NativeTestCase::weighted_precise`]); ordinary booleans keep the one-byte
/// sampler. Callers must pass `0.0 < p < 1.0` (`random_bool` panics outside
/// `[0, 1]`).
pub(crate) fn weighted_boolean_sample_precise(p: f64, rng: &mut EngineRng) -> bool {
    rng.random_bool(p)
}

/// Interesting string constants: logic keywords, numeric edge cases,
/// common Unicode stress strings. Stored as codepoint vectors so they can
/// be validated against and inserted into the draw_string nasty pool.
static GLOBAL_CONSTANTS_STRINGS: Lazy<Vec<Vec<u32>>> = Lazy::new(|| {
    let strings: &[&str] = &[
        "undefined",
        "null",
        "NULL",
        "nil",
        "NIL",
        "true",
        "false",
        "True",
        "False",
        "TRUE",
        "FALSE",
        "None",
        "none",
        "if",
        "then",
        "else",
        "__dict__",
        "__proto__",
        "0",
        "1e100",
        "0..0",
        "0/0",
        "1/0",
        "+0.0",
        "Infinity",
        "-Infinity",
        "Inf",
        "INF",
        "NaN",
        "999999999999999999999999999999",
        ",./;'[]\\-=<>?:\"{}|_+!@#$%^&*()`~",
        "Ω≈ç√∫˜µ≤≥÷åß∂ƒ©˙∆˚¬…æœ∑´®†¥¨ˆøπ\u{201C}\u{2018}¡™£¢∞§¶•ªº–≠¸˛Ç◊ı˜Â¯˘¿ÅÍÎÏ˝ÓÔÒÚÆ☃Œ„´‰ˇÁ¨ˆØ∏\u{201D}\u{2019}`⁄€‹›ﬁﬂ‡°·‚—±",
        "Ⱥ",
        "Ⱦ",
        "æœÆŒﬀʤʨß",
        "(╯°□°）╯︵ ┻━┻)",
        "😍",
        "🇺🇸",
        "🏻",
        "👍🏻",
        "الكل في المجمو عة",
        "᚛ᚄᚓᚐᚋᚒᚄ ᚑᚄᚂᚑᚏᚅ᚜",
        "กา",
        "ก ำกำ",
        "𝐓𝐡𝐞 𝐪𝐮𝐢𝐜𝐤 𝐛𝐫𝐨𝐰𝐧 𝐟𝐨𝐱 𝐣𝐮𝐦𝐩𝐬 𝐨𝐯𝐞𝐫 𝐭𝐡𝐞 𝐥𝐚𝐳𝐲 𝐝𝐨𝐠",
        "𝕿𝖍𝖊 𝖖𝖚𝖎𝖈𝖐 𝖇𝖗𝖔𝖜𝖓 𝖋𝖔𝖝 𝖏𝖚𝖒𝖕𝖘 𝖔𝖛𝖊𝖗 𝖙𝖍𝖊 𝖑𝖆𝖟𝖞 𝖉𝖔𝖌",
        "𝑻𝒉𝒆 𝒒𝒖𝒊𝒄𝒌 𝒃𝒓𝒐𝒘𝒏 𝒇𝒐𝒙 𝒋𝒖𝒎𝒑𝒔 𝒐𝒗𝒆𝒓 𝒕𝒉𝒆 𝒍𝒂𝒛𝒚 𝒅𝒐𝒈",
        "𝓣𝓱𝓮 𝓺𝓾𝓲𝓬𝓴 𝓫𝓻𝓸𝔀𝓷 𝓯𝓸𝔁 𝓳𝓾𝓶𝓹𝓼 𝓸𝓿𝓮𝓻 𝓽𝓱𝓮 𝓵𝓪𝔃𝔂 𝓭𝓸𝓰",
        "𝕋𝕙𝕖 𝕢𝕦𝕚𝕔𝕜 𝕓𝕣𝕠𝕨𝕟 𝕗𝕠𝕩 𝕛𝕦𝕞𝕡𝕤 𝕠𝕧𝕖𝕣 𝕥𝕙𝕖 𝕝𝕒𝕫𝕪 𝕕𝕠𝕘",
        "ʇǝɯɐ ʇᴉs ɹolop ɯnsdᴉ ɯǝɹo˥",
        "NUL",
        "COM1",
        "LPT1",
        "Scunthorpe",
        "Ṱ̺̺̕o͞ ̷i̲̬͇̪͙n̝̗͕v̟̜̘̦͟o̶̙̰̠kè͚̮̺̪̹̱̤ ̖t̝͕̳̣̻̪͞h̼͓̲̦̳̘̲e͇̣̰̦̬͎ ̢̼̻̱̘h͚͎͙̜̣̲ͅi̦̲̣̰̤v̻͍e̺̭̳̪̰-m̢iͅn̖̺̞̲̯̰d̵̼̟͙̩̼̘̳ ̞̥̱̳̭r̛̗̘e͙p͠r̼̞̻̭̗e̺̠̣͟s̘͇̳͍̝͉e͉̥̯̞̲͚̬͜ǹ̬͎͎̟̖͇̤t͍̬̤͓̼̭͘ͅi̪̱n͠g̴͉ ͏͉ͅc̬̟h͡a̫̻̯͘o̫̟̖͍̙̝͉s̗̦̲.̨̹͈̣",
        "मनीष منش",
        "पन्ह पन्ह त्र र्च कृकृ ड्ड न्हृे إلا بسم الله",
        "lorem لا بسم الله ipsum 你好1234你好",
        "a\u{000A}b\u{000D}c\u{0085}d\u{000B}e\u{000C}f\u{2028}g\u{2029}h\u{000D}\u{000A}i",
    ];
    strings
        .iter()
        .map(|s| s.chars().map(|c| c as u32).collect::<Vec<u32>>())
        .collect()
});

/// Boundary-biased sample for strings. Builds a "nasty" pool from the
/// simplest values plus [`GLOBAL_CONSTANTS_STRINGS`] entries that satisfy
/// the kind's constraint, drawing from it with probability proportional to
/// `count * BOUNDARY_PROBABILITY`. Otherwise picks a small 1–10 codepoint
/// sub-alphabet from the kind's [`IntervalSet`] (biased toward the
/// first 256 shrink-order positions for large alphabets, an ASCII bias)
/// and samples a length-`many_draw_length` string from it.
///
/// The sub-alphabet step concentrates draws into a small character set so
/// that string-shape bugs (repeated characters, ordering, run-length) get
/// exercised within a feasible test budget. A pure first-256 uniform draw
/// from the full alphabet (~1.1M codepoints) almost never produces the
/// `XXY`-shape strings that property tests of, for example, run-length
/// encoding need to find.
/// Per-alphabet cache of which [`GLOBAL_CONSTANTS_STRINGS`] entries consist
/// solely of codepoints the alphabet contains. Validating the ~60 constants
/// (some 40+ codepoints long) against the alphabet on every string draw is
/// the dominant cost of `biased_string_sample`, and the containment result
/// depends only on the immutable `IntervalSet`, so it is memoised per
/// allocation. Entries are keyed by the `Arc`'s address with a `Weak`
/// identity check, so an address reused after a drop cannot serve a stale
/// mask — it recomputes and overwrites its slot.
fn constants_in_alphabet(intervals: &Arc<IntervalSet>) -> Arc<[bool]> {
    type Cache = Mutex<HashMap<usize, (alloc::sync::Weak<IntervalSet>, Arc<[bool]>)>>;
    static CACHE: Lazy<Cache> = Lazy::new(|| Mutex::new(HashMap::default()));
    let key = Arc::as_ptr(intervals) as usize;
    {
        let guard = CACHE.lock();
        if let Some((weak, mask)) = guard.get(&key) {
            if weak
                .upgrade()
                .is_some_and(|live| Arc::ptr_eq(&live, intervals))
            {
                return Arc::clone(mask);
            }
        }
    }
    let mask: Arc<[bool]> = GLOBAL_CONSTANTS_STRINGS
        .iter()
        .map(|cps| cps.iter().all(|&cp| intervals.contains(cp)))
        .collect();
    CACHE
        .lock()
        .insert(key, (Arc::downgrade(intervals), Arc::clone(&mask)));
    mask
}

pub(crate) fn biased_string_sample(
    sc: &StringChoice,
    rng: &mut EngineRng,
) -> Result<Vec<u32>, InternalError> {
    if sc.intervals.is_empty() {
        return Ok(Vec::new());
    }
    let want_empty = sc.min_size == 0 && sc.max_size > 0;
    let want_one = sc.min_size <= 1 && sc.max_size >= 1;
    let want_two = sc.min_size <= 2 && sc.max_size >= 2;
    let small_count = 1 + want_empty as usize + want_one as usize + want_two as usize;
    let global_pool = &*GLOBAL_CONSTANTS_STRINGS;
    let contained = constants_in_alphabet(&sc.intervals);
    let size_ok = |cps: &[u32]| sc.min_size <= cps.len() && cps.len() <= sc.max_size;
    let valid_global_count = global_pool
        .iter()
        .zip(contained.iter())
        .filter(|(cps, m)| **m && size_ok(cps))
        .count();
    let count = small_count + valid_global_count;
    let threshold = (count as f64 * BOUNDARY_PROBABILITY).min(0.5);
    if rng.random::<f64>() < threshold {
        let idx = rng.random_range(0..count);
        if idx < small_count {
            let simplest_cp = sc.simplest_codepoint()?;
            let mut slot = idx;
            if slot == 0 {
                return sc.simplest();
            }
            slot -= 1;
            if want_empty {
                if slot == 0 {
                    return Ok(Vec::new());
                }
                slot -= 1;
            }
            if want_one {
                if slot == 0 {
                    return Ok(vec![simplest_cp]);
                }
                slot -= 1;
            }
            hegel_internal_debug_assert!(want_two && slot == 0);
            return Ok(vec![simplest_cp, simplest_cp]);
        }
        let picked = global_pool
            .iter()
            .zip(contained.iter())
            .filter(|(cps, m)| **m && size_ok(cps))
            .nth(idx - small_count);
        let (cps, _) = hegel_internal_unwrap!(
            picked,
            "the second validate pass found fewer candidates than valid_global_count"
        );
        return Ok(cps.clone());
    }

    let alpha = sc.intervals.len();
    let pick_position = |rng: &mut EngineRng| -> usize {
        if alpha > 256 {
            if rng.random::<f64>() < 0.2 {
                rng.random_range(256..alpha)
            } else {
                rng.random_range(0..256)
            }
        } else {
            rng.random_range(0..alpha)
        }
    };

    let alpha_size = rng.random_range(1..=10.min(alpha));
    let mut sub_alphabet: Vec<u32> = Vec::with_capacity(alpha_size);
    while sub_alphabet.len() < alpha_size {
        let cp = sc.intervals.char_in_shrink_order(pick_position(rng)) as u32;
        sub_alphabet.push(cp);
    }

    let len = many_draw_length(rng, sc.min_size, sc.max_size)?;
    Ok((0..len)
        .map(|_| sub_alphabet[rng.random_range(0..sub_alphabet.len())])
        .collect())
}

/// Convert a codepoint sequence to a Rust `String`, dropping any surrogate
/// codepoints (`0xD800..=0xDFFF`). The engine never produces surrogates
/// during generation (rejected by `validate` and by `biased_string_sample`),
/// but a user-supplied prefix could feed one in.
pub(crate) fn codepoints_to_string(cps: &[u32]) -> String {
    cps.iter().filter_map(|&cp| char::from_u32(cp)).collect()
}

/// The smallest nonnegative integer not in `used`.
fn smallest_unused_id(used: &BTreeSet<i64>) -> i64 {
    let mut candidate = 0;
    for &id in used {
        if id > candidate {
            break;
        }
        if id == candidate {
            candidate += 1;
        }
    }
    candidate
}

/// A pool of variable IDs for stateful testing.
pub struct NativeVariables {
    variables: Vec<i64>,
    removed: crate::native::HashSet<i64>,
}

impl NativeVariables {
    pub fn new() -> Self {
        NativeVariables {
            variables: Vec::new(),
            removed: crate::native::HashSet::default(),
        }
    }

    /// Add a variable ID (from [`NativeTestCase::draw_fresh_id`]) to the pool.
    pub fn add(&mut self, id: i64) {
        self.variables.push(id);
    }

    /// Return the IDs of variables that have not been consumed, in order.
    pub fn active(&self) -> Vec<i64> {
        self.variables
            .iter()
            .filter(|id| !self.removed.contains(*id))
            .copied()
            .collect()
    }

    /// Mark a variable as consumed and trim trailing consumed variables.
    pub fn consume(&mut self, variable_id: i64) {
        self.removed.insert(variable_id);
        while let Some(&last) = self.variables.last() {
            if self.removed.contains(&last) {
                self.variables.pop();
                self.removed.remove(&last);
            } else {
                break;
            }
        }
    }
}

/// A span within the choice sequence, labelled by draw kind or by the
/// numeric label of an enclosing `start_span` call.
///
/// Recorded to enable span-mutation exploration (see `try_span_mutation`)
/// and to expose the structure of a test case to the shrinker, mutator,
/// and assertion-style tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub label: String,
    /// Depth of this span in the span tree. The top-level span has depth 0.
    pub depth: u32,
    /// Index of the directly-enclosing span, or `None` for the top-level span.
    pub parent: Option<usize>,
    /// True iff this span's `stop_span` was called with `discard=true`.
    pub discarded: bool,
}

/// A span-boundary event, captured live (in `start_span` / `stop_span`) in
/// fire order so the data tree can faithfully replay the span structure —
/// including zero-width spans, whose open/close order can't be recovered from
/// the finished [`Span`] list alone — without re-executing the test body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanEvent {
    /// `start_span(label)` was called.
    Open { label: u64 },
    /// `stop_span(discarded)` was called.
    Close { discarded: bool },
}

/// Maximum nested span depth before the engine marks the test case
/// `Status::Invalid`.
pub const MAX_DEPTH: u32 = 100;

/// A tag identifying a structural-coverage class for a span label.
///
/// Two tags compare equal iff they were produced from the same label, and
/// [`structural_coverage`] interns them so that callers also get
/// pointer-equal results for equal labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoverageTag {
    pub label: u64,
}

static STRUCTURAL_COVERAGE_CACHE: Lazy<Mutex<HashMap<u64, &'static CoverageTag>>> =
    Lazy::new(|| Mutex::new(HashMap::default()));

/// Look up (or insert) the [`CoverageTag`] for `label`.
///
/// Repeated calls with the same `label` return the same `&'static`
/// reference.
pub fn structural_coverage(label: u64) -> &'static CoverageTag {
    let mut cache = STRUCTURAL_COVERAGE_CACHE.lock();
    cache
        .entry(label)
        .or_insert_with(|| Box::leak(Box::new(CoverageTag { label })))
}

/// A collection of spans recorded during a single test case, with
/// wrap-around signed indexing semantics on top of [`Vec<Span>`].
///
/// Indexing accepts negative indices (`-1` is the last span) and panics
/// with an "out of range" message on out-of-bounds access.
#[derive(Clone, Debug, Default)]
pub struct Spans {
    inner: Vec<Span>,
}

impl Spans {
    /// Construct an empty `Spans` collection.
    pub fn new() -> Self {
        Spans { inner: Vec::new() }
    }

    /// Number of recorded spans.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if no spans have been recorded.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Append a span (interior bookkeeping; pushes after any
    /// already-recorded spans).
    pub fn push(&mut self, span: Span) {
        self.inner.push(span);
    }

    /// Mutable access to a span by raw index.
    pub fn get_mut(&mut self, i: usize) -> Option<&mut Span> {
        self.inner.get_mut(i)
    }

    /// Access by raw (non-negative) index, returning `None` on
    /// out-of-bounds. Analogous to `Vec::get`.
    pub fn get(&self, i: usize) -> Option<&Span> {
        self.inner.get(i)
    }

    /// True iff every non-forced choice inside the span at `span_idx` is at
    /// its kind's simplest value.  A forced choice can't be lowered further,
    /// so it counts as trivial for this purpose.  Out-of-range `span_idx`
    /// returns `false`.
    pub fn trivial(&self, span_idx: usize, nodes: &[ChoiceNode]) -> Result<bool, InternalError> {
        let Some(span) = self.inner.get(span_idx) else {
            return Ok(false);
        };
        let end = span.end.min(nodes.len());
        if span.start > end {
            return Ok(false);
        }
        for n in &nodes[span.start..end] {
            if !(n.was_forced || n.data.is_simplest()?) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// View as a slice, for code that wants raw indexing.
    pub fn as_slice(&self) -> &[Span] {
        &self.inner
    }

    /// Consume the collection and return the underlying `Vec`.
    pub fn into_vec(self) -> Vec<Span> {
        self.inner
    }
}

impl From<Vec<Span>> for Spans {
    fn from(inner: Vec<Span>) -> Self {
        Spans { inner }
    }
}

impl core::ops::Deref for Spans {
    type Target = [Span];
    fn deref(&self) -> &[Span] {
        &self.inner
    }
}

impl<'a> IntoIterator for &'a Spans {
    type Item = &'a Span;
    type IntoIter = core::slice::Iter<'a, Span>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl core::ops::Index<usize> for Spans {
    type Output = Span;
    fn index(&self, i: usize) -> &Span {
        &self.inner[i]
    }
}

/// Observer hook called by [`NativeTestCase`] after each draw and on
/// conclusion.  All methods have default no-op implementations so
/// concrete observers only need to override the callbacks they care
/// about.
pub trait DataObserver: Send {
    fn draw_boolean(&mut self, _value: bool, _was_forced: bool) {}
    fn draw_integer(&mut self, _value: &BigInt, _was_forced: bool) {}
    fn draw_float(&mut self, _value: f64, _was_forced: bool) {}
    fn draw_bytes(&mut self, _value: &[u8], _was_forced: bool) {}
    fn draw_string(&mut self, _value: &str, _was_forced: bool) {}
    fn conclude_test(&mut self, _status: Status, _origin: Option<InterestingOrigin>) {}
}

/// Shared handle to one stream of a test case family. The root stream is
/// owned directly by whoever constructed it; cloned streams are only ever
/// reachable through this handle.
pub type NativeTestCaseHandle = Arc<Mutex<NativeTestCase>>;

/// The [`Status`] mirror value meaning "not concluded yet" in
/// [`FamilyCore::concluded_status`].
const STATUS_UNSET: u8 = u8::MAX;

/// State shared by every stream of one test-case *family*: the root
/// [`NativeTestCase`] plus every stream cloned from it, directly or
/// transitively, via [`NativeTestCase::clone_stream`].
///
/// A family concludes exactly once: the first stream to conclude wins, and
/// every other stream's subsequent draws fail fast with the family's
/// verdict. The draw budget and `tc.target()` observations are likewise
/// family-wide. Collections, variable pools, and state machines live in
/// caller-owned handles rather than here; any stream of the family can
/// drive one, drawing from its own choice sequence.
pub struct FamilyCore {
    /// The family's write-once conclusion, with the interesting origin for
    /// an `Interesting` verdict.
    conclusion: Mutex<Option<(Status, Option<InterestingOrigin>)>>,
    /// Lock-free mirror of the conclusion's status for the draw hot path:
    /// [`STATUS_UNSET`] until concluded, then the status discriminant.
    concluded_status: AtomicU8,
    /// Draws made across every stream of the family.
    total_draws: AtomicUsize,
    /// Cap on [`Self::total_draws`]. `usize::MAX` for bare replays, whose
    /// per-stream prefix caps already bound every stream; the requested
    /// `max_size` whenever an RNG or trailing template can extend draws.
    budget: AtomicUsize,
    /// `tc.target()` observations, keyed by label. Family-wide so the
    /// once-per-test-case label uniqueness holds across clones.
    pub(crate) target_observations: Mutex<HashMap<String, f64>>,
    /// When set, state machines draw no step cap and never report their
    /// rule sequence as done. Set for single-test-case runs, which explore
    /// one unbounded test case instead of many capped ones.
    state_machine_steps_unbounded: AtomicBool,
    /// Target number of rounds a stateful test case runs. Bounds the
    /// per-round stop decision in [`NativeStateMachine::next_group`];
    /// ignored when [`Self::state_machine_steps_unbounded`] is set.
    /// Defaults to 50, overridden per run from the `stateful_step_count`
    /// setting.
    stateful_step_count: AtomicI64,
    /// Set when a state machine with `max_concurrency > 1` was requested on
    /// any stream of this family: the test asked for real concurrency, so
    /// its behaviour depends on thread scheduling and the run driving this
    /// family is nondeterministic. Set even when the creation itself is
    /// rejected (see [`Self::reject_concurrent_machine`]). The engine reads
    /// this after each execution and flips the whole run into
    /// nondeterministic mode.
    concurrent_machine: AtomicBool,
    /// Set by the engine on every test case of a run that is not (yet)
    /// known to be nondeterministic: a state machine creation with
    /// `max_concurrency > 1` must then fail with an assume violation, so
    /// the case is discarded like a failed assumption while
    /// [`Self::concurrent_machine`] still tells the engine to flip the run.
    /// Every later case is stamped as nondeterministic up front, so its
    /// whole execution — including draws made before the machine is
    /// created — can be emitted for the failure report. Defaults to false
    /// (allow), which standalone handles (single-test-case runs, blob
    /// replays, embeddings driving the engine directly) keep.
    reject_concurrent_machine: AtomicBool,
    /// Identifiers handed out by [`NativeTestCase::draw_fresh_id`], family-wide
    /// so an identifier is unique across every stream of the test case.
    fresh_ids: Mutex<BTreeSet<i64>>,
    /// This test case's swarm [`GenerationParameters`], drawn once when the
    /// root stream's RNG is attached (see [`NativeTestCase::with_random`]) and
    /// shared by every clone-stream so the whole case has one consistent
    /// distribution. Unset for replay-only families, which never sample.
    generation_parameters: OnceBox<GenerationParameters>,
}

impl FamilyCore {
    fn new(budget: usize) -> Self {
        FamilyCore {
            conclusion: Mutex::new(None),
            concluded_status: AtomicU8::new(STATUS_UNSET),
            total_draws: AtomicUsize::new(0),
            budget: AtomicUsize::new(budget),
            target_observations: Mutex::new(HashMap::default()),
            state_machine_steps_unbounded: AtomicBool::new(false),
            stateful_step_count: AtomicI64::new(50),
            concurrent_machine: AtomicBool::new(false),
            reject_concurrent_machine: AtomicBool::new(false),
            fresh_ids: Mutex::new(BTreeSet::new()),
            generation_parameters: OnceBox::new(),
        }
    }

    /// Record that a state machine with `max_concurrency > 1` was requested
    /// on a stream of this family (see [`Self::concurrent_machine`]).
    pub(crate) fn set_concurrent_machine(&self) {
        self.concurrent_machine.store(true, Ordering::Relaxed);
    }

    /// Whether a state machine with `max_concurrency > 1` was requested on
    /// any stream of this family.
    pub(crate) fn concurrent_machine(&self) -> bool {
        self.concurrent_machine.load(Ordering::Relaxed)
    }

    /// Set whether a state machine creation with `max_concurrency > 1`
    /// must be rejected on this family (see
    /// [`Self::reject_concurrent_machine`]).
    pub(crate) fn set_reject_concurrent_machine(&self, reject: bool) {
        self.reject_concurrent_machine
            .store(reject, Ordering::Relaxed);
    }

    /// Whether a state machine creation with `max_concurrency > 1` must be
    /// rejected on this family.
    pub(crate) fn reject_concurrent_machine(&self) -> bool {
        self.reject_concurrent_machine.load(Ordering::Relaxed)
    }

    /// Record this test case's swarm parameters. Called once when the root
    /// stream's RNG is attached; later calls (there are none in practice) are
    /// ignored by the `OnceBox`.
    fn set_generation_parameters(&self, params: GenerationParameters) {
        let _ = self.generation_parameters.set(Box::new(params));
    }

    /// This test case's swarm parameters, or [`GenerationParameters::default`]
    /// if none were drawn (a replay-only family, which never samples).
    pub(crate) fn generation_parameters(&self) -> GenerationParameters {
        self.generation_parameters
            .get()
            .copied()
            .unwrap_or_default()
    }

    /// Make every state machine of this family run without a step cap.
    pub(crate) fn set_state_machine_steps_unbounded(&self) {
        self.state_machine_steps_unbounded
            .store(true, Ordering::Relaxed);
    }

    /// Whether state machines of this family run without a step cap.
    pub(crate) fn state_machine_steps_unbounded(&self) -> bool {
        self.state_machine_steps_unbounded.load(Ordering::Relaxed)
    }

    /// Set the target number of steps a stateful test case runs.
    pub(crate) fn set_stateful_step_count(&self, count: i64) {
        self.stateful_step_count.store(count, Ordering::Relaxed);
    }

    /// The target number of steps a stateful test case runs.
    pub(crate) fn stateful_step_count(&self) -> i64 {
        self.stateful_step_count.load(Ordering::Relaxed)
    }

    /// The family's concluded status, or `None` while still running.
    pub fn status(&self) -> Option<Status> {
        match self.concluded_status.load(Ordering::Acquire) {
            STATUS_UNSET => None,
            raw => Some(match raw {
                0 => Status::EarlyStop,
                1 => Status::Invalid,
                2 => Status::Valid,
                3 => Status::Interesting,
                _ => unreachable!("concluded_status only stores Status discriminants"),
            }),
        }
    }

    /// Claim the family-wide conclusion. First caller wins; later calls are
    /// no-ops.
    pub fn conclude(&self, status: Status, origin: Option<InterestingOrigin>) {
        let mut guard = self.conclusion.lock();
        if guard.is_none() {
            *guard = Some((status, origin));
            self.concluded_status.store(status as u8, Ordering::Release);
        }
    }

    /// The full conclusion (status plus origin), or `None` while running.
    pub fn conclusion(&self) -> Option<(Status, Option<InterestingOrigin>)> {
        self.conclusion.lock().clone()
    }

    /// Reserve one draw against the family budget. Returns `false` when the
    /// budget is exhausted.
    fn try_take_draw(&self) -> bool {
        self.total_draws.fetch_add(1, Ordering::Relaxed) < self.budget.load(Ordering::Relaxed)
    }

    fn set_budget(&self, budget: usize) {
        self.budget.store(budget, Ordering::Relaxed);
    }
}

/// A test case backed by a sequence of typed choices.
///
/// During random generation, choices are drawn from the RNG.
/// During replay/shrinking, choices are drawn from a prefix.
///
/// One `NativeTestCase` is one *stream* of a test-case family: the root
/// stream, or a cloned stream created by [`Self::clone_stream`]. Each stream
/// has its own prefix, RNG, nodes, and span structure, so streams driven
/// from different threads generate independently; the conclusion, draw
/// budget, and stateful bookkeeping are shared through [`FamilyCore`].
pub struct NativeTestCase {
    prefix: Vec<ChoiceValue>,
    prefix_nodes: Option<Vec<ChoiceNode>>,
    rng: Option<EngineRng>,
    max_size: usize,
    pub nodes: Vec<ChoiceNode>,
    /// Set to `true` by [`Self::freeze`] on the first call; subsequent calls
    /// are no-ops. A dedicated boolean (rather than checking the family's
    /// status) lets `conclude_test` conclude before calling `freeze()`
    /// without triggering the idempotency early-return.
    frozen: bool,
    /// Whether this test case belongs to a run already known to be
    /// nondeterministic. Copied into every cloned stream.
    is_nondeterministic: bool,
    /// State shared with every other stream of this test case's family.
    pub(crate) family: Arc<FamilyCore>,
    /// This stream's position in the clone tree: empty for the root, the
    /// parent's id plus the parent's clone counter for a cloned stream.
    clone_id: Vec<usize>,
    /// Number of clones made from this stream so far.
    clone_counter: usize,
    /// Streams cloned from this one, each with the index of its clone node
    /// in [`Self::nodes`]. Drained by [`Self::reassemble`].
    clone_children: Vec<(usize, NativeTestCaseHandle)>,
    pub spans: Spans,
    /// Indices into `spans` for currently-open spans, in nesting order.
    /// Each entry was pushed by `start_span` and is awaiting a matching
    /// `stop_span` call.
    pub span_stack: Vec<usize>,
    /// Span open/close events in fire order, each tagged with the draw
    /// position (`nodes.len()`) at which it occurred. Recorded so the data
    /// tree can replay the span structure faithfully (see [`SpanEvent`]).
    pub span_events: Vec<(usize, SpanEvent)>,
    /// True iff any `stop_span(discard=true)` has been observed during this test
    /// case. Filters that retry mark the rejected attempts as discarded, which
    /// the shrinker uses to prioritise removing them.
    pub has_discards: bool,
    /// Structural-coverage tags accumulated by closing non-discarded
    /// spans. When a span closes without `discard`, every label collected
    /// by it (including its non-discarded descendants) is added here as a
    /// [`structural_coverage`] tag. Discarded spans drop their labels
    /// (and their descendants' labels) on the floor.
    pub tags: HashSet<&'static CoverageTag>,
    /// Per-open-span sets of labels awaiting promotion into [`Self::tags`].
    ///
    /// Each `start_span` pushes a fresh `{label}` frame; `stop_span`
    /// pops it and either merges the frame into its parent (non-discard)
    /// or discards it (discard). When the outermost frame closes
    /// without discard, its labels are converted to [`CoverageTag`]s
    /// and added to `tags`.
    labels_for_structure_stack: Vec<HashSet<u64>>,
    /// Optional observer notified after each draw and on conclusion.
    /// Set by [`Self::for_choices`] and called by each draw method and
    /// by [`Self::freeze`].
    observer: Option<Box<dyn DataObserver>>,
    /// Optional template applied to every draw past the explicit `prefix`.
    /// `count` is mutated in-place as draws consume the template; when
    /// `count` reaches zero the next draw is overrun
    /// (`Status::EarlyStop` + `EngineError`). `None` means "no template" —
    /// draws past the prefix go to `rng` or panic, as before.
    trailing_template: Option<ChoiceTemplate>,
}

impl NativeTestCase {
    pub fn new_random(rng: EngineRng) -> Result<Self, InternalError> {
        Self::for_choices_and_template(&[], None, None, BUFFER_SIZE, None).with_random(rng)
    }

    /// Like [`Self::new_random`], but generating from the given swarm
    /// parameters rather than drawing fresh ones — used by the exploration
    /// loop so the novel-prefix walk and the test case share one distribution.
    pub fn new_random_with_params(rng: EngineRng, params: GenerationParameters) -> Self {
        Self::for_choices_and_template(&[], None, None, BUFFER_SIZE, None)
            .with_random_and_params(rng, params)
    }

    /// Replay `choices` in order, then for every further draw resolve via
    /// `trailing` if set.
    ///
    /// `max_size` is the upper bound on the total number of choices the test
    /// case will make.  It is floored to `choices.len()` so a too-tight value
    /// can never truncate the explicit prefix.
    pub fn for_choices_and_template(
        choices: &[ChoiceValue],
        prefix_nodes: Option<&[ChoiceNode]>,
        trailing: Option<ChoiceTemplate>,
        max_size: usize,
        observer: Option<Box<dyn DataObserver>>,
    ) -> Self {
        let max_size = max_size.max(choices.len());
        let budget = if trailing.is_some() {
            max_size
        } else {
            usize::MAX
        };
        Self::new_stream(
            choices.to_vec(),
            prefix_nodes.map(|n| n.to_vec()),
            None,
            trailing,
            max_size,
            observer,
            false,
            Arc::new(FamilyCore::new(budget)),
            Vec::new(),
        )
    }

    /// Build one stream — the root (fresh family) or a clone (shared
    /// family). The only place a `NativeTestCase` is constructed.
    fn new_stream(
        prefix: Vec<ChoiceValue>,
        prefix_nodes: Option<Vec<ChoiceNode>>,
        rng: Option<EngineRng>,
        trailing_template: Option<ChoiceTemplate>,
        max_size: usize,
        observer: Option<Box<dyn DataObserver>>,
        is_nondeterministic: bool,
        family: Arc<FamilyCore>,
        clone_id: Vec<usize>,
    ) -> Self {
        NativeTestCase {
            prefix,
            prefix_nodes,
            rng,
            max_size,
            nodes: Vec::new(),
            frozen: false,
            is_nondeterministic,
            family,
            clone_id,
            clone_counter: 0,
            clone_children: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            span_events: Vec::new(),
            has_discards: false,
            tags: HashSet::default(),
            labels_for_structure_stack: Vec::new(),
            observer,
            trailing_template,
        }
    }

    /// A test case where every draw past the explicit prefix returns
    /// `kind.simplest()` of the requested choice kind. A deterministic
    /// all-simplest probe of the choice tree's "left leaf" before random
    /// sampling begins.
    pub fn for_simplest(max_size: usize) -> Result<Self, InternalError> {
        Ok(Self::for_choices_and_template(
            &[],
            None,
            Some(ChoiceTemplate::simplest(None)?),
            max_size,
            None,
        ))
    }

    /// Construct a `NativeTestCase` that replays `choices` in order,
    /// notifying `observer` after each draw and on conclusion.
    pub fn for_choices(
        choices: &[ChoiceValue],
        prefix_nodes: Option<&[ChoiceNode]>,
        observer: Option<Box<dyn DataObserver>>,
    ) -> Self {
        Self::for_choices_and_template(choices, prefix_nodes, None, choices.len(), observer)
    }

    /// A test case that replays `prefix` for the first positions and then
    /// draws randomly from `rng` for subsequent positions, up to a total of
    /// `max_size` choices.
    ///
    /// Used by `mutate_and_shrink`.
    pub fn for_probe(
        prefix: &[ChoiceValue],
        rng: EngineRng,
        max_size: usize,
    ) -> Result<Self, InternalError> {
        Self::for_choices_and_template(prefix, None, None, max_size, None).with_random(rng)
    }

    /// Like [`Self::for_probe`], but generating from the given swarm parameters
    /// rather than drawing fresh ones — used by the exploration loop so the
    /// novel-prefix walk and the test-case tail share one distribution.
    pub fn for_probe_with_params(
        prefix: &[ChoiceValue],
        rng: EngineRng,
        max_size: usize,
        params: GenerationParameters,
    ) -> Self {
        Self::for_choices_and_template(prefix, None, None, max_size, None)
            .with_random_and_params(rng, params)
    }

    /// Attach an RNG for post-prefix random draws.  Internal builder used by
    /// `new_random` and `for_probe` to share the [`Self::for_choices_and_template`]
    /// constructor without duplicating the struct literal. Random draws can
    /// extend any stream, so the family budget becomes the requested
    /// `max_size` rather than the bare-replay `usize::MAX`.
    fn with_random(self, mut rng: EngineRng) -> Result<Self, InternalError> {
        // Draw this test case's swarm parameters from the RNG up front, before
        // any value is sampled. Callers that generate a novel prefix separately
        // (the main exploration loop) draw the parameters themselves and use
        // [`Self::with_random_and_params`] so the prefix walk and the test case
        // share one distribution; the simpler callers get a fresh draw here.
        let params = GenerationParameters::draw(&mut rng)?;
        Ok(self.with_random_and_params(rng, params))
    }

    /// Attach an RNG and use the given, already-drawn swarm parameters (rather
    /// than drawing fresh ones). The parameters are held on the shared family,
    /// so every draw and every clone-stream of this test case generates from
    /// one consistent distribution.
    fn with_random_and_params(mut self, rng: EngineRng, params: GenerationParameters) -> Self {
        self.family.set_generation_parameters(params);
        self.rng = Some(rng);
        self.family.set_budget(self.max_size);
        self
    }

    /// The family state shared by every stream of this test case.
    pub(crate) fn family(&self) -> &Arc<FamilyCore> {
        &self.family
    }

    /// Mark this test case as belonging to a nondeterministic run.
    pub(crate) fn set_nondeterministic(&mut self) {
        self.is_nondeterministic = true;
    }

    /// Whether this test case belongs to a nondeterministic run.
    pub(crate) fn is_nondeterministic(&self) -> bool {
        self.is_nondeterministic
    }

    /// Create an independent cloned stream of this test case.
    ///
    /// The clone occupies one choice position in this stream (a
    /// [`ChoiceKind::Clone`] node) and gets its own prefix, RNG, and span
    /// structure, so it can be drawn from concurrently with every other
    /// stream without perturbing them. On replay, a [`ChoiceValue::Clone`]
    /// prefix value at this position hands the child its recorded stream; a
    /// non-clone prefix value puns to an empty child (which overruns on its
    /// first draw in a bare replay, or extends randomly under a probe).
    ///
    /// Fails with the family's verdict if the family has concluded, and
    /// marks the family invalid when clones nest deeper than
    /// [`MAX_CLONE_DEPTH`].
    pub fn clone_stream(&mut self) -> Result<NativeTestCaseHandle, EngineError> {
        self.pre_choice()?;
        if self.clone_id.len() + 1 > MAX_CLONE_DEPTH {
            self.conclude(Status::Invalid, None);
            self.freeze();
            return Err(EngineError::InvalidTestCase);
        }
        let idx = self.nodes.len();
        let (child_prefix, child_prefix_nodes) = match self.prefix.get(idx) {
            Some(ChoiceValue::Clone(record)) => (
                record.owned_values(),
                record.realized_nodes().map(<[ChoiceNode]>::to_vec),
            ),
            _ => (Vec::new(), None),
        };
        let child_rng = self.rng.as_mut().map(EngineRng::spawn);
        let child_template = self.trailing_template.as_ref().map(|t| ChoiceTemplate {
            kind: t.kind,
            count: None,
        });
        let child_max_size = if child_rng.is_some() || child_template.is_some() {
            usize::MAX
        } else {
            child_prefix.len()
        };
        let mut child_id = self.clone_id.clone();
        child_id.push(self.clone_counter);
        self.clone_counter += 1;

        let child = Self::new_stream(
            child_prefix,
            child_prefix_nodes,
            child_rng,
            child_template,
            child_max_size,
            None,
            self.is_nondeterministic,
            Arc::clone(&self.family),
            child_id,
        );
        let handle = Arc::new(Mutex::new(child));
        self.nodes.push(ChoiceNode::clone_stream(
            Arc::new(RealizedStream::empty()),
            false,
        ));
        self.clone_children.push((idx, Arc::clone(&handle)));
        Ok(handle)
    }

    /// Replace each clone node's placeholder value with the realized record
    /// of its stream — nodes, spans, and span events — recursively, so
    /// [`Self::nodes`] becomes the self-contained pieced-together choice
    /// sequence of the whole family.
    ///
    /// A no-op until the family has concluded: streams can still grow while
    /// the family is running, and a concluded family's streams cannot (every
    /// draw fails fast), so the records are snapshotted exactly once.
    pub fn reassemble(&mut self) {
        if self.family.status().is_none() {
            return;
        }
        for (idx, handle) in core::mem::take(&mut self.clone_children) {
            let mut child = handle.lock();
            child.freeze();
            child.reassemble();
            let stream = RealizedStream::new(
                child.nodes.clone(),
                child.spans.clone().into_vec(),
                child.span_events.clone(),
            );
            let was_forced = self.nodes[idx].was_forced;
            self.nodes[idx] = ChoiceNode::clone_stream(Arc::new(stream), was_forced);
        }
    }

    /// Open a new span at the current choice position, labelled with `label`.
    ///
    /// Returns the index assigned to the span in `self.spans`.  The span's
    /// `end` is set to `self.nodes.len()` as a placeholder and overwritten
    /// when [`Self::stop_span`] is called.
    ///
    /// If opening this span would push depth past [`MAX_DEPTH`], the test
    /// case is marked invalid and `start_span` returns the assigned index
    /// without further bookkeeping; subsequent draws on a frozen test case
    /// will trip the existing freeze guard.
    pub fn start_span(&mut self, label: u64) -> usize {
        let parent = self.span_stack.last().copied();
        let depth = self.span_stack.len() as u32;
        let idx = self.spans.len();
        let start = self.nodes.len();
        self.spans.push(Span {
            start,
            end: start,
            label: label.to_string(),
            depth,
            parent,
            discarded: false,
        });
        self.span_stack.push(idx);
        self.span_events.push((start, SpanEvent::Open { label }));
        let mut frame = HashSet::default();
        frame.insert(label);
        self.labels_for_structure_stack.push(frame);
        if depth + 1 > MAX_DEPTH {
            self.conclude(Status::Invalid, None);
            self.freeze();
        }
        idx
    }

    /// Close the innermost currently-open span.
    ///
    /// `discard=true` marks the span as discarded (used by filter retries
    /// to flag rejected attempts) and sets `has_discards` on the test case.
    pub fn stop_span(&mut self, discard: bool) {
        let Some(idx) = self.span_stack.pop() else {
            return;
        };
        let end = self.nodes.len();
        if let Some(span) = self.spans.get_mut(idx) {
            span.end = end;
            span.discarded = discard;
        }
        self.span_events
            .push((end, SpanEvent::Close { discarded: discard }));
        if discard {
            self.has_discards = true;
        }
        let labels = self.labels_for_structure_stack.pop().unwrap_or_default();
        if !discard {
            if let Some(parent) = self.labels_for_structure_stack.last_mut() {
                parent.extend(labels);
            } else {
                self.tags
                    .extend(labels.into_iter().map(structural_coverage));
            }
        }
    }

    /// Mark the test case as completed, defaulting to `Status::Valid` when
    /// no terminal status was set during the run.
    ///
    /// Idempotent: calling `freeze()` on an already-frozen test case is
    /// a no-op (early return on `self.frozen`).
    ///
    /// Closes any currently-open spans, setting their `end` to the final
    /// choice position, so that freeze implicitly closes intervals left
    /// open by an exception or overrun.
    pub fn freeze(&mut self) {
        if self.frozen {
            return;
        }
        self.frozen = true;
        let end = self.nodes.len();
        while let Some(idx) = self.span_stack.pop() {
            if let Some(span) = self.spans.get_mut(idx) {
                span.end = end;
            }
        }
        self.conclude(Status::Valid, None);
        if let Some(ref mut obs) = self.observer {
            let (status, origin) = self
                .family
                .conclusion()
                .unwrap_or_else(|| unreachable!("freeze just concluded the family"));
            obs.conclude_test(status, origin);
        }
    }

    /// Conclude the test case with `status` (and `origin`, for an interesting
    /// verdict). This is the single, write-once status assignment for the
    /// whole family: if any stream has already concluded — an overrun or
    /// failed assume during a draw, a too-deep nesting, or the body's
    /// reported verdict — that conclusion stands and this is a no-op. Every
    /// status the engine or the test body assigns flows through here, so a
    /// concluded case can never be re-concluded.
    pub fn conclude(&mut self, status: Status, origin: Option<InterestingOrigin>) {
        self.family.conclude(status, origin);
    }

    /// The family's concluded status, or `None` while still running.
    pub fn status(&self) -> Option<Status> {
        self.family.status()
    }

    /// Draw a random integer in `[min_value, max_value]`.
    ///
    /// The type parameter `T` determines the input and output type.
    /// Internally all arithmetic uses `BigInt`; the bounds are widened on
    /// entry and the result is narrowed back to `T` on exit.
    pub fn draw_integer<T: Into<BigInt> + TryFrom<BigInt>>(
        &mut self,
        min_value: T,
        max_value: T,
    ) -> Result<T, EngineError> {
        let min_value = min_value.into();
        let max_value = max_value.into();
        hegel_internal_assert!(
            min_value <= max_value,
            "Invalid range [{min_value:?}, {max_value:?}]"
        );

        let kind = IntegerChoice {
            min_value,
            max_value,
            shrink_towards: BigInt::zero(),
        };

        let params = self.family.generation_parameters();
        let (v, was_forced) = self.resolve_choice(
            || Ok(kind.simplest()),
            || Ok(kind.unit()),
            |v| match v {
                ChoiceValue::Integer(n) if kind.validate(n) => Some(n.clone()),
                _ => None,
            },
            |rng| biased_integer_sample(&kind, rng, params),
        )?;

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(&v, was_forced);
        }

        self.nodes
            .push(ChoiceNode::integer(kind, v.clone(), was_forced));

        Ok(hegel_internal_unwrap!(
            T::try_from(v).ok(),
            "draw_integer: validated value does not fit the requested width"
        ))
    }

    /// Record a forced integer draw in `[min_value, max_value]`.
    ///
    /// Mirrors `weighted(_, forced: Some(_))`: consumes a choice position
    /// without consulting the prefix or RNG, recording the node as forced so
    /// the shrinker and data tree leave it alone.
    pub fn draw_integer_forced<T: Into<BigInt>>(
        &mut self,
        min_value: T,
        max_value: T,
        forced: T,
    ) -> Result<(), EngineError> {
        let kind = IntegerChoice {
            min_value: min_value.into(),
            max_value: max_value.into(),
            shrink_towards: BigInt::zero(),
        };
        let v: BigInt = forced.into();
        hegel_internal_assert!(
            kind.min_value <= v && v <= kind.max_value,
            "forced value {v:?} outside [{:?}, {:?}]",
            kind.min_value,
            kind.max_value
        );

        self.pre_choice()?;

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(&v, true);
        }

        self.nodes.push(ChoiceNode::integer(kind, v, true));

        Ok(())
    }

    /// Draw an integer identifier that is arbitrary but unique within this
    /// test case's family.
    ///
    /// The identifier is recorded in the choice sequence *by value*, so a
    /// replayed identifier stays stable when unrelated earlier draws are
    /// deleted during shrinking. A replayed value that is already in use is
    /// repaired to the smallest unused identifier; fresh generation always
    /// hands out the smallest unused identifier.
    ///
    /// The recorded range is `[0, max + 2]`, where `max` is the largest
    /// identifier the family has ever drawn (`-1` before the first draw, so
    /// the first range is `[0, 1]`). The `+ 2` headroom keeps single-deletion
    /// holes stable: with smallest-unused generation every identifier sits at
    /// the top of its window, so a `+ 1` bound would push the next survivor's
    /// recorded value out of range as soon as one earlier addition is
    /// deleted, and the renumbering would cascade. Anchoring on ids *ever
    /// drawn* (the registry, which only grows) rather than any live pool
    /// keeps the bound monotone within a run, and it always admits the
    /// smallest-unused fallback, so the recorded range never depends on the
    /// realized value. The registry lock is held across validation and
    /// registration, so concurrently drawing streams cannot realize
    /// duplicate identifiers. For a single-threaded body the registry state
    /// at each draw is a deterministic function of the values drawn so far,
    /// so the recorded kind at a choice-tree position never varies; racing
    /// clone streams can skew each other's windows, but their records live
    /// in clone nodes, which tolerate kind drift.
    pub fn draw_fresh_id(&mut self) -> Result<i64, EngineError> {
        let family = Arc::clone(&self.family);
        let mut used = family.fresh_ids.lock();
        let window_hi = used.iter().next_back().copied().unwrap_or(-1) + 2;
        let fallback = smallest_unused_id(&used);
        let (v, was_forced) = self.resolve_choice(
            || Ok(BigInt::from(fallback)),
            || Ok(BigInt::from(fallback)),
            |v| match v {
                ChoiceValue::Integer(n)
                    if n.to_i64()
                        .is_some_and(|id| (0..=window_hi).contains(&id) && !used.contains(&id)) =>
                {
                    Some(n.clone())
                }
                _ => None,
            },
            |_rng| Ok(BigInt::from(fallback)),
        )?;

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(&v, was_forced);
        }

        let id = hegel_internal_unwrap!(v.to_i64(), "draw_fresh_id: id does not fit i64");
        let kind = IntegerChoice {
            min_value: BigInt::zero(),
            max_value: BigInt::from(window_hi),
            shrink_towards: BigInt::zero(),
        };
        self.nodes.push(ChoiceNode::integer(kind, v, was_forced));
        used.insert(id);
        Ok(id)
    }

    /// Draw one of `members` (nonnegative, not necessarily sorted or
    /// deduplicated), recording the chosen member *by value* rather than as
    /// an index into `members`.
    ///
    /// A replayed value that is no longer a member is repaired to the
    /// largest member below it (or the smallest member overall), so a
    /// recorded choice keeps meaning the same member while that member
    /// survives, references shrink towards earlier members, and deleting
    /// other members never shifts what a recorded choice refers to.
    ///
    /// The recorded range is `[0, max + 1]` with `max` the largest
    /// identifier the family has ever drawn. Members always come from
    /// [`Self::draw_fresh_id`] (the pool pattern) and the registry only
    /// grows, so every member fits the range even when concurrent streams
    /// add to the pool, and the recorded kind never depends on the realized
    /// value. The range is small enough for novel-prefix generation to
    /// enumerate, and it keeps one identifier of headroom so a replayed
    /// value just above a deleted top member still validates and repairs
    /// monotonically; values beyond the window are punned to the smallest
    /// member.
    pub fn draw_from_set(&mut self, members: &[i64]) -> Result<i64, EngineError> {
        hegel_internal_assert!(
            !members.is_empty(),
            "draw_from_set requires at least one member"
        );
        let mut sorted = members.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        hegel_internal_assert!(sorted[0] >= 0, "draw_from_set requires nonnegative members");
        let window_hi = {
            let used = self.family.fresh_ids.lock();
            used.iter().next_back().copied().unwrap_or(-1) + 1
        };
        hegel_internal_assert!(
            *sorted.last().unwrap() <= window_hi,
            "draw_from_set members must be identifiers from draw_fresh_id"
        );
        let smallest = sorted[0];
        let (v, was_forced) = self.resolve_choice(
            || Ok(BigInt::from(smallest)),
            || Ok(BigInt::from(smallest)),
            |v| match v {
                ChoiceValue::Integer(n)
                    if n.to_i64().is_some_and(|id| (0..=window_hi).contains(&id)) =>
                {
                    Some(n.clone())
                }
                _ => None,
            },
            |rng| {
                let idx = rng.random_range(0..sorted.len());
                Ok(BigInt::from(sorted[idx]))
            },
        )?;

        let raw = hegel_internal_unwrap!(v.to_i64(), "draw_from_set: value does not fit i64");
        let chosen = match sorted.binary_search(&raw) {
            Ok(idx) => sorted[idx],
            Err(0) => sorted[0],
            Err(idx) => sorted[idx - 1],
        };
        let kind = IntegerChoice {
            min_value: BigInt::zero(),
            max_value: BigInt::from(window_hi),
            shrink_towards: BigInt::zero(),
        };
        let value = BigInt::from(chosen);

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(&value, was_forced);
        }

        self.nodes
            .push(ChoiceNode::integer(kind, value, was_forced));
        Ok(chosen)
    }

    /// Draw a floating-point value in `[min_value, max_value]`. NaN is drawn
    /// only when `allow_nan` is set; ±∞ only when `allow_infinity` is set and
    /// the relevant endpoint is unbounded. Magnitudes below
    /// `smallest_nonzero_magnitude` (other than zero itself) are never drawn;
    /// pass `5e-324` for no restriction.
    pub fn draw_float(
        &mut self,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
        smallest_nonzero_magnitude: f64,
    ) -> Result<f64, EngineError> {
        let kind = FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
            smallest_nonzero_magnitude,
        };

        let (v, was_forced) = self.resolve_choice(
            || kind.simplest(),
            || kind.unit(),
            |v| match v {
                ChoiceValue::Float(f) if kind.validate(*f) => Some(*f),
                _ => None,
            },
            |rng| biased_float_sample(&kind, rng),
        )?;

        self.nodes.push(ChoiceNode::float(kind, v, was_forced));

        if let Some(ref mut obs) = self.observer {
            obs.draw_float(v, was_forced);
        }

        Ok(v)
    }

    /// Draw a bytes value with length in `[min_size, max_size]`.
    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Result<Vec<u8>, EngineError> {
        hegel_internal_assert!(
            min_size <= max_size,
            "min_size ({min_size}) must be <= max_size ({max_size})"
        );
        let kind = BytesChoice { min_size, max_size };

        let (v, was_forced) = self.resolve_choice(
            || Ok(kind.simplest()),
            || Ok(kind.unit()),
            |v| match v {
                ChoiceValue::Bytes(b) if kind.validate(b) => Some(b.clone()),
                _ => None,
            },
            |rng| biased_bytes_sample(&kind, rng),
        )?;

        self.nodes
            .push(ChoiceNode::bytes(kind, v.clone(), was_forced));

        if let Some(ref mut obs) = self.observer {
            obs.draw_bytes(&v, was_forced);
        }

        Ok(v)
    }

    /// Draw a Unicode string with length in `[min_size, max_size]` whose
    /// codepoints lie in the given [`IntervalSet`] alphabet.
    pub fn draw_string(
        &mut self,
        intervals: Arc<IntervalSet>,
        min_size: usize,
        max_size: usize,
    ) -> Result<String, EngineError> {
        hegel_internal_assert!(min_size <= max_size);
        hegel_internal_assert!(
            !intervals.is_empty() || max_size == 0,
            "draw_string with empty alphabet must have max_size == 0"
        );

        let kind = StringChoice {
            intervals,
            min_size,
            max_size,
        };

        let (v, was_forced) = self.resolve_choice(
            || kind.simplest(),
            || kind.unit(),
            |v| match v {
                ChoiceValue::String(s) if kind.validate(s) => Some(s.clone()),
                _ => None,
            },
            |rng| biased_string_sample(&kind, rng),
        )?;

        self.nodes
            .push(ChoiceNode::string(kind, v.clone(), was_forced));

        let s = codepoints_to_string(&v);
        if let Some(ref mut obs) = self.observer {
            obs.draw_string(&s, was_forced);
        }

        Ok(s)
    }

    /// Draw a boolean with probability `p` of being true, sampled with the
    /// one-byte [`weighted_boolean_sample`]. If `forced` is Some, the result is
    /// forced to that value.
    pub fn weighted(&mut self, p: f64, forced: Option<bool>) -> Result<bool, EngineError> {
        self.weighted_with(p, forced, weighted_boolean_sample)
    }

    /// Like [`Self::weighted`], but samples with the full-precision
    /// [`weighted_boolean_sample_precise`], so probabilities below the one-byte
    /// sampler's 1/256 floor (e.g. a stateful stop signal at `p = 2^-16`) are
    /// honored. Routed here from `generate_boolean`.
    pub fn weighted_precise(&mut self, p: f64, forced: Option<bool>) -> Result<bool, EngineError> {
        self.weighted_with(p, forced, weighted_boolean_sample_precise)
    }

    fn weighted_with(
        &mut self,
        p: f64,
        forced: Option<bool>,
        sample: impl Fn(f64, &mut EngineRng) -> bool,
    ) -> Result<bool, EngineError> {
        let kind = BooleanChoice { p };

        let forced_value = forced.or(if p <= 0.0 {
            Some(false)
        } else if p >= 1.0 {
            Some(true)
        } else {
            None
        });

        let (v, was_forced) = if let Some(f) = forced_value {
            self.pre_choice()?;
            (f, true)
        } else {
            self.resolve_choice(
                || Ok(kind.simplest()),
                || Ok(kind.unit()),
                |v| match v {
                    ChoiceValue::Boolean(b) => Some(*b),
                    _ => None,
                },
                |rng| Ok(sample(p, rng)),
            )?
        };

        self.nodes.push(ChoiceNode::boolean(kind, v, was_forced));

        if let Some(ref mut obs) = self.observer {
            obs.draw_boolean(v, was_forced);
        }

        Ok(v)
    }
    fn pre_choice(&mut self) -> Result<(), EngineError> {
        if let Some(status) = self.family.status() {
            return Err(match status {
                Status::Invalid => EngineError::InvalidTestCase,
                _ => EngineError::Overrun,
            });
        }
        if self.nodes.len() >= self.max_size || !self.family.try_take_draw() {
            self.conclude(Status::EarlyStop, None);
            return Err(EngineError::Overrun);
        }
        Ok(())
    }

    /// Resolve a typed choice value from forced, prefix, or random.
    ///
    /// `from_prefix` both validates a replayed prefix value against the
    /// draw's constraint and extracts the typed payload, so a successful
    /// replay hands back a value proven to fit the draw. A prefix value
    /// that doesn't fit puns exactly as before: to the draw's `simplest()`
    /// when the stale value was its original kind's simplest, and to
    /// `unit()` otherwise.
    fn resolve_choice<V>(
        &mut self,
        simplest: impl FnOnce() -> Result<V, InternalError>,
        unit: impl FnOnce() -> Result<V, InternalError>,
        from_prefix: impl FnOnce(&ChoiceValue) -> Option<V>,
        random: impl FnOnce(&mut EngineRng) -> Result<V, InternalError>,
    ) -> Result<(V, bool), EngineError> {
        self.pre_choice()?;

        let idx = self.nodes.len();

        if idx < self.prefix.len() {
            let prefix_value = &self.prefix[idx];
            if let Some(v) = from_prefix(prefix_value) {
                return Ok((v, false));
            }
            let is_simplest = match self.prefix_nodes.as_ref().and_then(|pn| pn.get(idx)) {
                Some(pn) => *prefix_value == pn.data.simplest_value()?,
                None => false,
            };
            return Ok((if is_simplest { simplest()? } else { unit()? }, false));
        }

        if let Some(template) = self.trailing_template.as_mut() {
            if matches!(template.count, Some(0)) {
                self.conclude(Status::EarlyStop, None);
                return Err(EngineError::Overrun);
            }
            let value = match template.kind {
                ChoiceTemplateKind::Simplest => simplest()?,
            };
            if let Some(c) = template.count.as_mut() {
                *c -= 1;
            }
            return Ok((value, false));
        }

        let rng = hegel_internal_unwrap!(
            self.rng.as_mut(),
            "resolve_choice: no RNG available for random generation"
        );
        Ok((random(rng)?, false))
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_clone_tests.rs"]
mod clone_tests;
