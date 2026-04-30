// Stateful types: NativeTestCase, ManyState, NativeVariables, Span.

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Write};
use std::sync::{LazyLock, Mutex};

use rand::RngExt;
use rand::rngs::SmallRng;

use super::choices::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
    InterestingOrigin, Status, StopTest, StringChoice,
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

/// Interesting string constants seeded from Hypothesis's GLOBAL_CONSTANTS
/// (providers.py `_constant_strings`): logic keywords, numeric edge cases,
/// common Unicode stress strings.  Stored as codepoint vectors so they can
/// be validated against and inserted into the draw_string nasty pool.
static GLOBAL_CONSTANTS_STRINGS: LazyLock<Vec<Vec<u32>>> = LazyLock::new(|| {
    let strings: &[&str] = &[
        // strings interpretable as code / logic
        "undefined",
        "null",
        "NULL",
        "nil",
        "NIL",
        "true",
        "false",
        "True",
        "False",
        "TRUE",
        "FALSE",
        "None",
        "none",
        "if",
        "then",
        "else",
        "__dict__",
        "__proto__",
        // strings interpretable as numbers
        "0",
        "1e100",
        "0..0",
        "0/0",
        "1/0",
        "+0.0",
        "Infinity",
        "-Infinity",
        "Inf",
        "INF",
        "NaN",
        "999999999999999999999999999999",
        // common ASCII punctuation / special chars
        ",./;'[]\\-=<>?:\"{}|_+!@#$%^&*()`~",
        // common Unicode characters
        "Ω≈ç√∫˜µ≤≥÷åß∂ƒ©˙∆˚¬…æœ∑´®†¥¨ˆøπ\u{201C}\u{2018}¡™£¢∞§¶•ªº–≠¸˛Ç◊ı˜Â¯˘¿ÅÍÎÏ˝ÓÔÒÚÆ☃Œ„´‰ˇÁ¨ˆØ∏\u{201D}\u{2019}`⁄€‹›ﬁﬂ‡°·‚—±",
        // characters that increase in length when lowercased
        "Ⱥ",
        "Ⱦ",
        // ligatures
        "æœÆŒﬀʤʨß",
        // emoticons
        "(╯°□°）╯︵ ┻━┻)",
        // emojis
        "😍",
        "🇺🇸",
        "🏻",
        "👍🏻",
        // RTL text
        "الكل في المجمو عة",
        // Ogham text
        "᚛ᚄᚓᚐᚋᚒᚄ ᚑᚄᚂᚑᚏᚅ᚜",
        // Thai consonant + spacing vowel
        "กา",
        "ก ำกำ",
        // mathematical bold/fraktur/script text
        "𝐓𝐡𝐞 𝐪𝐮𝐢𝐜𝐤 𝐛𝐫𝐨𝐰𝐧 𝐟𝐨𝐱 𝐣𝐮𝐦𝐩𝐬 𝐨𝐯𝐞𝐫 𝐭𝐡𝐞 𝐥𝐚𝐳𝐲 𝐝𝐨𝐠",
        "𝕿𝖍𝖊 𝖖𝖚𝖎𝖈𝖐 𝖇𝖗𝖔𝖜𝖓 𝖋𝖔𝖝 𝖏𝖚𝖒𝖕𝖘 𝖔𝖛𝖊𝖗 𝖙𝖍𝖊 𝖑𝖆𝖟𝖞 𝖉𝖔𝖌",
        "𝑻𝒉𝒆 𝒒𝒖𝒊𝒄𝒌 𝒃𝒓𝒐𝒘𝒏 𝒇𝒐𝒙 𝒋𝒖𝒎𝒑𝒔 𝒐𝒗𝒆𝒓 𝒕𝒉𝒆 𝒍𝒂𝒛𝒚 𝒅𝒐𝒈",
        "𝓣𝓱𝓮 𝓺𝓾𝓲𝓬𝓴 𝓫𝓻𝓸𝔀𝓷 𝓯𝓸𝔁 𝓳𝓾𝓶𝓹𝓼 𝓸𝓿𝓮𝓻 𝓽𝓱𝓮 𝓵𝓪𝔃𝔂 𝓭𝓸𝓰",
        "𝕋𝕙𝕖 𝕢𝕦𝕚𝕔𝕜 𝕓𝕣𝕠𝕨𝕟 𝕗𝕠𝕩 𝕛𝕦𝕞𝕡𝕤 𝕠𝕧𝕖𝕣 𝕥𝕙𝕖 𝕝𝕒𝕫𝕪 𝕕𝕠𝕘",
        // upside-down text
        "ʇǝɯɐ ʇᴉs ɹolop ɯnsdᴉ ɯǝɹo˥",
        // Windows reserved names
        "NUL",
        "COM1",
        "LPT1",
        // Scunthorpe problem
        "Scunthorpe",
        // zalgo text
        "Ṱ̺̺̕o͞ ̷i̲̬͇̪͙n̝̗͕v̟̜̘̦͟o̶̙̰̠kè͚̮̺̪̹̱̤ ̖t̝͕̳̣̻̪͞h̼͓̲̦̳̘̲e͇̣̰̦̬͎ ̢̼̻̱̘h͚͎͙̜̣̲ͅi̦̲̣̰̤v̻͍e̺̭̳̪̰-m̢iͅn̖̺̞̲̯̰d̵̼̟͙̩̼̘̳ ̞̥̱̳̭r̛̗̘e͙p͠r̼̞̻̭̗e̺̠̣͟s̘͇̳͍̝͉e͉̥̯̞̲͚̬͜ǹ̬͎͎̟̖͇̤t͍̬̤͓̼̭͘ͅi̪̱n͠g̴͉ ͏͉ͅc̬̟h͡a̫̻̯͘o̫̟̖͍̙̝͉s̗̦̲.̨̹͈̣",
        // examples from https://faultlore.com/blah/text-hates-you/
        "मनीष منش",
        "पन्ह पन्ह त्र र्च कृकृ ड्ड न्हृे إلا بسم الله",
        "lorem لا بسم الله ipsum 你好1234你好",
        // unconditional Unicode line-break characters (UAX #14)
        "a\u{000A}b\u{000D}c\u{0085}d\u{000B}e\u{000C}f\u{2028}g\u{2029}h\u{000D}\u{000A}i",
    ];
    strings
        .iter()
        .map(|s| s.chars().map(|c| c as u32).collect::<Vec<u32>>())
        .collect()
});

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
    fn draw_string(&mut self, _value: &str, _was_forced: bool) {}
    fn conclude_test(&mut self, _status: Status, _origin: Option<InterestingOrigin>) {}
}

/// Snapshot of a completed `NativeTestCase`'s observable state.
///
/// Mirrors the relevant subset of Hypothesis's `ConjectureResult`
/// (`internal/conjecture/data.py`).  Returned from
/// [`NativeTestCase::as_result`] for any test case that did not overrun.
#[derive(Clone, Debug)]
pub struct NativeConjectureResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub length: usize,
    pub output: String,
    pub has_discards: bool,
    pub spans: Spans,
}

/// Result of executing a `NativeTestCase`.
///
/// Mirrors Hypothesis's `ConjectureResult | _Overrun` union returned from
/// `ConjectureData.as_result()` (`internal/conjecture/data.py`).
/// `NativeResult::Overrun` is the analog of Hypothesis's `_Overrun`
/// singleton: the test case ran out of buffer (`Status::EarlyStop`)
/// before completing.  `NativeResult::Conjecture` carries an immutable
/// snapshot of the completed test case.
#[derive(Clone, Debug)]
pub enum NativeResult {
    Overrun,
    Conjecture(NativeConjectureResult),
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
    /// Free-form text accumulated by `note()` calls during a run.  Mirrors
    /// `ConjectureData.output`: the runner surfaces this in counterexample
    /// reproducers so user-visible context survives shrinking.
    output: String,
    /// User-defined event labels, mirroring `ConjectureData.events`.  Hegel
    /// hands callers mutable access via [`Self::events_mut`] so they can
    /// stash arbitrary key/value annotations the way Hypothesis tests do
    /// with `data.events[key] = value`.
    events: HashMap<String, String>,
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
    /// Per-test-case targeting observations: maps label to score.
    /// Populated by `target_observation` calls from the test body.
    pub target_observations: HashMap<String, f64>,
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
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            output: String::new(),
            events: HashMap::new(),
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
            target_observations: HashMap::new(),
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
            force_simplest: false,
            nodes: Vec::new(),
            status: None,
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            output: String::new(),
            events: HashMap::new(),
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer,
            interesting_origin: None,
            target_observations: HashMap::new(),
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
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            output: String::new(),
            events: HashMap::new(),
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
            target_observations: HashMap::new(),
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
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            output: String::new(),
            events: HashMap::new(),
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
            target_observations: HashMap::new(),
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
            frozen: false,
            collections: HashMap::new(),
            next_collection_id: 0,
            variable_pools: Vec::new(),
            spans: Spans::new(),
            span_stack: Vec::new(),
            has_discards: false,
            output: String::new(),
            events: HashMap::new(),
            tags: HashSet::new(),
            labels_for_structure_stack: Vec::new(),
            observer: None,
            interesting_origin: None,
            target_observations: HashMap::new(),
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

    /// Whether the test case has been frozen and may no longer accept draws.
    ///
    /// Hypothesis's `ConjectureData.frozen` flag is its own boolean; the
    /// native engine collapses that flag onto the post-completion
    /// `status` value, so any non-`None` status means the test case is
    /// frozen.
    pub fn frozen(&self) -> bool {
        self.frozen
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

    /// Explicitly conclude the test case with a given status and optional
    /// interesting origin, then raise `StopTest`.
    ///
    /// Mirrors `ConjectureData.conclude_test(status, interesting_origin)` from
    /// `hypothesis.internal.conjecture.data`: sets the status and origin on the
    /// test case, calls `freeze()` (which notifies the observer), then returns
    /// `Err(StopTest)` so callers can propagate the stop signal with `?`.
    pub fn conclude_test(
        &mut self,
        status: Status,
        origin: Option<InterestingOrigin>,
    ) -> Result<(), StopTest> {
        self.status = Some(status);
        self.interesting_origin = origin;
        self.freeze();
        Err(StopTest)
    }

    /// Mark the test case as invalid, optionally recording why.
    ///
    /// Native analog of Hypothesis's `ConjectureData.mark_invalid(reason)`:
    /// records the reason in events (under `"invalid because"`) and concludes
    /// the test with `Status::Invalid`, returning `Err(StopTest)`.
    ///
    /// This is the draw-by-strategy result for a `nothing()`-equivalent
    /// strategy: a strategy that can never produce a value marks the test case
    /// invalid rather than panicking (unlike `NativeConjectureData::mark_invalid`,
    /// which panics to signal the runner).
    pub fn mark_invalid(&mut self, why: Option<String>) -> Result<(), StopTest> {
        if let Some(reason) = why {
            self.events_mut()
                .insert("invalid because".to_string(), reason);
        }
        self.conclude_test(Status::Invalid, None)
    }

    /// Snapshot the test case as a [`NativeResult`].
    ///
    /// Mirrors `ConjectureData.as_result()` from
    /// `hypothesis-python/.../conjecture/data.py`: an `EarlyStop` test
    /// case (Hypothesis's `Status.OVERRUN`) becomes
    /// [`NativeResult::Overrun`]; anything else returns a snapshot of
    /// the completed run.  For an in-progress test case (no terminal
    /// status set), the snapshot reports `Status::Valid` to match
    /// Hypothesis's behaviour where `as_result()` is only called after
    /// `freeze()` has settled the status.
    pub fn as_result(&self) -> NativeResult {
        if self.status == Some(Status::EarlyStop) {
            NativeResult::Overrun
        } else {
            NativeResult::Conjecture(NativeConjectureResult {
                status: self.status.unwrap_or(Status::Valid),
                nodes: self.nodes.clone(),
                length: self.nodes.len(),
                output: self.output.clone(),
                has_discards: self.has_discards,
                spans: self.spans.clone(),
            })
        }
    }

    /// Append `value`'s `Debug` rendering to [`Self::output`].  Mirrors
    /// `ConjectureData.note` for non-string inputs (`format!("{:?}", x)` is
    /// the closest Rust analog of Python's `repr`); use [`Self::note_str`]
    /// for the verbatim string case.
    pub fn note<T: Debug>(&mut self, value: T) {
        let _ = write!(&mut self.output, "{value:?}");
    }

    /// Append `value` verbatim to [`Self::output`], without surrounding
    /// quotes or escaping.  Mirrors `ConjectureData.note(<str>)`, which
    /// short-circuits the `repr()` formatting for `str` inputs.
    pub fn note_str(&mut self, value: &str) {
        self.output.push_str(value);
    }

    /// Read-only view of the text accumulated by `note*` calls.
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Mutable access to the event annotations map.  Mirrors
    /// `ConjectureData.events`: tests stash arbitrary key/value pairs via
    /// `data.events[key] = value`, and the runner surfaces them in
    /// post-run statistics.
    pub fn events_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.events
    }

    /// Read-only view of the event annotations map.
    pub fn events(&self) -> &HashMap<String, String> {
        &self.events
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
                for &v in GLOBAL_CONSTANTS_INTEGERS.iter() {
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

        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(v, was_forced);
        }

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

        if let Some(ref mut obs) = self.observer {
            obs.draw_boolean(v, was_forced);
        }

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

        if let Some(ref mut obs) = self.observer {
            obs.draw_float(v, was_forced);
        }

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

        if let Some(ref mut obs) = self.observer {
            obs.draw_bytes(&v, was_forced);
        }

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
        // counterexamples), plus GLOBAL_CONSTANTS strings that satisfy the
        // current constraint.
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
            for cps in GLOBAL_CONSTANTS_STRINGS.iter() {
                if kind.validate(cps) && !v.contains(cps) {
                    v.push(cps.clone());
                }
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
        let s = codepoints_to_string(&v);
        if let Some(ref mut obs) = self.observer {
            obs.draw_string(&s, was_forced);
        }
        Ok(s)
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
        if let Some(ref mut obs) = self.observer {
            obs.draw_integer(forced, true);
        }
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
        if let Some(ref mut obs) = self.observer {
            obs.draw_float(forced, true);
        }
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
        if let Some(ref mut obs) = self.observer {
            obs.draw_bytes(&forced, true);
        }
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
        let s = codepoints_to_string(&codepoints);
        if let Some(ref mut obs) = self.observer {
            obs.draw_string(&s, true);
        }
        Ok(s)
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
