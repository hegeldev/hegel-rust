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

/// Probability of drawing a boundary/special value per special candidate.
/// With k candidates and 1000 draws: P(all drawn at least once) = (1-(1-p)^1000)^k > 0.9999.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;

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

impl FloatChoice {
    /// The simplest (lowest-sort-key) valid float for this choice.
    pub fn simplest(&self) -> f64 {
        if self.validate(0.0) {
            return 0.0;
        }
        // Among finite boundaries, pick the one with the smallest sort key.
        // Positive floats sort before negative of the same magnitude.
        let mut best: Option<f64> = None;
        let mut best_key: (u64, bool) = (u64::MAX, true);
        for &v in &[self.min_value, self.max_value] {
            if v.is_finite() {
                let is_neg = v.is_sign_negative();
                let key = (float_to_index(v.abs()), is_neg);
                if key < best_key {
                    best = Some(v);
                    best_key = key;
                }
            }
        }
        if let Some(v) = best {
            return v;
        }
        if self.allow_infinity && self.validate(f64::INFINITY) {
            return f64::INFINITY;
        }
        if self.allow_nan {
            // Canonical quiet NaN.
            return f64::NAN;
        }
        panic!("FloatChoice::simplest: no valid float for this choice")
    }

    /// Second-simplest valid float (for type punning during replay).
    pub fn unit(&self) -> f64 {
        let s = self.simplest();
        if s.is_nan() {
            return s;
        }
        // Try the next index up from simplest (in absolute magnitude).
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
            // Directional check: -inf is below any finite min; +inf is above any finite max.
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
    /// NaN sorts last (u64::MAX, false). Positive is simpler than negative
    /// with the same magnitude.
    pub fn sort_index(&self, v: f64) -> (u64, bool) {
        if v.is_nan() {
            return (u64::MAX, false);
        }
        let is_neg = v.is_sign_negative();
        let mag = if is_neg { -v } else { v };
        (float_to_index(mag), is_neg)
    }
}

// ---------------------------------------------------------------------------
// Hypothesis float ordering
// ---------------------------------------------------------------------------
//
// Port of hypothesis/internal/conjecture/floats.py.
// Maps non-negative floats to dense lexicographic indices where:
// - Small non-negative integers (0, 1, 2, ...) have the smallest indices
// - Non-integer fractions with "simpler" denominators (like 1.5) come next
// - Large or irrational-looking floats come last
// - Infinity is ordered last among finite-or-infinite floats
//
// This ordering makes the shrinker prefer "nice" values (integers, simple
// fractions) over values that are merely close to the boundary.

/// Encode a biased exponent to a Hypothesis lex rank.
/// Exponents closer to 1023 (values near 1.0) rank first.
fn encode_exponent(biased_exp: u64) -> u64 {
    if biased_exp == 2047 {
        return 2047; // inf/NaN exponent: rank last
    }
    if biased_exp >= 1023 {
        biased_exp - 1023
    } else {
        2046 - biased_exp
    }
}

/// Decode a lex rank back to a biased exponent.
fn decode_exponent(enc: u64) -> u64 {
    if enc == 2047 {
        return 2047;
    }
    if enc <= 1023 {
        enc + 1023
    } else {
        2046 - enc
    }
}

/// Reverse the lowest `n` bits of `v`.
fn reverse_bits_n(v: u64, n: u64) -> u64 {
    v.reverse_bits() >> (64 - n)
}

/// Adjust mantissa bits so that low-denominator fractions have smaller indices.
///
/// For values in [1, 2) (unbiased_exp=0), reverses all 52 fractional bits so
/// that 1.5 (mantissa=2^51, reversed=1) is simpler than 1.1 (mantissa=complex).
/// For values in [2, 4) (unbiased_exp=1), only the lower 51 fractional bits are
/// reversed. For large exponents (>= 52), no change needed (no fractional part).
pub fn update_mantissa(unbiased_exp: i64, mantissa: u64) -> u64 {
    if unbiased_exp <= 0 {
        reverse_bits_n(mantissa, 52)
    } else if unbiased_exp <= 51 {
        let n_frac = (52 - unbiased_exp) as u64;
        let frac_mask = (1u64 << n_frac) - 1;
        let frac = mantissa & frac_mask;
        (mantissa ^ frac) | reverse_bits_n(frac, n_frac)
    } else {
        mantissa
    }
}

/// True if `v` is a non-negative integer representable in 56 bits.
/// These are mapped directly to their integer value in the lex ordering.
fn is_simple_float(v: f64) -> bool {
    if v.is_sign_negative() || !v.is_finite() {
        return false;
    }
    let i = v as u64;
    i as f64 == v && i < (1u64 << 56)
}

/// Map a non-negative (finite or infinite) float to its Hypothesis lex index.
///
/// Port of Hypothesis's `float_to_lex`. Integer floats 0, 1, 2, ... map to
/// 0, 1, 2, ... Non-integer floats map to values with bit 63 set.
pub fn float_to_index(v: f64) -> u64 {
    debug_assert!(!v.is_sign_negative(), "float_to_index called on negative: {v}");
    debug_assert!(!v.is_nan(), "float_to_index called on NaN");
    if is_simple_float(v) {
        return v as u64;
    }
    let bits = v.to_bits();
    let biased_exp = (bits >> 52) & 0x7FF;
    let mantissa = bits & ((1u64 << 52) - 1);
    let unbiased_exp = biased_exp as i64 - 1023;
    let mantissa_enc = update_mantissa(unbiased_exp, mantissa);
    let exp_enc = encode_exponent(biased_exp);
    (1u64 << 63) | (exp_enc << 52) | mantissa_enc
}

/// Map a Hypothesis lex index back to a non-negative float.
///
/// Port of Hypothesis's `lex_to_float`. Inverse of `float_to_index`.
pub fn index_to_float(i: u64) -> f64 {
    if i >> 63 == 0 {
        // Integer path: low 56 bits as integer value.
        let integral = i & ((1u64 << 56) - 1);
        return integral as f64;
    }
    let exp_enc = (i >> 52) & 0x7FF;
    let biased_exp = decode_exponent(exp_enc);
    let mantissa_enc = i & ((1u64 << 52) - 1);
    let unbiased_exp = biased_exp as i64 - 1023;
    // update_mantissa is its own inverse (bit reversal is self-inverse).
    let mantissa = update_mantissa(unbiased_exp, mantissa_enc);
    f64::from_bits((biased_exp << 52) | mantissa)
}

/// Convert a lexicographically ordered u64 to a float covering the full float space.
/// Used for random float generation. Port of pbtkit's `_lex_to_float`.
pub fn lex_to_float(bits: u64) -> f64 {
    let bits = if bits >> 63 != 0 {
        bits ^ (1u64 << 63)
    } else {
        bits ^ u64::MAX
    };
    f64::from_bits(bits)
}

/// The kind of choice made at a particular point.
#[derive(Clone, Debug)]
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

    /// The second simplest value, used for type punning during replay.
    pub fn unit(&self) -> ChoiceValue {
        match self {
            ChoiceKind::Integer(ic) => ChoiceValue::Integer(ic.unit()),
            ChoiceKind::Boolean(bc) => ChoiceValue::Boolean(bc.unit()),
            ChoiceKind::Float(fc) => ChoiceValue::Float(fc.unit()),
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
                    return ChoiceValue::Integer(min_value);
                }
                // Edge case boosting: draw boundary/special values with elevated probability.
                let mut nasty: Vec<i128> = vec![min_value, max_value];
                if min_value <= 0 && 0 <= max_value && min_value != 0 && max_value != 0 {
                    nasty.push(0);
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
        let kind = FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
        };

        let bounded = min_value.is_finite() && max_value.is_finite();
        let half_bounded = !bounded && (min_value.is_finite() || max_value.is_finite());

        // Build edge case candidates for boosting.
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
                // Edge case boosting: draw boundary/special values with elevated probability.
                if rng.random::<f64>() < nasty_threshold {
                    let idx = rng.random_range(0..nasty_floats.len());
                    return ChoiceValue::Float(nasty_floats[idx]);
                }
                let f = if bounded {
                    // Uniform in [min, max], clamped to handle overflow.
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
                    // Random NaN: set exponent to all 1s, random non-zero mantissa.
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
                // Ensure the generated value satisfies the schema constraints.
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
