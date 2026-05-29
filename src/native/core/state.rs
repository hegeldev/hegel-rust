// Stateful types: NativeTestCase, ManyState, NativeVariables, Span.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{LazyLock, Mutex};

use rand::RngExt;
use rand::rngs::SmallRng;

use super::choices::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceTemplate, ChoiceTemplateKind,
    ChoiceValue, EngineError, FloatChoice, IntegerChoice, InterestingOrigin, Status, StringChoice,
};
use super::float_index::lex_to_float;
use super::{BOUNDARY_PROBABILITY, BUFFER_SIZE};
use crate::native::intervalsets::IntervalSet;
use crate::native::statistics::{
    Distribution, LogStudentTDistribution, PiecewiseDistribution, UniformDistribution,
};

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
static GLOBAL_CONSTANTS_INTEGERS: LazyLock<Vec<i128>> = LazyLock::new(|| {
    let mut base: Vec<i128> = Vec::new();
    // Powers of 2 (2^16 to 2^65)
    for n in 16u32..66 {
        base.push(1i128 << n);
    }
    // Powers of 10 (10^5 to 10^19)
    let mut p10 = 100_000i128;
    for _ in 5..20u32 {
        base.push(p10);
        p10 *= 10;
    }
    // Factorials (9! to 20!)
    let mut f = 362_880i128; // 9!
    base.push(f);
    for i in 10u32..=20 {
        f *= i as i128;
        base.push(f);
    }
    // Primorial numbers
    base.extend_from_slice(&[
        510_510i128,
        6_469_693_230,
        304_250_263_527_210,
        32_589_158_477_190_044_730,
    ]);
    // Extend with n-1 and n+1
    let n_base = base.len();
    for i in 0..n_base {
        base.push(base[i] - 1);
        base.push(base[i] + 1);
    }
    // Extend with negations of all values so far
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
fn many_draw_length(rng: &mut SmallRng, min_size: usize, max_size: usize) -> usize {
    if min_size == max_size {
        return min_size;
    }
    let p_continue = length_p_continue(min_size, Some(max_size));
    // Geometric variate: `extra ~ floor(log(U) / log(p_continue))` for
    // `U ~ Uniform(0, 1)`. `rng.random::<f64>()` returns `[0, 1)`, so `U`
    // can be exactly `0` — that yields `-inf / log(p) = +inf` which
    // saturates to `usize::MAX` via the float cast; the final `.min` clamps.
    let u: f64 = rng.random();
    let extra = (u.ln() / p_continue.ln()).floor();
    assert!(extra >= 0.0);
    min_size.saturating_add(extra as usize).min(max_size)
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
static INTEGERS_DISTRIBUTION: LazyLock<
    PiecewiseDistribution<UniformDistribution, LogStudentTDistribution>,
> = LazyLock::new(|| {
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
fn integer_sample_from_distribution(min_value: i128, max_value: i128, rng: &mut SmallRng) -> i128 {
    let dist = &*INTEGERS_DISTRIBUTION;
    // i128 endpoints can lose precision crossing into f64, but the final
    // `clamp` mops up any out-of-range round-off so the contract holds.
    let lo = dist.cdf(min_value as f64 - 0.5);
    let hi = dist.cdf(max_value as f64 + 0.5);
    // A tighter CDF window than ~1e-13 leaves the inverse-CDF nothing to
    // spread samples across, so collapse to uniform.
    if hi - lo < 1e-13 {
        return rng.random_range(min_value..=max_value);
    }
    // `inverse_cdf` requires strictly `0 < p < 1`. `rng.random::<f64>()`
    // returns `[0, 1)`, so `p < hi ≤ 1` already; the only way to land on
    // an endpoint is `p == 0.0`, which needs `lo == 0.0` and a zero-bit
    // draw. `.max(f64::MIN_POSITIVE)` nudges that case to the smallest
    // positive float — equivalent to the inverse-CDF at the very far
    // tail, which the final `clamp` then brings back into range.
    let p = (lo + rng.random::<f64>() * (hi - lo)).max(f64::MIN_POSITIVE);
    // `f64 as i128` saturates out-of-range values to ±i128::MAX, then
    // `clamp` brings them into the requested range.
    (dist.inverse_cdf(p).round() as i128).clamp(min_value, max_value)
}

/// Hand-picked "interesting" boundary values: powers of two and their
/// neighbours, plus the `i{16,32,64}::{MIN,MAX}` boundaries. Merged into
/// [`SORTED_NASTY_POOL`] at startup.
static INTERESTING_INTEGERS: &[i128] = &[
    0,
    1,
    -1,
    2,
    -2,
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

/// Sorted, deduped union of [`INTERESTING_INTEGERS`] and
/// [`GLOBAL_CONSTANTS_INTEGERS`]. Used by [`biased_integer_sample`] to find
/// the in-range boundary candidates via two `partition_point` calls instead
/// of an O(n²) per-call dedup loop.
static SORTED_NASTY_POOL: LazyLock<Vec<i128>> = LazyLock::new(|| {
    let mut all: Vec<i128> = INTERESTING_INTEGERS
        .iter()
        .copied()
        .chain(GLOBAL_CONSTANTS_INTEGERS.iter().copied())
        .collect();
    all.sort_unstable();
    all.dedup();
    all
});

/// Boundary-biased sample for integers.
///
/// Implements the "nasty value" boost used by both the
/// [`NativeTestCase::draw_integer`] code path and the data-tree
/// novel-prefix walk. Sharing the implementation keeps the two
/// random-generation routes consistent: when `generate_novel_prefix`
/// chooses a child to recurse into, it now picks special values
/// (0, 1, ±powers-of-two, factorials, …) with the same frequency as
/// `draw_integer` does for fresh draws.
///
/// Returns a value in `[ic.min_value, ic.max_value]` (inclusive). With
/// probability proportional to `count * BOUNDARY_PROBABILITY` (≈ 0.5 for
/// unbounded ranges, where `count` is the number of in-range boundary
/// values including `min_value` and `max_value`) the result is one of those
/// nasty/interesting values; otherwise it is drawn from
/// [`INTEGERS_DISTRIBUTION`] restricted to the requested range.
pub(crate) fn biased_integer_sample(ic: &IntegerChoice, rng: &mut SmallRng) -> i128 {
    if ic.min_value == ic.max_value {
        return ic.min_value;
    }
    // The static boundary pool is sorted, so the in-range subset is a
    // contiguous slice that two binary searches locate in O(log n).
    let pool = &*SORTED_NASTY_POOL;
    let lo = pool.partition_point(|&v| v < ic.min_value);
    let hi = pool.partition_point(|&v| v <= ic.max_value);
    let static_slice = &pool[lo..hi];
    // `ic.min_value` / `ic.max_value` are always candidates; add them only
    // if the static slice doesn't already cover them (then `min < max` past
    // the early return guarantees they're distinct).
    let need_min = static_slice.first() != Some(&ic.min_value);
    let need_max = static_slice.last() != Some(&ic.max_value);
    let count = static_slice.len() + (need_min as usize) + (need_max as usize);
    let threshold = count as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < threshold {
        let idx = rng.random_range(0..count);
        if need_min && idx == 0 {
            ic.min_value
        } else if need_max && idx == count - 1 {
            ic.max_value
        } else {
            static_slice[idx - need_min as usize]
        }
    } else {
        integer_sample_from_distribution(ic.min_value, ic.max_value, rng)
    }
}

/// Float counterpart of [`biased_integer_sample`]: draws boundary / "nasty"
/// values (`0.0`, `-0.0`, `±1.0`, `±MAX`, `±INFINITY`, `MIN_POSITIVE`, NaN,
/// plus the user's `min_value`/`max_value`) with probability proportional to
/// `BOUNDARY_PROBABILITY × |nasty|`, falling back to a uniform-ish lex draw
/// otherwise. Shared with the data-tree walk so novel-prefix exploration
/// hits the same boundary distribution as fresh draws.
pub(crate) fn biased_float_sample(fc: &FloatChoice, rng: &mut SmallRng) -> f64 {
    let bounded = fc.min_value.is_finite() && fc.max_value.is_finite();
    let half_bounded = !bounded && (fc.min_value.is_finite() || fc.max_value.is_finite());

    let candidates = [
        fc.min_value,
        fc.max_value,
        0.0,
        -0.0_f64,
        1.0,
        -1.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::MIN_POSITIVE,
        f64::MAX,
        -f64::MAX,
    ];
    let valid_count = candidates.iter().filter(|&&v| fc.validate(v)).count();
    let nasty_threshold = valid_count as f64 * BOUNDARY_PROBABILITY;

    if rng.random::<f64>() < nasty_threshold {
        let idx = rng.random_range(0..valid_count);
        // Walk the fixed-size array again to find the idx-th in-range entry.
        // 12 elements, no allocation; cheaper than the legacy Vec<f64>.
        let mut skip = idx;
        for &v in candidates.iter() {
            if fc.validate(v) {
                if skip == 0 {
                    return v;
                }
                skip -= 1;
            }
        }
        unreachable!("valid_count agrees with the second validate pass");
    }
    let f = if bounded {
        let r: f64 = rng.random();
        let v = fc.min_value + r * (fc.max_value - fc.min_value);
        v.max(fc.min_value).min(fc.max_value)
    } else if half_bounded {
        let use_inf = fc.allow_infinity && rng.random::<f64>() < 0.05;
        if use_inf {
            if fc.max_value == f64::INFINITY {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            }
        } else {
            loop {
                let bits: u64 = rng.random();
                let mag = lex_to_float(bits).abs();
                if mag.is_finite() {
                    break if fc.min_value.is_finite() {
                        fc.min_value + mag
                    } else {
                        fc.max_value - mag
                    };
                }
            }
        }
    } else if fc.allow_nan && rng.random::<f64>() < 0.01 {
        let exponent: u64 = 0x7FF << 52;
        let sign: u64 = (rng.random::<u64>() >> 63) << 63;
        let mantissa: u64 = (rng.random::<u64>() & ((1u64 << 52) - 1)).max(1);
        f64::from_bits(sign | exponent | mantissa)
    } else {
        loop {
            let bits: u64 = rng.random();
            let v = lex_to_float(bits);
            if !v.is_nan() {
                break v;
            }
        }
    };
    if fc.validate(f) { f } else { fc.simplest() }
}

/// Boundary-biased sample for bytes. Draws the simplest (`min_size` zeros),
/// the all-zeros minimum-plus-one length, or a single-`0xff` byte with
/// probability proportional to `BOUNDARY_PROBABILITY × |nasty|`, falling
/// back to a length drawn from [`many_draw_length`] with uniformly random
/// byte values.
pub(crate) fn biased_bytes_sample(bc: &BytesChoice, rng: &mut SmallRng) -> Vec<u8> {
    let want_zero = bc.min_size == 0 && bc.max_size > 0;
    let want_ff = bc.min_size <= 1 && bc.max_size >= 1;
    // At most 3 candidates: simplest(), [0x00], [0xff]. Compute the count
    // without materialising the Vec<Vec<u8>>, then synthesise the chosen one.
    let count = 1 + want_zero as usize + want_ff as usize;
    let nasty_threshold = count as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < nasty_threshold {
        let mut slot = rng.random_range(0..count);
        if slot == 0 {
            return bc.simplest();
        }
        slot -= 1;
        if want_zero {
            if slot == 0 {
                return vec![0u8];
            }
            slot -= 1;
        }
        debug_assert!(want_ff && slot == 0);
        return vec![0xffu8];
    }
    let len = many_draw_length(rng, bc.min_size, bc.max_size);
    (0..len).map(|_| rng.random::<u8>()).collect()
}

/// Interesting string constants: logic keywords, numeric edge cases,
/// common Unicode stress strings. Stored as codepoint vectors so they can
/// be validated against and inserted into the draw_string nasty pool.
static GLOBAL_CONSTANTS_STRINGS: LazyLock<Vec<Vec<u32>>> = LazyLock::new(|| {
    let strings: &[&str] = &[
        // strings interpretable as code / logic
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
        // strings interpretable as numbers
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
        // common ASCII punctuation / special chars
        ",./;'[]\\-=<>?:\"{}|_+!@#$%^&*()`~",
        // common Unicode characters
        "Ω≈ç√∫˜µ≤≥÷åß∂ƒ©˙∆˚¬…æœ∑´®†¥¨ˆøπ\u{201C}\u{2018}¡™£¢∞§¶•ªº–≠¸˛Ç◊ı˜Â¯˘¿ÅÍÎÏ˝ÓÔÒÚÆ☃Œ„´‰ˇÁ¨ˆØ∏\u{201D}\u{2019}`⁄€‹›ﬁﬂ‡°·‚—±",
        // characters that increase in length when lowercased
        "Ⱥ",
        "Ⱦ",
        // ligatures
        "æœÆŒﬀʤʨß",
        // emoticons
        "(╯°□°）╯︵ ┻━┻)",
        // emojis
        "😍",
        "🇺🇸",
        "🏻",
        "👍🏻",
        // RTL text
        "الكل في المجمو عة",
        // Ogham text
        "᚛ᚄᚓᚐᚋᚒᚄ ᚑᚄᚂᚑᚏᚅ᚜",
        // Thai consonant + spacing vowel
        "กา",
        "ก ำกำ",
        // mathematical bold/fraktur/script text
        "𝐓𝐡𝐞 𝐪𝐮𝐢𝐜𝐤 𝐛𝐫𝐨𝐰𝐧 𝐟𝐨𝐱 𝐣𝐮𝐦𝐩𝐬 𝐨𝐯𝐞𝐫 𝐭𝐡𝐞 𝐥𝐚𝐳𝐲 𝐝𝐨𝐠",
        "𝕿𝖍𝖊 𝖖𝖚𝖎𝖈𝖐 𝖇𝖗𝖔𝖜𝖓 𝖋𝖔𝖝 𝖏𝖚𝖒𝖕𝖘 𝖔𝖛𝖊𝖗 𝖙𝖍𝖊 𝖑𝖆𝖟𝖞 𝖉𝖔𝖌",
        "𝑻𝒉𝒆 𝒒𝒖𝒊𝒄𝒌 𝒃𝒓𝒐𝒘𝒏 𝒇𝒐𝒙 𝒋𝒖𝒎𝒑𝒔 𝒐𝒗𝒆𝒓 𝒕𝒉𝒆 𝒍𝒂𝒛𝒚 𝒅𝒐𝒈",
        "𝓣𝓱𝓮 𝓺𝓾𝓲𝓬𝓴 𝓫𝓻𝓸𝔀𝓷 𝓯𝓸𝔁 𝓳𝓾𝓶𝓹𝓼 𝓸𝓿𝓮𝓻 𝓽𝓱𝓮 𝓵𝓪𝔃𝔂 𝓭𝓸𝓰",
        "𝕋𝕙𝕖 𝕢𝕦𝕚𝕔𝕜 𝕓𝕣𝕠𝕨𝕟 𝕗𝕠𝕩 𝕛𝕦𝕞𝕡𝕤 𝕠𝕧𝕖𝕣 𝕥𝕙𝕖 𝕝𝕒𝕫𝕪 𝕕𝕠𝕘",
        // upside-down text
        "ʇǝɯɐ ʇᴉs ɹolop ɯnsdᴉ ɯǝɹo˥",
        // Windows reserved names
        "NUL",
        "COM1",
        "LPT1",
        // Scunthorpe problem
        "Scunthorpe",
        // zalgo text
        "Ṱ̺̺̕o͞ ̷i̲̬͇̪͙n̝̗͕v̟̜̘̦͟o̶̙̰̠kè͚̮̺̪̹̱̤ ̖t̝͕̳̣̻̪͞h̼͓̲̦̳̘̲e͇̣̰̦̬͎ ̢̼̻̱̘h͚͎͙̜̣̲ͅi̦̲̣̰̤v̻͍e̺̭̳̪̰-m̢iͅn̖̺̞̲̯̰d̵̼̟͙̩̼̘̳ ̞̥̱̳̭r̛̗̘e͙p͠r̼̞̻̭̗e̺̠̣͟s̘͇̳͍̝͉e͉̥̯̞̲͚̬͜ǹ̬͎͎̟̖͇̤t͍̬̤͓̼̭͘ͅi̪̱n͠g̴͉ ͏͉ͅc̬̟h͡a̫̻̯͘o̫̟̖͍̙̝͉s̗̦̲.̨̹͈̣",
        // examples from https://faultlore.com/blah/text-hates-you/
        "मनीष منش",
        "पन्ह पन्ह त्र र्च कृकृ ड्ड न्हृे إلا بسم الله",
        "lorem لا بسم الله ipsum 你好1234你好",
        // unconditional Unicode line-break characters (UAX #14)
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
pub(crate) fn biased_string_sample(sc: &StringChoice, rng: &mut SmallRng) -> Vec<u32> {
    let want_empty = sc.min_size == 0 && sc.max_size > 0;
    let want_one = sc.min_size <= 1 && sc.max_size >= 1;
    let want_two = sc.min_size <= 2 && sc.max_size >= 2;
    let small_count = 1 + want_empty as usize + want_one as usize + want_two as usize;
    // Count the in-range global candidates without materialising them. The
    // pool only has ~70 entries; one validate pass is cheap and avoids the
    // per-call `Vec<Vec<u32>>` allocation and the legacy O(n²) `contains`.
    // Note: the legacy code also deduped against the small-candidate set, but
    // those entries are all monomorphic runs of `simplest_codepoint()`, none
    // of which occur in `GLOBAL_CONSTANTS_STRINGS` — so the dedup never fired
    // in practice.
    let global_pool = &*GLOBAL_CONSTANTS_STRINGS;
    let valid_global_count = global_pool.iter().filter(|cps| sc.validate(cps)).count();
    let count = small_count + valid_global_count;
    let threshold = count as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < threshold {
        let idx = rng.random_range(0..count);
        if idx < small_count {
            // Materialise the chosen small candidate. Order is fixed:
            // simplest, then empty, [cp], [cp, cp] in the conditional slots.
            let simplest_cp = sc.simplest_codepoint();
            let mut slot = idx;
            if slot == 0 {
                return sc.simplest();
            }
            slot -= 1;
            if want_empty {
                if slot == 0 {
                    return Vec::new();
                }
                slot -= 1;
            }
            if want_one {
                if slot == 0 {
                    return vec![simplest_cp];
                }
                slot -= 1;
            }
            debug_assert!(want_two && slot == 0);
            return vec![simplest_cp, simplest_cp];
        }
        // Walk the global pool again to find the `(idx - small_count)`-th
        // in-range entry. Two passes of ~70 ≪ the old `clone` + `contains`.
        let mut skip = idx - small_count;
        for cps in global_pool.iter() {
            if sc.validate(cps) {
                if skip == 0 {
                    return cps.clone();
                }
                skip -= 1;
            }
        }
        unreachable!("valid_global_count agrees with the second validate pass");
    }

    let alpha = sc.intervals.len();
    let pick_position = |rng: &mut SmallRng| -> usize {
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

    let len = many_draw_length(rng, sc.min_size, sc.max_size);
    (0..len)
        .map(|_| sub_alphabet[rng.random_range(0..sub_alphabet.len())])
        .collect()
}

/// Convert a codepoint sequence to a Rust `String`, dropping any surrogate
/// codepoints (`0xD800..=0xDFFF`). The engine never produces surrogates
/// during generation (rejected by `validate` and by `biased_string_sample`),
/// but a user-supplied prefix could feed one in.
pub(crate) fn codepoints_to_string(cps: &[u32]) -> String {
    cps.iter().filter_map(|&cp| char::from_u32(cp)).collect()
}

/// A pool of variable IDs for stateful testing.
pub struct NativeVariables {
    last_id: i64,
    variables: Vec<i64>,
    removed: std::collections::HashSet<i64>,
}

impl NativeVariables {
    pub fn new() -> Self {
        NativeVariables {
            last_id: 0,
            variables: Vec::new(),
            removed: std::collections::HashSet::new(),
        }
    }

    /// Add a new variable and return its ID.
    pub fn next(&mut self) -> i64 {
        self.last_id += 1;
        self.variables.push(self.last_id);
        self.last_id
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

/// A span within the choice sequence, labelled by schema type or by the
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

static STRUCTURAL_COVERAGE_CACHE: LazyLock<Mutex<HashMap<u64, &'static CoverageTag>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Look up (or insert) the [`CoverageTag`] for `label`.
///
/// Repeated calls with the same `label` return the same `&'static`
/// reference.
pub fn structural_coverage(label: u64) -> &'static CoverageTag {
    let mut cache = STRUCTURAL_COVERAGE_CACHE.lock().unwrap();
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

    /// Access by signed index with wrap-around (`-1` = last).  Returns
    /// `None` for any out-of-range index.
    pub fn get_signed(&self, i: i64) -> Option<&Span> {
        let n = self.inner.len() as i64;
        if i < -n || i >= n {
            return None;
        }
        let idx = if i < 0 { (i + n) as usize } else { i as usize };
        self.inner.get(idx)
    }

    /// Indices of the direct children of the span at `i`, in
    /// preorder (the order in which they were started).
    ///
    /// Computed from each span's `parent` field; runs in O(n) over the
    /// span list.
    pub fn children(&self, i: usize) -> Vec<usize> {
        self.inner
            .iter()
            .enumerate()
            .filter_map(|(j, s)| (s.parent == Some(i)).then_some(j))
            .collect()
    }

    /// True iff every non-forced choice inside the span at `span_idx` is at
    /// its kind's simplest value.  A forced choice can't be lowered further,
    /// so it counts as trivial for this purpose.  Out-of-range `span_idx`
    /// returns `false`.
    pub fn trivial(&self, span_idx: usize, nodes: &[ChoiceNode]) -> bool {
        let Some(span) = self.inner.get(span_idx) else {
            return false;
        };
        let end = span.end.min(nodes.len());
        nodes[span.start..end]
            .iter()
            .all(|n| n.was_forced || n.value == n.kind.simplest())
    }

    /// View as a slice, for code that wants raw indexing.
    pub fn as_slice(&self) -> &[Span] {
        &self.inner
    }

    /// Mutable slice access.
    pub fn as_mut_slice(&mut self) -> &mut [Span] {
        &mut self.inner
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

impl std::ops::Deref for Spans {
    type Target = [Span];
    fn deref(&self) -> &[Span] {
        &self.inner
    }
}

impl<'a> IntoIterator for &'a Spans {
    type Item = &'a Span;
    type IntoIter = std::slice::Iter<'a, Span>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl std::ops::Index<usize> for Spans {
    type Output = Span;
    fn index(&self, i: usize) -> &Span {
        &self.inner[i]
    }
}

impl std::ops::Index<i64> for Spans {
    type Output = Span;
    fn index(&self, i: i64) -> &Span {
        let n = self.inner.len();
        self.get_signed(i).unwrap_or_else(|| {
            panic!("Index {i} out of range [-{n}, {n})");
        })
    }
}

/// Observer hook called by [`NativeTestCase`] after each draw and on
/// conclusion.  All methods have default no-op implementations so
/// concrete observers only need to override the callbacks they care
/// about.
pub trait DataObserver: Send {
    fn draw_boolean(&mut self, _value: bool, _was_forced: bool) {}
    fn draw_integer(&mut self, _value: i128, _was_forced: bool) {}
    fn draw_float(&mut self, _value: f64, _was_forced: bool) {}
    fn draw_bytes(&mut self, _value: &[u8], _was_forced: bool) {}
    fn draw_string(&mut self, _value: &str, _was_forced: bool) {}
    fn conclude_test(&mut self, _status: Status, _origin: Option<InterestingOrigin>) {}
}

/// A test case backed by a sequence of typed choices.
///
/// During random generation, choices are drawn from the RNG.
/// During replay/shrinking, choices are drawn from a prefix.
pub struct NativeTestCase {
    prefix: Vec<ChoiceValue>,
    prefix_nodes: Option<Vec<ChoiceNode>>,
    rng: Option<SmallRng>,
    max_size: usize,
    pub nodes: Vec<ChoiceNode>,
    pub status: Option<Status>,
    /// Set to `true` by [`Self::freeze`] on the first call; subsequent calls
    /// are no-ops. A dedicated boolean (rather than checking `status`) lets
    /// `conclude_test` set `self.status` before calling `freeze()` without
    /// triggering the idempotency early-return.
    frozen: bool,
    pub collections: HashMap<i64, ManyState>,
    next_collection_id: i64,
    pub variable_pools: Vec<NativeVariables>,
    pub spans: Spans,
    /// Indices into `spans` for currently-open spans, in nesting order.
    /// Each entry was pushed by `start_span` and is awaiting a matching
    /// `stop_span` call.
    pub span_stack: Vec<usize>,
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
    /// The interesting origin set by [`Self::conclude_test`], if any.
    /// `None` for test cases concluded by [`Self::freeze`] directly
    /// (`Status::Valid`).
    interesting_origin: Option<InterestingOrigin>,
    /// Optional template applied to every draw past the explicit `prefix`.
    /// `count` is mutated in-place as draws consume the template; when
    /// `count` reaches zero the next draw is overrun
    /// (`Status::EarlyStop` + `EngineError`). `None` means "no template" —
    /// draws past the prefix go to `rng` or panic, as before.
    trailing_template: Option<ChoiceTemplate>,
}

impl NativeTestCase {
    pub fn new_random(rng: SmallRng) -> Self {
        Self::for_choices_and_template(&[], None, None, BUFFER_SIZE, None).with_random(rng)
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
        NativeTestCase {
            prefix: choices.to_vec(),
            prefix_nodes: prefix_nodes.map(|n| n.to_vec()),
            rng: None,
            max_size: max_size.max(choices.len()),
            nodes: Vec::new(),
            status: None,
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer,
            interesting_origin: None,
            trailing_template: trailing,
        }
    }

    /// A test case where every draw past the explicit prefix returns
    /// `kind.simplest()` of the requested choice kind. A deterministic
    /// all-simplest probe of the choice tree's "left leaf" before random
    /// sampling begins.
    pub fn for_simplest(max_size: usize) -> Self {
        Self::for_choices_and_template(
            &[],
            None,
            Some(ChoiceTemplate::simplest(None)),
            max_size,
            None,
        )
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
    pub fn for_probe(prefix: &[ChoiceValue], rng: SmallRng, max_size: usize) -> Self {
        Self::for_choices_and_template(prefix, None, None, max_size, None).with_random(rng)
    }

    /// Attach an RNG for post-prefix random draws.  Internal builder used by
    /// `new_random` and `for_probe` to share the [`Self::for_choices_and_template`]
    /// constructor without duplicating the struct literal.
    fn with_random(mut self, rng: SmallRng) -> Self {
        self.rng = Some(rng);
        self
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
        let mut frame = HashSet::new();
        frame.insert(label);
        self.labels_for_structure_stack.push(frame);
        if depth + 1 > MAX_DEPTH && self.status.is_none() {
            self.status = Some(Status::Invalid);
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
        if self.status.is_none() {
            self.status = Some(Status::Valid);
        }
        if let Some(ref mut obs) = self.observer {
            let origin = self.interesting_origin.clone();
            obs.conclude_test(self.status.unwrap(), origin);
        }
    }

    /// Allocate a new collection ID and store the given state.
    pub fn new_collection(&mut self, state: ManyState) -> i64 {
        let id = self.next_collection_id;
        self.next_collection_id += 1;
        self.collections.insert(id, state);
        id
    }

    /// Draw a random integer in [min_value, max_value].
    pub fn draw_integer(&mut self, min_value: i128, max_value: i128) -> Result<i128, EngineError> {
        assert!(
            min_value <= max_value,
            "Invalid range [{min_value}, {max_value}]"
        );

        let kind = IntegerChoice {
            min_value,
            max_value,
            shrink_towards: 0,
        };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Integer(kind.clone()),
            || ChoiceValue::Integer(kind.simplest()),
            || ChoiceValue::Integer(kind.unit()),
            |v| matches!(v, ChoiceValue::Integer(n) if kind.validate(*n)),
            |rng| ChoiceValue::Integer(biased_integer_sample(&kind, rng)),
        )?;

        let ChoiceValue::Integer(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Integer(kind),
            value: ChoiceValue::Integer(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(v, was_forced);
        }

        Ok(v)
    }

    /// Draw a floating-point value in `[min_value, max_value]`. NaN is drawn
    /// only when `allow_nan` is set; ±∞ only when `allow_infinity` is set and
    /// the relevant endpoint is unbounded.
    pub fn draw_float(
        &mut self,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
    ) -> Result<f64, EngineError> {
        let kind = FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
        };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Float(kind.clone()),
            || ChoiceValue::Float(kind.simplest()),
            || ChoiceValue::Float(kind.unit()),
            |v| matches!(v, ChoiceValue::Float(f) if kind.validate(*f)),
            |rng| ChoiceValue::Float(biased_float_sample(&kind, rng)),
        )?;

        let ChoiceValue::Float(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Float(kind),
            value: ChoiceValue::Float(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_float(v, was_forced);
        }

        Ok(v)
    }

    /// Draw a bytes value with length in `[min_size, max_size]`.
    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Result<Vec<u8>, EngineError> {
        assert!(
            min_size <= max_size,
            "min_size ({min_size}) must be <= max_size ({max_size})"
        );
        let kind = BytesChoice { min_size, max_size };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Bytes(kind.clone()),
            || ChoiceValue::Bytes(kind.simplest()),
            || ChoiceValue::Bytes(kind.unit()),
            |v| matches!(v, ChoiceValue::Bytes(b) if kind.validate(b)),
            |rng| ChoiceValue::Bytes(biased_bytes_sample(&kind, rng)),
        )?;

        let ChoiceValue::Bytes(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Bytes(kind),
            value: ChoiceValue::Bytes(v.clone()),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_bytes(&v, was_forced);
        }

        Ok(v)
    }

    /// Draw a Unicode string with length in `[min_size, max_size]` whose
    /// codepoints lie in the given [`IntervalSet`] alphabet.
    pub fn draw_string(
        &mut self,
        intervals: IntervalSet,
        min_size: usize,
        max_size: usize,
    ) -> Result<String, EngineError> {
        assert!(min_size <= max_size);
        assert!(
            !intervals.is_empty() || max_size == 0,
            "draw_string with empty alphabet must have max_size == 0"
        );

        let kind = StringChoice {
            intervals,
            min_size,
            max_size,
        };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::String(kind.clone()),
            || ChoiceValue::String(kind.simplest()),
            || ChoiceValue::String(kind.unit()),
            |v| matches!(v, ChoiceValue::String(s) if kind.validate(s)),
            |rng| ChoiceValue::String(biased_string_sample(&kind, rng)),
        )?;

        let ChoiceValue::String(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::String(kind),
            value: ChoiceValue::String(v.clone()),
            was_forced,
        });

        let s = codepoints_to_string(&v);
        if let Some(ref mut obs) = self.observer {
            obs.draw_string(&s, was_forced);
        }

        Ok(s)
    }

    /// Draw a boolean with probability `p` of being true.
    /// If `forced` is Some, the result is forced to that value.
    pub fn weighted(&mut self, p: f64, forced: Option<bool>) -> Result<bool, EngineError> {
        let kind = BooleanChoice;

        let forced_value = forced.or(if p <= 0.0 {
            Some(false)
        } else if p >= 1.0 {
            Some(true)
        } else {
            None
        });

        let (value, was_forced) = if let Some(f) = forced_value {
            self.pre_choice()?;
            (ChoiceValue::Boolean(f), true)
        } else {
            self.resolve_choice(
                &ChoiceKind::Boolean(kind.clone()),
                || ChoiceValue::Boolean(kind.simplest()),
                || ChoiceValue::Boolean(kind.unit()),
                |v| matches!(v, ChoiceValue::Boolean(_)),
                |rng| ChoiceValue::Boolean(rng.random::<f64>() <= p),
            )?
        };

        let ChoiceValue::Boolean(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Boolean(kind),
            value: ChoiceValue::Boolean(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_boolean(v, was_forced);
        }

        Ok(v)
    }
    fn pre_choice(&mut self) -> Result<(), EngineError> {
        // A test case can become frozen mid-execution when `start_span`
        // exceeds `MAX_DEPTH` and sets `status = Some(Status::Invalid)`.
        // Subsequent draws must propagate `EngineError` so the test halts.
        if self.status.is_some() {
            return Err(EngineError::StopTest);
        }
        if self.nodes.len() >= self.max_size {
            self.status = Some(Status::EarlyStop);
            return Err(EngineError::StopTest);
        }
        Ok(())
    }

    /// Resolve a choice value from forced, prefix, or random.
    ///
    /// Implements punning logic for replaying choice sequences whose
    /// schema has shifted across runs.
    fn resolve_choice(
        &mut self,
        _kind: &ChoiceKind,
        simplest: impl FnOnce() -> ChoiceValue,
        unit: impl FnOnce() -> ChoiceValue,
        validate: impl FnOnce(&ChoiceValue) -> bool,
        random: impl FnOnce(&mut SmallRng) -> ChoiceValue,
    ) -> Result<(ChoiceValue, bool), EngineError> {
        self.pre_choice()?;

        let idx = self.nodes.len();

        // Branch 1: replay from the concrete prefix. When the prefix value's
        // recorded kind doesn't match the requested kind (e.g. a schema
        // shifted between runs), `prefix_nodes` carries the original kind so
        // we can route to `simplest()` or `unit()` of the *new* kind — the
        // "punning" logic.
        if idx < self.prefix.len() {
            let prefix_value = &self.prefix[idx];
            if validate(prefix_value) {
                return Ok((prefix_value.clone(), false));
            }
            let is_simplest = self
                .prefix_nodes
                .as_ref()
                .and_then(|pn| pn.get(idx))
                .is_some_and(|pn| *prefix_value == pn.kind.simplest());
            return Ok((if is_simplest { simplest() } else { unit() }, false));
        }

        // Branch 2: trailing template. Resolves every post-prefix draw to
        // the template's kind, decrementing `count` if finite. When `count`
        // reaches zero the next draw marks overrun without producing a
        // value.
        if let Some(template) = self.trailing_template.as_mut() {
            if matches!(template.count, Some(0)) {
                self.status = Some(Status::EarlyStop);
                return Err(EngineError::StopTest);
            }
            let value = match template.kind {
                ChoiceTemplateKind::Simplest => simplest(),
            };
            if let Some(c) = template.count.as_mut() {
                *c -= 1;
            }
            return Ok((value, false));
        }

        // Branch 3: random fallback.
        let rng = self
            .rng
            .as_mut()
            .expect("No RNG available for random generation");
        Ok((random(rng), false))
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_tests.rs"]
mod tests;
