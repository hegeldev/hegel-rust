// Stateful types: NativeTestCase, ManyState, NativeVariables, Span.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{LazyLock, Mutex};

use rand::RngExt;
use rand::rngs::SmallRng;

use super::choices::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
    InterestingOrigin, Status, StopTest,
};
use super::float_index::lex_to_float;
use super::{BOUNDARY_PROBABILITY, BUFFER_SIZE};

/// State for a variable-length collection (port of Hypothesis's `many` class).
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
        ManyState {
            min_size,
            max_size: max_size.map_or(f64::INFINITY, |n| n as f64),
            p_continue: length_p_continue(min_size, max_size),
            count: 0,
            rejections: 0,
            force_stop: false,
        }
    }
}

/// Probability of extending a length draw beyond its current size. Port of
/// Hypothesis's `many()`: length clusters around an `average_size` derived
/// from `min(max(min_size * 2, min_size + 5), 0.5 * (min_size + max_size))`.
pub(crate) fn length_p_continue(min_size: usize, max_size: Option<usize>) -> f64 {
    let max_f = max_size.map_or(f64::INFINITY, |n| n as f64);
    let min_f = min_size as f64;
    let average = f64::min(f64::max(min_f * 2.0, min_f + 5.0), 0.5 * (min_f + max_f));
    let desired_extra = average - min_f;
    let max_extra = max_f - min_f;

    if desired_extra >= max_extra {
        0.99
    } else if max_f.is_infinite() {
        1.0 - 1.0 / (1.0 + desired_extra)
    } else {
        1.0 - 1.0 / (2.0 + desired_extra)
    }
}

/// Interesting integer constants seeded from Hypothesis's GLOBAL_CONSTANTS
/// (providers.py): powers of 2 (2^16..2^65), powers of 10 (10^5..10^19),
/// factorials (9!..20!), primorials — plus their ±1 neighbours and negations.
static GLOBAL_CONSTANTS_INTEGERS: LazyLock<Vec<i128>> = LazyLock::new(|| {
    let mut base: Vec<i128> = Vec::new();
    // Powers of 2 (2^16 to 2^65)
    for n in 16u32..66 {
        base.push(1i128 << n);
    }
    // Powers of 10 (10^5 to 10^19)
    let mut p10 = 100_000i128;
    for _ in 5..20u32 {
        base.push(p10);
        p10 *= 10;
    }
    // Factorials (9! to 20!)
    let mut f = 362_880i128; // 9!
    base.push(f);
    for i in 10u32..=20 {
        f *= i as i128;
        base.push(f);
    }
    // Primorial numbers
    base.extend_from_slice(&[
        510_510i128,
        6_469_693_230,
        304_250_263_527_210,
        32_589_158_477_190_044_730,
    ]);
    // Extend with n-1 and n+1
    let n_base = base.len();
    for i in 0..n_base {
        base.push(base[i] - 1);
        base.push(base[i] + 1);
    }
    // Extend with negations of all values so far
    let n_half = base.len();
    for i in 0..n_half {
        base.push(-base[i]);
    }
    base.sort_unstable();
    base.dedup();
    base
});

/// Geometric-distribution length draw for variable-length collections.
///
/// Drawing length uniformly from `[min_size, max_size]` produces huge
/// values when `max_size` is large; instead, the size follows a geometric
/// variate with stop probability derived from [`length_p_continue`].
///
/// Hypothesis: `conjecture/providers.py::HypothesisProvider.draw_bytes`
/// (and `draw_string`).
fn many_draw_length(rng: &mut SmallRng, min_size: usize, max_size: usize) -> usize {
    if min_size == max_size {
        return min_size;
    }
    let p_continue = length_p_continue(min_size, Some(max_size));
    // Geometric variate: `extra ~ floor(log(U) / log(p_continue))` for
    // `U ~ Uniform(0, 1)`. `rng.random::<f64>()` returns `[0, 1)`, so `U`
    // can be exactly `0` — that yields `-inf / log(p) = +inf` which
    // saturates to `usize::MAX` via the float cast; the final `.min` clamps.
    let u: f64 = rng.random();
    let extra = (u.ln() / p_continue.ln()).floor();
    assert!(extra >= 0.0);
    min_size.saturating_add(extra as usize).min(max_size)
}

/// Boundary-biased uniform sample for integers.
///
/// Implements the "nasty value" boost used by both the
/// [`NativeTestCase::draw_integer`] code path and the data-tree
/// [`pick_non_exhausted_value`](crate::native::conjecture_runner) path
/// during novel-prefix walks. Sharing the implementation keeps the two
/// random-generation routes consistent: when `generate_novel_prefix`
/// chooses a child to recurse into, it now picks special values
/// (0, 1, ±powers-of-two, factorials, …) with the same frequency as
/// `draw_integer` does for fresh draws.
///
/// Returns a value in `[ic.min_value, ic.max_value]` (inclusive). With
/// probability proportional to `nasty.len() * BOUNDARY_PROBABILITY` (≈ 0.5
/// for unbounded ranges) the result is one of those nasty/interesting
/// values; otherwise it's uniform across the range.
pub(crate) fn biased_integer_sample(ic: &IntegerChoice, rng: &mut SmallRng) -> i128 {
    if ic.min_value == ic.max_value {
        return ic.min_value;
    }
    let mut nasty: Vec<i128> = vec![ic.min_value, ic.max_value];
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
        if ic.validate(v) && !nasty.contains(&v) {
            nasty.push(v);
        }
    }
    for &v in GLOBAL_CONSTANTS_INTEGERS.iter() {
        if ic.validate(v) && !nasty.contains(&v) {
            nasty.push(v);
        }
    }
    let threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < threshold {
        let idx = rng.random_range(0..nasty.len());
        nasty[idx]
    } else {
        rng.random_range(ic.min_value..=ic.max_value)
    }
}

/// Float counterpart of [`biased_integer_sample`]: draws boundary / "nasty"
/// values (`0.0`, `-0.0`, `±1.0`, `±MAX`, `±INFINITY`, `MIN_POSITIVE`, NaN,
/// plus the user's `min_value`/`max_value`) with probability proportional to
/// `BOUNDARY_PROBABILITY × |nasty|`, falling back to a uniform-ish lex draw
/// otherwise. Shared with the data-tree walk so novel-prefix exploration
/// hits the same boundary distribution as fresh draws.
pub(crate) fn biased_float_sample(fc: &FloatChoice, rng: &mut SmallRng) -> f64 {
    let bounded = fc.min_value.is_finite() && fc.max_value.is_finite();
    let half_bounded = !bounded && (fc.min_value.is_finite() || fc.max_value.is_finite());

    let candidates = [
        fc.min_value,
        fc.max_value,
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
    let nasty: Vec<f64> = candidates
        .iter()
        .copied()
        .filter(|&v| fc.validate(v))
        .collect();
    let nasty_threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;

    if rng.random::<f64>() < nasty_threshold {
        let idx = rng.random_range(0..nasty.len());
        return nasty[idx];
    }
    let f = if bounded {
        let r: f64 = rng.random();
        let v = fc.min_value + r * (fc.max_value - fc.min_value);
        v.max(fc.min_value).min(fc.max_value)
    } else if half_bounded {
        let use_inf = fc.allow_infinity && rng.random::<f64>() < 0.05;
        if use_inf {
            if fc.max_value == f64::INFINITY {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            }
        } else {
            loop {
                let bits: u64 = rng.random();
                let mag = lex_to_float(bits).abs();
                if mag.is_finite() {
                    break if fc.min_value.is_finite() {
                        fc.min_value + mag
                    } else {
                        fc.max_value - mag
                    };
                }
            }
        }
    } else if fc.allow_nan && rng.random::<f64>() < 0.01 {
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
    if fc.validate(f) { f } else { fc.simplest() }
}

/// Boundary-biased sample for bytes. Draws the simplest (`min_size` zeros),
/// the all-zeros minimum-plus-one length, or a single-`0xff` byte with
/// probability proportional to `BOUNDARY_PROBABILITY × |nasty|`, falling
/// back to a length drawn from [`many_draw_length`] with uniformly random
/// byte values.
pub(crate) fn biased_bytes_sample(bc: &BytesChoice, rng: &mut SmallRng) -> Vec<u8> {
    let mut nasty: Vec<Vec<u8>> = vec![bc.simplest()];
    if bc.min_size == 0 && bc.max_size > 0 {
        nasty.push(vec![0u8]);
    }
    if bc.min_size <= 1 && bc.max_size >= 1 {
        nasty.push(vec![0xffu8]);
    }
    let nasty_threshold = nasty.len() as f64 * BOUNDARY_PROBABILITY;
    if rng.random::<f64>() < nasty_threshold {
        let idx = rng.random_range(0..nasty.len());
        return nasty[idx].clone();
    }
    let len = many_draw_length(rng, bc.min_size, bc.max_size);
    (0..len).map(|_| rng.random::<u8>()).collect()
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Maximum nested span depth before the engine marks the test case
/// `Status::Invalid`.  Mirrors Hypothesis's
/// `internal/conjecture/data.py::MAX_DEPTH`.
pub const MAX_DEPTH: u32 = 100;

/// A tag identifying a structural-coverage class for a span label.
///
/// Mirrors Hypothesis's `StructuralCoverageTag` in
/// `internal/conjecture/data.py`.  Two tags compare equal iff they
/// were produced from the same label, and [`structural_coverage`]
/// interns them so that callers also get pointer-equal results for
/// equal labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoverageTag {
    pub label: u64,
}

static STRUCTURAL_COVERAGE_CACHE: LazyLock<Mutex<HashMap<u64, &'static CoverageTag>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Look up (or insert) the [`CoverageTag`] for `label`.
///
/// Repeated calls with the same `label` return the same `&'static`
/// reference; this is the Rust analog of Hypothesis's
/// `STRUCTURAL_COVERAGE_CACHE` interning in `data.py`.
pub fn structural_coverage(label: u64) -> &'static CoverageTag {
    let mut cache = STRUCTURAL_COVERAGE_CACHE.lock().unwrap();
    cache
        .entry(label)
        .or_insert_with(|| Box::leak(Box::new(CoverageTag { label })))
}

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

/// Observer hook called by [`NativeTestCase`] after each draw and on
/// conclusion.  All methods have default no-op implementations so
/// concrete observers only need to override the callbacks they care
/// about.
///
/// Mirrors `hypothesis.internal.conjecture.data.DataObserver`.
pub trait DataObserver: Send {
    fn draw_boolean(&mut self, _value: bool, _was_forced: bool) {}
    fn draw_integer(&mut self, _value: i128, _was_forced: bool) {}
    fn draw_float(&mut self, _value: f64, _was_forced: bool) {}
    fn draw_bytes(&mut self, _value: &[u8], _was_forced: bool) {}
    fn conclude_test(&mut self, _status: Status, _origin: Option<InterestingOrigin>) {}
}

/// Snapshot of a completed `NativeTestCase`'s observable state.
///
/// Mirrors the relevant subset of Hypothesis's `ConjectureResult`
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
    /// Set to `true` by [`Self::freeze`] on the first call; subsequent calls
    /// are no-ops.  Mirrors `ConjectureData.frozen` in Python, which is a
    /// dedicated boolean so that `conclude_test` can set `self.status` before
    /// calling `freeze()` without triggering the idempotency early-return.
    frozen: bool,
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
    /// Structural-coverage tags accumulated by closing non-discarded
    /// spans.  Mirrors `ConjectureData.tags` in `data.py`: when a span
    /// closes without `discard`, every label collected by it (including
    /// its non-discarded descendants) is added here as a
    /// [`structural_coverage`] tag.  Discarded spans drop their labels
    /// (and their descendants' labels) on the floor.
    pub tags: HashSet<&'static CoverageTag>,
    /// Per-open-span sets of labels awaiting promotion into [`Self::tags`].
    ///
    /// Each `start_span` pushes a fresh `{label}` frame; `stop_span`
    /// pops it and either merges the frame into its parent (non-discard)
    /// or discards it (discard).  When the outermost frame closes
    /// without discard, its labels are converted to [`CoverageTag`]s
    /// and added to `tags`.  Mirrors `ConjectureData.labels_for_structure_stack`.
    labels_for_structure_stack: Vec<HashSet<u64>>,
    /// Optional observer notified after each draw and on conclusion.
    /// Set by [`Self::for_choices`] and called by each draw method and
    /// by [`Self::freeze`].  Mirrors `ConjectureData._observer`.
    observer: Option<Box<dyn DataObserver>>,
    /// The interesting origin set by [`Self::conclude_test`], if any.
    /// `None` for test cases concluded by [`Self::freeze`] directly
    /// (`Status::Valid`).  Mirrors `ConjectureData.interesting_origin`.
    interesting_origin: Option<InterestingOrigin>,
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
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
        }
    }

    /// Construct a `NativeTestCase` that replays `choices` in order,
    /// notifying `observer` after each draw and on conclusion.
    ///
    /// Mirrors `ConjectureData.for_choices(choices, observer=observer)`
    /// from `hypothesis.internal.conjecture.data`.
    pub fn for_choices(
        choices: &[ChoiceValue],
        prefix_nodes: Option<&[ChoiceNode]>,
        observer: Option<Box<dyn DataObserver>>,
    ) -> Self {
        NativeTestCase {
            prefix: choices.to_vec(),
            prefix_nodes: prefix_nodes.map(|n| n.to_vec()),
            rng: None,
            max_size: choices.len(),
            nodes: Vec::new(),
            status: None,
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer,
            interesting_origin: None,
        }
    }

    /// A test case that replays `prefix` for the first positions and then
    /// draws randomly from `rng` for subsequent positions, up to a total of
    /// `max_size` choices.
    ///
    /// Used by `mutate_and_shrink`; Hypothesis's equivalent
    /// `ConjectureData(prefix=..., random=..., max_size=...)`
    /// construction in `shrinking/mutation.py`.
    pub fn for_probe(prefix: &[ChoiceValue], rng: SmallRng, max_size: usize) -> Self {
        NativeTestCase {
            prefix: prefix.to_vec(),
            prefix_nodes: None,
            rng: Some(rng),
            max_size,
            nodes: Vec::new(),
            status: None,
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
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
        let mut frame = HashSet::new();
        frame.insert(label);
        self.labels_for_structure_stack.push(frame);
        if depth + 1 > MAX_DEPTH && self.status.is_none() {
            self.status = Some(Status::Invalid);
            self.freeze();
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
        let labels = self.labels_for_structure_stack.pop().unwrap_or_default();
        if !discard {
            if let Some(parent) = self.labels_for_structure_stack.last_mut() {
                parent.extend(labels);
            } else {
                self.tags
                    .extend(labels.into_iter().map(structural_coverage));
            }
        }
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
        if self.frozen {
            return;
        }
        self.frozen = true;
        let end = self.nodes.len();
        while let Some(idx) = self.span_stack.pop() {
            if let Some(span) = self.spans.get_mut(idx) {
                span.end = end;
            }
        }
        if self.status.is_none() {
            self.status = Some(Status::Valid);
        }
        if let Some(ref mut obs) = self.observer {
            let origin = self.interesting_origin.clone();
            obs.conclude_test(self.status.unwrap(), origin);
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
            shrink_towards: 0,
        };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Integer(kind.clone()),
            || ChoiceValue::Integer(kind.simplest()),
            || ChoiceValue::Integer(kind.unit()),
            |v| matches!(v, ChoiceValue::Integer(n) if kind.validate(*n)),
            |rng| ChoiceValue::Integer(biased_integer_sample(&kind, rng)),
        )?;

        let ChoiceValue::Integer(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Integer(kind),
            value: ChoiceValue::Integer(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(v, was_forced);
        }

        Ok(v)
    }

    /// Draw a floating-point value in `[min_value, max_value]`. NaN is drawn
    /// only when `allow_nan` is set; ±∞ only when `allow_infinity` is set and
    /// the relevant endpoint is unbounded.
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

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Float(kind.clone()),
            || ChoiceValue::Float(kind.simplest()),
            || ChoiceValue::Float(kind.unit()),
            |v| matches!(v, ChoiceValue::Float(f) if kind.validate(*f)),
            |rng| ChoiceValue::Float(biased_float_sample(&kind, rng)),
        )?;

        let ChoiceValue::Float(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Float(kind),
            value: ChoiceValue::Float(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_float(v, was_forced);
        }

        Ok(v)
    }

    /// Draw a bytes value with length in `[min_size, max_size]`.
    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Result<Vec<u8>, StopTest> {
        assert!(
            min_size <= max_size,
            "min_size ({min_size}) must be <= max_size ({max_size})"
        );
        let kind = BytesChoice { min_size, max_size };

        let (value, was_forced) = self.resolve_choice(
            &ChoiceKind::Bytes(kind.clone()),
            || ChoiceValue::Bytes(kind.simplest()),
            || ChoiceValue::Bytes(kind.unit()),
            |v| matches!(v, ChoiceValue::Bytes(b) if kind.validate(b)),
            |rng| ChoiceValue::Bytes(biased_bytes_sample(&kind, rng)),
        )?;

        let ChoiceValue::Bytes(v) = value else {
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Bytes(kind),
            value: ChoiceValue::Bytes(v.clone()),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_bytes(&v, was_forced);
        }

        Ok(v)
    }

    /// Draw a boolean with probability `p` of being true.
    /// If `forced` is Some, the result is forced to that value.
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
            unreachable!("kind/value invariant violated: outer match guaranteed this variant")
        };

        self.nodes.push(ChoiceNode {
            kind: ChoiceKind::Boolean(kind),
            value: ChoiceValue::Boolean(v),
            was_forced,
        });

        if let Some(ref mut obs) = self.observer {
            obs.draw_boolean(v, was_forced);
        }

        Ok(v)
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
    /// Implements Hypothesis's punning logic for replaying choice
    /// sequences whose schema has shifted across runs.
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_tests.rs"]
mod tests;
