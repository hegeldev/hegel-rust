// Choice types: the recorded decisions a test case makes.

use std::sync::Arc;

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
        // Values are enumerated by increasing distance `d` from `s`, emitting
        // the up value `s + d` before the down value `s - d` at each distance,
        // and dropping whichever side has run past its bound. The number of
        // values within distance `d` is therefore
        //     total(d) = min(d, above) + min(d, below),
        // a piecewise-linear function of `d` with breakpoints at `a` and `b`
        // (the smaller and larger of `above`/`below`):
        //     d <= a       -> 2d        (both sides live)
        //     a < d <= b   -> a + d     (small side exhausted)
        //     d >  b       -> a + b     (both sides exhausted = max_index)
        // Each regime inverts in closed form, so no search over `d` is needed.
        if index > &above + &below {
            return None;
        }
        let two_a = std::cmp::min(&above, &below) << 1usize;
        let one = BigUint::from(1u32);
        let (d, up) = if index <= two_a {
            // Both sides live: index 2d-1 is `s + d`, index 2d is `s - d`.
            let d = (&index + &one) >> 1u32;
            let up = !(&index % &BigUint::from(2u32)).is_zero();
            (d, up)
        } else {
            // Only the larger side continues, one value per distance beyond `a`.
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

    /// Shortlex sort key: `(length, bytes)`.
    pub fn sort_key(&self, value: &[u8]) -> (usize, Vec<u8>) {
        (value.len(), value.to_vec())
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
        assert!(
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
        // Pick the second-simplest character in the alphabet's shrink order
        // (position 1). If the alphabet has only one character, fall back to
        // lengthening the simplest, or to `simplest()` if the length is also
        // fixed.
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

    /// Shortlex sort key: `(length, Vec<shrink_order_position>)`.
    pub fn sort_key(&self, value: &[u32]) -> (usize, Vec<u32>) {
        let keys: Vec<u32> = value.iter().map(|&cp| self.codepoint_key(cp)).collect();
        (keys.len(), keys)
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
        assert!(!alpha.is_zero(), "StringChoice::from_index: empty alphabet");
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
}

/// Bit-exact equality so a `FloatChoice` recorded with `-0.0` doesn't compare
/// equal to one recorded with `0.0`, and distinct NaN payloads stay distinct.
impl PartialEq for FloatChoice {
    fn eq(&self, other: &Self) -> bool {
        self.min_value.to_bits() == other.min_value.to_bits()
            && self.max_value.to_bits() == other.max_value.to_bits()
            && self.allow_nan == other.allow_nan
            && self.allow_infinity == other.allow_infinity
    }
}

impl Eq for FloatChoice {}

impl FloatChoice {
    /// The simplest (lowest-sort-key) valid float for this choice.
    pub fn simplest(&self) -> f64 {
        use super::float_index::{float_to_index, index_to_float};

        if self.validate(0.0) {
            return 0.0;
        }

        let mut best: Option<f64> = None;
        let mut best_key: (u64, bool) = (u64::MAX, true);

        // Update best if v is valid and has a smaller sort key.
        macro_rules! try_candidate {
            ($v:expr) => {{
                let v: f64 = $v;
                if !v.is_nan() && self.validate(v) {
                    let is_neg = v.is_sign_negative();
                    let mag = if is_neg { -v } else { v };
                    let key = (float_to_index(mag), is_neg);
                    if key < best_key {
                        best = Some(v);
                        best_key = key;
                    }
                }
            }};
        }

        // Check boundaries first.
        if self.min_value.is_finite() {
            try_candidate!(self.min_value);
        }
        if self.max_value.is_finite() {
            try_candidate!(self.max_value);
        }

        // Smallest valid non-negative integer in range.
        if self.max_value >= 0.0 {
            let lo_int = self.min_value.max(0.0).ceil() as i64;
            try_candidate!(lo_int as f64);
        }
        // Largest valid non-positive integer in range.
        if self.min_value <= 0.0 {
            let hi_int = self.max_value.min(0.0).floor() as i64;
            try_candidate!(hi_int as f64);
        }

        // Simple non-integer fractions at each exponent level.
        for exp_enc in 0u64..64 {
            let base_idx = (1u64 << 63) | (exp_enc << 52);
            if (base_idx, false) >= best_key {
                break;
            }
            for mantissa_enc in 0u64..8 {
                let idx = base_idx | mantissa_enc;
                if (idx, false) >= best_key {
                    break;
                }
                let v = index_to_float(idx);
                try_candidate!(v);
                try_candidate!(-v);
            }
        }

        if let Some(v) = best {
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
        // 2^52 NaN payloads (one bit forced to 1) × 2 signs = 2^53 NaN slots.
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
        // NaN payload (one bit always forced to 1, see `from_index` below).
        let bits = v.to_bits();
        let nan_offset = bits & ((1u64 << 52) - 1);
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
        // Force bit 51 to 1 so the mantissa is non-zero.
        let mantissa = mantissa_base | (1u64 << 51);
        let bits = (sign << 63) | (0x7FFu64 << 52) | mantissa;
        let v = f64::from_bits(bits);
        return if v.is_nan() { Some(v) } else { None };
    }
    let is_neg_u: u64 = (&rank % BigUint::from(2u32))
        .try_into()
        .expect("mod 2 fits in u64");
    let mag_big = rank / BigUint::from(2u32);
    let mag_idx: u64 = (&mag_big).try_into().ok()?;
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
    let mut power: u128 = 1; // alphabet^0
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
        use rand::RngExt;
        match self {
            ChoiceKind::Integer(ic) => {
                ChoiceValue::Integer(crate::native::core::state::biased_integer_sample(ic, rng))
            }
            ChoiceKind::Boolean(_) => ChoiceValue::Boolean(rng.random::<bool>()),
            ChoiceKind::Float(fc) => {
                ChoiceValue::Float(crate::native::core::state::biased_float_sample(fc, rng))
            }
            ChoiceKind::Bytes(bc) => {
                ChoiceValue::Bytes(crate::native::core::state::biased_bytes_sample(bc, rng))
            }
            ChoiceKind::String(sc) => {
                ChoiceValue::String(crate::native::core::state::biased_string_sample(sc, rng))
            }
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
            // `max_children` for a `FloatChoice` is at least `2^53` (the NaN
            // payload count), which always exceeds the `cap: u64` early-return
            // threshold above. No caller can ever land here.
            ChoiceKind::Float(_) => {
                unreachable!("FloatChoice max_children always exceeds u64::MAX cap")
            }
            // For `BytesChoice` only the `max_size == 0` corner has a
            // sensible enumeration (one empty value). Any non-zero
            // `max_size` technically fits the `u64::MAX` cap up to
            // `max_size = 7` (`Σ 256^k` through `k = 7` stays just under
            // `2^64`), but the 256-way fan-out makes materialising the list
            // pointless for any caller that would actually want it.
            ChoiceKind::Bytes(bc) => {
                if bc.max_size == 0 {
                    Some(vec![ChoiceValue::Bytes(Vec::new())])
                } else {
                    None
                }
            }
            // Mirror the `BytesChoice` arm: only `max_size == 0` has a sensible
            // enumeration. Any larger size has alpha-way fan-out that makes
            // materialising the list pointless even when it fits in `cap`.
            ChoiceKind::String(sc) => {
                if sc.max_size == 0 {
                    Some(vec![ChoiceValue::String(Vec::new())])
                } else {
                    None
                }
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
            assert!(n > 0, "ChoiceTemplate count must be positive (got 0)");
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

    pub fn sort_key(&self) -> NodeSortKey {
        match (self.kind.as_ref(), &self.value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => {
                let (mag, neg) = ic.sort_key(v);
                NodeSortKey::Scalar(mag, neg)
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => {
                NodeSortKey::Scalar(BigUint::from(u32::from(*v)), false)
            }
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => {
                let (mag, neg) = fc.sort_key(*v);
                NodeSortKey::Scalar(BigUint::from(mag), neg)
            }
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => {
                let (len, bytes) = bc.sort_key(v);
                NodeSortKey::Sequence(len, bytes.into_iter().map(u32::from).collect())
            }
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => {
                let (len, keys) = sc.sort_key(v);
                NodeSortKey::Sequence(len, keys)
            }
            _ => unreachable!("mismatched choice kind and value"),
        }
    }
}

/// Comparable key for ordering choice nodes during shrinking.
///
/// Scalar kinds (integer, boolean, float) compare on a `(magnitude, sign)`
/// pair; sequence kinds (bytes, strings) compare shortlex on `(length,
/// elements)`. Per-element keys are stored as `u32` so the string choice kind
/// (with codepoint-key elements up to `0x10FFFF`) fits the same shape.
/// Variants are never mixed at a given node position; the cross-variant order
/// `Scalar < Sequence` (by derived enum order) is a total-ordering
/// fall-through for the sort key of an entire sequence-of-nodes that contains
/// different shapes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeSortKey {
    Scalar(crate::native::bignum::BigUint, bool),
    Sequence(usize, Vec<u32>),
}

/// Borrowed view of a [`ChoiceNode`]'s sort key.
///
/// `Ord` matches [`NodeSortKey`]'s ordering exactly: `Scalar < Sequence`
/// cross-variant, scalar by `(magnitude, sign)`, sequence variants shortlex on
/// length then per-element keys. The per-element keys for `Bytes` and `String`
/// are resolved lazily during comparison — `String` defers `codepoint_key` to
/// the moment of compare — so no `Vec<u32>` ever gets allocated.
pub enum NodeSortKeyRef<'a> {
    Scalar(crate::native::bignum::BigUint, bool),
    Bytes(&'a [u8]),
    String(&'a StringChoice, &'a [u32]),
}

impl<'a> NodeSortKeyRef<'a> {
    fn category(&self) -> u8 {
        match self {
            NodeSortKeyRef::Scalar(..) => 0,
            NodeSortKeyRef::Bytes(..) | NodeSortKeyRef::String(..) => 1,
        }
    }

    /// Length of the underlying element sequence. Only meaningful for
    /// sequence variants; the only call site (the sequence-vs-sequence
    /// arm of `cmp`) gates on category() before invoking.
    fn seq_len(&self) -> usize {
        match self {
            NodeSortKeyRef::Bytes(b) => b.len(),
            NodeSortKeyRef::String(_, cps) => cps.len(),
            NodeSortKeyRef::Scalar(..) => unreachable!("seq_len on scalar"),
        }
    }

    /// `i`-th per-element key in the sort-order alphabet. `i` must be in
    /// `0..self.seq_len()`. Calling on `Scalar` is unreachable in the
    /// only call sites (sequence-element comparison).
    fn seq_key_at(&self, i: usize) -> u32 {
        match self {
            NodeSortKeyRef::Bytes(b) => b[i] as u32,
            NodeSortKeyRef::String(sc, cps) => sc.codepoint_key(cps[i]),
            NodeSortKeyRef::Scalar(..) => unreachable!("seq_key_at on scalar"),
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
            (NodeSortKeyRef::Scalar(..), _) | (_, NodeSortKeyRef::Scalar(..)) => {
                self.category().cmp(&other.category())
            }
            _ => {
                // Both sides are sequence variants.
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
            _ => unreachable!("mismatched choice kind and value"),
        }
    }
}

/// Shortlex sort key for a sequence of choice nodes, as a borrowed view.
/// Shorter sequences are simpler; among equal lengths, smaller per-element
/// keys win. Comparison is allocation-free: per-element keys are resolved
/// lazily and the first inequality short-circuits.
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
        match self.0.len().cmp(&other.0.len()) {
            Ordering::Equal => {}
            ord => return ord,
        }
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            match a.sort_key_ref().cmp(&b.sort_key_ref()) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        Ordering::Equal
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
    /// The test case ran out of data and should stop executing.
    StopTest,
    /// A caller-supplied schema was semantically invalid (unknown type,
    /// empty character set, unparseable regex, etc.). The string is a
    /// human-readable diagnostic.
    InvalidArgument(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::StopTest => write!(f, "test case should stop executing (StopTest)"),
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
