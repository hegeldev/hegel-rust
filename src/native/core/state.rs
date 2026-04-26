// Stateful types: NativeTestCase, ManyState, NativeVariables, Span.

use std::collections::HashMap;

use rand::RngExt;
use rand::rngs::SmallRng;

use super::choices::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
    Status, StopTest, StringChoice,
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
        let average = f64::min(f64::max(min_f * 2.0, min_f + 5.0), 0.5 * (min_f + max_f));
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

/// Hypothesis `many()`-style length for atomic collection choices (bytes, strings).
///
/// Instead of drawing length uniformly from `[min_size, max_size]` (which produces
/// huge values when max_size is large), this uses the same geometric distribution
/// as Hypothesis's `many()` mechanism: length clusters around a small `average_size`
/// computed as `min(max(min_size * 2, min_size + 5), 0.5 * (min_size + max_size))`.
///
/// Hypothesis: `conjecture/providers.py::HypothesisProvider.draw_string` (and
/// `draw_bytes`). pbtkit's `text.py::_draw_string` uses uniform instead; we match
/// Hypothesis here as it is the behavioural ground truth.
fn many_draw_length(rng: &mut SmallRng, min_size: usize, max_size: usize) -> usize {
    if min_size == max_size {
        return min_size;
    }
    let many = ManyState::new(min_size, Some(max_size));
    let mut len = min_size;
    while len < max_size && rng.random::<f64>() < many.p_continue {
        len += 1;
    }
    len
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

/// A span within the choice sequence, labelled by schema type or by the
/// numeric label of an enclosing `start_span` call.
///
/// Recorded to enable span-mutation exploration (see `try_span_mutation`)
/// and to expose the structure of a test case to the shrinker, mutator,
/// and assertion-style tests.  Mirrors Hypothesis's `Span` in
/// `internal/conjecture/data.py`.
#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub label: String,
    /// Depth of this span in the span tree. The top-level span has depth 0.
    pub depth: u32,
    /// Index of the directly-enclosing span, or `None` for the top-level span.
    pub parent: Option<usize>,
    /// True iff this span's `stop_span` was called with `discard=true`.
    pub discarded: bool,
}

impl Span {
    /// Number of choice nodes covered by this span.
    pub fn choice_count(&self) -> usize {
        self.end - self.start
    }
}

/// Maximum nested span depth before the engine marks the test case
/// `Status::Invalid`.  Mirrors Hypothesis's
/// `internal/conjecture/data.py::MAX_DEPTH`.
pub const MAX_DEPTH: u32 = 100;

/// A collection of spans recorded during a single test case, with
/// Python-style indexing semantics on top of [`Vec<Span>`].
///
/// Indexing accepts negative indices (`-1` is the last span) and panics
/// with an "out of range" message on out-of-bounds access, matching the
/// `IndexError` raised by Python's [`Spans`][1].
///
/// [1]: https://github.com/HypothesisWorks/hypothesis/blob/master/hypothesis-python/src/hypothesis/internal/conjecture/data.py
#[derive(Clone, Debug, Default)]
pub struct Spans {
    inner: Vec<Span>,
}

impl Spans {
    /// Construct an empty `Spans` collection.
    pub fn new() -> Self {
        Spans { inner: Vec::new() }
    }

    /// Number of recorded spans.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if no spans have been recorded.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate spans in order.
    pub fn iter(&self) -> std::slice::Iter<'_, Span> {
        self.inner.iter()
    }

    /// Append a span (interior bookkeeping; pushes after any
    /// already-recorded spans).
    pub fn push(&mut self, span: Span) {
        self.inner.push(span);
    }

    /// Mutable access to a span by raw index.
    pub fn get_mut(&mut self, i: usize) -> Option<&mut Span> {
        self.inner.get_mut(i)
    }

    /// Access by raw (non-negative) index, returning `None` on
    /// out-of-bounds.  Mirrors `Vec::get`.
    pub fn get(&self, i: usize) -> Option<&Span> {
        self.inner.get(i)
    }

    /// Access by signed index (Python-style: `-1` = last).  Returns
    /// `None` for any out-of-range index.
    pub fn get_signed(&self, i: i64) -> Option<&Span> {
        let n = self.inner.len() as i64;
        if i < -n || i >= n {
            return None;
        }
        let idx = if i < 0 { (i + n) as usize } else { i as usize };
        self.inner.get(idx)
    }

    /// Indices of the direct children of the span at `i`, in
    /// preorder (the order in which they were started).
    ///
    /// Computed from each span's `parent` field; runs in O(n) over the
    /// span list.
    pub fn children(&self, i: usize) -> Vec<usize> {
        self.inner
            .iter()
            .enumerate()
            .filter_map(|(j, s)| (s.parent == Some(i)).then_some(j))
            .collect()
    }

    /// View as a slice, for code that wants raw indexing.
    pub fn as_slice(&self) -> &[Span] {
        &self.inner
    }

    /// Mutable slice access.
    pub fn as_mut_slice(&mut self) -> &mut [Span] {
        &mut self.inner
    }

    /// Consume the collection and return the underlying `Vec`.
    pub fn into_vec(self) -> Vec<Span> {
        self.inner
    }
}

impl From<Vec<Span>> for Spans {
    fn from(inner: Vec<Span>) -> Self {
        Spans { inner }
    }
}

impl std::ops::Deref for Spans {
    type Target = [Span];
    fn deref(&self) -> &[Span] {
        &self.inner
    }
}

impl<'a> IntoIterator for &'a Spans {
    type Item = &'a Span;
    type IntoIter = std::slice::Iter<'a, Span>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl std::ops::Index<usize> for Spans {
    type Output = Span;
    fn index(&self, i: usize) -> &Span {
        &self.inner[i]
    }
}

impl std::ops::Index<i64> for Spans {
    type Output = Span;
    fn index(&self, i: i64) -> &Span {
        let n = self.inner.len();
        self.get_signed(i).unwrap_or_else(|| {
            panic!("Index {i} out of range [-{n}, {n})");
        })
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
    /// When true, every draw beyond `prefix` resolves to the kind's
    /// simplest value rather than panicking on a missing RNG.  Mirrors
    /// Hypothesis's `ChoiceTemplate("simplest", count=None)` template
    /// used by `generate_new_examples` to probe the all-zero leaf of
    /// the choice tree at the start of each generation phase.
    force_simplest: bool,
    pub nodes: Vec<ChoiceNode>,
    pub status: Option<Status>,
    pub collections: HashMap<i64, ManyState>,
    next_collection_id: i64,
    pub variable_pools: Vec<NativeVariables>,
    pub spans: Spans,
    /// Indices into `spans` for currently-open spans, in nesting order.
    /// Each entry was pushed by `start_span` and is awaiting a matching
    /// `stop_span` call.
    pub span_stack: Vec<usize>,
    /// True iff any `stop_span(discard=true)` has been observed during this test
    /// case. Mirrors Hypothesis's `ConjectureData.has_discards`: filters that
    /// retry mark the rejected attempts as discarded, which the shrinker uses
    /// to prioritise removing them.
    pub has_discards: bool,
}

impl NativeTestCase {
    pub fn new_random(rng: SmallRng) -> Self {
        NativeTestCase {
            prefix: Vec::new(),
            prefix_nodes: None,
            rng: Some(rng),
            max_size: BUFFER_SIZE,
            force_simplest: false,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
        }
    }

    pub fn for_choices(choices: &[ChoiceValue], prefix_nodes: Option<&[ChoiceNode]>) -> Self {
        NativeTestCase {
            prefix: choices.to_vec(),
            prefix_nodes: prefix_nodes.map(|n| n.to_vec()),
            rng: None,
            max_size: choices.len(),
            force_simplest: false,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
        }
    }

    /// A test case that resolves every draw to the kind's simplest
    /// value, up to `max_size` choices.  Mirrors Hypothesis's
    /// `cached_test_function((ChoiceTemplate("simplest", count=None),))`
    /// at the head of `generate_new_examples`: a one-shot probe of the
    /// all-simplest leaf so the runner discovers tiny counterexamples
    /// before random exploration kicks in.
    pub fn for_simplest(max_size: usize) -> Self {
        NativeTestCase {
            prefix: Vec::new(),
            prefix_nodes: None,
            rng: None,
            max_size,
            force_simplest: true,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
        }
    }

    /// A test case that replays `prefix` for the first positions and then
    /// draws randomly from `rng` for subsequent positions, up to a total of
    /// `max_size` choices.
    ///
    /// Used by `mutate_and_shrink`: port of pbtkit's
    /// `TestCase(prefix=..., random=..., max_size=...)` construction in
    /// `shrinking/mutation.py`.
    pub fn for_probe(prefix: &[ChoiceValue], rng: SmallRng, max_size: usize) -> Self {
        NativeTestCase {
            prefix: prefix.to_vec(),
            prefix_nodes: None,
            rng: Some(rng),
            max_size,
            force_simplest: false,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
        }
    }

    /// A test case that replays `prefix` for the first positions and has
    /// no RNG to fall back on, capping the total number of choices at
    /// `max_choices`.  Mirrors Hypothesis's
    /// `ConjectureData(prefix=..., random=None, max_choices=...)`
    /// construction: any draw past either the prefix or `max_choices`
    /// sets `status = OVERRUN` and returns `StopTest`.
    pub fn for_prefix_with_max(prefix: &[ChoiceValue], max_choices: usize) -> Self {
        NativeTestCase {
            prefix: prefix.to_vec(),
            prefix_nodes: None,
            rng: None,
            max_size: max_choices,
            force_simplest: false,
            nodes: Vec::new(),
            status: None,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
        }
    }

    /// Record a finished span covering choice nodes `[start, end)` with the
    /// given label.  The span is assigned a parent (the innermost
    /// currently-open span, if any) and a depth (one greater than that
    /// parent's depth, or 0 if there is no enclosing span).
    ///
    /// Used by leaf-schema interpretation in `schema/mod.rs` and by
    /// `feature_flag` draws.  Higher-level callers should prefer
    /// [`Self::start_span`] / [`Self::stop_span`], which preserve span-tree
    /// structure for nested draws.
    pub fn record_span(&mut self, start: usize, end: usize, label: String) {
        if end > start {
            let parent = self.span_stack.last().copied();
            let depth = self.span_stack.len() as u32;
            self.spans.push(Span {
                start,
                end,
                label,
                depth,
                parent,
                discarded: false,
            });
        }
    }

    /// Open a new span at the current choice position, labelled with `label`.
    ///
    /// Returns the index assigned to the span in `self.spans`.  The span's
    /// `end` is set to `self.nodes.len()` as a placeholder and overwritten
    /// when [`Self::stop_span`] is called.
    ///
    /// If opening this span would push depth past [`MAX_DEPTH`], the test
    /// case is marked invalid and `start_span` returns the assigned index
    /// without further bookkeeping; subsequent draws on a frozen test case
    /// will trip the existing freeze guard.
    pub fn start_span(&mut self, label: u64) -> usize {
        let parent = self.span_stack.last().copied();
        let depth = self.span_stack.len() as u32;
        let idx = self.spans.len();
        let start = self.nodes.len();
        self.spans.push(Span {
            start,
            end: start,
            label: label.to_string(),
            depth,
            parent,
            discarded: false,
        });
        self.span_stack.push(idx);
        if depth + 1 > MAX_DEPTH && self.status.is_none() {
            self.status = Some(Status::Invalid);
        }
        idx
    }

    /// Close the innermost currently-open span.
    ///
    /// `discard=true` marks the span as discarded (used by filter retries
    /// to flag rejected attempts) and sets `has_discards` on the test case.
    pub fn stop_span(&mut self, discard: bool) {
        let Some(idx) = self.span_stack.pop() else {
            return;
        };
        let end = self.nodes.len();
        if let Some(span) = self.spans.get_mut(idx) {
            span.end = end;
            span.discarded = discard;
        }
        if discard {
            self.has_discards = true;
        }
    }

    /// Whether the test case has been frozen and may no longer accept draws.
    ///
    /// Hypothesis's `ConjectureData.frozen` flag is its own boolean; the
    /// native engine collapses that flag onto the post-completion
    /// `status` value, so any non-`None` status means the test case is
    /// frozen.
    pub fn frozen(&self) -> bool {
        self.status.is_some()
    }

    /// Mark the test case as completed, defaulting to `Status::Valid` when
    /// no terminal status was set during the run.
    ///
    /// Idempotent: calling `freeze()` on an already-frozen test case is
    /// a no-op, mirroring `ConjectureData.freeze`'s early return on
    /// `self.frozen` in `conjecture/data.py`.
    ///
    /// Closes any currently-open spans, setting their `end` to the final
    /// choice position (matching Hypothesis's behaviour where freeze
    /// implicitly closes intervals left open by an exception or overrun).
    pub fn freeze(&mut self) {
        let already_frozen = self.status.is_some();
        let end = self.nodes.len();
        while let Some(idx) = self.span_stack.pop() {
            if let Some(span) = self.spans.get_mut(idx) {
                span.end = end;
            }
        }
        if !already_frozen {
            self.status = Some(Status::Valid);
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
                    0,
                    1,
                    -1,
                    2,
                    -2,
                    7,
                    -7,
                    8,
                    -8,
                    15,
                    -15,
                    16,
                    -16,
                    31,
                    -31,
                    32,
                    -32,
                    63,
                    -63,
                    64,
                    -64,
                    127,
                    -127,
                    128,
                    -128,
                    255,
                    -255,
                    256,
                    -256,
                    511,
                    -511,
                    512,
                    -512,
                    1023,
                    -1023,
                    1024,
                    -1024,
                    2047,
                    -2047,
                    2048,
                    -2048,
                    4095,
                    -4095,
                    4096,
                    -4096,
                    8191,
                    -8191,
                    8192,
                    -8192,
                    i16::MAX as i128,
                    i16::MIN as i128,
                    i32::MAX as i128,
                    i32::MIN as i128,
                    i64::MAX as i128,
                    i64::MIN as i128,
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
    // nocov start
    pub fn weighted(&mut self, p: f64, forced: Option<bool>) -> Result<bool, StopTest> {
        let kind = BooleanChoice;

        let forced_value = forced.or(if p <= 0.0 {
            Some(false)
        } else if p >= 1.0 {
            Some(true)
        } else {
            None
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
    // nocov end

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
            candidates
                .iter()
                .copied()
                .filter(|&v| kind.validate(v))
                .collect()
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
                        if max_value == f64::INFINITY {
                            f64::INFINITY
                        } else {
                            f64::NEG_INFINITY
                        }
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

    /// Draw a bytes value with length in `[min_size, max_size]`.
    ///
    /// Port of pbtkit's `_draw_bytes` / `draw_bytes` method.
    // nocov start
    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Result<Vec<u8>, StopTest> {
        assert!(
            min_size <= max_size,
            "min_size ({min_size}) must be <= max_size ({max_size})"
        );
        let kind = BytesChoice { min_size, max_size };

        // Edge-case-boosting candidates: simplest, empty, all-zeros single,
        // all-0xff single — any of which land on common counterexample shapes.
        let nasty: Vec<Vec<u8>> = {
            let mut v = vec![kind.simplest()];
            if min_size == 0 && max_size > 0 {
                v.push(vec![0u8]);
            }
            if min_size <= 1 && max_size >= 1 {
                v.push(vec![0xffu8]);
            }
            v
        };
        let nasty_threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Bytes(kind.clone()),
            || ChoiceValue::Bytes(kind.simplest()),
            || ChoiceValue::Bytes(kind.unit()),
            |v| matches!(v, ChoiceValue::Bytes(b) if kind.validate(b)),
            |rng| {
                if rng.random::<f64>() < nasty_threshold {
                    let idx = rng.random_range(0..nasty.len());
                    return ChoiceValue::Bytes(nasty[idx].clone());
                }
                let len = many_draw_length(rng, min_size, max_size);
                let bytes: Vec<u8> = (0..len).map(|_| rng.random::<u8>()).collect();
                ChoiceValue::Bytes(bytes)
            },
        )?;

        let ChoiceValue::Bytes(v) = value else {
            unreachable!()
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Bytes(kind),
            value: ChoiceValue::Bytes(v.clone()),
            was_forced,
        });

        Ok(v)
    }
    // nocov end

    /// Draw a string value with codepoint range `[min_codepoint, max_codepoint]`
    /// (surrogates automatically excluded) and length in `[min_size, max_size]`.
    ///
    /// Port of pbtkit's `_draw_string` / `draw_string` method. Only covers the
    /// "simple codepoint range" alphabet shape; filtered alphabets (categories,
    /// explicit include/exclude lists) continue to go through the decomposed
    /// integer-per-char path in `interpret_string`.
    pub fn draw_string(
        &mut self,
        min_codepoint: u32,
        max_codepoint: u32,
        min_size: usize,
        max_size: usize,
    ) -> Result<String, StopTest> {
        assert!(
            min_codepoint <= max_codepoint,
            "Invalid codepoint range [{min_codepoint}, {max_codepoint}]"
        );
        assert!(min_size <= max_size);

        let kind = StringChoice {
            min_codepoint,
            max_codepoint,
            min_size,
            max_size,
        };

        // Edge-case-boosting: simplest, empty (if allowed), single simplest
        // codepoint (if allowed), two simplest codepoints (for duplicate-char
        // counterexamples).
        let nasty: Vec<Vec<u32>> = {
            let simplest = kind.simplest();
            let simplest_cp = kind.simplest_codepoint();
            let mut v = vec![simplest];
            if min_size == 0 && max_size > 0 {
                v.push(Vec::new());
            }
            if min_size <= 1 && max_size >= 1 {
                v.push(vec![simplest_cp]);
            }
            if min_size <= 2 && max_size >= 2 {
                v.push(vec![simplest_cp, simplest_cp]);
            }
            v
        };
        let nasty_threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;

        let kind_rand = kind.clone();
        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::String(kind.clone()),
            || ChoiceValue::String(kind.simplest()),
            || ChoiceValue::String(kind.unit()),
            |v| matches!(v, ChoiceValue::String(s) if kind.validate(s)),
            |rng| {
                if rng.random::<f64>() < nasty_threshold {
                    let idx = rng.random_range(0..nasty.len());
                    return ChoiceValue::String(nasty[idx].clone());
                }
                // Build a small sub-alphabet of valid codepoints (1..=10).
                // Each entry has a 20% chance of being drawn from the ASCII
                // sub-range (if any), matching pbtkit's _draw_string.
                let ascii_hi = kind_rand.max_codepoint.min(127);
                let has_ascii = kind_rand.min_codepoint <= ascii_hi;
                let alpha_size = rng.random_range(1..=10);
                let mut alphabet: Vec<u32> = Vec::with_capacity(alpha_size);
                while alphabet.len() < alpha_size {
                    let cp = if has_ascii && rng.random::<f64>() < 0.2 {
                        rng.random_range(kind_rand.min_codepoint..=ascii_hi)
                    } else {
                        loop {
                            let cp =
                                rng.random_range(kind_rand.min_codepoint..=kind_rand.max_codepoint);
                            if !(0xD800..=0xDFFF).contains(&cp) {
                                break cp;
                            }
                        }
                    };
                    alphabet.push(cp);
                }
                let len = many_draw_length(rng, kind_rand.min_size, kind_rand.max_size);
                let s: Vec<u32> = (0..len)
                    .map(|_| alphabet[rng.random_range(0..alphabet.len())])
                    .collect();
                ChoiceValue::String(s)
            },
        )?;

        let ChoiceValue::String(v) = value else {
            unreachable!()
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::String(kind),
            value: ChoiceValue::String(v.clone()),
            was_forced,
        });

        // Boundary: convert the internal codepoint sequence back to a Rust
        // `String`, dropping any surrogate codepoints (which can't be
        // represented as a `char`). In practice the engine never produces
        // surrogates here — generation rejection-samples them and `validate`
        // rejects them — but a user-supplied prefix could feed one in, so we
        // drop rather than panic.
        Ok(codepoints_to_string(&v))
    }

    /// Draw an integer, forced to `forced`. Panics if `forced` is outside `[min_value, max_value]`.
    ///
    /// Forcing counterpart of [`draw_integer`]. Records a `ChoiceNode` with
    /// `was_forced = true` so the written sequence replays to the same value
    /// under [`NativeTestCase::for_choices`]. Mirrors the pattern of
    /// [`weighted`] for boolean forcing.
    pub fn draw_integer_forced(
        &mut self,
        min_value: i128,
        max_value: i128,
        forced: i128,
    ) -> Result<i128, StopTest> {
        assert!(
            min_value <= max_value,
            "Invalid range [{min_value}, {max_value}]"
        );
        let kind = IntegerChoice {
            min_value,
            max_value,
        };
        assert!(kind.validate(forced), "forced value outside range");
        self.pre_choice()?;
        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Integer(kind),
            value: ChoiceValue::Integer(forced),
            was_forced: true,
        });
        Ok(forced)
    }

    /// Draw a float, forced to `forced`. Panics if `forced` is not permitted by
    /// the constraints. Bit-exact: `-0.0` and `0.0`, distinct NaN payloads, etc.
    /// are preserved.
    pub fn draw_float_forced(
        &mut self,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
        forced: f64,
    ) -> Result<f64, StopTest> {
        let kind = FloatChoice {
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
        };
        assert!(kind.validate(forced), "forced value outside constraints");
        self.pre_choice()?;
        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Float(kind),
            value: ChoiceValue::Float(forced),
            was_forced: true,
        });
        Ok(forced)
    }

    /// Draw bytes, forced to `forced`. Panics if the length is outside bounds.
    pub fn draw_bytes_forced(
        &mut self,
        min_size: usize,
        max_size: usize,
        forced: Vec<u8>,
    ) -> Result<Vec<u8>, StopTest> {
        assert!(min_size <= max_size);
        let kind = BytesChoice { min_size, max_size };
        assert!(kind.validate(&forced), "forced bytes outside length bounds");
        self.pre_choice()?;
        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Bytes(kind),
            value: ChoiceValue::Bytes(forced.clone()),
            was_forced: true,
        });
        Ok(forced)
    }

    /// Draw a string, forced to `forced`. Panics if any codepoint is outside
    /// the codepoint range or the length is outside bounds.
    pub fn draw_string_forced(
        &mut self,
        min_codepoint: u32,
        max_codepoint: u32,
        min_size: usize,
        max_size: usize,
        forced: &str,
    ) -> Result<String, StopTest> {
        assert!(min_codepoint <= max_codepoint);
        assert!(min_size <= max_size);
        let kind = StringChoice {
            min_codepoint,
            max_codepoint,
            min_size,
            max_size,
        };
        let codepoints: Vec<u32> = forced.chars().map(|c| c as u32).collect();
        assert!(
            kind.validate(&codepoints),
            "forced string outside constraints"
        );
        self.pre_choice()?;
        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::String(kind),
            value: ChoiceValue::String(codepoints.clone()),
            was_forced: true,
        });
        Ok(codepoints_to_string(&codepoints))
    }

    fn pre_choice(&mut self) -> Result<(), StopTest> {
        // A test case can become frozen mid-execution when `start_span`
        // exceeds `MAX_DEPTH` and sets `status = Some(Status::Invalid)`,
        // mirroring Hypothesis's `mark_invalid` from `ConjectureData.draw`.
        // Subsequent draws must propagate `StopTest` so the test halts.
        if self.status.is_some() {
            return Err(StopTest);
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
        } else if self.force_simplest {
            Ok((simplest(), false))
        } else {
            let rng = self
                .rng
                .as_mut()
                .expect("No RNG available for random generation");
            Ok((random(rng), false))
        }
    }
}

/// Convert an internal codepoint sequence into a Rust `String`.
///
/// This is the boundary where the engine's raw-`u32` codepoint model meets
/// Rust's scalar-value-only `char`. Surrogate codepoints (`0xD800..=0xDFFF`)
/// can't be represented as a `char`, so they are dropped. Engine-produced
/// values never contain surrogates in practice, but a user-supplied prefix
/// could.
fn codepoints_to_string(cps: &[u32]) -> String {
    cps.iter().filter_map(|&cp| char::from_u32(cp)).collect()
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_tests.rs"]
mod tests;
