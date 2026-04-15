// Core types for the native pbtkit-style test engine.
//
// This is a Rust port of the core concepts from pbtkit
// (https://github.com/DRMacIver/pbtkit), specifically core.py.
// It implements choice-based test case generation with integrated shrinking.

use std::collections::HashMap;

use rand::RngExt;
use rand::rngs::SmallRng;

/// Maximum number of choices a single test case can make.
/// Prevents unbounded test case growth.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum iterations of the outer shrink loop.
pub const MAX_SHRINK_ITERATIONS: usize = 500;

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

/// A boolean choice with a probability parameter.
#[derive(Clone, Debug)]
pub struct BooleanChoice {
    pub p: f64,
}

impl BooleanChoice {
    pub fn simplest(&self) -> bool {
        false
    }

    pub fn unit(&self) -> bool {
        true
    }
}

/// The kind of choice made at a particular point.
#[derive(Clone, Debug)]
pub enum ChoiceKind {
    Integer(IntegerChoice),
    Boolean(BooleanChoice),
}

/// The value produced by a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// The second simplest value, used for type punning during replay.
    pub fn unit(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.unit()),
            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.unit()),
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
            (ChoiceKind::Boolean(_), ChoiceValue::Boolean(v)) => {
                NodeSortKey(u128::from(*v), false)
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

/// State for a variable-length collection (port of pbtkit's `many` class).
///
/// Tracks count, rejections, and continuation probability so that
/// `new_collection`/`collection_more`/`collection_reject` protocol commands
/// can be handled statelessly from the choice sequence.
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
        let max_f = max_size.map_or(f64::INFINITY, |n| n as f64);
        let min_f = min_size as f64;
        let average = f64::min(
            f64::max(min_f * 2.0, min_f + 5.0),
            0.5 * (min_f + max_f),
        );
        let desired_extra = average - min_f;
        let max_extra = max_f - min_f;

        let p_continue = if desired_extra >= max_extra {
            0.99
        } else if max_f.is_infinite() {
            1.0 - 1.0 / (1.0 + desired_extra)
        } else {
            1.0 - 1.0 / (2.0 + desired_extra)
        };

        ManyState {
            min_size,
            max_size: max_f,
            p_continue,
            count: 0,
            rejections: 0,
            force_stop: false,
        }
    }
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
    /// Active collection states keyed by collection ID.
    pub collections: HashMap<i64, ManyState>,
    next_collection_id: i64,
}

impl NativeTestCase {
    /// Create a test case for random generation.
    pub fn new_random(rng: SmallRng) -> Self {
        NativeTestCase {
            prefix: Vec::new(),
            prefix_nodes: None,
            rng: Some(rng),
            max_size: BUFFER_SIZE,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
        }
    }

    /// Create a test case that replays a specific choice sequence.
    pub fn for_choices(choices: &[ChoiceValue], prefix_nodes: Option<&[ChoiceNode]>) -> Self {
        NativeTestCase {
            prefix: choices.to_vec(),
            prefix_nodes: prefix_nodes.map(|n| n.to_vec()),
            rng: None,
            max_size: choices.len(),
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
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
    pub fn draw_integer(&mut self, min_value: i128, max_value: i128) -> Result<i128, StopTest> {
        assert!(
            min_value <= max_value,
            "Invalid range [{min_value}, {max_value}]"
        );

        let kind = IntegerChoice {
            min_value,
            max_value,
        };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Integer(kind.clone()),
            || ChoiceValue::Integer(kind.simplest()),
            || ChoiceValue::Integer(kind.unit()),
            |v| matches!(v, ChoiceValue::Integer(n) if kind.validate(*n)),
            |rng| {
                if min_value == max_value {
                    ChoiceValue::Integer(min_value)
                } else {
                    ChoiceValue::Integer(rng.random_range(min_value..=max_value))
                }
            },
        )?;

        let ChoiceValue::Integer(v) = value else {
            unreachable!()
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Integer(kind),
            value: ChoiceValue::Integer(v),
            was_forced,
        });

        Ok(v)
    }

    /// Draw a boolean with probability `p` of being true.
    /// If `forced` is Some, the result is forced to that value.
    pub fn weighted(&mut self, p: f64, forced: Option<bool>) -> Result<bool, StopTest> {
        let kind = BooleanChoice { p };

        let forced_value = forced.or_else(|| {
            if p <= 0.0 {
                Some(false)
            } else if p >= 1.0 {
                Some(true)
            } else {
                None
            }
        });

        let (value, was_forced) = if let Some(f) = forced_value {
            self.pre_choice()?;
            (ChoiceValue::Boolean(f), true)
        } else {
            self.resolve_choice(
                &ChoiceKind::Boolean(kind.clone()),
                || ChoiceValue::Boolean(false), // simplest
                || ChoiceValue::Boolean(true),   // unit
                |v| matches!(v, ChoiceValue::Boolean(_)),
                |rng| ChoiceValue::Boolean(rng.random::<f64>() <= p),
            )?
        };

        let ChoiceValue::Boolean(v) = value else {
            unreachable!()
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Boolean(kind),
            value: ChoiceValue::Boolean(v),
            was_forced,
        });

        Ok(v)
    }

    /// Common pre-choice validation.
    fn pre_choice(&mut self) -> Result<(), StopTest> {
        if self.status.is_some() {
            panic!("Frozen: attempted choice on completed test case");
        }
        if self.nodes.len() >= self.max_size {
            self.status = Some(Status::EarlyStop);
            return Err(StopTest);
        }
        Ok(())
    }

    /// Resolve a choice value from forced, prefix, or random.
    ///
    /// This implements the pbtkit punning logic: when replaying from a prefix
    /// and the value doesn't validate for the current kind, we map
    /// simplest->simplest, everything else->unit.
    fn resolve_choice(
        &mut self,
        _kind: &ChoiceKind,
        simplest: impl FnOnce() -> ChoiceValue,
        unit: impl FnOnce() -> ChoiceValue,
        validate: impl FnOnce(&ChoiceValue) -> bool,
        random: impl FnOnce(&mut SmallRng) -> ChoiceValue,
    ) -> Result<(ChoiceValue, bool), StopTest> {
        self.pre_choice()?;

        let idx = self.nodes.len();

        if idx < self.prefix.len() {
            // Replay from prefix
            let prefix_value = &self.prefix[idx];
            if validate(prefix_value) {
                Ok((prefix_value.clone(), false))
            } else {
                // Punning: if the prefix value was the simplest of its original kind,
                // map to the simplest of the new kind. Otherwise, map to unit.
                let is_simplest = self
                    .prefix_nodes
                    .as_ref()
                    .and_then(|pn| pn.get(idx))
                    .is_some_and(|pn| *prefix_value == pn.kind.simplest());

                if is_simplest {
                    Ok((simplest(), false))
                } else {
                    Ok((unit(), false))
                }
            }
        } else {
            // Random generation
            let rng = self
                .rng
                .as_mut()
                .expect("No RNG available for random generation");
            Ok((random(rng), false))
        }
    }
}
