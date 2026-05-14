// Choice types: the recorded decisions a test case makes.

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

/// The kind of choice made at a particular point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Integer(IntegerChoice),
    Boolean(BooleanChoice),
}

/// The value produced by a choice.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChoiceValue {
    Integer(i128),
    Boolean(bool),
}

impl ChoiceKind {
    /// The simplest value for this choice kind.
    pub fn simplest(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.simplest()),
            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.simplest()),
        }
    }

    /// Largest valid index for [`from_index`].
    ///
    /// Hypothesis: `core.py::ChoiceType.max_index`.
    pub fn max_index(&self) -> crate::native::bignum::BigUint {
        match self {
            ChoiceKind::Integer(ic) => ic.max_index(),
            ChoiceKind::Boolean(bc) => bc.max_index(),
        }
    }

    /// Convert a value to its dense index under this kind's sort order.
    ///
    /// Hypothesis: `core.py::ChoiceType.to_index`.
    pub fn to_index(&self, value: &ChoiceValue) -> crate::native::bignum::BigUint {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.to_index(*v),
            (ChoiceKind::Boolean(bc), ChoiceValue::Boolean(v)) => bc.to_index(*v),
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
        }
    }

    /// Whether `value` is a valid draw for this kind.
    pub fn validate(&self, value: &ChoiceValue) -> bool {
        match (self, value) {
            (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => ic.validate(*v),
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(_)) => true,
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
                NodeSortKey(abs, neg)
            }
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => NodeSortKey(u128::from(*v), false),
            _ => unreachable!("mismatched choice kind and value"),
        }
    }
}

/// Comparable key for ordering choice nodes during shrinking: (magnitude, sign).
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
