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

    pub fn max_children(&self) -> BigUint {
        self.max_index() + BigUint::from(1u32)
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
    pub intervals: IntervalSet,
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
    /// the schema layer before constructing the `StringChoice`.
    pub(crate) fn simplest_codepoint(&self) -> u32 {
        hegel_internal_assert!(
            !self.intervals.is_empty(),
            "StringChoice::simplest_codepoint: empty alphabet"
        );
        self.intervals.char_in_shrink_order(0) as u32
    }

    /// The simplest sequence of codepoints of length `min_size`, built
    /// from repeated [`simplest_codepoint`].
    pub fn simplest(&self) -> Vec<u32> {
        vec![self.simplest_codepoint(); self.min_size]
    }

    /// Second-simplest codepoint sequence, used for type-punning during replay.
    pub fn unit(&self) -> Vec<u32> {
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
        hegel_internal_assert!(!alpha.is_zero(), "StringChoice::from_index: empty alphabet");
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
        let is_neg = s.is_sign_negative();
        for offset in 1u64..4 {
            let v_mag = index_to_float(base + offset);
            let v = if is_neg { -v_mag } else { v_mag };
            if !v.is_nan() && self.validate(v) {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Integer(IntegerChoice),
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

/// The children of a [`CloneRecord`]: either bare choice values (a record
/// deserialized from storage, where kinds and spans were never persisted) or
/// the realized nodes of an executed stream together with its span structure.
#[derive(Clone, Debug)]
enum CloneChildren {
    Values(Vec<ChoiceValue>),
    Realized {
        nodes: Vec<ChoiceNode>,
        spans: Vec<Span>,
        span_events: Vec<(usize, SpanEvent)>,
    },
}

/// The choice sequence of one cloned stream, carried as the value of a
/// [`ChoiceKind::Clone`] node in its parent stream.
///
/// A record's *identity* — equality, hashing, and its contribution to sort
/// keys — is the sequence of child choice values, recursively. The realized
/// info (child kinds, forced flags, spans, span events) is carried when the
/// record was produced by executing the stream, and is what the shrinker and
/// data tree interrogate; it is never serialized and never part of equality,
/// so a record round-tripped through storage compares equal to the realized
/// record it came from.
#[derive(Clone, Debug)]
pub struct CloneRecord {
    children: CloneChildren,
    /// Cached [`flattened_len`] of the children, so sort-key comparison of
    /// deep trees costs one integer read per record instead of a walk.
    flat_len: usize,
}

impl CloneRecord {
    /// A record from bare child values (deserialized storage, or a
    /// hand-built replay prefix). Carries no realized info.
    pub fn from_values(values: Vec<ChoiceValue>) -> Self {
        let flat_len = flattened_len_of_values(values.iter());
        CloneRecord {
            children: CloneChildren::Values(values),
            flat_len,
        }
    }

    /// A record from an executed stream: its realized nodes plus the span
    /// structure recorded alongside them.
    pub fn from_run(
        nodes: Vec<ChoiceNode>,
        spans: Vec<Span>,
        span_events: Vec<(usize, SpanEvent)>,
    ) -> Self {
        let flat_len = flattened_len(&nodes);
        CloneRecord {
            children: CloneChildren::Realized {
                nodes,
                spans,
                span_events,
            },
            flat_len,
        }
    }

    /// The empty record: a clone that drew nothing.
    pub fn empty() -> Self {
        Self::from_values(Vec::new())
    }

    /// Number of direct children (top-level choices in the cloned stream).
    pub fn len(&self) -> usize {
        match &self.children {
            CloneChildren::Values(v) => v.len(),
            CloneChildren::Realized { nodes, .. } => nodes.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `i`-th child choice value.
    pub fn value_at(&self, i: usize) -> &ChoiceValue {
        match &self.children {
            CloneChildren::Values(v) => &v[i],
            CloneChildren::Realized { nodes, .. } => &nodes[i].value,
        }
    }

    /// The child choice values, in order.
    pub fn values(&self) -> impl Iterator<Item = &ChoiceValue> + '_ {
        let (values, nodes) = match &self.children {
            CloneChildren::Values(v) => (Some(v.iter()), None),
            CloneChildren::Realized { nodes, .. } => (None, Some(nodes.iter())),
        };
        values
            .into_iter()
            .flatten()
            .chain(nodes.into_iter().flatten().map(|n| &n.value))
    }

    /// The realized child nodes, when this record came from an execution.
    pub fn realized_nodes(&self) -> Option<&[ChoiceNode]> {
        match &self.children {
            CloneChildren::Values(_) => None,
            CloneChildren::Realized { nodes, .. } => Some(nodes),
        }
    }

    /// The cloned stream's recorded spans (empty for a values-only record).
    pub fn spans(&self) -> &[Span] {
        match &self.children {
            CloneChildren::Values(_) => &[],
            CloneChildren::Realized { spans, .. } => spans,
        }
    }

    /// The cloned stream's span open/close events, tagged with the child
    /// draw position at which each fired (empty for a values-only record).
    pub fn span_events(&self) -> &[(usize, SpanEvent)] {
        match &self.children {
            CloneChildren::Values(_) => &[],
            CloneChildren::Realized { span_events, .. } => span_events,
        }
    }

    /// Total number of choices in the cloned stream, counting nested clones'
    /// children recursively. Cached at construction.
    pub fn flat_len(&self) -> usize {
        self.flat_len
    }
}

impl PartialEq for CloneRecord {
    fn eq(&self, other: &Self) -> bool {
        self.flat_len == other.flat_len
            && self.len() == other.len()
            && self.values().zip(other.values()).all(|(a, b)| a == b)
    }
}

impl Eq for CloneRecord {}

impl std::hash::Hash for CloneRecord {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for v in self.values() {
            v.hash(state);
        }
    }
}

/// Total number of choices in `nodes`, counting each clone node as one
/// choice plus its children, recursively. Equal to `nodes.len()` for a
/// sequence with no clone nodes.
pub fn flattened_len(nodes: &[ChoiceNode]) -> usize {
    flattened_len_of_values(nodes.iter().map(|n| &n.value))
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
        std::mem::discriminant(self).hash(state);
        match self {
            ChoiceValue::Integer(n) => n.hash(state),
            ChoiceValue::Boolean(b) => b.hash(state),
            ChoiceValue::Float(f) => f.to_bits().hash(state),
            ChoiceValue::Bytes(v) => v.hash(state),
            ChoiceValue::String(v) => v.hash(state),
            ChoiceValue::Clone(r) => r.hash(state),
        }
    }
}

/// `Σ_{len=min_size..=max_size} alphabet^len` — the number of distinct
/// sequences over an `alphabet`-symbol set — saturating at `cap`.
///
/// Backs [`ChoiceKind::max_children_saturating`] for the `Bytes` / `String`
/// kinds: it accumulates in `u128` and returns `cap` the instant the running
/// total reaches it, so a huge `max_size` never forces a multi-hundred-bit
/// `BigUint`. `saturating_mul` pins `power` at `u128::MAX` once the alphabet
/// outgrows the word, which then drives `total` to `cap` on the next term.
fn sequence_max_children_saturating(
    alphabet: u128,
    min_size: usize,
    max_size: usize,
    cap: u128,
) -> u128 {
    let mut total: u128 = 0;
    let mut power: u128 = 1;
    for len in 0..=max_size {
        if len >= min_size {
            total = total.saturating_add(power);
            if total >= cap {
                return cap;
            }
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

    /// Largest valid index for [`from_index`].
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        match self {
            ChoiceKind::Integer(ic) => ic.max_index(),
            ChoiceKind::Boolean(bc) => bc.max_index(),
            ChoiceKind::Float(fc) => fc.max_index(),
            ChoiceKind::Bytes(bc) => bc.max_index(),
            ChoiceKind::String(sc) => sc.max_index(),
            ChoiceKind::Clone => unreachable!("clone choices have no dense index"),
        }
    }

    /// Convert a value to its dense index under this kind's sort order.
    pub fn to_index(&self, value: &ChoiceValue) -> crate::native::bignum::BigUint {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.to_index(v),
            (ChoiceKind::Boolean(bc), ChoiceValue::Boolean(v)) => bc.to_index(*v),
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.to_index(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.to_index(v),
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => sc.to_index(v),
            (ChoiceKind::Clone, _) => unreachable!("clone choices have no dense index"),
            _ => panic!("ChoiceKind::to_index: kind/value mismatch"),
        }
    }

    /// Inverse of [`to_index`]. Returns `None` when the index is out of range.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<ChoiceValue> {
        match self {
            ChoiceKind::Integer(ic) => ic.from_index(index).map(ChoiceValue::Integer),
            ChoiceKind::Boolean(bc) => bc.from_index(index).map(ChoiceValue::Boolean),
            ChoiceKind::Float(fc) => fc.from_index(index).map(ChoiceValue::Float),
            ChoiceKind::Bytes(bc) => bc.from_index(index).map(ChoiceValue::Bytes),
            ChoiceKind::String(sc) => sc.from_index(index).map(ChoiceValue::String),
            ChoiceKind::Clone => unreachable!("clone choices have no dense index"),
        }
    }

    /// Whether `value` is a valid draw for this kind.
    pub fn validate(&self, value: &ChoiceValue) -> bool {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.validate(v),
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(_)) => true,
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.validate(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.validate(v),
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => sc.validate(v),
            (ChoiceKind::Clone, ChoiceValue::Clone(_)) => true,
            _ => false,
        }
    }

    /// Cardinality of this kind's choice space.
    pub fn max_children(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        match self {
            ChoiceKind::Integer(ic) => ic.max_children(),
            ChoiceKind::Boolean(_) => BigUint::from(2u32),
            ChoiceKind::Float(fc) => fc.max_index() + BigUint::from(1u32),
            ChoiceKind::Bytes(bc) => bc.max_index() + BigUint::from(1u32),
            ChoiceKind::String(sc) => sc.max_index() + BigUint::from(1u32),
            ChoiceKind::Clone => unreachable!("clone choices have no dense index"),
        }
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

    /// Random value sampled from this kind's domain (with kind-appropriate bias).
    pub fn random_value(&self, rng: &mut crate::native::rng::EngineRng) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => {
                ChoiceValue::Integer(crate::native::core::state::biased_integer_sample(ic, rng))
            }
            ChoiceKind::Boolean(_) => ChoiceValue::Boolean(
                crate::native::core::state::weighted_boolean_sample(0.5, rng),
            ),
            ChoiceKind::Float(fc) => {
                ChoiceValue::Float(crate::native::core::state::biased_float_sample(fc, rng))
            }
            ChoiceKind::Bytes(bc) => {
                ChoiceValue::Bytes(crate::native::core::state::biased_bytes_sample(bc, rng))
            }
            ChoiceKind::String(sc) => {
                ChoiceValue::String(crate::native::core::state::biased_string_sample(sc, rng))
            }
            ChoiceKind::Clone => unreachable!("clone values are never randomly sampled"),
        }
    }

    /// Every possible value of this kind, if the total count fits under `cap`.
    pub fn enumerate(&self, cap: u64) -> Option<Vec<ChoiceValue>> {
        if self.max_children_saturating(cap as u128 + 1) > cap as u128 {
            return None;
        }
        match self {
            ChoiceKind::Integer(ic) => {
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
            ChoiceKind::Boolean(_) => Some(vec![
                ChoiceValue::Boolean(false),
                ChoiceValue::Boolean(true),
            ]),
            ChoiceKind::Float(_) => {
                unreachable!("FloatChoice max_children always exceeds u64::MAX cap")
            }
            ChoiceKind::Bytes(bc) => {
                if bc.max_size == 0 {
                    Some(vec![ChoiceValue::Bytes(Vec::new())])
                } else {
                    None
                }
            }
            ChoiceKind::String(sc) => {
                if sc.max_size == 0 {
                    Some(vec![ChoiceValue::String(Vec::new())])
                } else {
                    None
                }
            }
            ChoiceKind::Clone => {
                unreachable!("Clone max_children always exceeds the enumeration cap")
            }
        }
    }
}

/// A single recorded choice in a test case.
///
/// The `kind` is wrapped in `Arc` because the shrinker clones entire
/// `Vec<ChoiceNode>` vectors thousands of times per shrink run, while the
/// kind almost never changes. This turns three `BigInt` deep-clones per
/// integer node into a single pointer bump.
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceNode {
    pub kind: Arc<ChoiceKind>,
    pub value: ChoiceValue,
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
    pub fn new(kind: ChoiceKind, value: ChoiceValue, was_forced: bool) -> Self {
        Self {
            kind: Arc::new(kind),
            value,
            was_forced,
        }
    }

    pub fn with_value(&self, value: ChoiceValue) -> ChoiceNode {
        ChoiceNode {
            kind: Arc::clone(&self.kind),
            value,
            was_forced: self.was_forced,
        }
    }
}

/// Borrowed view of a [`ChoiceNode`]'s sort key, used to order nodes during
/// shrinking (via [`NodesSortKey`]).
///
/// Cross-variant order is `Scalar < Sequence < Clone`; scalars compare by
/// `(magnitude, sign)`, sequence variants shortlex on length then per-element
/// keys, and clones recursively by their child sequences' [`NodesSortKey`].
/// The per-element keys for `Bytes` and `String` are resolved lazily
/// during comparison — `String` defers `codepoint_key` to the moment of
/// compare — so no `Vec<u32>` ever gets allocated.
pub enum NodeSortKeyRef<'a> {
    Scalar(crate::native::bignum::BigUint, bool),
    Bytes(&'a [u8]),
    String(&'a StringChoice, &'a [u32]),
    Clone(&'a CloneRecord),
}

impl<'a> NodeSortKeyRef<'a> {
    fn category(&self) -> u8 {
        match self {
            NodeSortKeyRef::Scalar(..) => 0,
            NodeSortKeyRef::Bytes(..) | NodeSortKeyRef::String(..) => 1,
            NodeSortKeyRef::Clone(..) => 2,
        }
    }

    /// Length of the underlying element sequence. Only meaningful for
    /// sequence variants; the only call site (the sequence-vs-sequence
    /// arm of `cmp`) gates on category() before invoking.
    fn seq_len(&self) -> usize {
        match self {
            NodeSortKeyRef::Bytes(b) => b.len(),
            NodeSortKeyRef::String(_, cps) => cps.len(),
            NodeSortKeyRef::Scalar(..) | NodeSortKeyRef::Clone(..) => {
                unreachable!("seq_len on non-sequence")
            }
        }
    }

    /// `i`-th per-element key in the sort-order alphabet. `i` must be in
    /// `0..self.seq_len()`. Calling on `Scalar` or `Clone` is unreachable in
    /// the only call sites (sequence-element comparison).
    fn seq_key_at(&self, i: usize) -> u32 {
        match self {
            NodeSortKeyRef::Bytes(b) => b[i] as u32,
            NodeSortKeyRef::String(sc, cps) => sc.codepoint_key(cps[i]),
            NodeSortKeyRef::Scalar(..) | NodeSortKeyRef::Clone(..) => {
                unreachable!("seq_key_at on non-sequence")
            }
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
            (NodeSortKeyRef::Clone(a), NodeSortKeyRef::Clone(b)) => clone_records_cmp(a, b),
            _ if self.category() != other.category() => self.category().cmp(&other.category()),
            _ => {
                let la = self.seq_len();
                let lb = other.seq_len();
                match la.cmp(&lb) {
                    Ordering::Equal => {}
                    ord => return ord,
                }
                for i in 0..la {
                    match self.seq_key_at(i).cmp(&other.seq_key_at(i)) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
        }
    }
}

/// Ordering between two clone records: flattened choice count first, then
/// child count, then per-child node keys. The elementwise step needs the
/// children's kinds and so requires realized records; sort keys are only ever
/// computed over realized choice sequences (a values-only record exists only
/// inside replay prefixes, which are never sort-key-compared).
fn clone_records_cmp(a: &CloneRecord, b: &CloneRecord) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match a.flat_len().cmp(&b.flat_len()) {
        Ordering::Equal => {}
        ord => return ord,
    }
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {}
        ord => return ord,
    }
    let a_nodes = a
        .realized_nodes()
        .unwrap_or_else(|| unreachable!("sort keys are only computed over realized sequences"));
    let b_nodes = b
        .realized_nodes()
        .unwrap_or_else(|| unreachable!("sort keys are only computed over realized sequences"));
    elementwise_nodes_cmp(a_nodes, b_nodes)
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
    /// Borrowed counterpart of [`Self::sort_key`]. Returns a
    /// [`NodeSortKeyRef`] that borrows the node's value (and, for
    /// `String`, its choice config). Same ordering, no allocation.
    pub fn sort_key_ref(&self) -> NodeSortKeyRef<'_> {
        match (self.kind.as_ref(), &self.value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => {
                let (mag, neg) = ic.sort_key(v);
                NodeSortKeyRef::Scalar(mag, neg)
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => {
                NodeSortKeyRef::Scalar(BigUint::from(u32::from(*v)), false)
            }
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => {
                let (mag, neg) = fc.sort_key(*v);
                NodeSortKeyRef::Scalar(BigUint::from(mag), neg)
            }
            (ChoiceKind::Bytes(_), ChoiceValue::Bytes(v)) => NodeSortKeyRef::Bytes(v),
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => NodeSortKeyRef::String(sc, v),
            (ChoiceKind::Clone, ChoiceValue::Clone(r)) => NodeSortKeyRef::Clone(r),
            _ => unreachable!("mismatched choice kind and value"),
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

/// Error raised while interpreting a schema / drawing from the engine.
///
/// `StopTest` is the overwhelmingly common case: normal data-exhaustion
/// control flow that ends the current test case. `InvalidArgument` carries a
/// caller-supplied-schema diagnostic that must surface as an error
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
    /// A caller-supplied schema was semantically invalid (unknown type,
    /// empty character set, unparseable regex, etc.). The string is a
    /// human-readable diagnostic.
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
