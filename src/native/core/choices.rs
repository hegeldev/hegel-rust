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

/// The kind of choice made at a particular point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Integer(IntegerChoice),
    Boolean(BooleanChoice),
    Float(FloatChoice),
}

/// The value produced by a choice.
#[derive(Clone, Debug)]
pub enum ChoiceValue {
    Integer(i128),
    Boolean(bool),
    Float(f64),
}

impl PartialEq for ChoiceValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => a == b,
            (ChoiceValue::Boolean(a), ChoiceValue::Boolean(b)) => a == b,
            // Bitwise equality so NaN == NaN for replay/punning logic.
            (ChoiceValue::Float(a), ChoiceValue::Float(b)) => a.to_bits() == b.to_bits(),
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
                NodeSortKey(abs, neg)
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => NodeSortKey(u128::from(*v), false),
            (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => {
                let (idx, neg) = fc.sort_index(*v);
                NodeSortKey(idx as u128, neg)
            }
            _ => unreachable!("mismatched choice kind and value"),
        }
    }
}

/// Comparable key for ordering choice nodes during shrinking.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeSortKey(pub u128, pub bool);

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
