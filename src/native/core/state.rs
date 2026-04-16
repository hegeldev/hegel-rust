// Stateful types: NativeTestCase, ManyState, NativeVariables, Span.

use std::collections::HashMap;

use rand::RngExt;
use rand::rngs::SmallRng;

use super::choices::{
    BooleanChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice, Status,
    StopTest,
};
use super::{BOUNDARY_PROBABILITY, BUFFER_SIZE};

/// State for a variable-length collection (port of pbtkit's `many` class).
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

/// A pool of variable IDs for stateful testing.
///
/// Port of hegel-core's `Variables` class from server.py.
pub struct NativeVariables {
    last_id: i128,
    variables: Vec<i128>,
    removed: std::collections::HashSet<i128>,
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
    pub fn next(&mut self) -> i128 {
        self.last_id += 1;
        self.variables.push(self.last_id);
        self.last_id
    }

    /// Return the IDs of variables that have not been consumed, in order.
    pub fn active(&self) -> Vec<i128> {
        self.variables
            .iter()
            .filter(|id| !self.removed.contains(*id))
            .copied()
            .collect()
    }

    /// Mark a variable as consumed and trim trailing consumed variables.
    pub fn consume(&mut self, variable_id: i128) {
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

/// A span within the choice sequence, labelled by schema type.
///
/// Recorded by `interpret_schema` to enable span-mutation exploration.
#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub label: String,
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
    pub collections: HashMap<i64, ManyState>,
    next_collection_id: i64,
    pub variable_pools: Vec<NativeVariables>,
    pub spans: Vec<Span>,
}

impl NativeTestCase {
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
            variable_pools: Vec::new(),
            spans: Vec::new(),
        }
    }

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
            variable_pools: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Record a span covering choice nodes [start, end) with the given label.
    pub fn record_span(&mut self, start: usize, end: usize, label: String) {
        if end > start {
            self.spans.push(Span { start, end, label });
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
                    return ChoiceValue::Integer(min_value);
                }
                let mut nasty: Vec<i128> = vec![min_value, max_value];
                let interesting: &[i128] = &[
                    0, 1, -1, 2, -2,
                    7, -7, 8, -8,
                    15, -15, 16, -16,
                    31, -31, 32, -32,
                    63, -63, 64, -64,
                    127, -127, 128, -128,
                    255, -255, 256, -256,
                    511, -511, 512, -512,
                    1023, -1023, 1024, -1024,
                    2047, -2047, 2048, -2048,
                    4095, -4095, 4096, -4096,
                    8191, -8191, 8192, -8192,
                    i16::MAX as i128, i16::MIN as i128,
                    i32::MAX as i128, i32::MIN as i128,
                    i64::MAX as i128, i64::MIN as i128,
                ];
                for &v in interesting {
                    if kind.validate(v) && !nasty.contains(&v) {
                        nasty.push(v);
                    }
                }
                let threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;
                if rng.random::<f64>() < threshold {
                    let idx = rng.random_range(0..nasty.len());
                    return ChoiceValue::Integer(nasty[idx]);
                }
                ChoiceValue::Integer(rng.random_range(min_value..=max_value))
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
        let kind = BooleanChoice;

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
                || ChoiceValue::Boolean(kind.simplest()),
                || ChoiceValue::Boolean(kind.unit()),
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

    /// Draw a floating-point value.
    ///
    /// Port of pbtkit's `_draw_float` / `draw_float` method.
    pub fn draw_float(
        &mut self,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
    ) -> Result<f64, StopTest> {
        use super::float_index::lex_to_float;

        let kind = FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
        };

        let bounded = min_value.is_finite() && max_value.is_finite();
        let half_bounded = !bounded && (min_value.is_finite() || max_value.is_finite());

        let nasty_floats: Vec<f64> = {
            let candidates = [
                min_value,
                max_value,
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
            candidates.iter().copied().filter(|&v| kind.validate(v)).collect()
        };
        let nasty_threshold = nasty_floats.len() as f64 * BOUNDARY_PROBABILITY;

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Float(kind.clone()),
            || ChoiceValue::Float(kind.simplest()),
            || ChoiceValue::Float(kind.unit()),
            |v| matches!(v, ChoiceValue::Float(f) if kind.validate(*f)),
            |rng| {
                if rng.random::<f64>() < nasty_threshold {
                    let idx = rng.random_range(0..nasty_floats.len());
                    return ChoiceValue::Float(nasty_floats[idx]);
                }
                let f = if bounded {
                    let r: f64 = rng.random();
                    let v = min_value + r * (max_value - min_value);
                    v.max(min_value).min(max_value)
                } else if half_bounded {
                    let use_inf = allow_infinity && rng.random::<f64>() < 0.05;
                    if use_inf {
                        if max_value == f64::INFINITY { f64::INFINITY } else { f64::NEG_INFINITY }
                    } else {
                        loop {
                            let bits: u64 = rng.random();
                            let mag = lex_to_float(bits).abs();
                            if mag.is_finite() {
                                break if min_value.is_finite() {
                                    min_value + mag
                                } else {
                                    max_value - mag
                                };
                            }
                        }
                    }
                } else if allow_nan && rng.random::<f64>() < 0.01 {
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
                let f = if kind.validate(f) { f } else { kind.simplest() };
                ChoiceValue::Float(f)
            },
        )?;

        let ChoiceValue::Float(v) = value else {
            unreachable!()
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Float(kind),
            value: ChoiceValue::Float(v),
            was_forced,
        });

        Ok(v)
    }

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
    /// Implements the pbtkit punning logic.
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
            let prefix_value = &self.prefix[idx];
            if validate(prefix_value) {
                Ok((prefix_value.clone(), false))
            } else {
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
            let rng = self
                .rng
                .as_mut()
                .expect("No RNG available for random generation");
            Ok((random(rng), false))
        }
    }
}
