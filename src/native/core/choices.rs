// Choice types: the recorded decisions a test case makes.

use crate::native::floats::sign_aware_lte;

/// An integer choice with bounded range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerChoice {
    pub min_value: i128,
    pub max_value: i128,
}

impl IntegerChoice {
    /// The simplest (most "shrunk") value: 0 if in range,
    /// otherwise the endpoint closest to 0.
    pub fn simplest(&self) -> i128 {
        if self.min_value <= 0 && 0 <= self.max_value {
            0
        } else if self.min_value.unsigned_abs() <= self.max_value.unsigned_abs() {
            self.min_value
        } else {
            self.max_value
        }
    }

    /// The second simplest value, used for punning when types change.
    pub fn unit(&self) -> i128 {
        let s = self.simplest();
        if self.validate(s + 1) {
            s + 1
        } else if self.validate(s - 1) {
            s - 1
        } else {
            s
        }
    }

    pub fn validate(&self, value: i128) -> bool {
        self.min_value <= value && value <= self.max_value
    }

    /// Sort key for shrinking: smaller absolute values are simpler,
    /// positive values are simpler than negative at the same magnitude.
    pub fn sort_key(&self, value: i128) -> (u128, bool) {
        (value.unsigned_abs(), value < 0)
    }

    /// pbtkit: `core.py::IntegerChoice.max_index`.
    // nocov start
    #[allow(dead_code)]
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        // max_value - min_value can exceed i128 positive range (e.g. full
        // i128 span). Two's-complement wrapping_sub reinterpreted as u128
        // recovers the correct non-negative distance.
        let diff = (self.max_value as u128).wrapping_sub(self.min_value as u128);
        BigUint::from(diff)
    }
    // nocov end

    /// pbtkit: `core.py::IntegerChoice.to_index`.
    #[allow(dead_code)]
    pub fn to_index(&self, value: i128) -> crate::native::bignum::BigUint {
        use crate::native::bignum::{BigUint, Zero};
        let s = self.simplest();
        if value == s {
            return BigUint::zero();
        }
        let above = BigUint::from((self.max_value as u128).wrapping_sub(s as u128));
        let below = BigUint::from((s as u128).wrapping_sub(self.min_value as u128));
        let d_abs_u = if value > s {
            (value as u128).wrapping_sub(s as u128)
        } else {
            (s as u128).wrapping_sub(value as u128)
        };
        let d_abs = BigUint::from(d_abs_u);
        let d_minus_one = &d_abs - BigUint::from(1u32);
        let mut count = std::cmp::min(&d_minus_one, &above).clone()
            + std::cmp::min(&d_minus_one, &below).clone();
        if value > s {
            return count + BigUint::from(1u32);
        }
        if d_abs <= above {
            count += BigUint::from(1u32);
        }
        count + BigUint::from(1u32)
    }

    /// pbtkit: `core.py::IntegerChoice.from_index`.
    #[allow(dead_code, clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<i128> {
        use crate::native::bignum::{BigUint, Zero};
        let s = self.simplest();
        if index.is_zero() {
            return Some(s);
        }
        let above_u = (self.max_value as u128).wrapping_sub(s as u128);
        let below_u = (s as u128).wrapping_sub(self.min_value as u128);
        let above = BigUint::from(above_u);
        let below = BigUint::from(below_u);
        let mut lo = BigUint::from(1u32);
        let mut hi = &above + &below;
        let two = BigUint::from(2u32);
        while lo < hi {
            let mid = (&lo + &hi) / &two;
            let total = std::cmp::min(&mid, &above).clone() + std::cmp::min(&mid, &below).clone();
            if total >= index {
                hi = mid;
            } else {
                lo = mid + BigUint::from(1u32);
            }
        }
        let d = lo;
        let total_at_d = std::cmp::min(&d, &above).clone() + std::cmp::min(&d, &below).clone();
        if total_at_d < index {
            return None;
        }
        let d_minus_one = &d - BigUint::from(1u32);
        let before = std::cmp::min(&d_minus_one, &above).clone()
            + std::cmp::min(&d_minus_one, &below).clone();
        let pos_in_d = &index - before;
        let d_u: u128 = (&d)
            .try_into()
            .expect("d fits in u128 (range is <= u128::MAX)");
        if pos_in_d == BigUint::from(1u32) && d <= above {
            return Some((s as u128).wrapping_add(d_u) as i128);
        }
        debug_assert!(d <= below);
        Some((s as u128).wrapping_sub(d_u) as i128)
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

    /// Sort key matching the ordering used for shrinking: `false` < `true`.
    #[allow(dead_code)]
    pub fn sort_key(&self, value: bool) -> u32 {
        u32::from(value)
    }

    /// pbtkit: `core.py::BooleanChoice.max_index`.
    // nocov start
    #[allow(dead_code)]
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(1u32)
    }
    // nocov end

    /// pbtkit: `core.py::BooleanChoice.to_index`.
    #[allow(dead_code)]
    pub fn to_index(&self, value: bool) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(u32::from(value))
    }

    /// pbtkit: `core.py::BooleanChoice.from_index`.
    #[allow(dead_code, clippy::wrong_self_convention)]
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

/// A float choice with bounded range.
///
/// Port of pbtkit's FloatChoice.
#[derive(Clone, Debug)]
pub struct FloatChoice {
    pub min_value: f64,
    pub max_value: f64,
    pub allow_nan: bool,
    pub allow_infinity: bool,
}

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
    // nocov start
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

        // Check the smallest valid positive integer in range.
        if self.max_value >= 0.0 {
            let lo_int = self.min_value.max(0.0).ceil() as i64;
            try_candidate!(lo_int as f64);
        }
        // Also the largest valid negative integer (closest to zero).
        if self.min_value <= 0.0 {
            let hi_int = self.max_value.min(0.0).floor() as i64;
            try_candidate!(hi_int as f64);
        }

        // Check simple non-integer fractions at each exponent level.
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
    // nocov end

    /// Second-simplest valid float (for type punning during replay).
    // nocov start
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
    // nocov end

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
    pub fn sort_index(&self, v: f64) -> (u64, bool) {
        use super::float_index::float_to_index;
        if v.is_nan() {
            return (u64::MAX, false);
        }
        let is_neg = v.is_sign_negative();
        let mag = if is_neg { -v } else { v };
        (float_to_index(mag), is_neg)
    }

    /// Alias for [`sort_index`]: matches pbtkit's `FloatChoice.sort_key` name
    /// for the index-invariant tests.
    #[allow(dead_code)]
    pub fn sort_key(&self, v: f64) -> (u64, bool) {
        self.sort_index(v)
    }

    /// pbtkit: `floats.py::FloatChoice.max_index`. Largest valid index for
    /// [`from_index`]. Indexes the full finite range (both signs) followed
    /// by `+inf`, `-inf`, then all NaN payloads.
    // nocov start
    #[allow(dead_code)]
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        // 2^52 NaN payloads (one bit forced to 1) × 2 signs = 2^53 NaN slots.
        max_finite_global_rank() + BigUint::from(2u32) + BigUint::from(1u64 << 53)
    }
    // nocov end

    /// pbtkit: `floats.py::FloatChoice.to_index`.
    ///
    /// Implementation note: pbtkit defines `to_index` as
    /// `_float_to_index(value) - _float_to_index(simplest)` over its own
    /// raw-index ordering, which puts `65672.5` before `65673.0`. Native's
    /// `simplest` uses the Hypothesis lex ordering (which prefers integer
    /// `65673.0` for the range `[65672.5, 65673.0]`), so the pbtkit recipe
    /// would underflow whenever `value` is below `simplest` in raw-index
    /// terms. Instead, we build the dense ordering directly from native's
    /// `sort_key` = `(float_to_index(|v|), is_neg)`.
    #[allow(dead_code)]
    pub fn to_index(&self, value: f64) -> crate::native::bignum::BigUint {
        float_global_rank(value) - float_global_rank(self.simplest())
    }

    /// pbtkit: `floats.py::FloatChoice.from_index`.
    #[allow(dead_code, clippy::wrong_self_convention)]
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

/// Dense rank of `v` under native's `sort_key` ordering: finite floats
/// indexed by `(float_to_index(|v|), is_neg)`, then `+inf`, `-inf`, then NaN
/// payloads.
// nocov start
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
// nocov end

/// Inverse of [`float_global_rank`]. Returns `None` if `rank` falls in the
/// NaN-payload region for a sign+offset combination that would not actually
/// be a NaN bit pattern.
// nocov start
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
        // Force bit 51 to 1 so the mantissa is non-zero (matches pbtkit).
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
// nocov end

/// Largest dense rank used by any finite float. The maximum lex index over
/// any finite float is `(1<<63) | (2046<<52) | mantissa_max` — bit 63 set
/// (non-simple), encoded exponent 2046 (the last non-NaN/inf slot), and
/// every fractional bit set. (Note: this is *not* `float_to_index(f64::MAX)`,
/// because Hypothesis's lex ordering ranks fractions like `0.5` — encoded
/// exponent 1024 — *higher* than huge integers like `f64::MAX`, which has
/// encoded exponent 1023.) The `+1` is the negative-sign slot for that lex
/// index, since `float_global_rank` packs sign into the low bit.
fn max_finite_global_rank() -> crate::native::bignum::BigUint {
    use crate::native::bignum::BigUint;
    let max_finite_lex = (1u64 << 63) | (2046u64 << 52) | ((1u64 << 52) - 1);
    BigUint::from(max_finite_lex) * BigUint::from(2u32) + BigUint::from(1u32)
}

/// Map a codepoint to its sort-key position.
///
/// Port of pbtkit's `_codepoint_key`. Reorders the low 128 codepoints so
/// that '0' (48) maps to key 0 (simplest), '1' to 1, ..., '/' to 47, and
/// anything above 127 keeps its natural position. This makes digits
/// sort-simpler than letters, which in turn are simpler than punctuation.
pub fn codepoint_key(c: u32) -> u32 {
    if c < 128 {
        ((c as i32 - b'0' as i32).rem_euclid(128)) as u32
    } else {
        c
    }
}

/// Inverse of [`codepoint_key`].
pub fn key_to_codepoint(k: u32) -> u32 {
    if k < 128 { (k + b'0' as u32) % 128 } else { k }
}

/// A bytes choice with bounded length.
///
/// Port of pbtkit's BytesChoice. Ordered by shortlex (shorter is simpler,
/// then lexicographic on the bytes themselves).
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
    ///
    /// If `min_size > 0`: the simplest except the last byte is 1.
    /// Else if `max_size > 0`: a single 0x01 byte.
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

    /// Shortlex sort key: (length, bytes).
    pub fn sort_key(&self, value: &[u8]) -> (usize, Vec<u8>) {
        (value.len(), value.to_vec())
    }

    /// pbtkit: `bytes.py::BytesChoice.max_index`.
    #[allow(dead_code)]
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        self.to_index(&vec![0xffu8; self.max_size])
    }

    /// pbtkit: `bytes.py::BytesChoice.to_index`.
    #[allow(dead_code)]
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

    /// pbtkit: `bytes.py::BytesChoice.from_index`.
    #[allow(dead_code, clippy::wrong_self_convention)]
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

/// A string choice with bounded length and codepoint range.
///
/// Port of pbtkit's StringChoice. Values are sequences of raw Unicode
/// codepoints (`Vec<u32>`) in `0..=0x10FFFF`; the no-surrogate filter is
/// applied at the user-facing boundary where the engine hands a `String`
/// back, not in the core representation. Ordered by shortlex over
/// `codepoint_key`-remapped codepoints (so '0' is the simplest codepoint,
/// then '1', and so on).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringChoice {
    pub min_codepoint: u32,
    pub max_codepoint: u32,
    pub min_size: usize,
    pub max_size: usize,
}

impl StringChoice {
    /// Return the simplest codepoint in [min_codepoint, min(max_codepoint, 127)]
    /// under [`codepoint_key`] ordering, or the smallest non-surrogate codepoint
    /// at or above `min_codepoint` (clamped to `max_codepoint`) if the range has
    /// no ASCII overlap. Guaranteed to be a valid non-surrogate scalar value.
    pub(crate) fn simplest_codepoint(&self) -> u32 {
        let upper = self.max_codepoint.min(127);
        if self.min_codepoint > upper {
            // No ASCII in range; pick the smallest non-surrogate codepoint.
            if self.min_codepoint < 0xD800 || self.min_codepoint > 0xDFFF {
                return self.min_codepoint;
            }
            // min_codepoint falls inside the surrogate block; step past it.
            return 0xE000u32.min(self.max_codepoint); // nocov
        }
        let mut best = self.min_codepoint;
        let mut best_key = codepoint_key(best);
        for cp in (self.min_codepoint + 1)..=upper {
            let k = codepoint_key(cp);
            if k < best_key {
                best = cp;
                best_key = k;
            }
        }
        best
    }

    /// The simplest sequence of codepoints of length `min_size`, built from
    /// repeated [`simplest_codepoint`].
    pub fn simplest(&self) -> Vec<u32> {
        vec![self.simplest_codepoint(); self.min_size]
    }

    /// Second-simplest codepoint sequence, used for type-punning during replay.
    pub fn unit(&self) -> Vec<u32> {
        // Pick the "second-simplest" codepoint under codepoint_key ordering,
        // falling back to the simplest codepoint if that lies outside the range
        // or inside the surrogate block.
        let candidate = key_to_codepoint(1);
        let second_cp = if self.min_codepoint <= candidate
            && candidate <= self.max_codepoint
            && !(0xD800..=0xDFFF).contains(&candidate)
        {
            candidate
        } else {
            self.simplest_codepoint()
        };

        // Single-codepoint alphabet → lengthen if possible, else simplest.
        if second_cp == self.simplest_codepoint() {
            if self.min_size < self.max_size {
                return vec![self.simplest_codepoint(); self.min_size + 1];
            }
            return self.simplest();
        }

        if self.min_size > 0 {
            let mut v = self.simplest();
            *v.last_mut().unwrap() = second_cp;
            return v;
        }
        if self.max_size > 0 {
            return vec![second_cp];
        }
        self.simplest()
    }

    pub fn validate(&self, value: &[u32]) -> bool {
        if !(self.min_size <= value.len() && value.len() <= self.max_size) {
            return false;
        }
        value.iter().all(|&cp| {
            self.min_codepoint <= cp && cp <= self.max_codepoint && !(0xD800..=0xDFFF).contains(&cp)
        })
    }

    /// Shortlex sort key: `(length, Vec<codepoint_key>)`.
    pub fn sort_key(&self, value: &[u32]) -> (usize, Vec<u32>) {
        let keys: Vec<u32> = value.iter().map(|&cp| codepoint_key(cp)).collect();
        (keys.len(), keys)
    }

    /// Count of valid codepoints in range, excluding surrogates.
    /// pbtkit: `text.py::StringChoice._alpha_size`.
    #[allow(dead_code)]
    pub fn alpha_size(&self) -> u64 {
        let total = u64::from(self.max_codepoint - self.min_codepoint + 1);
        let sur_lo = self.min_codepoint.max(0xD800);
        let sur_hi = self.max_codepoint.min(0xDFFF);
        if sur_lo <= sur_hi {
            total - u64::from(sur_hi - sur_lo + 1)
        } else {
            total
        }
    }

    /// Largest valid index for [`from_index`].
    ///
    /// pbtkit: `text.py::StringChoice.max_index`. Returned as arbitrary
    /// precision because realistic alphabet/length combinations
    /// (`256^{max_size}`-style) blow past `u128` — see
    /// `src/native/bignum.rs`.
    #[allow(dead_code)]
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::{BigUint, Zero};
        let alpha = BigUint::from(self.alpha_size());
        let mut total = BigUint::zero();
        for length in self.min_size..=self.max_size {
            total += alpha.pow(length as u32);
        }
        total - BigUint::from(1u32)
    }

    /// Rank of a codepoint within valid (non-surrogate) codepoints in range,
    /// sorted by [`codepoint_key`].
    ///
    /// pbtkit: `text.py::StringChoice._codepoint_rank`.
    #[allow(dead_code)]
    pub fn codepoint_rank(&self, codepoint: u32) -> u64 {
        let key = codepoint_key(codepoint);
        let mut count: u64 = 0;
        // Low codepoints (< 128) are reordered by codepoint_key, so count by scan.
        let lo = self.min_codepoint;
        let hi = self.max_codepoint.min(127);
        if lo <= hi {
            for c in lo..=hi {
                if codepoint_key(c) < key {
                    count += 1;
                }
            }
        }
        // High codepoints (>= 128) preserve natural order; count those < key.
        let hi_lo = self.min_codepoint.max(128);
        let hi_hi = self.max_codepoint;
        if hi_lo <= hi_hi && key > hi_lo {
            let end = (key - 1).min(hi_hi);
            let mut n = u64::from(end - hi_lo + 1);
            // Subtract surrogates that fall within [hi_lo, end].
            let sur_lo = hi_lo.max(0xD800);
            let sur_hi = end.min(0xDFFF);
            if sur_lo <= sur_hi {
                n -= u64::from(sur_hi - sur_lo + 1);
            }
            count += n;
        }
        count
    }

    /// Codepoint at the given rank among valid (non-surrogate) codepoints in
    /// range, sorted by [`codepoint_key`].
    ///
    /// pbtkit: `text.py::StringChoice._codepoint_at_rank`. Panics if `rank`
    /// exceeds [`alpha_size`].
    #[allow(dead_code)]
    pub fn codepoint_at_rank(&self, rank: u64) -> u32 {
        let lo = self.min_codepoint;
        let hi = self.max_codepoint.min(127);
        let mut low_sorted: Vec<u32> = if lo <= hi {
            (lo..=hi).collect()
        } else {
            Vec::new()
        };
        low_sorted.sort_by_key(|&c| codepoint_key(c));
        if rank < low_sorted.len() as u64 {
            return low_sorted[rank as usize];
        }
        let rank = rank - low_sorted.len() as u64;
        let hi_lo = self.min_codepoint.max(128);
        let mut c = hi_lo + rank as u32;
        if c >= 0xD800 {
            c += 0xDFFF - 0xD800 + 1;
        }
        assert!(
            c <= self.max_codepoint,
            "rank out of range for StringChoice"
        );
        c
    }

    /// Shortlex index of `value` under this choice's mapped-codepoint alphabet.
    ///
    /// pbtkit: `text.py::StringChoice.to_index`. Inverse of [`from_index`].
    /// Arbitrary precision — see [`max_index`] for the bound rationale.
    #[allow(dead_code)]
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
    /// exceeds the total bucket size (i.e. > [`max_index`]).
    ///
    /// pbtkit: `text.py::StringChoice.from_index`. Inverse of [`to_index`].
    #[allow(dead_code, clippy::wrong_self_convention)]
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
    Integer(i128),
    Boolean(bool),
    Float(f64),
    Bytes(Vec<u8>),
    /// A sequence of Unicode codepoints (raw `u32`s in `0..=0x10FFFF`). The
    /// engine reasons internally about any codepoint, including surrogates;
    /// conversion to a `char`/`String` (with the surrogate filter applied)
    /// happens at the user-facing boundary.
    String(Vec<u32>),
}

impl PartialEq for ChoiceValue {
    // nocov start
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => a == b,
            (ChoiceValue::Boolean(a), ChoiceValue::Boolean(b)) => a == b,
            // Bitwise equality so NaN == NaN for replay/punning logic.
            (ChoiceValue::Float(a), ChoiceValue::Float(b)) => a.to_bits() == b.to_bits(),
            (ChoiceValue::Bytes(a), ChoiceValue::Bytes(b)) => a == b,
            (ChoiceValue::String(a), ChoiceValue::String(b)) => a == b,
            _ => false,
        }
    }
    // nocov end
}

impl Eq for ChoiceValue {}

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

    /// Largest valid index for [`from_index`].
    ///
    /// pbtkit: `core.py::ChoiceType.max_index`.
    #[allow(dead_code)]
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
    ///
    /// pbtkit: `core.py::ChoiceType.to_index`.
    #[allow(dead_code)]
    pub fn to_index(&self, value: &ChoiceValue) -> crate::native::bignum::BigUint {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.to_index(*v),
            (ChoiceKind::Boolean(bc), ChoiceValue::Boolean(v)) => bc.to_index(*v),
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.to_index(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.to_index(v),
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => sc.to_index(v),
            _ => panic!("ChoiceKind::to_index: kind/value mismatch"),
        }
    }

    /// Inverse of [`to_index`]. Returns `None` when the index is out of range
    /// (or falls on a bounded-range gap for float kinds).
    ///
    /// pbtkit: `core.py::ChoiceType.from_index`.
    #[allow(dead_code, clippy::wrong_self_convention)]
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
    #[allow(dead_code)]
    pub fn validate(&self, value: &ChoiceValue) -> bool {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.validate(*v),
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(_)) => true,
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.validate(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.validate(v),
            (ChoiceKind::String(sc), ChoiceValue::String(v)) => sc.validate(v),
            _ => false,
        }
    }
}

/// A single recorded choice in a test case.
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceNode {
    pub kind: ChoiceKind,
    pub value: ChoiceValue,
    pub was_forced: bool,
}

impl ChoiceNode {
    pub fn with_value(&self, value: ChoiceValue) -> ChoiceNode {
        ChoiceNode {
            kind: self.kind.clone(),
            value,
            was_forced: self.was_forced,
        }
    }

    /// Whether this node is at its simplest value and cannot be simplified
    /// further in isolation.
    ///
    /// Port of Hypothesis's `ChoiceNode.trivial` from
    /// `hypothesis.internal.conjecture.choice`. The float path is
    /// deliberately conservative (sound but not complete): it matches
    /// upstream's "value equals 0 clamped into the interval's integer span"
    /// test, not native's richer `FloatChoice::simplest`. Some values that
    /// are actually trivial in shrinking will be reported as non-trivial
    /// here.
    #[allow(dead_code)]
    pub fn trivial(&self) -> bool {
        if self.was_forced {
            return true;
        }
        if let (ChoiceKind::Float(fc), ChoiceValue::Float(v)) = (&self.kind, &self.value) {
            let min_value = fc.min_value;
            let max_value = fc.max_value;
            let mut shrink_towards = 0.0_f64;

            if min_value == f64::NEG_INFINITY && max_value == f64::INFINITY {
                return v.to_bits() == shrink_towards.to_bits();
            }

            if !min_value.is_infinite()
                && !max_value.is_infinite()
                && min_value.ceil() <= max_value.floor()
            {
                shrink_towards = min_value.ceil().max(shrink_towards);
                shrink_towards = max_value.floor().min(shrink_towards);
                return v.to_bits() == shrink_towards.to_bits();
            }

            return false;
        }
        self.kind.simplest() == self.value
    }

    pub fn sort_key(&self) -> NodeSortKey {
        match (&self.kind, &self.value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => {
                let (abs, neg) = ic.sort_key(*v);
                NodeSortKey::Scalar(abs, neg)
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => {
                NodeSortKey::Scalar(u128::from(*v), false)
            }
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => {
                let (idx, neg) = fc.sort_index(*v);
                NodeSortKey::Scalar(idx as u128, neg)
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
/// Scalar kinds (integer, boolean, float) use a fixed (magnitude, sign) pair;
/// sequence kinds (bytes, string) use a shortlex (length, per-element key)
/// pair, where for bytes the per-element key is the byte value and for
/// strings it is [`codepoint_key`] applied to the character.
/// Comparison across variants is well-defined by enum discriminant order but
/// never happens in practice — a given node position always has one kind.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeSortKey {
    Scalar(u128, bool),
    Sequence(usize, Vec<u32>),
}

/// Shortlex sort key for a sequence of choice nodes.
/// Shorter sequences are simpler; among equal lengths, smaller values win.
pub fn sort_key(nodes: &[ChoiceNode]) -> (usize, Vec<NodeSortKey>) {
    (nodes.len(), nodes.iter().map(|n| n.sort_key()).collect())
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

/// Raised when a test case should stop executing.
pub struct StopTest;

#[cfg(test)]
#[path = "../../../tests/embedded/native/choices_tests.rs"]
mod tests;
