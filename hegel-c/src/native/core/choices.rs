use std::sync::Arc;

use super::state::{Span, SpanEvent};
use crate::control::hegel_internal_assert;
use crate::native::bignum::{BigInt, BigUint, Zero};
use crate::native::floats::sign_aware_lte;
use crate::native::intervalsets::IntervalSet;

/// An integer choice with bounded range, using `BigInt` for all widths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerChoice {
    pub min_value: BigInt,
    pub max_value: BigInt,
    /// The "preferred" value the shrinker aims at (default 0). All of
    /// [`Self::simplest`], [`Self::unit`], and [`Self::sort_key`] are
    /// anchored at `shrink_towards.clamp(min_value, max_value)`, so
    /// integer-shrinking passes converge on this value rather than on 0.
    pub shrink_towards: BigInt,
}

impl IntegerChoice {
    pub(crate) fn clamped_shrink_towards(&self) -> BigInt {
        self.shrink_towards
            .clone()
            .clamp(self.min_value.clone(), self.max_value.clone())
    }

    pub fn simplest(&self) -> BigInt {
        self.clamped_shrink_towards()
    }

    pub fn unit(&self) -> BigInt {
        let s = self.simplest();
        let succ = &s + BigInt::from(1);
        if self.validate(&succ) {
            return succ;
        }
        let pred = &s - BigInt::from(1);
        if self.validate(&pred) {
            return pred;
        }
        s
    }

    pub fn validate(&self, value: &BigInt) -> bool {
        self.min_value <= *value && *value <= self.max_value
    }

    pub fn sort_key(&self, value: &BigInt) -> (BigUint, bool) {
        let target = self.clamped_shrink_towards();
        let distance = (value - &target).magnitude();
        (distance, *value < target)
    }

    pub fn max_index(&self) -> BigUint {
        (&self.max_value - &self.min_value).magnitude()
    }

    pub fn to_index(&self, value: &BigInt) -> BigUint {
        let s = self.simplest();
        if *value == s {
            return BigUint::zero();
        }
        let above = (&self.max_value - &s).magnitude();
        let below = (&s - &self.min_value).magnitude();
        let d_abs = (value - &s).magnitude();
        let one = BigUint::from(1u32);
        let d_minus_one = &d_abs - &one;
        let mut count = std::cmp::min(&d_minus_one, &above) + std::cmp::min(&d_minus_one, &below);
        if *value > s {
            return count + &one;
        }
        if d_abs <= above {
            count += BigUint::from(1u32);
        }
        count + BigUint::from(1u32)
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: BigUint) -> Option<BigInt> {
        let s = self.simplest();
        if index.is_zero() {
            return Some(s);
        }
        let above = (&self.max_value - &s).magnitude();
        let below = (&s - &self.min_value).magnitude();
        if index > &above + &below {
            return None;
        }
        let two_a = std::cmp::min(&above, &below) << 1usize;
        let one = BigUint::from(1u32);
        let (d, up) = if index <= two_a {
            let d = (&index + &one) >> 1u32;
            let up = !(&index % &BigUint::from(2u32)).is_zero();
            (d, up)
        } else {
            let d = &index - std::cmp::min(&above, &below);
            (d, above > below)
        };
        let d = BigInt::from(d);
        if up { Some(s + d) } else { Some(s - d) }
    }

    pub fn value_from_bigint(&self, v: &BigInt) -> Option<BigInt> {
        if self.validate(v) {
            Some(v.clone())
        } else {
            None
        }
    }
}

/// A boolean choice. Simplest value is `false`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BooleanChoice;

impl BooleanChoice {
    pub fn simplest(&self) -> bool {
        false
    }

    pub fn unit(&self) -> bool {
        true
    }

    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(1u32)
    }

    pub fn to_index(&self, value: bool) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(u32::from(value))
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<bool> {
        use crate::native::bignum::BigUint;
        if index == BigUint::from(0u32) {
            Some(false)
        } else if index == BigUint::from(1u32) {
            Some(true)
        } else {
            None
        }
    }
}

/// A bytes choice with bounded length.
///
/// Ordered by shortlex: shorter sequences are simpler, then lexicographic
/// on the bytes themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytesChoice {
    pub min_size: usize,
    pub max_size: usize,
}

impl BytesChoice {
    /// The simplest (most "shrunk") value: `min_size` zero bytes.
    pub fn simplest(&self) -> Vec<u8> {
        vec![0u8; self.min_size]
    }

    /// The second-simplest value, used for punning when types change.
    /// If `min_size > 0`: the simplest except the last byte is 1.
    /// Else if `max_size > 0`: a single `0x01` byte.
    /// Else: the simplest (empty).
    pub fn unit(&self) -> Vec<u8> {
        if self.min_size > 0 {
            let mut v = vec![0u8; self.min_size];
            *v.last_mut().unwrap() = 1;
            v
        } else if self.max_size > 0 {
            vec![1u8]
        } else {
            self.simplest()
        }
    }

    pub fn validate(&self, value: &[u8]) -> bool {
        self.min_size <= value.len() && value.len() <= self.max_size
    }

    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        self.to_index(&vec![0xffu8; self.max_size])
    }

    /// Indexes byte sequences in shortlex order over `[min_size, max_size]`:
    /// all length-`min_size` sequences first, then length `min_size + 1`, and
    /// so on; within each length, lexicographic on the bytes.
    pub fn to_index(&self, value: &[u8]) -> crate::native::bignum::BigUint {
        use crate::native::bignum::{BigUint, Zero};
        let base = BigUint::from(256u32);
        let mut offset = BigUint::zero();
        for length in self.min_size..value.len() {
            offset += base.pow(length as u32);
        }
        let mut position = BigUint::zero();
        for &b in value {
            position = position * &base + BigUint::from(b);
        }
        offset + position
    }

    /// Inverse of [`to_index`]. Returns `None` if the index is past the
    /// last representable sequence.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<Vec<u8>> {
        use crate::native::bignum::BigUint;
        let base = BigUint::from(256u32);
        let mut remaining = index;
        for length in self.min_size..=self.max_size {
            let bucket = base.pow(length as u32);
            if remaining < bucket {
                let mut result: Vec<u8> = Vec::with_capacity(length);
                for _ in 0..length {
                    let b: u8 = (&remaining % &base)
                        .try_into()
                        .expect("byte < 256 fits in u8");
                    result.push(b);
                    remaining /= &base;
                }
                result.reverse();
                return Some(result);
            }
            remaining -= bucket;
        }
        None
    }
}

/// A string choice with bounded length and a Unicode codepoint alphabet
/// represented as an [`IntervalSet`].
///
/// Values are sequences of Unicode codepoints (`Vec<u32>`) drawn from the
/// `intervals` set. Ordered by shortlex under the alphabet-relative shrink
/// ordering exposed by [`IntervalSet::index_from_char_in_shrink_order`]:
/// `'0'` is the simplest character whenever the alphabet contains it,
/// followed by `'1'`..`'9'`, `'A'`..`'Z'`, then characters below `'0'` in
/// reverse, then characters above `'Z'` in natural order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringChoice {
    pub intervals: Arc<IntervalSet>,
    pub min_size: usize,
    pub max_size: usize,
}

impl StringChoice {
    /// Position of `codepoint` in the alphabet's shrink-preferred ordering.
    /// Panics if `codepoint` is not in the alphabet.
    pub fn codepoint_key(&self, codepoint: u32) -> u32 {
        let c = char::from_u32(codepoint).expect("non-surrogate codepoint");
        self.intervals.index_from_char_in_shrink_order(c) as u32
    }

    /// Codepoint at shrink-order position `key`, or `None` if `key` is past
    /// the alphabet's size.
    pub fn key_to_codepoint(&self, key: u32) -> Option<u32> {
        let key = key as usize;
        if key >= self.intervals.len() {
            return None;
        }
        Some(self.intervals.char_in_shrink_order(key) as u32)
    }

    /// The simplest codepoint in the alphabet (shrink-order position 0).
    /// Panics on an empty alphabet — callers must reject empty alphabets at
    /// the draws layer before constructing the `StringChoice`.
    pub(crate) fn simplest_codepoint(&self) -> u32 {
        hegel_internal_assert!(
            !self.intervals.is_empty(),
            "StringChoice::simplest_codepoint: empty alphabet"
        );
        self.intervals.char_in_shrink_order(0) as u32
    }

    /// The simplest sequence of codepoints of length `min_size`, built
    /// from repeated [`simplest_codepoint`]. An empty alphabet is legal when
    /// `min_size == 0` (the empty string is then the choice's only value).
    pub fn simplest(&self) -> Vec<u32> {
        if self.min_size == 0 {
            return Vec::new();
        }
        vec![self.simplest_codepoint(); self.min_size]
    }

    /// Second-simplest codepoint sequence, used for type-punning during replay.
    pub fn unit(&self) -> Vec<u32> {
        if self.intervals.is_empty() {
            return self.simplest();
        }
        let simplest_cp = self.simplest_codepoint();
        let second_cp = self.key_to_codepoint(1);
        match second_cp {
            Some(cp) if cp != simplest_cp => {
                if self.min_size > 0 {
                    let mut v = self.simplest();
                    *v.last_mut().unwrap() = cp;
                    v
                } else if self.max_size > 0 {
                    vec![cp]
                } else {
                    self.simplest()
                }
            }
            _ => {
                if self.min_size < self.max_size {
                    vec![simplest_cp; self.min_size + 1]
                } else {
                    self.simplest()
                }
            }
        }
    }

    pub fn validate(&self, value: &[u32]) -> bool {
        if !(self.min_size <= value.len() && value.len() <= self.max_size) {
            return false;
        }
        value.iter().all(|&cp| self.intervals.contains(cp))
    }

    /// Cardinality of the alphabet.
    pub fn alpha_size(&self) -> u64 {
        self.intervals.len() as u64
    }

    /// Rank of `codepoint` in the alphabet's shrink-preferred ordering. Same
    /// as [`codepoint_key`] but cast to the `u64` width used by the index
    /// machinery.
    pub fn codepoint_rank(&self, codepoint: u32) -> u64 {
        u64::from(self.codepoint_key(codepoint))
    }

    /// Codepoint at the given shrink-order rank. Panics if `rank` exceeds
    /// `alpha_size`.
    pub fn codepoint_at_rank(&self, rank: u64) -> u32 {
        self.key_to_codepoint(rank as u32)
            .expect("rank within alpha_size")
    }

    /// Largest valid index for [`from_index`].
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::{BigUint, Zero};
        let alpha = BigUint::from(self.alpha_size());
        let mut total = BigUint::zero();
        for length in self.min_size..=self.max_size {
            total += alpha.pow(length as u32);
        }
        total - BigUint::from(1u32)
    }

    /// Shortlex index of `value` under this choice's shrink-ordered alphabet.
    pub fn to_index(&self, value: &[u32]) -> crate::native::bignum::BigUint {
        use crate::native::bignum::{BigUint, Zero};
        let alpha = BigUint::from(self.alpha_size());
        let mut offset = BigUint::zero();
        for length in self.min_size..value.len() {
            offset += alpha.pow(length as u32);
        }
        let mut position = BigUint::zero();
        for &cp in value {
            position = position * &alpha + BigUint::from(self.codepoint_rank(cp));
        }
        offset + position
    }

    /// Codepoint sequence at the given shortlex index, or `None` if `index`
    /// exceeds the total bucket size.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<Vec<u32>> {
        use crate::native::bignum::{BigUint, Zero};
        let alpha = BigUint::from(self.alpha_size());
        hegel_internal_assert!(
            !alpha.is_zero() || self.max_size == 0,
            "StringChoice::from_index: empty alphabet with nonzero max_size"
        );
        let mut remaining = index;
        for length in self.min_size..=self.max_size {
            let bucket_size = alpha.pow(length as u32);
            if remaining < bucket_size {
                let mut cps: Vec<u32> = Vec::with_capacity(length);
                for _ in 0..length {
                    let r: u64 = (&remaining % &alpha)
                        .try_into()
                        .expect("rank < alpha_size fits in u64");
                    cps.push(self.codepoint_at_rank(r));
                    remaining /= &alpha;
                }
                cps.reverse();
                return Some(cps);
            }
            remaining -= bucket_size;
        }
        None
    }
}

/// A float choice with bounded range.
#[derive(Clone, Debug)]
pub struct FloatChoice {
    pub min_value: f64,
    pub max_value: f64,
    pub allow_nan: bool,
    pub allow_infinity: bool,
    /// Smallest positive magnitude the choice may produce: values `v` with
    /// `0 < |v| < smallest_nonzero_magnitude` are invalid. Port of
    /// Hypothesis's float constraint of the same name (`allow_subnormal =
    /// false` sets it to the width's smallest *normal*). The default,
    /// `5e-324` (the smallest subnormal), imposes no restriction.
    pub smallest_nonzero_magnitude: f64,
}

/// Bit-exact equality so a `FloatChoice` recorded with `-0.0` doesn't compare
/// equal to one recorded with `0.0`, and distinct NaN payloads stay distinct.
impl PartialEq for FloatChoice {
    fn eq(&self, other: &Self) -> bool {
        self.min_value.to_bits() == other.min_value.to_bits()
            && self.max_value.to_bits() == other.max_value.to_bits()
            && self.allow_nan == other.allow_nan
            && self.allow_infinity == other.allow_infinity
            && self.smallest_nonzero_magnitude.to_bits()
                == other.smallest_nonzero_magnitude.to_bits()
    }
}

impl Eq for FloatChoice {}

impl FloatChoice {
    /// The simplest (lowest-sort-key) valid float for this choice.
    ///
    /// Exact: [`to_index`](Self::to_index) subtracts this value's global
    /// rank, so anything less than the true minimum makes that subtraction
    /// underflow (and panic) for the simpler in-range values.
    pub fn simplest(&self) -> f64 {
        use super::float_index::{float_to_index, simplest_in_range};

        if self.validate(0.0) {
            return 0.0;
        }
        if self.validate(-0.0) {
            return -0.0;
        }

        let mut best: Option<((u64, bool), f64)> = None;
        if self.max_value > 0.0 {
            let lo = self.min_value.max(self.smallest_nonzero_magnitude);
            let hi = self.max_value.min(f64::MAX);
            if lo <= hi {
                let v = simplest_in_range(lo, hi);
                best = Some(((float_to_index(v), false), v));
            }
        }
        if self.min_value < 0.0 {
            let lo = (-self.max_value).max(self.smallest_nonzero_magnitude);
            let hi = (-self.min_value).min(f64::MAX);
            if lo <= hi {
                let v = simplest_in_range(lo, hi);
                let key = (float_to_index(v), true);
                if best.is_none_or(|(best_key, _)| key < best_key) {
                    best = Some((key, -v));
                }
            }
        }
        if let Some((_, v)) = best {
            return v;
        }
        if self.allow_infinity && self.validate(f64::INFINITY) {
            return f64::INFINITY;
        }
        if self.allow_infinity && self.validate(f64::NEG_INFINITY) {
            return f64::NEG_INFINITY;
        }
        if self.allow_nan {
            return f64::NAN;
        }
        panic!("FloatChoice::simplest: no valid float for this choice")
    }

    /// Second-simplest valid float (for type punning during replay).
    pub fn unit(&self) -> f64 {
        use super::float_index::{float_to_index, index_to_float};

        let s = self.simplest();
        if s.is_nan() {
            return s;
        }
        let base = float_to_index(s.abs());
        for offset in 1u64..4 {
            let v_mag = index_to_float(base + offset);
            for v in [v_mag, -v_mag] {
                if !v.is_nan() && self.validate(v) {
                    return v;
                }
            }
        }
        for v in [self.min_value, self.max_value, -s] {
            if !v.is_nan() && v.to_bits() != s.to_bits() && self.validate(v) {
                return v;
            }
        }
        s
    }

    pub fn validate(&self, v: f64) -> bool {
        if v.is_nan() {
            return self.allow_nan;
        }
        if v.is_infinite() {
            if !self.allow_infinity {
                return false;
            }
            if v == f64::NEG_INFINITY && self.min_value > f64::NEG_INFINITY {
                return false;
            }
            if v == f64::INFINITY && self.max_value < f64::INFINITY {
                return false;
            }
            return true;
        }
        if v != 0.0 && v.abs() < self.smallest_nonzero_magnitude {
            return false;
        }
        sign_aware_lte(self.min_value, v) && sign_aware_lte(v, self.max_value)
    }

    /// Sort key for shrinking. Returns `(magnitude_index, is_negative)`.
    /// NaN sorts last (u64::MAX, false).
    pub fn sort_key(&self, v: f64) -> (u64, bool) {
        use super::float_index::float_to_index;
        if v.is_nan() {
            return (u64::MAX, false);
        }
        let is_neg = v.is_sign_negative();
        let mag = if is_neg { -v } else { v };
        (float_to_index(mag), is_neg)
    }

    /// Largest valid index for [`from_index`]. Indexes the full finite range
    /// (both signs) followed by `+inf`, `-inf`, then all NaN payloads.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        max_finite_global_rank() + BigUint::from(2u32) + BigUint::from(1u64 << 53)
    }

    /// Implementation note: the naive formula
    /// `to_index = _float_to_index(value) - _float_to_index(simplest)` over
    /// the raw-index ordering would underflow whenever `value` is below
    /// `simplest` in raw-index terms (which can happen because `simplest`
    /// prefers nearby integers — `65673.0` for the range `[65672.5, 65673.0]`
    /// — even though their raw lex indices put `65672.5` first). The dense
    /// ordering used by the shrinker is `(float_to_index(|v|), is_neg)`, so
    /// we build the index directly from that and subtract the rank of
    /// `simplest`.
    pub fn to_index(&self, value: f64) -> crate::native::bignum::BigUint {
        float_global_rank(value) - float_global_rank(self.simplest())
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<f64> {
        let raw = float_global_rank(self.simplest()) + index;
        let value = float_from_global_rank(raw)?;
        if self.validate(value) {
            Some(value)
        } else {
            None
        }
    }
}

/// Dense rank of `v` under the float sort order: finite floats indexed by
/// `(float_to_index(|v|), is_neg)`, then `+inf`, `-inf`, then NaN payloads.
fn float_global_rank(v: f64) -> crate::native::bignum::BigUint {
    use super::float_index::float_to_index;
    use crate::native::bignum::BigUint;

    if v.is_nan() {
        let bits = v.to_bits();
        let nan_offset = (bits & ((1u64 << 52) - 1)) ^ (1u64 << 51);
        let sign = bits >> 63;
        return max_finite_global_rank()
            + BigUint::from(3u32)
            + BigUint::from(nan_offset) * BigUint::from(2u32)
            + BigUint::from(sign);
    }
    if v.is_infinite() {
        return if v > 0.0 {
            max_finite_global_rank() + BigUint::from(1u32)
        } else {
            max_finite_global_rank() + BigUint::from(2u32)
        };
    }
    let is_neg = v.is_sign_negative();
    let mag = if is_neg { -v } else { v };
    let mag_idx = float_to_index(mag);
    BigUint::from(mag_idx) * BigUint::from(2u32) + BigUint::from(u32::from(is_neg))
}

/// Inverse of [`float_global_rank`]. Returns `None` if `rank` falls in the
/// NaN-payload region for a sign+offset combination that would not be a
/// valid NaN bit pattern.
fn float_from_global_rank(rank: crate::native::bignum::BigUint) -> Option<f64> {
    use super::float_index::index_to_float;
    use crate::native::bignum::BigUint;

    let max_finite = max_finite_global_rank();
    if rank > max_finite {
        let offset = &rank - &max_finite;
        if offset == BigUint::from(1u32) {
            return Some(f64::INFINITY);
        }
        if offset == BigUint::from(2u32) {
            return Some(f64::NEG_INFINITY);
        }
        let nan_rel = offset - BigUint::from(3u32);
        let sign: u64 = (&nan_rel % BigUint::from(2u32))
            .try_into()
            .expect("mod 2 fits in u64");
        let mantissa_base: u64 = (nan_rel / BigUint::from(2u32)).try_into().ok()?;
        if mantissa_base >> 52 != 0 {
            return None;
        }
        let mantissa = mantissa_base ^ (1u64 << 51);
        let bits = (sign << 63) | (0x7FFu64 << 52) | mantissa;
        let v = f64::from_bits(bits);
        return if v.is_nan() { Some(v) } else { None };
    }
    let is_neg_u: u64 = (&rank % BigUint::from(2u32))
        .try_into()
        .expect("mod 2 fits in u64");
    let mag_big = rank / BigUint::from(2u32);
    let mag_idx: u64 = (&mag_big).try_into().ok()?;
    if mag_idx >> 63 == 0 && mag_idx >> 56 != 0 {
        return None;
    }
    let mag = index_to_float(mag_idx);
    Some(if is_neg_u == 1 { -mag } else { mag })
}

/// Largest dense rank used by any finite float. The maximum lex index over
/// any finite float is `(1<<63) | (2046<<52) | mantissa_max` — bit 63 set
/// (non-simple), encoded exponent 2046 (the last non-NaN/inf slot), and
/// every fractional bit set. (Note: this is *not* `float_to_index(f64::MAX)`,
/// because the lex ordering ranks fractions like `0.5` — encoded
/// exponent 1024 — *higher* than huge integers like `f64::MAX`, which has
/// encoded exponent 1023.) The `+1` is the negative-sign slot for that lex
/// index, since `float_global_rank` packs sign into the low bit.
fn max_finite_global_rank() -> crate::native::bignum::BigUint {
    use crate::native::bignum::BigUint;
    let max_finite_lex = (1u64 << 63) | (2046u64 << 52) | ((1u64 << 52) - 1);
    BigUint::from(max_finite_lex) * BigUint::from(2u32) + BigUint::from(1u32)
}

/// The kind of choice made at a particular point.
///
/// The `IntegerChoice` payload is shared via `Arc`: the shrinker and data
/// tree clone kinds constantly, and the integer constraint holds three
/// `BigInt`s, so sharing turns those deep clones into a pointer bump. The
/// other constraints are a few machine words and are carried by value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Integer(Arc<IntegerChoice>),
    Boolean(BooleanChoice),
    Float(FloatChoice),
    Bytes(BytesChoice),
    String(StringChoice),
    /// A clone of the test case was created at this position. The choice's
    /// value is the cloned stream's own choice sequence (a [`CloneRecord`]);
    /// the clone's identity (its counter within the parent stream) is
    /// deterministic, so the kind itself carries no configuration.
    Clone,
}

/// The value produced by a choice.
#[derive(Clone, Debug)]
pub enum ChoiceValue {
    Integer(BigInt),
    Boolean(bool),
    Float(f64),
    Bytes(Vec<u8>),
    /// A sequence of Unicode codepoints (raw `u32`s in `0..=0x10FFFF`). The
    /// engine reasons internally about any codepoint, including surrogates;
    /// conversion to `String` (with the surrogate filter) happens at the
    /// user-facing boundary.
    String(Vec<u32>),
    /// The choice sequence of a cloned stream, recursively. Shared via `Arc`
    /// so shrink candidates that only differ outside this clone reuse the
    /// child sequence instead of deep-copying it.
    Clone(Arc<CloneRecord>),
}

/// The realized execution of one cloned stream: its child [`ChoiceNode`]s
/// plus the span structure recorded alongside them. This is what the
/// shrinker and data tree interrogate, and what a [`ChoiceData::Clone`]
/// node carries — a node's clone payload is realized *by construction*, so
/// consumers never have to re-prove it.
#[derive(Debug)]
pub struct RealizedStream {
    nodes: Vec<ChoiceNode>,
    spans: Vec<Span>,
    span_events: Vec<(usize, SpanEvent)>,
    /// Cached [`flattened_len`] of the children, so sort-key comparison of
    /// deep trees costs one integer read per stream instead of a walk.
    flat_len: usize,
}

impl RealizedStream {
    /// A realized stream from an execution: its nodes plus the span
    /// structure recorded alongside them.
    pub fn new(
        nodes: Vec<ChoiceNode>,
        spans: Vec<Span>,
        span_events: Vec<(usize, SpanEvent)>,
    ) -> Self {
        let flat_len = flattened_len(&nodes);
        RealizedStream {
            nodes,
            spans,
            span_events,
            flat_len,
        }
    }

    /// The empty stream: a clone that drew nothing.
    pub fn empty() -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new())
    }

    /// The realized child nodes, in order.
    pub fn nodes(&self) -> &[ChoiceNode] {
        &self.nodes
    }

    /// The cloned stream's recorded spans.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// The cloned stream's span open/close events, tagged with the child
    /// draw position at which each fired.
    pub fn span_events(&self) -> &[(usize, SpanEvent)] {
        &self.span_events
    }

    /// Number of direct children (top-level choices in the cloned stream).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Total number of choices in the cloned stream, counting nested clones'
    /// children recursively. Cached at construction.
    pub fn flat_len(&self) -> usize {
        self.flat_len
    }
}

/// The children of a [`CloneRecord`]: either bare choice values (a record
/// deserialized from storage, where kinds and spans were never persisted) or
/// the realized stream of an execution.
#[derive(Clone, Debug)]
enum CloneChildren {
    Values {
        values: Vec<ChoiceValue>,
        flat_len: usize,
    },
    Realized(Arc<RealizedStream>),
}

/// The choice sequence of one cloned stream, carried as the value of a
/// clone position in its parent stream ([`ChoiceValue::Clone`]).
///
/// A record's *identity* — equality, hashing, and its contribution to sort
/// keys — is the sequence of child choice values, recursively. The realized
/// info (child kinds, forced flags, spans, span events) is carried when the
/// record was produced by executing the stream (as a shared
/// [`RealizedStream`]); it is never serialized and never part of equality,
/// so a record round-tripped through storage compares equal to the realized
/// record it came from.
#[derive(Clone, Debug)]
pub struct CloneRecord {
    children: CloneChildren,
}

impl CloneRecord {
    /// A record from bare child values (deserialized storage, or a
    /// hand-built replay prefix). Carries no realized info.
    pub fn from_values(values: Vec<ChoiceValue>) -> Self {
        let flat_len = flattened_len_of_values(values.iter());
        CloneRecord {
            children: CloneChildren::Values { values, flat_len },
        }
    }

    /// A record wrapping an already-realized stream, sharing it rather than
    /// copying its nodes.
    pub fn from_stream(stream: Arc<RealizedStream>) -> Self {
        CloneRecord {
            children: CloneChildren::Realized(stream),
        }
    }

    /// A record from an executed stream: its realized nodes plus the span
    /// structure recorded alongside them.
    pub fn from_run(
        nodes: Vec<ChoiceNode>,
        spans: Vec<Span>,
        span_events: Vec<(usize, SpanEvent)>,
    ) -> Self {
        Self::from_stream(Arc::new(RealizedStream::new(nodes, spans, span_events)))
    }

    /// The empty record: a clone that drew nothing.
    pub fn empty() -> Self {
        Self::from_run(Vec::new(), Vec::new(), Vec::new())
    }

    /// Number of direct children (top-level choices in the cloned stream).
    pub fn len(&self) -> usize {
        match &self.children {
            CloneChildren::Values { values, .. } => values.len(),
            CloneChildren::Realized(stream) => stream.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `i`-th child choice value, as a borrowed view.
    pub fn value_at(&self, i: usize) -> ChoiceValueRef<'_> {
        match &self.children {
            CloneChildren::Values { values, .. } => ChoiceValueRef::from(&values[i]),
            CloneChildren::Realized(stream) => stream.nodes[i].data.value_ref(),
        }
    }

    /// The child choice values, in order, as owned values. Children stored
    /// as bare values are cloned; realized children are rebuilt from their
    /// nodes (nested realized streams stay shared).
    pub fn owned_values(&self) -> Vec<ChoiceValue> {
        match &self.children {
            CloneChildren::Values { values, .. } => values.clone(),
            CloneChildren::Realized(stream) => {
                stream.nodes.iter().map(|n| n.data.value()).collect()
            }
        }
    }

    /// The realized stream, when this record came from an execution.
    pub fn realized(&self) -> Option<&Arc<RealizedStream>> {
        match &self.children {
            CloneChildren::Values { .. } => None,
            CloneChildren::Realized(stream) => Some(stream),
        }
    }

    /// The realized child nodes, when this record came from an execution.
    pub fn realized_nodes(&self) -> Option<&[ChoiceNode]> {
        self.realized().map(|stream| stream.nodes())
    }

    /// Total number of choices in the cloned stream, counting nested clones'
    /// children recursively. Cached at construction.
    pub fn flat_len(&self) -> usize {
        match &self.children {
            CloneChildren::Values { flat_len, .. } => *flat_len,
            CloneChildren::Realized(stream) => stream.flat_len(),
        }
    }
}

impl PartialEq for CloneRecord {
    fn eq(&self, other: &Self) -> bool {
        CloneValues::Record(self) == CloneValues::Record(other)
    }
}

impl Eq for CloneRecord {}

impl std::hash::Hash for CloneRecord {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        CloneValues::Record(self).hash(state);
    }
}

/// Total number of choices in `nodes`, counting each clone node as one
/// choice plus its children, recursively. Equal to `nodes.len()` for a
/// sequence with no clone nodes.
pub fn flattened_len(nodes: &[ChoiceNode]) -> usize {
    nodes
        .iter()
        .map(|n| match &n.data {
            ChoiceData::Clone(rs) => 1 + rs.flat_len(),
            _ => 1,
        })
        .sum()
}

/// [`flattened_len`] over bare choice values (e.g. a replay prefix).
pub fn flattened_values_len(values: &[ChoiceValue]) -> usize {
    flattened_len_of_values(values.iter())
}

fn flattened_len_of_values<'a>(values: impl Iterator<Item = &'a ChoiceValue>) -> usize {
    values
        .map(|v| match v {
            ChoiceValue::Clone(record) => 1 + record.flat_len(),
            _ => 1,
        })
        .sum()
}

/// Bit-exact equality for floats keeps `-0.0` distinct from `0.0` and
/// preserves NaN payloads; other choice types use natural equality.
impl PartialEq for ChoiceValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => a == b,
            (ChoiceValue::Boolean(a), ChoiceValue::Boolean(b)) => a == b,
            (ChoiceValue::Float(a), ChoiceValue::Float(b)) => a.to_bits() == b.to_bits(),
            (ChoiceValue::Bytes(a), ChoiceValue::Bytes(b)) => a == b,
            (ChoiceValue::String(a), ChoiceValue::String(b)) => a == b,
            (ChoiceValue::Clone(a), ChoiceValue::Clone(b)) => Arc::ptr_eq(a, b) || a == b,
            _ => false,
        }
    }
}

impl Eq for ChoiceValue {}

impl std::hash::Hash for ChoiceValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        ChoiceValueRef::from(self).hash(state);
    }
}

/// Borrowed view of a choice value, unifying values stored bare
/// ([`ChoiceValue`]) with values stored paired with their constraint
/// ([`ChoiceData`]). Equality and hashing match [`ChoiceValue`]'s
/// semantics: floats compare bit-exactly and clones compare by their child
/// value sequences, recursively.
#[derive(Clone, Copy, Debug)]
pub enum ChoiceValueRef<'a> {
    Integer(&'a BigInt),
    Boolean(bool),
    Float(f64),
    Bytes(&'a [u8]),
    String(&'a [u32]),
    Clone(CloneValues<'a>),
}

impl<'a> From<&'a ChoiceValue> for ChoiceValueRef<'a> {
    fn from(v: &'a ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueRef::Integer(n),
            ChoiceValue::Boolean(b) => ChoiceValueRef::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueRef::Float(*f),
            ChoiceValue::Bytes(v) => ChoiceValueRef::Bytes(v),
            ChoiceValue::String(v) => ChoiceValueRef::String(v),
            ChoiceValue::Clone(r) => ChoiceValueRef::Clone(CloneValues::Record(r)),
        }
    }
}

impl PartialEq for ChoiceValueRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChoiceValueRef::Integer(a), ChoiceValueRef::Integer(b)) => a == b,
            (ChoiceValueRef::Boolean(a), ChoiceValueRef::Boolean(b)) => a == b,
            (ChoiceValueRef::Float(a), ChoiceValueRef::Float(b)) => a.to_bits() == b.to_bits(),
            (ChoiceValueRef::Bytes(a), ChoiceValueRef::Bytes(b)) => a == b,
            (ChoiceValueRef::String(a), ChoiceValueRef::String(b)) => a == b,
            (ChoiceValueRef::Clone(a), ChoiceValueRef::Clone(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ChoiceValueRef<'_> {}

impl PartialEq<ChoiceValue> for ChoiceValueRef<'_> {
    fn eq(&self, other: &ChoiceValue) -> bool {
        *self == ChoiceValueRef::from(other)
    }
}

impl std::hash::Hash for ChoiceValueRef<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            ChoiceValueRef::Integer(n) => n.hash(state),
            ChoiceValueRef::Boolean(b) => b.hash(state),
            ChoiceValueRef::Float(f) => f.to_bits().hash(state),
            ChoiceValueRef::Bytes(v) => v.hash(state),
            ChoiceValueRef::String(v) => v.hash(state),
            ChoiceValueRef::Clone(c) => c.hash(state),
        }
    }
}

/// Borrowed view of a cloned stream's child values: either a
/// [`CloneRecord`] (bare values or realized) or a bare [`RealizedStream`].
/// Equality and hashing are by child values, recursively, so the two
/// storage shapes compare interchangeably.
#[derive(Clone, Copy, Debug)]
pub enum CloneValues<'a> {
    Record(&'a CloneRecord),
    Stream(&'a RealizedStream),
}

impl<'a> CloneValues<'a> {
    /// Number of direct children.
    pub fn len(&self) -> usize {
        match self {
            CloneValues::Record(r) => r.len(),
            CloneValues::Stream(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total number of choices, counting nested clones' children
    /// recursively.
    pub fn flat_len(&self) -> usize {
        match self {
            CloneValues::Record(r) => r.flat_len(),
            CloneValues::Stream(s) => s.flat_len(),
        }
    }

    /// The `i`-th child choice value.
    pub fn value_at(&self, i: usize) -> ChoiceValueRef<'a> {
        match self {
            CloneValues::Record(r) => r.value_at(i),
            CloneValues::Stream(s) => s.nodes[i].data.value_ref(),
        }
    }

    /// The child choice values, in order.
    pub fn values(&self) -> impl Iterator<Item = ChoiceValueRef<'a>> + 'a {
        let this = *self;
        (0..this.len()).map(move |i| this.value_at(i))
    }
}

impl PartialEq for CloneValues<'_> {
    fn eq(&self, other: &Self) -> bool {
        let identical = match (self, other) {
            (CloneValues::Record(a), CloneValues::Record(b)) => std::ptr::eq(*a, *b),
            (CloneValues::Stream(a), CloneValues::Stream(b)) => std::ptr::eq(*a, *b),
            _ => false,
        };
        identical
            || (self.flat_len() == other.flat_len()
                && self.len() == other.len()
                && self.values().zip(other.values()).all(|(a, b)| a == b))
    }
}

impl Eq for CloneValues<'_> {}

impl std::hash::Hash for CloneValues<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for v in self.values() {
            v.hash(state);
        }
    }
}

/// `base^exp`, saturating at `u128::MAX`, in `O(log exp)` multiplications.
fn saturating_pow(mut base: u128, mut exp: usize) -> u128 {
    let mut result: u128 = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result.saturating_mul(base);
        }
        exp >>= 1;
        if exp > 0 {
            base = base.saturating_mul(base);
        }
    }
    result
}

/// `Σ_{len=min_size..=max_size} alphabet^len` — the number of distinct
/// sequences over an `alphabet`-symbol set — saturating at `cap`.
///
/// Backs [`ChoiceKind::max_children_saturating`] for the `Bytes` / `String`
/// kinds: it accumulates in `u128` and returns `cap` the instant the running
/// total reaches it, so a huge `max_size` never forces a multi-hundred-bit
/// `BigUint`. The degenerate alphabets get closed forms and the sum starts
/// directly at `alphabet^min_size`, so a huge `min_size` (the draws layer
/// imposes no size cap) costs `O(log min_size)` rather than a spin up to it;
/// for `alphabet >= 2` the running total then reaches any realistic `cap`
/// within a bounded number of doublings.
fn sequence_max_children_saturating(
    alphabet: u128,
    min_size: usize,
    max_size: usize,
    cap: u128,
) -> u128 {
    if alphabet == 0 {
        return u128::from(min_size == 0).min(cap);
    }
    if alphabet == 1 {
        return ((max_size - min_size) as u128 + 1).min(cap);
    }
    let mut total: u128 = 0;
    let mut power = saturating_pow(alphabet, min_size);
    for _ in min_size..=max_size {
        total = total.saturating_add(power);
        if total >= cap {
            return cap;
        }
        power = power.saturating_mul(alphabet);
    }
    total
}

impl ChoiceKind {
    /// The simplest value for this choice kind.
    pub fn simplest(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.simplest()),

            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.simplest()),
            ChoiceKind::Float(fc) => ChoiceValue::Float(fc.simplest()),
            ChoiceKind::Bytes(bc) => ChoiceValue::Bytes(bc.simplest()),
            ChoiceKind::String(sc) => ChoiceValue::String(sc.simplest()),
            ChoiceKind::Clone => ChoiceValue::Clone(Arc::new(CloneRecord::empty())),
        }
    }

    /// The "unit" value for this choice kind — the fallback a replayed draw
    /// resolves to when its prefix value fails this kind's validation and no
    /// original-kind information is available to pun towards `simplest()`.
    /// Mirrors the `unit()` branch of
    /// [`crate::native::core::state::NativeTestCase::resolve_choice`].
    pub fn unit(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.unit()),
            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.unit()),
            ChoiceKind::Float(fc) => ChoiceValue::Float(fc.unit()),
            ChoiceKind::Bytes(bc) => ChoiceValue::Bytes(bc.unit()),
            ChoiceKind::String(sc) => ChoiceValue::String(sc.unit()),
            ChoiceKind::Clone => ChoiceValue::Clone(Arc::new(CloneRecord::empty())),
        }
    }

    /// Largest valid index for [`from_index`](Self::from_index), or `None`
    /// for a clone kind (clone choices have no dense index).
    pub fn max_index(&self) -> Option<crate::native::bignum::BigUint> {
        match self {
            ChoiceKind::Integer(ic) => Some(ic.max_index()),
            ChoiceKind::Boolean(bc) => Some(bc.max_index()),
            ChoiceKind::Float(fc) => Some(fc.max_index()),
            ChoiceKind::Bytes(bc) => Some(bc.max_index()),
            ChoiceKind::String(sc) => Some(sc.max_index()),
            ChoiceKind::Clone => None,
        }
    }

    /// The value at `index` under this kind's sort order. Returns `None`
    /// when the index is out of range; a clone kind has no dense index, so
    /// every index is out of range for it.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<ChoiceValue> {
        match self {
            ChoiceKind::Integer(ic) => ic.from_index(index).map(ChoiceValue::Integer),
            ChoiceKind::Boolean(bc) => bc.from_index(index).map(ChoiceValue::Boolean),
            ChoiceKind::Float(fc) => fc.from_index(index).map(ChoiceValue::Float),
            ChoiceKind::Bytes(bc) => bc.from_index(index).map(ChoiceValue::Bytes),
            ChoiceKind::String(sc) => sc.from_index(index).map(ChoiceValue::String),
            ChoiceKind::Clone => None,
        }
    }

    /// Pair `value` with this kind, proving at the type level that the two
    /// agree: `Some` exactly when the value's variant matches the kind *and*
    /// passes its validation. Clone kinds resolve nothing — their values
    /// are handled structurally by the callers that walk clone positions —
    /// so they fall through to `None` with every other mismatch.
    pub fn resolve(&self, value: &ChoiceValue) -> Option<ChoiceData> {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) if ic.validate(v) => {
                Some(ChoiceData::Integer(Arc::clone(ic), v.clone()))
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => Some(ChoiceData::Boolean(*v)),
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) if fc.validate(*v) => {
                Some(ChoiceData::Float(fc.clone(), *v))
            }
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) if bc.validate(v) => {
                Some(ChoiceData::Bytes(bc.clone(), v.clone()))
            }
            (ChoiceKind::String(sc), ChoiceValue::String(v)) if sc.validate(v) => {
                Some(ChoiceData::String(sc.clone(), v.clone()))
            }
            _ => None,
        }
    }

    /// Cardinality of this kind's choice space, or `None` for a clone kind
    /// (a clone's child space is unbounded).
    pub fn max_children(&self) -> Option<crate::native::bignum::BigUint> {
        use crate::native::bignum::BigUint;
        self.max_index().map(|mi| mi + BigUint::from(1u32))
    }

    /// `min(max_children(), cap)`, computed *without* materialising the exact
    /// cardinality for sequence kinds.
    ///
    /// The data-tree exhaustion check only needs to compare a node's
    /// cardinality against a small explored-child count, never the exact value.
    /// [`max_children`](Self::max_children) for a `Bytes`/`String` choice is
    /// `Σ alphabet^len` — a `BigUint` of up to hundreds of bits whose
    /// `BigUint::pow` dominated generation in profiles. This variant sums in
    /// saturating `u128` and stops the moment the running total reaches `cap`,
    /// so the astronomically-large powers are never built. Scalar kinds reuse
    /// their (cheap, `pow`-free) `max_index`, saturating any value past `u128`
    /// to `cap`.
    pub fn max_children_saturating(&self, cap: u128) -> u128 {
        use crate::native::bignum::ToPrimitive;
        let scalar = |max_index: crate::native::bignum::BigUint| {
            max_index
                .to_u128()
                .map_or(cap, |mi| mi.saturating_add(1).min(cap))
        };
        match self {
            ChoiceKind::Boolean(_) => 2u128.min(cap),
            ChoiceKind::Clone => cap,
            ChoiceKind::Integer(ic) => scalar(ic.max_index()),
            ChoiceKind::Float(fc) => scalar(fc.max_index()),
            ChoiceKind::Bytes(bc) => {
                sequence_max_children_saturating(256, bc.min_size, bc.max_size, cap)
            }
            ChoiceKind::String(sc) => sequence_max_children_saturating(
                sc.intervals.len() as u128,
                sc.min_size,
                sc.max_size,
                cap,
            ),
        }
    }

    /// Random value sampled from this kind's domain (with kind-appropriate
    /// bias), or `None` for a clone kind (clone values arise from executing
    /// a stream, never from sampling).
    pub fn random_value(&self, rng: &mut crate::native::rng::EngineRng) -> Option<ChoiceValue> {
        match self {
            ChoiceKind::Integer(ic) => Some(ChoiceValue::Integer(
                crate::native::core::state::biased_integer_sample(ic, rng),
            )),
            ChoiceKind::Boolean(_) => Some(ChoiceValue::Boolean(
                crate::native::core::state::weighted_boolean_sample(0.5, rng),
            )),
            ChoiceKind::Float(fc) => Some(ChoiceValue::Float(
                crate::native::core::state::biased_float_sample(fc, rng),
            )),
            ChoiceKind::Bytes(bc) => Some(ChoiceValue::Bytes(
                crate::native::core::state::biased_bytes_sample(bc, rng),
            )),
            ChoiceKind::String(sc) => Some(ChoiceValue::String(
                crate::native::core::state::biased_string_sample(sc, rng),
            )),
            ChoiceKind::Clone => None,
        }
    }

    /// Every possible value of this kind, if the total count fits under
    /// `cap`. `None` for the kinds whose domains never fit — floats (every
    /// bit pattern is distinct), non-empty sequences, and clones (unbounded
    /// child space).
    pub fn enumerate(&self, cap: u64) -> Option<Vec<ChoiceValue>> {
        let fits = |kind: &ChoiceKind| kind.max_children_saturating(cap as u128 + 1) <= cap as u128;
        match self {
            ChoiceKind::Integer(ic) => {
                if !fits(self) {
                    return None;
                }
                let mut v = Vec::new();
                let mut n = ic.min_value.clone();
                loop {
                    v.push(ChoiceValue::Integer(n.clone()));
                    if n == ic.max_value {
                        break;
                    }
                    n += 1;
                }
                Some(v)
            }
            ChoiceKind::Boolean(_) => {
                fits(self).then(|| vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)])
            }
            ChoiceKind::Float(_) => None,
            ChoiceKind::Bytes(bc) => {
                (bc.max_size == 0 && fits(self)).then(|| vec![ChoiceValue::Bytes(Vec::new())])
            }
            ChoiceKind::String(sc) => {
                (sc.max_size == 0 && fits(self)).then(|| vec![ChoiceValue::String(Vec::new())])
            }
            ChoiceKind::Clone => None,
        }
    }
}

/// A choice's constraint paired with the value drawn under it — the payload
/// of a [`ChoiceNode`]. Carrying the two in one enum makes a kind/value
/// mismatch unrepresentable: a consumer that needs both matches a single
/// variant and gets a proven-consistent pair.
///
/// The `IntegerChoice` constraint is shared via `Arc` because the shrinker
/// clones entire `Vec<ChoiceNode>` vectors thousands of times per shrink
/// run, while the constraint almost never changes; this turns three
/// `BigInt` deep-clones per integer node into a pointer bump. A clone
/// node's payload is the realized stream of its execution — realized by
/// construction, so consumers never re-prove it.
///
/// Equality mirrors the value semantics of [`ChoiceValue`] plus constraint
/// equality: floats (both bounds and value) compare bit-exactly, and clone
/// payloads compare by child values recursively.
#[derive(Clone, Debug)]
pub enum ChoiceData {
    Integer(Arc<IntegerChoice>, BigInt),
    Boolean(bool),
    Float(FloatChoice, f64),
    Bytes(BytesChoice, Vec<u8>),
    String(StringChoice, Vec<u32>),
    Clone(Arc<RealizedStream>),
}

impl PartialEq for ChoiceData {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChoiceData::Integer(ac, av), ChoiceData::Integer(bc, bv)) => ac == bc && av == bv,
            (ChoiceData::Boolean(a), ChoiceData::Boolean(b)) => a == b,
            (ChoiceData::Float(ac, av), ChoiceData::Float(bc, bv)) => {
                ac == bc && av.to_bits() == bv.to_bits()
            }
            (ChoiceData::Bytes(ac, av), ChoiceData::Bytes(bc, bv)) => ac == bc && av == bv,
            (ChoiceData::String(ac, av), ChoiceData::String(bc, bv)) => ac == bc && av == bv,
            (ChoiceData::Clone(a), ChoiceData::Clone(b)) => {
                CloneValues::Stream(a) == CloneValues::Stream(b)
            }
            _ => false,
        }
    }
}

impl Eq for ChoiceData {}

impl ChoiceData {
    /// The constraint half of the pair, as a standalone [`ChoiceKind`].
    /// Cheap: the integer constraint shares its `Arc` and the others are a
    /// few machine words.
    pub fn kind(&self) -> ChoiceKind {
        match self {
            ChoiceData::Integer(ic, _) => ChoiceKind::Integer(Arc::clone(ic)),
            ChoiceData::Boolean(_) => ChoiceKind::Boolean(BooleanChoice),
            ChoiceData::Float(fc, _) => ChoiceKind::Float(fc.clone()),
            ChoiceData::Bytes(bc, _) => ChoiceKind::Bytes(bc.clone()),
            ChoiceData::String(sc, _) => ChoiceKind::String(sc.clone()),
            ChoiceData::Clone(_) => ChoiceKind::Clone,
        }
    }

    /// The value half of the pair, as an owned [`ChoiceValue`]. Scalar and
    /// sequence payloads are cloned; a clone payload is wrapped in a
    /// [`CloneRecord`] sharing the realized stream.
    pub fn value(&self) -> ChoiceValue {
        match self {
            ChoiceData::Integer(_, v) => ChoiceValue::Integer(v.clone()),
            ChoiceData::Boolean(v) => ChoiceValue::Boolean(*v),
            ChoiceData::Float(_, v) => ChoiceValue::Float(*v),
            ChoiceData::Bytes(_, v) => ChoiceValue::Bytes(v.clone()),
            ChoiceData::String(_, v) => ChoiceValue::String(v.clone()),
            ChoiceData::Clone(rs) => {
                ChoiceValue::Clone(Arc::new(CloneRecord::from_stream(Arc::clone(rs))))
            }
        }
    }

    /// The value half of the pair, as a borrowed [`ChoiceValueRef`].
    pub fn value_ref(&self) -> ChoiceValueRef<'_> {
        match self {
            ChoiceData::Integer(_, v) => ChoiceValueRef::Integer(v),
            ChoiceData::Boolean(v) => ChoiceValueRef::Boolean(*v),
            ChoiceData::Float(_, v) => ChoiceValueRef::Float(*v),
            ChoiceData::Bytes(_, v) => ChoiceValueRef::Bytes(v),
            ChoiceData::String(_, v) => ChoiceValueRef::String(v),
            ChoiceData::Clone(rs) => ChoiceValueRef::Clone(CloneValues::Stream(rs)),
        }
    }

    /// The simplest value of this pair's constraint (the value the shrinker
    /// aims at), as a [`ChoiceValue`].
    pub fn simplest_value(&self) -> ChoiceValue {
        self.kind().simplest()
    }

    /// Whether the value *is* the constraint's simplest value. Equivalent
    /// to `self.value() == self.simplest_value()` without the allocation
    /// for the scalar kinds.
    pub fn is_simplest(&self) -> bool {
        match self {
            ChoiceData::Integer(ic, v) => *v == ic.simplest(),
            ChoiceData::Boolean(v) => !v,
            ChoiceData::Float(fc, v) => v.to_bits() == fc.simplest().to_bits(),
            ChoiceData::Bytes(bc, v) => *v == bc.simplest(),
            ChoiceData::String(sc, v) => *v == sc.simplest(),
            ChoiceData::Clone(rs) => rs.is_empty(),
        }
    }

    /// The same constraint paired with its simplest value.
    pub fn with_simplest(&self) -> ChoiceData {
        match self {
            ChoiceData::Integer(ic, _) => ChoiceData::Integer(Arc::clone(ic), ic.simplest()),
            ChoiceData::Boolean(_) => ChoiceData::Boolean(false),
            ChoiceData::Float(fc, _) => ChoiceData::Float(fc.clone(), fc.simplest()),
            ChoiceData::Bytes(bc, _) => ChoiceData::Bytes(bc.clone(), bc.simplest()),
            ChoiceData::String(sc, _) => ChoiceData::String(sc.clone(), sc.simplest()),
            ChoiceData::Clone(_) => ChoiceData::Clone(Arc::new(RealizedStream::empty())),
        }
    }

    /// The same constraint paired with `value`: `Some` exactly when the
    /// value fits the constraint (matching variant and passing validation;
    /// for a clone, a realized record). This is the single place a bare
    /// [`ChoiceValue`] gets proven against an existing node's constraint.
    pub fn with_value(&self, value: &ChoiceValue) -> Option<ChoiceData> {
        match (self, value) {
            (ChoiceData::Clone(_), ChoiceValue::Clone(r)) => {
                r.realized().map(|rs| ChoiceData::Clone(Arc::clone(rs)))
            }
            _ => self.kind().resolve(value),
        }
    }

    /// The value's dense index under its constraint's sort order, or `None`
    /// for a clone (no dense index).
    pub fn to_index(&self) -> Option<crate::native::bignum::BigUint> {
        match self {
            ChoiceData::Integer(ic, v) => Some(ic.to_index(v)),
            ChoiceData::Boolean(v) => Some(BooleanChoice.to_index(*v)),
            ChoiceData::Float(fc, v) => Some(fc.to_index(*v)),
            ChoiceData::Bytes(bc, v) => Some(bc.to_index(v)),
            ChoiceData::String(sc, v) => Some(sc.to_index(v)),
            ChoiceData::Clone(_) => None,
        }
    }

    /// The constraint's value at `index`; see [`ChoiceKind::from_index`].
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<ChoiceValue> {
        self.kind().from_index(index)
    }

    /// The constraint's largest valid index; see [`ChoiceKind::max_index`].
    pub fn max_index(&self) -> Option<crate::native::bignum::BigUint> {
        self.kind().max_index()
    }

    /// The integer constraint and value, when this is an integer pair.
    pub fn as_integer(&self) -> Option<(&IntegerChoice, &BigInt)> {
        match self {
            ChoiceData::Integer(ic, v) => Some((ic, v)),
            _ => None,
        }
    }

    /// The float constraint and value, when this is a float pair.
    pub fn as_float(&self) -> Option<(&FloatChoice, f64)> {
        match self {
            ChoiceData::Float(fc, v) => Some((fc, *v)),
            _ => None,
        }
    }

    /// The bytes constraint and value, when this is a bytes pair.
    pub fn as_bytes(&self) -> Option<(&BytesChoice, &[u8])> {
        match self {
            ChoiceData::Bytes(bc, v) => Some((bc, v)),
            _ => None,
        }
    }

    /// The string constraint and value, when this is a string pair.
    pub fn as_string(&self) -> Option<(&StringChoice, &[u32])> {
        match self {
            ChoiceData::String(sc, v) => Some((sc, v)),
            _ => None,
        }
    }

    /// The realized stream, when this is a clone position.
    pub fn as_clone(&self) -> Option<&Arc<RealizedStream>> {
        match self {
            ChoiceData::Clone(rs) => Some(rs),
            _ => None,
        }
    }
}

/// A single recorded choice in a test case: the constraint/value pair plus
/// whether the draw was forced.
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceNode {
    pub data: ChoiceData,
    pub was_forced: bool,
}

/// Kind of fallback a [`ChoiceTemplate`] produces. Carried as an enum so
/// future kinds (e.g. `"random"`) can be added without changing the
/// surrounding API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChoiceTemplateKind {
    /// Resolve each templated draw to `kind.simplest()` of the requested
    /// choice kind.
    Simplest,
}

/// A deferred-resolution marker that drives every draw past the explicit
/// `prefix` of a [`crate::native::core::NativeTestCase`].
///
/// `count = None` is infinite — the template applies to every draw until
/// the test case ends naturally (e.g. `max_size` is hit). `count = Some(n)`
/// produces exactly `n` resolved values, after which the next draw marks
/// overrun (`Status::EarlyStop` + `StopTest`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceTemplate {
    pub kind: ChoiceTemplateKind,
    pub count: Option<usize>,
}

impl ChoiceTemplate {
    /// Build a [`ChoiceTemplateKind::Simplest`] template with the given
    /// remaining-draws count. `Some(0)` is rejected at construction time.
    pub fn simplest(count: Option<usize>) -> Self {
        if let Some(n) = count {
            hegel_internal_assert!(n > 0, "ChoiceTemplate count must be positive (got 0)");
        }
        Self {
            kind: ChoiceTemplateKind::Simplest,
            count,
        }
    }
}

impl ChoiceNode {
    pub fn new(data: ChoiceData, was_forced: bool) -> Self {
        Self { data, was_forced }
    }

    /// An integer node.
    pub fn integer(constraint: IntegerChoice, value: BigInt, was_forced: bool) -> Self {
        Self::new(ChoiceData::Integer(Arc::new(constraint), value), was_forced)
    }

    /// A boolean node.
    pub fn boolean(value: bool, was_forced: bool) -> Self {
        Self::new(ChoiceData::Boolean(value), was_forced)
    }

    /// A float node.
    pub fn float(constraint: FloatChoice, value: f64, was_forced: bool) -> Self {
        Self::new(ChoiceData::Float(constraint, value), was_forced)
    }

    /// A bytes node.
    pub fn bytes(constraint: BytesChoice, value: Vec<u8>, was_forced: bool) -> Self {
        Self::new(ChoiceData::Bytes(constraint, value), was_forced)
    }

    /// A string node.
    pub fn string(constraint: StringChoice, value: Vec<u32>, was_forced: bool) -> Self {
        Self::new(ChoiceData::String(constraint, value), was_forced)
    }

    /// A clone node carrying the realized stream of its execution.
    pub fn clone_stream(stream: Arc<RealizedStream>, was_forced: bool) -> Self {
        Self::new(ChoiceData::Clone(stream), was_forced)
    }

    /// The node's constraint, as a standalone [`ChoiceKind`].
    pub fn kind(&self) -> ChoiceKind {
        self.data.kind()
    }

    /// The node's value, as an owned [`ChoiceValue`].
    pub fn value(&self) -> ChoiceValue {
        self.data.value()
    }

    /// This node with `value` in place of its current value, when the value
    /// fits the node's constraint; see [`ChoiceData::with_value`].
    pub fn with_value(&self, value: &ChoiceValue) -> Option<ChoiceNode> {
        self.data.with_value(value).map(|data| ChoiceNode {
            data,
            was_forced: self.was_forced,
        })
    }

    /// This node with its constraint's simplest value.
    pub fn with_simplest(&self) -> ChoiceNode {
        ChoiceNode {
            data: self.data.with_simplest(),
            was_forced: self.was_forced,
        }
    }
}

/// Borrowed view of a [`ChoiceNode`]'s sort key, used to order nodes during
/// shrinking (via [`NodesSortKey`]).
///
/// Cross-variant order is `Scalar < Seq < Clone`; scalars compare by
/// `(magnitude, sign)`, sequence variants shortlex on length then per-element
/// keys, and clones recursively by their child sequences' [`NodesSortKey`].
/// The per-element keys for [`SeqKeys`] are resolved lazily during
/// comparison — `String` defers `codepoint_key` to the moment of compare —
/// so no `Vec<u32>` ever gets allocated.
pub enum NodeSortKeyRef<'a> {
    Scalar(crate::native::bignum::BigUint, bool),
    Seq(SeqKeys<'a>),
    Clone(&'a RealizedStream),
}

/// The per-element key sequence of a sequence-kind node: bytes key as
/// themselves, string codepoints key by their alphabet's shrink-order rank.
pub enum SeqKeys<'a> {
    Bytes(&'a [u8]),
    String(&'a StringChoice, &'a [u32]),
}

impl SeqKeys<'_> {
    fn len(&self) -> usize {
        match self {
            SeqKeys::Bytes(b) => b.len(),
            SeqKeys::String(_, cps) => cps.len(),
        }
    }

    fn key_at(&self, i: usize) -> u32 {
        match self {
            SeqKeys::Bytes(b) => b[i] as u32,
            SeqKeys::String(sc, cps) => sc.codepoint_key(cps[i]),
        }
    }
}

impl<'a> NodeSortKeyRef<'a> {
    fn category(&self) -> u8 {
        match self {
            NodeSortKeyRef::Scalar(..) => 0,
            NodeSortKeyRef::Seq(..) => 1,
            NodeSortKeyRef::Clone(..) => 2,
        }
    }
}

impl<'a> PartialEq for NodeSortKeyRef<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl<'a> Eq for NodeSortKeyRef<'a> {}

impl<'a> PartialOrd for NodeSortKeyRef<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for NodeSortKeyRef<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (NodeSortKeyRef::Scalar(am, an), NodeSortKeyRef::Scalar(bm, bn)) => {
                (am, an).cmp(&(bm, bn))
            }
            (NodeSortKeyRef::Clone(a), NodeSortKeyRef::Clone(b)) => realized_streams_cmp(a, b),
            (NodeSortKeyRef::Seq(a), NodeSortKeyRef::Seq(b)) => {
                match a.len().cmp(&b.len()) {
                    Ordering::Equal => {}
                    ord => return ord,
                }
                for i in 0..a.len() {
                    match a.key_at(i).cmp(&b.key_at(i)) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            _ => self.category().cmp(&other.category()),
        }
    }
}

/// Ordering between two realized clone streams: flattened choice count
/// first, then child count, then per-child node keys.
fn realized_streams_cmp(a: &RealizedStream, b: &RealizedStream) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match a.flat_len().cmp(&b.flat_len()) {
        Ordering::Equal => {}
        ord => return ord,
    }
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {}
        ord => return ord,
    }
    elementwise_nodes_cmp(a.nodes(), b.nodes())
}

/// Per-node key comparison of two equal-length node slices.
fn elementwise_nodes_cmp(a: &[ChoiceNode], b: &[ChoiceNode]) -> std::cmp::Ordering {
    for (na, nb) in a.iter().zip(b.iter()) {
        match na.sort_key_ref().cmp(&nb.sort_key_ref()) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

impl ChoiceNode {
    /// Borrowed view of the node's sort key: a [`NodeSortKeyRef`] that
    /// borrows the node's value (and, for `String`, its choice config).
    /// Comparison is allocation-free.
    pub fn sort_key_ref(&self) -> NodeSortKeyRef<'_> {
        match &self.data {
            ChoiceData::Integer(ic, v) => {
                let (mag, neg) = ic.sort_key(v);
                NodeSortKeyRef::Scalar(mag, neg)
            }
            ChoiceData::Boolean(v) => NodeSortKeyRef::Scalar(BigUint::from(u32::from(*v)), false),
            ChoiceData::Float(fc, v) => {
                let (mag, neg) = fc.sort_key(*v);
                NodeSortKeyRef::Scalar(BigUint::from(mag), neg)
            }
            ChoiceData::Bytes(_, v) => NodeSortKeyRef::Seq(SeqKeys::Bytes(v)),
            ChoiceData::String(sc, v) => NodeSortKeyRef::Seq(SeqKeys::String(sc, v)),
            ChoiceData::Clone(rs) => NodeSortKeyRef::Clone(rs),
        }
    }
}

/// Sort key for a sequence of choice nodes, as a borrowed view.
///
/// Sequences with fewer *total* choices — counting the children of clone
/// nodes recursively, see [`flattened_len`] — are simpler, so deleting a
/// draw inside a clone is progress just like deleting a top-level draw.
/// Among equal flattened counts, fewer top-level nodes win (plain shortlex;
/// for clone-free sequences the flattened count *is* the length, so this
/// matches the historical shortlex order exactly), and among equal lengths,
/// smaller per-element keys win. Comparison is allocation-free: per-element
/// keys are resolved lazily and the first inequality short-circuits.
#[derive(Clone, Copy)]
pub struct NodesSortKey<'a>(pub &'a [ChoiceNode]);

impl<'a> PartialEq for NodesSortKey<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl<'a> Eq for NodesSortKey<'a> {}

impl<'a> PartialOrd for NodesSortKey<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for NodesSortKey<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match flattened_len(self.0).cmp(&flattened_len(other.0)) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.0.len().cmp(&other.0.len()) {
            Ordering::Equal => {}
            ord => return ord,
        }
        elementwise_nodes_cmp(self.0, other.0)
    }
}

/// Shortlex sort key for a sequence of choice nodes.
/// Shorter sequences are simpler; among equal lengths, smaller values win.
/// Returns a borrowed view that compares allocation-free; see
/// [`NodesSortKey::to_owned`] when a long-lived snapshot is needed.
pub fn sort_key(nodes: &[ChoiceNode]) -> NodesSortKey<'_> {
    NodesSortKey(nodes)
}

/// Test case status, ordered from least to most "significant".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// Ran out of data before completing.
    EarlyStop = 0,
    /// Test case was invalid (e.g. assumption failed).
    Invalid = 1,
    /// Test case completed normally.
    Valid = 2,
    /// Test case found a failure.
    Interesting = 3,
}

/// Error raised while drawing from the engine.
///
/// `StopTest` is the overwhelmingly common case: normal data-exhaustion
/// control flow that ends the current test case. `InvalidArgument` carries a
/// caller-supplied-argument diagnostic that must surface as an error
/// (libhegel: `HEGEL_E_INVALID_ARG`) or a panic (main library), but never an
/// uncaught panic that crosses the FFI boundary and aborts the host process.
#[derive(Debug)]
pub enum EngineError {
    /// The test case ran out of data (choice buffer exhausted).
    Overrun,
    /// The engine concluded this test case is invalid (over-deep span,
    /// exhausted unique collection, regex pattern mismatch, etc.). Terminal:
    /// it sets the test case's status, so the conclusion is write-once and the
    /// body cannot later report a different outcome.
    InvalidTestCase,
    /// A single draw could not be satisfied (e.g. drawing from an exhausted
    /// variable pool), but the test case is *not* concluded. Recoverable: the
    /// caller may handle the rejection and still conclude the case however it
    /// likes. Unlike [`Self::InvalidTestCase`] it leaves the status unset and
    /// does not abort the data source.
    AssumeViolation,
    /// A caller-supplied draw argument was semantically invalid (inverted
    /// bounds, empty character set, unparseable regex, etc.). The string is
    /// a human-readable diagnostic.
    InvalidArgument(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Overrun => write!(f, "choice buffer exhausted (Overrun)"),
            EngineError::InvalidTestCase => write!(f, "engine rejected test case (Invalid)"),
            EngineError::AssumeViolation => write!(f, "draw could not be satisfied (Assume)"),
            EngineError::InvalidArgument(msg) => write!(f, "{msg}"),
        }
    }
}

/// Opaque key identifying one source of "interesting" outcomes
/// (one bug). Matches the cross-backend protocol contract: it's
/// whatever string `tc.mark_complete(status, origin)` carries, and
/// the native runner keys [`InterestingExample`]s on equality of
/// these strings.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterestingOrigin(pub String);

#[cfg(test)]
#[path = "../../../tests/embedded/native/choices_tests.rs"]
mod tests;
