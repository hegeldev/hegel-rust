// Choice types: the recorded decisions a test case makes.

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
        self.min_value <= v && v <= self.max_value
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
}

/// A string choice with bounded length and codepoint range.
///
/// Port of pbtkit's StringChoice. Values are sequences of raw Unicode
/// codepoints (`Vec<u32>`) in `0..=0x10FFFF`; the no-surrogate filter is
/// applied at the user-facing boundary where the engine hands a `String`
/// back, not in the core representation. Ordered by shortlex over
/// [`codepoint_key`]-remapped codepoints (so '0' is the simplest codepoint,
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
            return 0xE000u32.min(self.max_codepoint);
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
    /// pbtkit: `text.py::StringChoice.max_index`. Panics on overflow when the
    /// alphabet/length combination exceeds u128 range.
    #[allow(dead_code)]
    pub fn max_index(&self) -> u128 {
        let alpha = u128::from(self.alpha_size());
        let mut total: u128 = 0;
        for length in self.min_size..=self.max_size {
            let term = alpha
                .checked_pow(length as u32)
                .expect("StringChoice::max_index overflow");
            total = total
                .checked_add(term)
                .expect("StringChoice::max_index overflow");
        }
        total - 1
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
    #[allow(dead_code)]
    pub fn to_index(&self, value: &[u32]) -> u128 {
        let alpha = u128::from(self.alpha_size());
        let mut offset: u128 = 0;
        for length in self.min_size..value.len() {
            offset += alpha
                .checked_pow(length as u32)
                .expect("StringChoice::to_index overflow");
        }
        let mut position: u128 = 0;
        for &cp in value {
            position = position * alpha + u128::from(self.codepoint_rank(cp));
        }
        offset + position
    }

    /// Codepoint sequence at the given shortlex index, or `None` if `index`
    /// exceeds the total bucket size (i.e. > [`max_index`]).
    ///
    /// pbtkit: `text.py::StringChoice.from_index`. Inverse of [`to_index`].
    #[allow(dead_code, clippy::wrong_self_convention)]
    pub fn from_index(&self, index: u128) -> Option<Vec<u32>> {
        let alpha = u128::from(self.alpha_size());
        assert!(alpha > 0, "StringChoice::from_index: empty alphabet");
        let mut remaining = index;
        for length in self.min_size..=self.max_size {
            let bucket_size = alpha
                .checked_pow(length as u32)
                .expect("StringChoice::from_index overflow");
            if remaining < bucket_size {
                let mut cps: Vec<u32> = Vec::with_capacity(length);
                for _ in 0..length {
                    let r = (remaining % alpha) as u64;
                    cps.push(self.codepoint_at_rank(r));
                    remaining /= alpha;
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
}

/// A single recorded choice in a test case.
#[derive(Clone, Debug)]
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
