// Choice types: the recorded decisions a test case makes.

use crate::native::floats::sign_aware_lte;

/// An integer choice with bounded range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerChoice {
    pub min_value: i128,
    pub max_value: i128,
    /// The "preferred" value the shrinker aims at — analogous to
    /// upstream's `node.constraints["shrink_towards"]` (default 0).  All
    /// of [`Self::simplest`], [`Self::unit`], and [`Self::sort_key`]
    /// are anchored at `shrink_towards.clamp(min_value, max_value)`, so
    /// integer-shrinking passes converge on this value rather than on 0.
    pub shrink_towards: i128,
}

impl IntegerChoice {
    /// The shrink-target value clamped into the kind's range.  All shrink
    /// helpers compare against this rather than the raw `shrink_towards`
    /// to keep behaviour well-defined when callers pass an out-of-range
    /// hint.
    pub(crate) fn clamped_shrink_towards(&self) -> i128 {
        self.shrink_towards.clamp(self.min_value, self.max_value)
    }

    /// The simplest (most "shrunk") value: `shrink_towards` clamped to
    /// the kind's range.  With the default `shrink_towards = 0` this is
    /// `0` when in range and the closest endpoint otherwise — matching
    /// pre-A21 behaviour.
    pub fn simplest(&self) -> i128 {
        self.clamped_shrink_towards()
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

    /// Sort key for shrinking: smaller distance from `shrink_towards`
    /// is simpler, with values below `shrink_towards` ordered after
    /// values above at the same distance (mirrors upstream's
    /// `choice_to_index` semantics for integer kinds with non-zero
    /// `shrink_towards`).  With the default `shrink_towards = 0` this
    /// is `(value.unsigned_abs(), value < 0)` — matching pre-A21
    /// behaviour.
    pub fn sort_key(&self, value: i128) -> (u128, bool) {
        let target = self.clamped_shrink_towards();
        let distance = value.wrapping_sub(target).unsigned_abs();
        (distance, value < target)
    }

    /// Hypothesis: `core.py::IntegerChoice.max_index`.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        // max_value - min_value can exceed i128 positive range (e.g. full
        // i128 span). Two's-complement wrapping_sub reinterpreted as u128
        // recovers the correct non-negative distance.
        let diff = (self.max_value as u128).wrapping_sub(self.min_value as u128);
        BigUint::from(diff)
    }
    /// Hypothesis: `core.py::IntegerChoice.to_index`.
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

    /// Hypothesis: `core.py::IntegerChoice.from_index`.
    #[allow(clippy::wrong_self_convention)]
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

    /// Hypothesis: `core.py::BooleanChoice.max_index`.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(1u32)
    }
    /// Hypothesis: `core.py::BooleanChoice.to_index`.
    pub fn to_index(&self, value: bool) -> crate::native::bignum::BigUint {
        crate::native::bignum::BigUint::from(u32::from(value))
    }

    /// Hypothesis: `core.py::BooleanChoice.from_index`.
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

    /// Hypothesis: `core.py::BytesChoice.max_index`.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        self.to_index(&vec![0xffu8; self.max_size])
    }

    /// Hypothesis: `core.py::BytesChoice.to_index`. Indexes byte sequences in
    /// shortlex order over `[min_size, max_size]`: all length-`min_size`
    /// sequences first, then length `min_size + 1`, and so on; within each
    /// length, lexicographic on the bytes.
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

    /// Hypothesis: `core.py::FloatChoice.max_index`. Largest valid index for
    /// [`from_index`]. Indexes the full finite range (both signs) followed
    /// by `+inf`, `-inf`, then all NaN payloads.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        // 2^52 NaN payloads (one bit forced to 1) × 2 signs = 2^53 NaN slots.
        max_finite_global_rank() + BigUint::from(2u32) + BigUint::from(1u64 << 53)
    }

    /// Hypothesis: `core.py::FloatChoice.to_index`.
    ///
    /// Implementation note: a direct port of Hypothesis's
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

    /// Hypothesis: `core.py::FloatChoice.from_index`.
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
        // Force bit 51 to 1 so the mantissa is non-zero (matches Hypothesis).
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
}

/// The value produced by a choice.
#[derive(Clone, Debug)]
pub enum ChoiceValue {
    Integer(i128),
    Boolean(bool),
    Float(f64),
    Bytes(Vec<u8>),
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
        }
    }
}

impl ChoiceKind {
    /// The simplest value for this choice kind.
    pub fn simplest(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.simplest()),
            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.simplest()),
            ChoiceKind::Float(fc) => ChoiceValue::Float(fc.simplest()),
            ChoiceKind::Bytes(bc) => ChoiceValue::Bytes(bc.simplest()),
        }
    }

    /// Largest valid index for [`from_index`].
    ///
    /// Hypothesis: `core.py::ChoiceType.max_index`.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        match self {
            ChoiceKind::Integer(ic) => ic.max_index(),
            ChoiceKind::Boolean(bc) => bc.max_index(),
            ChoiceKind::Float(fc) => fc.max_index(),
            ChoiceKind::Bytes(bc) => bc.max_index(),
        }
    }

    /// Convert a value to its dense index under this kind's sort order.
    ///
    /// Hypothesis: `core.py::ChoiceType.to_index`.
    pub fn to_index(&self, value: &ChoiceValue) -> crate::native::bignum::BigUint {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.to_index(*v),
            (ChoiceKind::Boolean(bc), ChoiceValue::Boolean(v)) => bc.to_index(*v),
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.to_index(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.to_index(v),
            _ => panic!("ChoiceKind::to_index: kind/value mismatch"),
        }
    }

    /// Inverse of [`to_index`]. Returns `None` when the index is out of range.
    ///
    /// Hypothesis: `core.py::ChoiceType.from_index`.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_index(&self, index: crate::native::bignum::BigUint) -> Option<ChoiceValue> {
        match self {
            ChoiceKind::Integer(ic) => ic.from_index(index).map(ChoiceValue::Integer),
            ChoiceKind::Boolean(bc) => bc.from_index(index).map(ChoiceValue::Boolean),
            ChoiceKind::Float(fc) => fc.from_index(index).map(ChoiceValue::Float),
            ChoiceKind::Bytes(bc) => bc.from_index(index).map(ChoiceValue::Bytes),
        }
    }

    /// Whether `value` is a valid draw for this kind.
    pub fn validate(&self, value: &ChoiceValue) -> bool {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.validate(*v),
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(_)) => true,
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => fc.validate(*v),
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => bc.validate(v),
            _ => false,
        }
    }

    /// Cardinality of this kind's choice space.
    /// Port of upstream's `compute_max_children`.
    pub fn max_children(&self) -> crate::native::bignum::BigUint {
        use crate::native::bignum::BigUint;
        match self {
            ChoiceKind::Integer(ic) => {
                let diff = (ic.max_value as u128).wrapping_sub(ic.min_value as u128);
                BigUint::from(diff) + BigUint::from(1u32)
            }
            ChoiceKind::Boolean(_) => BigUint::from(2u32),
            ChoiceKind::Float(fc) => fc.max_index() + BigUint::from(1u32),
            ChoiceKind::Bytes(bc) => bc.max_index() + BigUint::from(1u32),
        }
    }

    /// Random value sampled from this kind's domain (with kind-appropriate bias).
    pub fn random_value(&self, rng: &mut rand::rngs::SmallRng) -> ChoiceValue {
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
        }
    }

    /// Every possible value of this kind, if the total count fits under `cap`.
    pub fn enumerate(&self, cap: u64) -> Option<Vec<ChoiceValue>> {
        use crate::native::bignum::BigUint;
        let max_c = self.max_children();
        if max_c > BigUint::from(cap) {
            return None;
        }
        match self {
            ChoiceKind::Integer(ic) => {
                let mut v = Vec::new();
                let mut n = ic.min_value;
                loop {
                    v.push(ChoiceValue::Integer(n));
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
                let (mag, neg) = fc.sort_key(*v);
                // `u64` widens losslessly into the `u128` magnitude slot used
                // for integer sort keys: float and integer choices end up in a
                // single totally-ordered space without losing precision.
                NodeSortKey::Scalar(u128::from(mag), neg)
            }
            (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => {
                let (len, bytes) = bc.sort_key(v);
                NodeSortKey::Sequence(len, bytes.into_iter().map(u32::from).collect())
            }
            _ => unreachable!("mismatched choice kind and value"),
        }
    }
}

/// Comparable key for ordering choice nodes during shrinking.
///
/// Scalar kinds (integer, boolean, float) compare on a fixed `(magnitude,
/// sign)` pair; sequence kinds (bytes) compare shortlex on `(length,
/// elements)`. Per-element keys are stored as `u32` so a future string
/// choice kind (with codepoint-key elements up to `0x10FFFF`) can join the
/// same shape without changing this type. Variants are never mixed at a
/// given node position; `Scalar < Sequence` by derived enum order is a
/// total-ordering fall-through for the sort key of an entire
/// sequence-of-nodes that contains both shapes.
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
