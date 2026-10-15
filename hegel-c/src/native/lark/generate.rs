//! Generating strings from an analysed grammar.
//!
//! A string drawn from a grammar is one choice of the test case: the walk
//! below draws every decision from the test case's RNG alone, and the
//! string is recorded under the grammar's constraint (see
//! [`super::constraint`]), so that a replay checks a stored string by
//! parsing it and the shrinker works on its derivation rather than on
//! the decisions that produced it.
//!
//! Every draw first picks a target length: with probability 3/4 a length
//! uniform between the smallest string the start rule can produce and
//! the size cap, otherwise the smallest. The grammar is then walked top
//! down with a character budget, in which every terminal counts for the
//! length of its strings on average ([`CompiledRegex::typical_width`]), so
//! that the structure a budget buys comes out near the budget. The
//! budget is the target less what the `%ignore` insertions between
//! tokens will add at the string's insertion rate
//! ([`ignore_cost`]). A rule's
//! alternatives are split into those that can grow without bound and
//! those that cannot, a weighted boolean whose probability rises with
//! the budget left over after the cheapest growing alternative chooses
//! between the two classes,
//! and the alternative is drawn uniformly from the class; a sequence
//! splits its spare budget between its flexible items at uniformly drawn
//! cut points, so no position is systematically larger than another; a
//! repetition of elements that can grow draws a log-uniform count that
//! fits its budget and shares the budget equally between the elements,
//! and one of elements that cannot grow takes as many as its budget buys
//! (or any count the grammar allows, when the grammar allows fewer), so
//! that a string of `"a"+` comes out near the target. Budgets steer
//! the structure only: each terminal is drawn by its own regex generator
//! (redrawn a few times when it alone overruns the size cap), and a
//! string that overruns the cap regardless is discarded and regenerated
//! with half the target, until [`MAX_ATTEMPTS`] strings have been
//! discarded and the test case is rejected. A budget never rules an
//! alternative out: a rule none of whose alternatives fit theirs (the
//! depth cap can leave only costlier ones) takes the cheapest the depth
//! allows.
//! A separate work limit ([`work_limit`]) bounds the nodes the walk
//! visits even when repetitions and nested rules derive no text;
//! exhausting it rejects the test case.
//!
//! Between adjacent tokens the generator inserts a `%ignore` terminal
//! either when the simulated lexer says the two would otherwise lex
//! differently, or at random with a rate drawn once per string. Each
//! terminal string is checked to lex back as itself, and each inserted
//! ignorable to be skipped between its neighbours, with a few retries
//! before the string is accepted as it is. A token of an ignored
//! terminal that a rule itself draws is one the parser needs, and the
//! checks treat it as any other token (see [`Token::needed`]): only the
//! inserted ones may be skipped or run through. Each terminal is drawn
//! on its own, with the text around it out of its pattern's sight, so a
//! terminal whose pattern asserts on the text around it (a lookbehind
//! at its start, a lookahead or `\B` at its end) never lexes back and
//! cannot be generated; the grammar is reported as having no string.
//! Every token keeps what the
//! lexers found at the start of its text on its own ([`TokenScans`]),
//! with how that depended on the text's edges, and most stretches of
//! tokens are settled from those findings alone ([`GrammarSpec::settled`]):
//! each token reads as itself, nothing about it looks at the text before
//! it, and what the end of its text left open the next token closes
//! without changing the reading. The rest are lexed in full; those checks
//! look at adjacent tokens only, and a backtracking regex can run on much
//! further (an escaped delimiter it prefers to read as such, say, that
//! then lets it reach the next delimiter in the string), so a finished
//! string with such a stretch is lexed once more as a whole and
//! regenerated when it does not read back as its tokens.
//!
//! A terminal whose strings come from outside the engine (an explicit
//! one, see [`GrammarOptions::explicit`]) stands in a drawn string as a
//! placeholder: a fixed marker, [`placeholder`], that the caller
//! replaces with a value of its own at each occurrence, in order. The
//! marker is a string terminal of the compiled grammar, above every
//! other in priority so that the lexers read it as itself wherever it
//! starts, and is what the parser matches on replay. The value spliced
//! in is out of the engine's sight, so a grammar with an ignorable has
//! one between every marker and any token beside it (a marker at an
//! edge of the string has nothing on that side): the generator insists
//! on it, and the constraint accepts no string without (see
//! [`GrammarConstraint::tree`]), so that shrinking and mutation keep it.
//! Keeping the values lexable in place is the caller's affair, and so
//! is their size: a marker counts for nothing toward the size cap, which
//! is on the characters outside the markers, and a caller whose values
//! overran the cap it has in mind draws again under a smaller one of its
//! own (the `max_size` of [`generate_grammar`]).

use super::analysis::{Alt, Analysis, Node, NodeKind, analyze};
use super::builder::GrammarFiles;
#[cfg(test)]
use super::builder::ImportSource;
use super::compile::{Grammar, RuleId, TermId, TerminalDef, compile, placeholder};
use super::constraint::GrammarConstraint;
use super::lexsim::{Lexed, Lexer, Scan, Starters};
use super::{GrammarError, GrammarResult};
use crate::control::hegel_internal_unwrap;
use crate::native::HashMap;
use crate::native::core::Constraint;
use crate::native::core::state::boolean_threshold;
use crate::native::core::{EngineError, NativeTestCase, Status, StringChoice};
use crate::native::draws::regex::{CompiledRegex, describe_alphabet, generate_regex_into};
use crate::native::draws::source::{RngDraws, SimplestDraws};
use crate::native::draws::{Draws, LABEL_REGEX, StringSpec, spanned};
use crate::native::labels::label_from_name;
use crate::native::rng::EngineRng;
use crate::sys::sync::Mutex;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::hash::Hasher;
use hashbrown::HashTable;
use rustc_hash::FxHasher;

/// The default cap on the length of a generated string, in characters.
pub const DEFAULT_MAX_SIZE: u64 = 8192;

/// The most characters a grammar's strings may be allowed: the generator
/// aims at the cap, so a cap without bound would have it fill memory.
pub const MAX_MAX_SIZE: u64 = 1 << 20;

/// The default cap on how deeply rules nest.
pub const DEFAULT_MAX_DEPTH: u32 = 200;

/// The most deeply a grammar's rules may be allowed to nest: the walk
/// that generates a string recurses a few frames per level, and this
/// many fit in a 1 MiB stack unoptimised, half of a test thread's.
pub const MAX_MAX_DEPTH: u32 = 500;

/// How many distinct token texts a generation remembers the lexing of
/// (see [`ReadingsCache::read_token`]) before the next draw forgets
/// them all, and how many its cache has room for from the start.
const READINGS_CAPACITY: usize = 1 << 13;

/// How many characters of remembered texts a generation's cache has room
/// for from the start, and how many it holds before the next draw
/// forgets them all likewise: a typical token text is a few characters
/// long, and a grammar whose tokens are long would otherwise fill the
/// cache with them.
const READINGS_TEXT_CAPACITY: usize = READINGS_CAPACITY * 16;

/// The number of strings one draw discards, for being oversize (halving
/// its target each time) or for not lexing back as their tokens, before
/// the test case is rejected.
const MAX_ATTEMPTS: u32 = 9;

/// How often a terminal is redrawn when the basic lexer would not read it
/// back as itself before the draw is accepted anyway.
const LEX_RETRIES: u32 = 3;

/// How many ignorables an insertion tries before giving up (dropping
/// an optional one, or keeping the last when the lexer needs one),
/// how many may follow one another to keep an ignorable the lexer would
/// otherwise run into the next token (a comment, which only a newline
/// ends) apart from it, how many strings one such chain may draw, and
/// how many times drawing an ignorable after a given one in a chain
/// may fail, for each time it has succeeded, before it is given up on
/// for the rest of the string (see [`Generation::hopeful`]).
const IGNORE_ATTEMPTS: u32 = 4;
const IGNORE_CHAIN: u32 = 2;
const IGNORE_CHAIN_DRAWS: u32 = 4;
const IGNORE_CHAIN_PATIENCE: u32 = 4;

/// The probability that a draw aims for a large string rather than the
/// smallest one.
const LARGE_TARGET_PROBABILITY: f64 = 0.75;

/// The spare budget, beyond the cheapest growing alternative, at which
/// a rule is as likely to pick a growing alternative as a closing one:
/// the probability of growing is `spare / (spare + GROW_SCALE)`.
const GROW_SCALE: f64 = 1.0;

/// The number of distinct `%ignore` insertion rates a string may draw;
/// the rate is `(k / (IGNORE_RATE_STEPS - 1))^2` for `k` drawn uniformly
/// in `0..IGNORE_RATE_STEPS`, so most strings carry little optional
/// whitespace and a few are saturated with it.
const IGNORE_RATE_STEPS: u64 = 5;

/// The granularity of the log-uniform draw of a repetition's extra
/// element count.
const COUNT_RESOLUTION: u64 = 1 << 20;

/// How many seeded draws stand in for the simplest walk when that one
/// draws no string (see [`GrammarSpec::ensure_simplest`]).
const SIMPLEST_FALLBACK_SEEDS: u64 = 8;

/// The fewest nodes a draw may visit before it is given up (see
/// [`work_limit`]).
const GENERATION_BUDGET: usize = 1 << 20;

const LABEL_GRAMMAR: u64 = label_from_name("hegel.lark");

/// How the strings of one terminal are drawn.
enum TerminalGen {
    /// From the terminal's own pattern, checked against the lexer.
    Pattern(Box<CompiledRegex>),
    /// A placeholder for a terminal whose strings the caller supplies
    /// (see [`placeholder`]): its marker, as it is.
    Placeholder(Vec<char>),
}

/// Options for building a [`GrammarSpec`].
pub struct GrammarOptions {
    pub start: String,
    /// The terminals whose strings the caller supplies: each occurrence
    /// in a drawn string is the terminal's [`placeholder`] marker, for
    /// the caller to replace.
    pub explicit: Vec<String>,
    pub alphabet: Option<Arc<StringSpec>>,
    /// The most characters a string may have, a placeholder marker
    /// counting as none.
    pub max_size: u64,
    pub max_depth: u32,
}

impl Default for GrammarOptions {
    fn default() -> Self {
        GrammarOptions {
            start: String::from("start"),
            explicit: Vec::new(),
            alphabet: None,
            max_size: DEFAULT_MAX_SIZE,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

/// A grammar ready to generate from: the analysed rules, a generator per
/// terminal, the lexer simulation, the size and depth caps, and the
/// choice a drawn string is recorded as.
pub struct GrammarSpec {
    analysis: Analysis,
    terminals: Vec<TerminalGen>,
    #[cfg(test)]
    terminal_names: Vec<String>,
    /// The explicit terminals' names and markers.
    placeholders: Vec<(String, String)>,
    ignore: Vec<TermId>,
    /// Which ignorables could start with each character, for
    /// [`GrammarSpec::scans`] to try only those.
    ignore_starters: Starters,
    ignore_cost: f64,
    lexer: Lexer,
    start: RuleId,
    /// The string choice every string drawn from the grammar is recorded
    /// as: one node, over every character, constrained to the grammar,
    /// which `constraint` is.
    kind: Arc<StringChoice>,
    constraint: Arc<GrammarConstraint>,
    /// How [`Generation::choose_alt`] classes each rule's alternatives,
    /// indexed by [`RuleId`], and each group's, indexed by [`super::analysis::GroupId`];
    /// `None` for more alternatives than a mask holds.
    classes: Vec<Option<AltClasses>>,
    group_classes: Vec<Option<AltClasses>>,
    max_size: u64,
    max_depth: u32,
    /// Working space for [`GrammarSpec::scans`] and
    /// [`GrammarSpec::lex_all`], kept from one check to the next so that
    /// the walks allocate nothing.
    scratch: Mutex<ScansScratch>,
    /// The vectors of the roomiest finished [`Generation`], emptied, and
    /// what it remembers of the token texts drawn, for the next draw to
    /// work in instead of growing its own from nothing.
    buffers: Mutex<Option<Buffers>>,
}

/// The vectors a [`Generation`] works in, emptied, and the readings it
/// remembers, between one draw and the next.
struct Buffers {
    text: Vec<char>,
    tokens: Vec<Token>,
    budgets: Vec<u64>,
    candidates: Vec<TermId>,
    chain_candidates: Vec<TermId>,
    chains: HashMap<(TermId, TermId), (u32, u32)>,
    readings: ReadingsCache,
}

impl Buffers {
    /// Empty buffers.
    fn new() -> Buffers {
        Buffers {
            text: Vec::new(),
            tokens: Vec::new(),
            budgets: Vec::new(),
            candidates: Vec::new(),
            chain_candidates: Vec::new(),
            chains: HashMap::default(),
            readings: ReadingsCache::new(),
        }
    }

    /// Put these buffers in `slot` when they have room for more text than
    /// the ones there, handing back whichever set is not kept.
    fn offer(self, slot: &mut Option<Buffers>) -> Option<Buffers> {
        match slot {
            Some(held) if held.text.capacity() >= self.text.capacity() => Some(self),
            _ => slot.replace(self),
        }
    }
}

/// The vectors [`GrammarSpec::scans`] works in: where each token lies,
/// which tokens the parser needs, the positions the lexer can reach and
/// the terminals tried at the position being walked; and the tokens
/// [`GrammarSpec::lex_all`] observes, with their extents, and what
/// [`GrammarSpec::basic_fit`] hands it and holds them against.
#[derive(Default)]
struct ScansScratch {
    bounds: Vec<(usize, usize)>,
    significant: Vec<usize>,
    reachable: Vec<bool>,
    tried: Vec<(TermId, Lexed)>,
    observed: Vec<(TermId, usize, usize)>,
    known: Vec<(usize, usize, usize)>,
    regions: Vec<(usize, usize, usize)>,
}

/// One text remembered as a token of terminal `id`, which lies at
/// `start..end` of the cache's `texts`, with the hash it is held under
/// (see [`text_hash`]), so that the table can grow without hashing its
/// texts again, and where in the cache's `findings` what the lexers
/// found in it is.
struct Remembered {
    hash: u64,
    id: TermId,
    start: usize,
    end: usize,
    scans: usize,
}

/// What the lexers found in each distinct token text drawn so far, by
/// terminal, so that a keyword, an operator or a stretch of whitespace
/// drawn again, in the same string or a later one, is lexed once. A
/// generation's own (see [`Buffers`]), so that its tokens refer to the
/// findings by index (see [`Token::scans`]) and the cache is emptied
/// only between one draw and the next, when no token does, once it
/// holds too many texts or too many characters of them (see
/// [`ReadingsCache::forget_when_full`]). The table and the vectors are
/// made with room for [`READINGS_CAPACITY`] texts (of
/// [`READINGS_TEXT_CAPACITY`] characters in all), so that filling the
/// cache moves nothing it holds.
struct ReadingsCache {
    /// The remembered texts by terminal and text (see [`text_hash`]).
    table: HashTable<Remembered>,
    /// The remembered texts, one after another, so that remembering a
    /// text allocates nothing of its own.
    texts: Vec<char>,
    /// What the lexers found in each remembered text, one after another
    /// likewise.
    findings: Vec<TokenScans>,
    len: usize,
}

/// The hash a text drawn as a token of terminal `id` is held under in
/// [`ReadingsCache`]: of the terminal, then of the text's characters two
/// at a time, since the hasher takes a word per write.
fn text_hash(id: TermId, text: &[char]) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write_usize(id);
    let mut pairs = text.chunks_exact(2);
    for pair in &mut pairs {
        hasher.write_u64(u64::from(pair[0]) << 32 | u64::from(pair[1]));
    }
    if let [last] = pairs.remainder() {
        hasher.write_u32(u32::from(*last));
    }
    hasher.finish()
}

impl ReadingsCache {
    /// An empty cache with room for its capacity.
    fn new() -> ReadingsCache {
        ReadingsCache {
            table: HashTable::with_capacity(READINGS_CAPACITY),
            texts: Vec::with_capacity(READINGS_TEXT_CAPACITY),
            findings: Vec::with_capacity(READINGS_CAPACITY),
            len: 0,
        }
    }

    /// Forget everything when the cache holds [`READINGS_CAPACITY`]
    /// texts or more, or [`READINGS_TEXT_CAPACITY`] characters of them
    /// or more, for a draw about to start, while no token refers to the
    /// findings.
    fn forget_when_full(&mut self) {
        if self.len >= READINGS_CAPACITY || self.texts.len() >= READINGS_TEXT_CAPACITY {
            self.table.clear();
            self.texts.clear();
            self.findings.clear();
            self.len = 0;
        }
    }

    /// Where in `findings` what was remembered of `text`, whose
    /// [`text_hash`] is `hash`, as a token of terminal `id` is.
    fn find(&self, id: TermId, hash: u64, text: &[char]) -> Option<usize> {
        let texts = &self.texts;
        self.table
            .find(hash, |held| {
                held.id == id && texts[held.start..held.end] == *text
            })
            .map(|held| held.scans)
    }

    /// Remember `scans` for `text`, whose [`text_hash`] is `hash`, as a
    /// token of terminal `id`, which nothing is remembered for yet (see
    /// [`ReadingsCache::recall`]): where in `findings` it is.
    fn remember(&mut self, id: TermId, hash: u64, text: &[char], scans: TokenScans) -> usize {
        let at = self.findings.len();
        self.findings.push(scans);
        let start = self.texts.len();
        self.texts.extend_from_slice(text);
        self.table.insert_unique(
            hash,
            Remembered {
                hash,
                id,
                start,
                end: self.texts.len(),
                scans: at,
            },
            |held| held.hash,
        );
        self.len += 1;
        at
    }

    /// The scans and reading of a token of terminal `id` with `text`,
    /// from the ones remembered when the text was drawn before, else
    /// computed for `spec` (with `own` for the terminal's own scan when
    /// the caller already has it) and remembered: where in `findings`
    /// they are.
    fn read_token(
        &mut self,
        spec: &GrammarSpec,
        id: TermId,
        text: &[char],
        own: Option<Scan>,
    ) -> usize {
        match self.recall(id, text) {
            Ok(scans) => scans,
            Err(hash) => self.learn(spec, id, hash, text, own),
        }
    }

    /// Where in `findings` the scans and reading remembered for a token
    /// of terminal `id` with `text` from when the text was drawn before
    /// are, or, when it was not, the text's hash, for
    /// [`ReadingsCache::learn`].
    fn recall(&self, id: TermId, text: &[char]) -> Result<usize, u64> {
        let hash = text_hash(id, text);
        self.find(id, hash, text).ok_or(hash)
    }

    /// Compute, for `spec`, the scans and reading of a token of terminal
    /// `id` with `text`, whose [`text_hash`] is `hash` and which
    /// [`ReadingsCache::recall`] found nothing for, with `own` for the
    /// terminal's own scan when the caller already has it, and remember
    /// them: where in `findings` they are.
    fn learn(
        &mut self,
        spec: &GrammarSpec,
        id: TermId,
        hash: u64,
        text: &[char],
        own: Option<Scan>,
    ) -> usize {
        let scans = spec.scan_token(id, text, own);
        self.remember(id, hash, text, scans)
    }
}

impl GrammarSpec {
    /// Build a spec from a compiled grammar whose start rules are exactly
    /// the one named in `options`.
    pub fn new(mut grammar: Grammar, options: GrammarOptions) -> GrammarResult<GrammarSpec> {
        let start = grammar.start[0];
        let alphabet = match options.alphabet.as_deref() {
            None => None,
            Some(StringSpec::Text { intervals, .. }) => Some((**intervals).clone()),
            Some(_) => {
                return Err(GrammarError::new(String::from(
                    "alphabet must be a text string generator",
                )));
            }
        };
        let mut placeholders = Vec::with_capacity(options.explicit.len());
        let mut markers: Vec<(TermId, Vec<char>)> = Vec::with_capacity(options.explicit.len());
        for name in &options.explicit {
            let Some(id) = grammar.terminal_id(name) else {
                return Err(GrammarError::new(format!(
                    "the grammar does not use explicit terminal '{name}'"
                )));
            };
            if markers.iter().any(|(seen, _)| *seen == id) {
                return Err(GrammarError::new(format!(
                    "explicit terminal '{name}' is listed more than once"
                )));
            }
            let marker = placeholder(name);
            grammar.terminals[id] = TerminalDef::placeholder(name)?;
            markers.push((id, marker.chars().collect()));
            placeholders.push((name.clone(), marker));
        }
        let mut terminals = Vec::with_capacity(grammar.terminals.len());
        let mut term_min = Vec::with_capacity(grammar.terminals.len());
        let mut term_typical = Vec::with_capacity(grammar.terminals.len());
        for (id, term) in grammar.terminals.iter().enumerate() {
            let marker = markers.iter().find(|(m, _)| *m == id).map(|(_, m)| m);
            match (marker, &term.regexp) {
                (Some(marker), _) => {
                    term_min.push(0);
                    term_typical.push(0);
                    terminals.push(TerminalGen::Placeholder(marker.clone()));
                }
                (None, Some(regexp)) => {
                    let compiled = CompiledRegex::compile(regexp, alphabet.clone())
                        .map_err(|e| GrammarError::new(format!("{e}")))?;
                    term_min.push(term.min_width);
                    term_typical.push(typical_size(&compiled, term.min_width));
                    terminals.push(TerminalGen::Pattern(Box::new(compiled)));
                }
                (None, None) => {
                    return Err(GrammarError::new(format!(
                        "Terminal '{}' is declared but has no definition: supply a generator \
                         for it with explicit",
                        term.name
                    )));
                }
            }
        }
        let ignore_cost = ignore_cost(&grammar.ignore, &term_typical);
        let analysis = analyze(&grammar, term_min, term_typical)?;
        if let Some(alphabet) = &alphabet {
            let mut used = reachable_terminals(&analysis, start, terminals.len());
            for &id in &grammar.ignore {
                used[id] = true;
            }
            let unproducible = used.iter().enumerate().find(|&(id, &used)| {
                used && matches!(&terminals[id], TerminalGen::Pattern(re) if !re.producible())
            });
            if let Some((id, _)) = unproducible {
                let term = &grammar.terminals[id];
                return Err(GrammarError::new(format!(
                    "terminal {} ({}) cannot be produced over the alphabet {}",
                    term.name,
                    term.regexp.as_deref().unwrap_or_default(),
                    describe_alphabet(alphabet)
                )));
            }
        }
        let lexer = Lexer::new(&grammar.terminals);
        let ignore_starters = Starters::new(&lexer, &grammar.ignore);
        let root = &analysis.rules[start];
        if options.max_size > MAX_MAX_SIZE {
            return Err(GrammarError::new(format!(
                "max_size is {} but may be at most {MAX_MAX_SIZE}",
                options.max_size
            )));
        }
        if options.max_size < root.min_size {
            return Err(GrammarError::new(format!(
                "max_size is {} but the smallest string '{}' can produce has {} characters",
                options.max_size, root.name, root.min_size
            )));
        }
        if options.max_depth > MAX_MAX_DEPTH {
            return Err(GrammarError::new(format!(
                "max_depth is {} but may be at most {MAX_MAX_DEPTH}",
                options.max_depth
            )));
        }
        if options.max_depth < root.min_depth {
            return Err(GrammarError::new(format!(
                "max_depth is {} but the shallowest string '{}' can produce nests {} rules deep",
                options.max_depth, root.name, root.min_depth
            )));
        }
        let classes = analysis
            .rules
            .iter()
            .map(|rule| AltClasses::of(&rule.alternatives))
            .collect();
        let group_classes = group_classes(&analysis);
        let internal = |e: crate::control::InternalError| GrammarError::new(format!("{e}"));
        let max_size = usize::try_from(options.max_size).unwrap_or(usize::MAX);
        #[cfg(test)]
        let terminal_names = grammar.terminals.iter().map(|t| t.name.clone()).collect();
        let ignore = grammar.ignore.clone();
        let constraint = Arc::new(
            GrammarConstraint::new(
                grammar,
                start,
                max_size,
                options.max_depth,
                alphabet.as_ref(),
            )
            .map_err(internal)?,
        );
        let kind = Arc::new(StringChoice {
            intervals: Arc::clone(constraint.intervals()),
            min_size: 0,
            max_size: if placeholders.is_empty() {
                max_size
            } else {
                usize::MAX
            },
            constraint: Some(Constraint::Grammar(Arc::clone(&constraint))),
        });
        let spec = GrammarSpec {
            analysis,
            terminals,
            #[cfg(test)]
            terminal_names,
            placeholders,
            ignore,
            ignore_starters,
            ignore_cost,
            lexer,
            start,
            kind,
            constraint,
            classes,
            group_classes,
            max_size: options.max_size,
            max_depth: options.max_depth,
            scratch: Mutex::new(ScansScratch::default()),
            buffers: Mutex::new(None),
        };
        Ok(spec)
    }

    /// The grammar constraint the strings drawn are recorded under.
    #[cfg(test)]
    pub fn constraint(&self) -> &Arc<GrammarConstraint> {
        &self.constraint
    }

    /// The simplest string of the grammar, for its constraint, computed
    /// at the first draw: the smallest string with every decision at its
    /// simplest (see [`SimplestDraws`]), or, when that walk draws nothing
    /// because the string it heads for cannot be lexed back, the first
    /// string a few fixed seeded random walks draw — of the smallest size
    /// first, then of any size — so that the choice is the same in every
    /// run. Each is checked to derive under the constraint (see
    /// [`GrammarSpec::derivable`]) before it is taken: the walk's lexing
    /// checks approximate the parser, and the parser decides. A grammar
    /// none of them draws from cannot be drawn from at all.
    pub fn ensure_simplest(&self) -> Result<&[u32], EngineError> {
        self.constraint.simplest_or_init(|| {
            if let Some(simplest) = smallest_string(&mut SimplestDraws::default(), self)
                .ok()
                .and_then(|string| self.derivable(string))
            {
                return Ok(simplest);
            }
            if let Some(simplest) = self
                .seeded_string(true)
                .or_else(|| self.seeded_string(false))
            {
                return Ok(simplest);
            }
            Err(EngineError::InvalidArgument(format!(
                "no string of '{}' could be generated: its strings exceed the generation work limit, \
                 do not fit the size cap, \
                 do not lex back as their tokens, or a terminal asserts on the text around it \
                 (a lookbehind at its start, a lookahead or `\\B` at its end)",
                self.analysis.rules[self.start].name
            )))
        })
    }

    /// The first string of [`SIMPLEST_FALLBACK_SEEDS`] seeded random
    /// walks that draws one the constraint derives: walks for a string
    /// of the smallest size when `smallest`, otherwise ordinary draws.
    fn seeded_string(&self, smallest: bool) -> Option<Vec<u32>> {
        for seed in 0..SIMPLEST_FALLBACK_SEEDS {
            let mut rng = EngineRng::seeded(seed);
            let mut draws = RngDraws::new(&mut rng, Default::default());
            let drawn = if smallest {
                smallest_string(&mut draws, self)
            } else {
                draw_grammar(&mut draws, self, self.max_size)
            };
            if let Some(string) = drawn.ok().and_then(|string| self.derivable(string)) {
                return Some(string);
            }
        }
        None
    }

    /// `string` when the constraint derives it, parsing it now.
    fn derivable(&self, string: Vec<u32>) -> Option<Vec<u32>> {
        self.constraint.tree(&string).is_some().then_some(string)
    }

    /// The smallest string the grammar can produce, in characters.
    pub fn min_size(&self) -> u64 {
        self.analysis.rules[self.start].min_size
    }

    /// The names of the terminals, indexed by [`TermId`].
    #[cfg(test)]
    pub fn terminal_names(&self) -> &[String] {
        &self.terminal_names
    }

    /// The explicit terminals' names, each with the marker standing for
    /// its values in a drawn string (see [`placeholder`]).
    pub fn placeholders(&self) -> &[(String, String)] {
        &self.placeholders
    }

    fn is_placeholder(&self, id: TermId) -> bool {
        matches!(self.terminals[id], TerminalGen::Placeholder(_))
    }

    fn is_ignore(&self, id: TermId) -> bool {
        self.ignore.contains(&id)
    }

    fn same_type(&self, expected: TermId, got: TermId) -> bool {
        expected == got || (self.is_ignore(expected) && self.is_ignore(got))
    }

    /// Whether the basic lexer reads `token`'s text on its own as one
    /// token: of its own type, or for a token the grammar needs, of
    /// another type (an inserted ignorable read as something the parser
    /// would see is not skipped, see [`GrammarSpec::basic_fit`]).
    fn single(&self, token: &Token, findings: &[TokenScans]) -> bool {
        match token.reading(findings) {
            Reading::Itself => true,
            Reading::Tokens(read) => read.len() == 1 && token.needed,
            Reading::Ignorables | Reading::Opaque => false,
        }
    }

    /// The tokens the basic lexer reads `text` as, with their extents,
    /// into `observed`. `known` lists, in order, where the tokens that
    /// make up `text` start, how long each is and where in `findings`
    /// what the lexer found at its start on its own is, for the lexer to
    /// carry over where it arrives at such a start (see
    /// [`Lexer::extend`]).
    fn lex_all(
        &self,
        text: &[char],
        known: &[(usize, usize, usize)],
        findings: &[TokenScans],
        observed: &mut Vec<(TermId, usize, usize)>,
    ) -> Result<(), Lexed> {
        observed.clear();
        self.lex_from(text, 0, known, findings, observed)
    }

    /// [`lex_all`](Self::lex_all) from `pos` on, after the tokens
    /// `observed` already holds.
    fn lex_from(
        &self,
        text: &[char],
        mut pos: usize,
        known: &[(usize, usize, usize)],
        findings: &[TokenScans],
        observed: &mut Vec<(TermId, usize, usize)>,
    ) -> Result<(), Lexed> {
        let mut k = 0;
        while pos < text.len() {
            while k < known.len() && known[k].0 < pos {
                k += 1;
            }
            let lexed = match known.get(k) {
                Some(&(start, len, scans)) if start == pos => {
                    self.lexer
                        .extend(&findings[scans].basic, text, pos, len, true)
                }
                _ => None,
            }
            .unwrap_or_else(|| self.lexer.next_token(text, pos));
            match lexed {
                Lexed::Token(id, end) => {
                    observed.push((id, pos, end));
                    pos = end;
                }
                other => return Err(other),
            }
        }
        Ok(())
    }

    /// How much of `text`, which terminal `id`'s pattern matches in full,
    /// the pattern takes when matched from the front: less than all of
    /// it after a lazy repeat or an earlier branch of an alternation, in
    /// which case no lexer reads the text back whole but every one reads
    /// back that prefix, itself a string the terminal matches. Also what
    /// the match found, for [`GrammarSpec::scan_token`] when nothing is
    /// cut off.
    fn whole(&self, id: TermId, text: &[char]) -> (usize, Scan) {
        let scan = self.lexer.match_terminal_scan(id, text, 0);
        (taken(&scan, text.len()), scan)
    }

    /// What the lexers find at the start of `text`, the text of a token
    /// of terminal `id`, on its own, and how the basic lexer reads all
    /// of it; `own` is the terminal's own match there when already known.
    fn scan_token(&self, id: TermId, text: &[char], own: Option<Scan>) -> TokenScans {
        let own = own.unwrap_or_else(|| self.lexer.match_terminal_scan(id, text, 0));
        let basic = self.lexer.next_token_scan_knowing(text, 0, id, &own);
        TokenScans {
            basic_follows_own: basic.follows(&own),
            reading: self.reading(id, text, &basic),
            own,
            basic,
        }
    }

    /// How the basic lexer reads the text of terminal `id` on its own,
    /// given what it finds at the start of it: that token first, since
    /// carrying the finding over the very text it was found on (see
    /// [`Lexer::extend`]) can only find it again, then the rest of the
    /// text lexed afresh.
    fn reading(&self, id: TermId, text: &[char], basic: &Scan) -> Reading {
        let mut scratch = self.scratch.lock();
        let got = &mut scratch.observed;
        got.clear();
        let mut pos = 0;
        if !text.is_empty() {
            let Lexed::Token(read, end) = basic.lexed else {
                return Reading::Opaque;
            };
            got.push((read, 0, end));
            pos = end;
        }
        if self.lex_from(text, pos, &[], &[], got).is_err() {
            return Reading::Opaque;
        }
        if got.len() == 1 && self.same_type(id, got[0].0) {
            Reading::Itself
        } else if self.is_ignore(id) && got.iter().all(|&(g, _, _)| self.is_ignore(g)) {
            Reading::Ignorables
        } else {
            Reading::Tokens(got.iter().map(|&(g, _, _)| g).collect())
        }
    }

    /// Whether the dynamic lexer reads `text` back as `tokens`: it
    /// matches each token's own pattern where the parser expects the
    /// token (a token of an ignored terminal the grammar needs, see
    /// [`Token::needed`], like any other) and the ignorable patterns
    /// anywhere between, so the check walks the positions the lexer can
    /// reach. A token's pattern may start early, inside the ignorables
    /// before it, and may run on past its end, as may an ignorable's:
    /// through inserted ignorables and tokens the grammar can do without
    /// (see [`Token::droppable`]), and into the next token the grammar
    /// needs as long as that token's own pattern still reaches its end
    /// from where the run-in stops (a comment that takes the `\r` of
    /// the `\r\n` after it, say); it may never take all of a needed
    /// token. A pattern too involved to match is given
    /// the benefit of the doubt. Where a token starts, its own pattern
    /// is not matched again: what it found on the token's text alone
    /// (see [`TokenScans::own`], in `findings`) is carried over the text
    /// after it; and an ignorable that cannot start with the character
    /// at a position (see [`Starters`]) is not matched there at all.
    fn scans(&self, text: &[char], tokens: &[Token], findings: &[TokenScans]) -> bool {
        let mut scratch = self.scratch.lock();
        let ScansScratch {
            bounds,
            significant,
            reachable,
            tried,
            ..
        } = &mut *scratch;
        bounds.clear();
        let mut pos = 0;
        for token in tokens {
            bounds.push((pos, pos + token.len()));
            pos += token.len();
        }
        significant.clear();
        significant.extend((0..tokens.len()).filter(|&i| tokens[i].needed));
        let kept = |k: usize| {
            significant[k..]
                .iter()
                .find(|&&i| !tokens[i].droppable)
                .copied()
        };
        let within = |got: usize, k: usize| kept(k).is_none_or(|i| got < bounds[i].1);
        reachable.clear();
        reachable.resize(text.len() + 1, false);
        reachable[0] = true;
        let mut k = 0;
        let mut t = 0;
        for p in 0..=text.len() {
            if !reachable[p] {
                continue;
            }
            while k < significant.len() && bounds[significant[k]].1 <= p {
                k += 1;
            }
            while t < tokens.len() && bounds[t].0 < p {
                t += 1;
            }
            if p == text.len() {
                return true;
            }
            let starting = (t < tokens.len() && bounds[t].0 == p).then_some(t);
            let carried =
                |scan: &Scan, j: usize| self.lexer.extend(scan, text, p, tokens[j].len(), false);
            tried.clear();
            for (j, &i) in significant.iter().enumerate().skip(k) {
                let id = tokens[i].id;
                let lexed = match tried.iter().find(|(tried, _)| *tried == id) {
                    Some(&(_, lexed)) => lexed,
                    None => {
                        let lexed = starting
                            .filter(|&s| s == i)
                            .and_then(|s| carried(&findings[tokens[s].scans].own, s))
                            .unwrap_or_else(|| self.lexer.match_terminal(id, text, p));
                        tried.push((id, lexed));
                        lexed
                    }
                };
                match lexed {
                    Lexed::Token(_, got) if got >= bounds[i].1 && within(got, j + 1) => {
                        reachable[got] = true;
                    }
                    Lexed::Unknown => reachable[bounds[i].1] = true,
                    _ => {}
                }
                if !tokens[i].droppable {
                    break;
                }
            }
            let starters = self.ignore_starters.at(text[p]);
            for (n, &ignore) in self.ignore.iter().enumerate() {
                if !Starters::has(starters, n) {
                    continue;
                }
                let lexed = starting
                    .filter(|&s| tokens[s].id == ignore)
                    .and_then(|s| carried(&findings[tokens[s].scans].own, s))
                    .unwrap_or_else(|| self.lexer.match_terminal(ignore, text, p));
                match lexed {
                    Lexed::Token(_, got) if got > p && within(got, k) => reachable[got] = true,
                    Lexed::Unknown => {
                        reachable[kept(k).map_or(text.len(), |i| bounds[i].0.max(p))] = true;
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// How well the lexers read `text`, the text of `tokens` one after
    /// another, back: [`Fit::Settled`] when what they found at the start
    /// of each token on its own already proves they read it as `tokens`
    /// (see [`GrammarSpec::settled`]), otherwise [`Fit::Wrong`] when the
    /// dynamic lexer does not read it as `tokens` at all (see
    /// [`GrammarSpec::scans`]), [`Fit::Exact`] when the basic lexer reads
    /// it as `tokens` too, and [`Fit::Retyped`] in between. `findings`
    /// is what the tokens' [`Token::scans`] index, here and in each check.
    fn fit(&self, text: &[char], tokens: &[Token], findings: &[TokenScans]) -> Fit {
        if self.settled(text, tokens, findings) {
            return Fit::Settled;
        }
        if !self.scans(text, tokens, findings) {
            return Fit::Wrong;
        }
        self.basic_fit(text, tokens, findings).max(Fit::Retyped)
    }

    /// Whether what the lexers found at the start of each token's text on
    /// its own proves that they read `text`, the concatenation of
    /// `tokens`, as exactly those tokens, and would still with any text
    /// after it: every token reads back as one token (of its own type or,
    /// as on its own, another) and its own pattern takes all of it,
    /// nothing a lexer tried on a token after the first looked at the
    /// text before it, and what the end of a token's text left open the
    /// text after it settles without changing what either lexer reads
    /// (see [`Lexer::settle`]).
    fn settled(&self, text: &[char], tokens: &[Token], findings: &[TokenScans]) -> bool {
        let mut start = 0;
        for (i, token) in tokens.iter().enumerate() {
            let len = token.len();
            let scans = &findings[token.scans];
            let Lexed::Token(read, _) = scans.basic.lexed else {
                return false;
            };
            if !self.single(token, findings)
                || !token.whole(findings)
                || (i > 0 && (scans.own.anchored || scans.basic.anchored))
            {
                return false;
            }
            if i + 1 < tokens.len() {
                let own = self.lexer.settle(&scans.own, text, start, len, false);
                if own != Some(Lexed::Token(token.id, start + len)) {
                    return false;
                }
                if !scans.basic_follows_own {
                    let basic = self.lexer.settle(&scans.basic, text, start, len, true);
                    if basic != Some(Lexed::Token(read, start + len)) {
                        return false;
                    }
                }
            }
            start += len;
        }
        true
    }

    /// How the basic lexer reads the concatenation of `tokens`: every
    /// token the grammar needs must come out as the token it is, with
    /// the same type and at least the same extent (a newline terminal may
    /// take over the whitespace that begins a comment after it), a token
    /// the lexer reads differently even on its own must come out as that
    /// same reading, and the stretches of inserted ignorables may be
    /// read as any ignorable tokens. The lexer reading a token with another type
    /// but the exact extent is only [`Fit::Retyped`]; anything else that
    /// differs is [`Fit::Wrong`]. A token the lexer cannot read at all
    /// makes the whole check moot.
    fn basic_fit(&self, text: &[char], tokens: &[Token], findings: &[TokenScans]) -> Fit {
        if tokens
            .iter()
            .any(|t| *t.reading(findings) == Reading::Opaque)
        {
            return Fit::Exact;
        }
        let mut scratch = self.scratch.lock();
        let ScansScratch {
            observed,
            known,
            regions,
            ..
        } = &mut *scratch;
        known.clear();
        let mut start = 0;
        for token in tokens {
            known.push((start, token.len(), token.scans));
            start += token.len();
        }
        match self.lex_all(text, known, findings, observed) {
            Ok(()) => {}
            Err(Lexed::NoMatch) => return Fit::Wrong,
            Err(_) => return Fit::Exact,
        }
        regions.clear();
        let mut pos = 0;
        for (t, token) in tokens.iter().enumerate() {
            let end = pos + token.len();
            if token.needed || !token.lexable(findings) {
                regions.push((pos, end, t));
            }
            pos = end;
        }
        let overlapping = |start: usize, end: usize| {
            regions
                .iter()
                .filter(move |&&(s, e, _)| start < e && s < end)
        };
        let mut fit = Fit::Exact;
        let mut i = 0;
        while i < observed.len() {
            let (got, start, end) = observed[i];
            let mut hits = overlapping(start, end);
            let Some(&(s, e, t)) = hits.next() else {
                if !self.is_ignore(got) {
                    fit = Fit::Retyped;
                }
                i += 1;
                continue;
            };
            if hits.next().is_some() {
                return Fit::Wrong;
            }
            let token = &tokens[t];
            if !token.needed {
                while i < observed.len() && observed[i].1 < e {
                    if overlapping(observed[i].1, observed[i].2).nth(1).is_some() {
                        return Fit::Wrong;
                    }
                    if !self.is_ignore(observed[i].0) {
                        fit = Fit::Retyped;
                    }
                    i += 1;
                }
                continue;
            }
            let expected: &[TermId] = match token.reading(findings) {
                Reading::Tokens(expected) => expected,
                _ => core::slice::from_ref(&token.id),
            };
            let mut j = i + 1;
            while j < observed.len() && observed[j].1 < e {
                if overlapping(observed[j].1, observed[j].2).nth(1).is_some() {
                    return Fit::Wrong;
                }
                j += 1;
            }
            let group = &observed[i..j];
            let last_end = group[group.len() - 1].2;
            if group.len() != expected.len() {
                return Fit::Wrong;
            }
            if !group
                .iter()
                .zip(expected)
                .all(|(&(g, _, _), &x)| self.same_type(x, g))
            {
                if start == s && last_end == e {
                    fit = Fit::Retyped;
                } else {
                    return Fit::Wrong;
                }
            }
            i = j;
        }
        fit
    }
}

/// What drawing one ignorable into the string came to.
enum Drawn {
    /// It is in the string, and the lexer reads the tokens around it
    /// back this well.
    Fits(Fit),
    /// It alone would have taken the string past the size cap, and was
    /// left out.
    Oversize,
    /// It is in the string, but the ignorable before it, matched on
    /// over it, still takes all of the token the chain is to free (see
    /// [`Generation::swallowed`]), so the lexer cannot read the tokens
    /// around it back, and was not asked.
    Swallowed,
}

/// The outcome of placing one ignorable (and any chain after it).
enum Placement {
    /// The tokens are in the string and the lexer reads them this well.
    Placed(Fit),
    /// Nothing is left in the string; the ignorable drawn first was of
    /// this terminal, and read back as itself on its own or not, or was
    /// too long for the string.
    Rejected {
        id: TermId,
        lexable: bool,
        oversize: bool,
    },
}

/// How the lexer reads one token's text on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reading {
    /// As a single token of its own type.
    Itself,
    /// For an ignorable, as several tokens, all of them ignorable.
    Ignorables,
    /// As this sequence of tokens.
    Tokens(Vec<TermId>),
    /// Not at all: the lexer does not know the terminal, cannot read the
    /// text, or ran out of budget.
    Opaque,
}

/// How the lexer reads a stretch of tokens, from worst to best: with
/// tokens merged, split or swallowed, with the right extents but other
/// types, exactly, or exactly and provably so with any text after it
/// (see [`GrammarSpec::settled`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fit {
    Wrong,
    Retyped,
    Exact,
    Settled,
}

/// Load a Lark grammar and build a [`GrammarSpec`] for it.
#[cfg(test)]
pub fn from_lark(
    text: &str,
    source: &dyn ImportSource,
    options: GrammarOptions,
) -> GrammarResult<GrammarSpec> {
    from_files(&GrammarFiles::gather(text, source)?, options)
}

/// Build a [`GrammarSpec`] for a Lark grammar whose files are all supplied.
pub fn from_files(files: &GrammarFiles, options: GrammarOptions) -> GrammarResult<GrammarSpec> {
    let grammar = compile(&files.build()?, &[options.start.as_str()])?;
    GrammarSpec::new(grammar, options)
}

#[derive(Clone)]
struct Token {
    id: TermId,
    /// Where its text lies in the string being generated
    /// ([`Generation::text`]).
    start: usize,
    end: usize,
    /// Whether the string stays in the grammar without this token: it
    /// is the only token of an element of a repetition that allows none
    /// (`x?`, `[x]`, `x*`), so a lexer that runs it into its neighbour
    /// or skips it leaves a string the parser still accepts.
    droppable: bool,
    /// Whether the parser needs this token: a rule drew it, as against
    /// an ignorable inserted between tokens (see
    /// [`Generation::maybe_ignore`]), which the lexer may skip. A rule
    /// may draw a terminal the grammar ignores, and its token is then
    /// one the parser needs like any other.
    needed: bool,
    /// Where in the generation's findings (see
    /// [`ReadingsCache::findings`]) what the lexers found in its text is.
    scans: usize,
}

/// What the lexers found in a token's text on its own, kept for the
/// token (see [`Token::scans`]) and remembered for the text (see
/// [`ReadingsCache::read_token`]) so that the checks on the longer texts the token is part of (its fit
/// with its neighbours, the finished string) need not match it again
/// where the finding carries over (see [`Lexer::extend`]).
#[derive(Clone)]
struct TokenScans {
    /// The token's own terminal matched at its start.
    own: Scan,
    /// The basic lexer's first token.
    basic: Scan,
    /// How the basic lexer reads the whole text.
    reading: Reading,
    /// Whether `basic` carries over any text exactly as `own` does (see
    /// [`Scan::follows`]): the basic lexer got as far as the token's own
    /// terminal without another leaving anything open, so settling the
    /// token for it (see [`GrammarSpec::settled`]) can only come out as
    /// settling its own match did, re-typed as the basic lexer read the
    /// token on its own.
    basic_follows_own: bool,
}

/// How much of a text of `len` characters a terminal's own match at its
/// start, `scan`, takes: all of it when the pattern found no match
/// there or could not be run.
fn taken(scan: &Scan, len: usize) -> usize {
    match scan.lexed {
        Lexed::Token(_, end) => end,
        Lexed::NoMatch | Lexed::Unknown => len,
    }
}

impl Token {
    fn len(&self) -> usize {
        self.end - self.start
    }

    /// How the basic lexer reads the token's text on its own, from
    /// `findings`, which its `scans` index.
    fn reading<'a>(&self, findings: &'a [TokenScans]) -> &'a Reading {
        &findings[self.scans].reading
    }

    /// Whether the basic lexer reads the token back as itself on its own
    /// (or, for an ignorable, as nothing but ignorable tokens).
    fn lexable(&self, findings: &[TokenScans]) -> bool {
        matches!(
            self.reading(findings),
            Reading::Itself | Reading::Ignorables
        )
    }

    /// Whether the token's own pattern takes all of its text.
    fn whole(&self, findings: &[TokenScans]) -> bool {
        findings[self.scans].own.lexed == Lexed::Token(self.id, self.len())
    }
}

enum Step {
    Engine(EngineError),
    TooLong,
    /// The finished string does not lex back as its tokens.
    Misread,
}

impl From<EngineError> for Step {
    fn from(e: EngineError) -> Self {
        Step::Engine(e)
    }
}

struct Generation<'s> {
    spec: &'s GrammarSpec,
    remaining: usize,
    /// The string so far: the text of `tokens`, in order and without
    /// gaps. Terminals are drawn onto its end, and a draw that is not
    /// kept is cut off again.
    text: Vec<char>,
    tokens: Vec<Token>,
    /// The size cap this draw is under (see [`Generation::fits`]).
    max_size: u64,
    /// How many characters of `text` are placeholder markers, which
    /// count for nothing toward the cap.
    markers: usize,
    /// How likely an ignorable goes before a token (see
    /// [`Generation::maybe_ignore`]), with the byte the engine compares
    /// against for it (see [`boolean_threshold`]).
    ignore_rate: f64,
    ignore_threshold: u8,
    /// Whether some stretch of the tokens was accepted without being
    /// settled (see [`GrammarSpec::settled`]), so that the finished
    /// string has to be lexed as a whole.
    unsettled: bool,
    /// Scratch for [`Generation::sequence`]: the budgets of the items of
    /// every sequence being generated, innermost last.
    budgets: Vec<u64>,
    /// Scratch for [`Generation::insert_ignore`]: the ignorables still
    /// worth drawing.
    candidates: Vec<TermId>,
    /// Scratch for [`Generation::ignore_chain`]: the ignorables that
    /// could end the last one's match, for each chain being drawn,
    /// innermost last.
    chain_candidates: Vec<TermId>,
    /// How often each ignorable drawn to end another's match in a chain
    /// (see [`Generation::ignore_chain`]) has failed and succeeded at
    /// it, by the pair of terminals.
    chains: HashMap<(TermId, TermId), (u32, u32)>,
    /// What the lexers found in each distinct token text drawn, by this
    /// generation and the ones before it that worked in these buffers
    /// (see [`ReadingsCache`]); the tokens' [`Token::scans`] index its
    /// findings.
    readings: ReadingsCache,
}

impl<'s> Generation<'s> {
    /// A generation working in the buffers `spec` has to spare.
    fn new(spec: &'s GrammarSpec, ignore_rate: f64, max_size: u64) -> Generation<'s> {
        let mut buffers = spec.buffers.lock().take().unwrap_or_else(Buffers::new);
        buffers.readings.forget_when_full();
        Generation {
            spec,
            remaining: work_limit(max_size, spec.max_depth),
            text: buffers.text,
            tokens: buffers.tokens,
            max_size,
            markers: 0,
            ignore_rate,
            ignore_threshold: boolean_threshold(ignore_rate),
            unsettled: false,
            budgets: buffers.budgets,
            candidates: buffers.candidates,
            chain_candidates: buffers.chain_candidates,
            chains: buffers.chains,
            readings: buffers.readings,
        }
    }

    /// Hand the buffers back to the spec, emptied but for the readings.
    fn recycle(self) {
        let Generation {
            spec,
            mut text,
            mut tokens,
            mut budgets,
            mut candidates,
            mut chain_candidates,
            mut chains,
            readings,
            ..
        } = self;
        text.clear();
        tokens.clear();
        budgets.clear();
        candidates.clear();
        chain_candidates.clear();
        chains.clear();
        let buffers = Buffers {
            text,
            tokens,
            budgets,
            candidates,
            chain_candidates,
            chains,
            readings,
        };
        let unkept = buffers.offer(&mut spec.buffers.lock());
        drop(unkept);
    }

    /// Forget the string.
    fn reset(&mut self) {
        self.text.clear();
        self.tokens.clear();
        self.markers = 0;
        self.unsettled = false;
        self.chains.clear();
    }

    /// The token of terminal `id` whose text is the string from `start`
    /// on, with `scans` where what the lexers find in it is (see
    /// [`Token::scans`]).
    fn token(&self, id: TermId, start: usize, scans: usize) -> Token {
        Token {
            id,
            start,
            end: self.text.len(),
            droppable: false,
            needed: true,
            scans,
        }
    }

    /// Whether the string is within the size cap, its placeholder
    /// markers counting for nothing.
    fn fits(&self) -> bool {
        (self.text.len() - self.markers) as u64 <= self.max_size
    }

    /// The tokens from index `from` on, leaving out the last `pending`
    /// (see [`Generation::maybe_ignore`]), with their text.
    fn window(&self, from: usize, pending: usize) -> (&[char], &[Token]) {
        let tokens = &self.tokens[from..self.tokens.len() - pending];
        let start = tokens.first().map_or(self.text.len(), |t| t.start);
        let end = tokens.last().map_or(start, |t| t.end);
        (&self.text[start..end], tokens)
    }

    /// The token before the last `pending`, if any.
    fn before(&self, pending: usize) -> Option<&Token> {
        self.tokens
            .len()
            .checked_sub(pending + 1)
            .map(|i| &self.tokens[i])
    }
}

/// Which of `count` terminals (by [`TermId`]) the rule `start` of
/// `analysis` can reach.
fn reachable_terminals(analysis: &Analysis, start: RuleId, count: usize) -> Vec<bool> {
    fn walk(items: &[Node], rules: &mut [bool], terminals: &mut [bool], analysis: &Analysis) {
        for node in items {
            match &node.kind {
                NodeKind::Terminal(id) => terminals[*id] = true,
                NodeKind::Rule(id) => visit(*id, rules, terminals, analysis),
                NodeKind::Group(_, alts) => {
                    for alt in alts {
                        walk(&alt.items, rules, terminals, analysis);
                    }
                }
                NodeKind::Repeat { inner, .. } => {
                    walk(core::slice::from_ref(inner), rules, terminals, analysis);
                }
            }
        }
    }
    fn visit(id: RuleId, rules: &mut [bool], terminals: &mut [bool], analysis: &Analysis) {
        if core::mem::replace(&mut rules[id], true) {
            return;
        }
        for alt in &analysis.rules[id].alternatives {
            walk(&alt.items, rules, terminals, analysis);
        }
    }
    let mut rules = alloc::vec![false; analysis.rules.len()];
    let mut terminals = alloc::vec![false; count];
    visit(start, &mut rules, &mut terminals, analysis);
    terminals
}

/// The `index`th of `candidates`, which there must be.
fn nth_candidate(
    mut candidates: impl Iterator<Item = TermId>,
    index: usize,
) -> Result<TermId, EngineError> {
    Ok(hegel_internal_unwrap!(
        candidates.nth(index),
        "ignore_chain: drew an index past the candidates"
    ))
}

/// Draw an index into `count` candidates, uniformly.
fn draw_index<D: Draws>(d: &mut D, count: usize) -> Result<usize, Step> {
    let index = d.draw_integer_uniform(0, count as u64 - 1)?;
    Ok(index as usize)
}

/// The length of `spec`'s strings on average, as a whole number of
/// characters and never below `min`, the shortest string it can produce.
fn typical_size(re: &CompiledRegex, min: u64) -> u64 {
    (libm::round(re.typical_width()) as u64).max(min)
}

/// The characters a `%ignore` insertion adds for each character a
/// token adds, on average, with `term_typical` the typical width of
/// each terminal: a string drawn with insertion rate `r` comes out
/// about `1 + r * ignore_cost` times the budget its tokens were given.
fn ignore_cost(ignore: &[TermId], term_typical: &[u64]) -> f64 {
    let mean = |ids: &mut dyn Iterator<Item = TermId>| {
        let (sum, count) = ids.fold((0u64, 0u64), |(sum, count), id| {
            (sum + term_typical[id], count + 1)
        });
        (count > 0).then(|| sum as f64 / count as f64)
    };
    let Some(inserted) = mean(&mut ignore.iter().copied()) else {
        return 0.0;
    };
    let token = mean(&mut (0..term_typical.len()).filter(|id| !ignore.contains(id)));
    inserted / token.unwrap_or(inserted).max(1.0)
}

/// The budget for the tokens of a string aiming at `target` characters
/// with `%ignore` insertion rate `rate`, never below `min`: the
/// insertions add about `rate * ignore_cost` characters per token one.
fn token_budget(target: u64, min: u64, rate: f64, ignore_cost: f64) -> u64 {
    min.max((target as f64 / (1.0 + rate * ignore_cost)) as u64)
}

/// How many nodes a draw under `max_size` and `max_depth` may visit
/// before it is given up as deriving nothing: enough for a string of
/// the cap's size with a chain of rules as deep as the depth cap over
/// every character, which is about what the most involved string within
/// the caps costs, and never fewer than [`GENERATION_BUDGET`], so that
/// a draw whose every node derives nothing (a repetition of an empty
/// rule, a tree of nullable rules) ends soon enough whatever the caps.
fn work_limit(max_size: u64, max_depth: u32) -> usize {
    let per_size = max_size.saturating_mul(u64::from(max_depth));
    usize::try_from(per_size)
        .unwrap_or(usize::MAX)
        .max(GENERATION_BUDGET)
}

/// How many alternatives of a rule [`Generation::choose_alt`] can hold in
/// a bit mask.
const MASKED_ALTS: usize = u64::BITS as usize;

/// The alternatives of `alts` that are feasible at `depth` with `budget`
/// characters to spend, as a bit mask, with the growing class among them
/// (see [`Generation::choose_alt`]) and the smallest typical size in it.
/// `alts` must hold no more alternatives than a mask does.
fn feasible_classes(alts: &[Alt], budget: u64, depth: u32, max_depth: u32) -> (u64, u64, u64) {
    let mut feasible = 0u64;
    let mut typical = u64::MAX;
    let mut any_closing = false;
    for (i, alt) in alts.iter().enumerate() {
        if alt.min_size <= budget && depth.saturating_add(alt.min_depth) <= max_depth {
            feasible |= 1 << i;
            typical = typical.min(alt.typical_size);
            any_closing |= !alt.growing();
        }
    }
    let mut growing = 0u64;
    let mut cheapest_growing = u64::MAX;
    let mut rest = feasible;
    while rest != 0 {
        let i = rest.trailing_zeros();
        rest &= rest - 1;
        let alt = &alts[i as usize];
        let closes = if any_closing {
            !alt.growing()
        } else {
            alt.typical_size == typical
        };
        if !closes {
            growing |= 1 << i;
            cheapest_growing = cheapest_growing.min(alt.typical_size);
        }
    }
    (feasible, growing, cheapest_growing)
}

/// What [`feasible_classes`] answers for the alternatives of a rule or
/// group at any budget, worked out once, for as long as the depth leaves
/// room for every one of them: the alternatives come sorted by minimum
/// size (see [`analyze`]), so the ones feasible with a budget are the
/// first so many of them, and the table holds the growing class and its
/// cheapest typical size for each such prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AltClasses {
    /// The largest minimum depth among the alternatives: every one of
    /// them has room at a depth that leaves this much.
    min_depth: u32,
    /// The growing class among the first `k` alternatives and the
    /// smallest typical size in it, at index `k - 1`.
    by_prefix: Vec<(u64, u64)>,
}

impl AltClasses {
    /// The classes of `alts` by prefix, or `None` when they are more than
    /// a mask holds or not sorted by minimum size.
    fn of(alts: &[Alt]) -> Option<AltClasses> {
        if alts.len() > MASKED_ALTS || !alts.is_sorted_by_key(|alt| alt.min_size) {
            return None;
        }
        Some(AltClasses {
            min_depth: alts.iter().map(|alt| alt.min_depth).max().unwrap_or(0),
            by_prefix: (1..=alts.len())
                .map(|k| {
                    let (_, growing, cheapest) =
                        feasible_classes(&alts[..k], u64::MAX, 0, u32::MAX);
                    (growing, cheapest)
                })
                .collect(),
        })
    }

    /// What [`feasible_classes`] answers for `alts`, the alternatives
    /// these are the classes of, at `depth` with `budget` characters to
    /// spend under `max_depth`, or `None` when the depth leaves room for
    /// only some of them.
    fn at(&self, alts: &[Alt], budget: u64, depth: u32, max_depth: u32) -> Option<(u64, u64, u64)> {
        if depth.saturating_add(self.min_depth) > max_depth {
            return None;
        }
        let mut k = alts.len();
        while k > 0 && alts[k - 1].min_size > budget {
            k -= 1;
        }
        Some(match k.checked_sub(1) {
            Some(last) => {
                let (growing, cheapest_growing) = self.by_prefix[last];
                (u64::MAX >> (MASKED_ALTS - k), growing, cheapest_growing)
            }
            None => (0, 0, u64::MAX),
        })
    }
}

/// The classes (see [`AltClasses`]) of every group's alternatives in
/// `analysis`, indexed by [`super::analysis::GroupId`].
fn group_classes(analysis: &Analysis) -> Vec<Option<AltClasses>> {
    fn walk(items: &[Node], classes: &mut [Option<AltClasses>]) {
        for node in items {
            match &node.kind {
                NodeKind::Group(group, alts) => {
                    classes[*group] = AltClasses::of(alts);
                    for alt in alts {
                        walk(&alt.items, classes);
                    }
                }
                NodeKind::Repeat { inner, .. } => walk(core::slice::from_ref(inner), classes),
                NodeKind::Terminal(_) | NodeKind::Rule(_) => {}
            }
        }
    }
    let mut classes = alloc::vec![None; analysis.groups];
    for rule in &analysis.rules {
        for alt in &rule.alternatives {
            walk(&alt.items, &mut classes);
        }
    }
    classes
}

/// The position of the `index`th set bit of `bits`, counting from the
/// lowest, when it has that many.
fn nth_set_bit(mut bits: u64, index: u64) -> Option<u32> {
    for _ in 0..index.min(u64::from(u64::BITS)) {
        bits &= bits.wrapping_sub(1);
    }
    (bits != 0).then_some(bits.trailing_zeros())
}

/// The probability of taking a growing alternative with `spare` budget
/// beyond the rule's minimum.
fn grow_probability(spare: u64) -> f64 {
    let spare = spare as f64;
    spare / (spare + GROW_SCALE)
}

impl Generation<'_> {
    fn spend_work<D: Draws>(&mut self, d: &mut D, amount: usize) -> Result<(), EngineError> {
        let Some(remaining) = self.remaining.checked_sub(amount) else {
            d.conclude(Status::Invalid);
            return Err(EngineError::InvalidTestCase);
        };
        self.remaining = remaining;
        Ok(())
    }

    /// Draw one of `alts` for a rule reached at `depth` with `budget`
    /// characters to spend: between the alternatives that close the
    /// derivation and the ones that can grow without bound (or, when
    /// every alternative can, between the cheapest and the rest, so that
    /// a rule like `sum: product | sum "+" product` does not recurse
    /// regardless of its budget) with a probability that rises with the
    /// budget to spare beyond the cheapest growing alternative, then
    /// uniformly within the class. When no alternative the depth allows
    /// fits the budget, the cheapest of them is taken (see
    /// [`Generation::cheapest_in_depth`]).
    ///
    /// The classes are kept as bit masks over the alternatives, so that
    /// each is sized and indexed without another pass over them, and
    /// looked up in `classes` (see [`AltClasses`]) when it has them for
    /// this depth, instead of being found again; a rule with more
    /// alternatives than a mask holds is drawn from by
    /// [`Generation::choose_alt_scanning`], which answers the same.
    fn choose_alt<'a, D: Draws>(
        &mut self,
        d: &mut D,
        alts: &'a [Alt],
        classes: Option<&AltClasses>,
        budget: u64,
        depth: u32,
    ) -> Result<&'a Alt, EngineError> {
        if alts.len() > MASKED_ALTS {
            return self.choose_alt_scanning(d, alts, budget, depth);
        }
        let max_depth = self.spec.max_depth;
        let (feasible, growing, cheapest_growing) = classes
            .and_then(|classes| classes.at(alts, budget, depth, max_depth))
            .unwrap_or_else(|| feasible_classes(alts, budget, depth, max_depth));
        if feasible == 0 {
            return self.cheapest_in_depth(alts, depth);
        }
        let grow = if growing == 0 {
            false
        } else {
            let p = grow_probability(budget.saturating_sub(cheapest_growing));
            d.weighted_precise(p, None)?
        };
        let class = if grow { growing } else { feasible & !growing };
        let index = d.draw_integer_uniform(0, u64::from(class.count_ones()) - 1)?;
        let chosen = hegel_internal_unwrap!(
            nth_set_bit(class, index),
            "choose_alt: drew an index past the class"
        );
        Ok(&alts[chosen as usize])
    }

    /// [`Generation::choose_alt`] for any number of alternatives, finding
    /// each class by scanning them.
    fn choose_alt_scanning<'a, D: Draws>(
        &mut self,
        d: &mut D,
        alts: &'a [Alt],
        budget: u64,
        depth: u32,
    ) -> Result<&'a Alt, EngineError> {
        let feasible = |alt: &Alt| {
            alt.min_size <= budget && depth.saturating_add(alt.min_depth) <= self.spec.max_depth
        };
        let mut count = 0;
        let mut typical = u64::MAX;
        let mut any_closing = false;
        for alt in alts.iter().filter(|alt| feasible(alt)) {
            count += 1;
            typical = typical.min(alt.typical_size);
            any_closing |= !alt.growing();
        }
        if count == 0 {
            return self.cheapest_in_depth(alts, depth);
        }
        let closing = |alt: &Alt| {
            if any_closing {
                !alt.growing()
            } else {
                alt.typical_size == typical
            }
        };
        let in_class = |alt: &Alt, grow: bool| feasible(alt) && closing(alt) != grow;
        let growing = alts.iter().filter(|alt| in_class(alt, true));
        let grow = match growing.clone().map(|alt| alt.typical_size).min() {
            None => false,
            Some(cheapest_growing) => {
                let p = grow_probability(budget.saturating_sub(cheapest_growing));
                d.weighted_precise(p, None)?
            }
        };
        let size = alts.iter().filter(|alt| in_class(alt, grow)).count();
        let index = d.draw_integer_uniform(0, size as u64 - 1)?;
        Ok(hegel_internal_unwrap!(
            alts.iter()
                .filter(|alt| in_class(alt, grow))
                .nth(index as usize),
            "choose_alt: drew an index past the class"
        ))
    }

    /// The alternative of `alts` with the smallest minimum size among
    /// those the depth cap leaves room for at `depth`, which there must
    /// be one of: what a rule takes when none of them fits its budget,
    /// since a budget steers the structure and never rules it out. The
    /// smallest string the grammar can produce is worked out without
    /// regard to the depth cap (see [`analyze`]), so the budget of a
    /// draw aiming at it can be below what the cap leaves.
    fn cheapest_in_depth<'a>(&self, alts: &'a [Alt], depth: u32) -> Result<&'a Alt, EngineError> {
        let max_depth = self.spec.max_depth;
        Ok(hegel_internal_unwrap!(
            alts.iter()
                .filter(|alt| depth.saturating_add(alt.min_depth) <= max_depth)
                .min_by_key(|alt| alt.min_size),
            "choose_alt: no alternative fits the depth cap"
        ))
    }

    fn rule<D: Draws>(
        &mut self,
        d: &mut D,
        id: RuleId,
        budget: u64,
        depth: u32,
    ) -> Result<(), Step> {
        let rule = &self.spec.analysis.rules[id];
        let alt = self.choose_alt(
            d,
            &rule.alternatives,
            self.spec.classes[id].as_ref(),
            budget,
            depth + 1,
        )?;
        self.sequence(d, alt, budget, depth + 1)
    }

    fn sequence<D: Draws>(
        &mut self,
        d: &mut D,
        alt: &Alt,
        budget: u64,
        depth: u32,
    ) -> Result<(), Step> {
        let base = self.budgets.len();
        let result = self.sequence_from(d, alt, budget, depth, base);
        self.budgets.truncate(base);
        result
    }

    /// Generate `alt`'s items with budgets held in `budgets` from `base`
    /// on: each item's typical size, plus for the items whose size is
    /// not fixed a share of what is left of `budget`, cut at uniformly
    /// drawn points so that no position is systematically larger than
    /// another; what an item with a maximum size cannot take is shared
    /// equally between the items without one.
    fn sequence_from<D: Draws>(
        &mut self,
        d: &mut D,
        alt: &Alt,
        budget: u64,
        depth: u32,
        base: usize,
    ) -> Result<(), Step> {
        let flexible = |item: &Node| item.max_size != Some(item.min_size);
        self.budgets
            .extend(alt.items.iter().map(|item| item.typical_size));
        let spare = budget.saturating_sub(alt.typical_size);
        let parts = alt.items.iter().filter(|item| flexible(item)).count();
        if parts > 0 && spare > 0 {
            let cuts = self.budgets.len();
            self.budgets.push(0);
            for _ in 1..parts {
                let cut = d.draw_integer_uniform(0, spare)?;
                self.budgets.push(cut);
            }
            self.budgets.push(spare);
            self.budgets[cuts..].sort_unstable();
            let mut leftover = 0u64;
            let mut growing = 0u64;
            let mut cut = cuts;
            for (i, item) in alt.items.iter().enumerate() {
                if !flexible(item) {
                    continue;
                }
                let share = self.budgets[cut + 1] - self.budgets[cut];
                cut += 1;
                match item.max_size {
                    Some(max) => {
                        let capacity = max.saturating_sub(item.typical_size);
                        self.budgets[base + i] += share.min(capacity);
                        leftover += share.saturating_sub(capacity);
                    }
                    None => {
                        self.budgets[base + i] += share;
                        growing += 1;
                    }
                }
            }
            self.budgets.truncate(cuts);
            if let Some(each) = leftover.checked_div(growing) {
                for (i, item) in alt.items.iter().enumerate() {
                    if item.max_size.is_none() {
                        self.budgets[base + i] += each;
                    }
                }
            }
        }
        for (i, item) in alt.items.iter().enumerate() {
            let item_budget = self.budgets[base + i];
            self.item(d, item, item_budget, depth)?;
        }
        Ok(())
    }

    fn item<D: Draws>(
        &mut self,
        d: &mut D,
        node: &Node,
        budget: u64,
        depth: u32,
    ) -> Result<(), Step> {
        self.spend_work(d, 1)?;
        match &node.kind {
            NodeKind::Terminal(t) => self.terminal(d, *t),
            NodeKind::Rule(r) => self.rule(d, *r, budget, depth),
            NodeKind::Group(group, alts) => {
                let classes = self.spec.group_classes[*group].as_ref();
                let alt = self.choose_alt(d, alts, classes, budget, depth)?;
                self.sequence(d, alt, budget, depth)
            }
            NodeKind::Repeat { inner, min, max } => {
                self.repeat(d, inner, *min, *max, budget, depth)
            }
        }
    }

    fn repeat<D: Draws>(
        &mut self,
        d: &mut D,
        inner: &Node,
        min: u32,
        max: Option<u32>,
        budget: u64,
        depth: u32,
    ) -> Result<(), Step> {
        let min = u64::from(min);
        let cost = inner.typical_size;
        let spare = budget.saturating_sub(min.saturating_mul(cost));
        let fits_depth = depth.saturating_add(inner.min_depth) <= self.spec.max_depth;
        let feasible_max = if !fits_depth || inner.max_size == Some(0) {
            min
        } else {
            let extra = spare.checked_div(cost).unwrap_or(spare);
            let cap = min.saturating_add(extra);
            max.map_or(cap, |max| cap.min(u64::from(max)))
        };
        let count = if feasible_max <= min {
            min
        } else if inner.max_size.is_none() {
            self.draw_count(d, min, feasible_max, spare)?
        } else if max.is_some_and(|max| u64::from(max) == feasible_max) {
            d.draw_integer_uniform(min, feasible_max)?
        } else if d.weighted_precise(grow_probability(spare), None)? {
            feasible_max
        } else {
            min
        };
        let each = budget
            .saturating_sub(count.saturating_mul(cost))
            .checked_div(count)
            .map_or(0, |share| cost + share);
        for _ in 0..count {
            let before = self.tokens.len();
            self.item(d, inner, each, depth)?;
            if min == 0 {
                self.mark_droppable(before);
            }
        }
        Ok(())
    }

    /// Mark the token an element of an optional repetition produced as
    /// one the grammar can do without, when it produced exactly one
    /// (inserted ignorables aside).
    fn mark_droppable(&mut self, from: usize) {
        let mut significant = self.tokens[from..].iter_mut().filter(|t| t.needed);
        if let (Some(token), None) = (significant.next(), significant.next()) {
            token.droppable = true;
        }
    }

    /// Draw how many elements a repetition of elements that can grow
    /// gets, between `min` and `feasible_max`: whether to go beyond the
    /// minimum at all is decided like a growing alternative, from the
    /// spare budget, and the number of extra elements is then
    /// log-uniform, so a large budget is as likely to go into a few long
    /// elements as into many short ones. A repetition of elements that
    /// cannot grow has only its count to spend the budget on, so it takes
    /// as many as the budget buys (see [`Generation::repeat`]).
    fn draw_count<D: Draws>(
        &mut self,
        d: &mut D,
        min: u64,
        feasible_max: u64,
        spare: u64,
    ) -> Result<u64, Step> {
        let p = grow_probability(spare);
        if !d.weighted_precise(p, None)? {
            return Ok(min);
        }
        let extra_max = feasible_max - min;
        let scale = libm::log((extra_max + 1) as f64);
        let u = d.draw_integer_uniform(0, COUNT_RESOLUTION)?;
        let extra = libm::exp(u as f64 / COUNT_RESOLUTION as f64 * scale) as u64;
        Ok(min + extra.clamp(1, extra_max))
    }

    /// Draw a string of terminal `id` onto the end of the text: from its
    /// pattern, or its placeholder's marker.
    fn draw_text<D: Draws>(&mut self, d: &mut D, id: TermId) -> Result<(), EngineError> {
        match &self.spec.terminals[id] {
            TerminalGen::Pattern(re) => spanned(d, LABEL_REGEX, |d| {
                generate_regex_into(d, re, true, &mut self.text, MAX_MAX_SIZE as usize)
            }),
            TerminalGen::Placeholder(marker) => {
                self.spend_work(d, marker.len())?;
                self.text.extend_from_slice(marker);
                self.markers += marker.len();
                Ok(())
            }
        }
    }

    /// Draw a string for terminal `id`: from its pattern, cut to what the
    /// pattern reads back from the front and redrawn a few times while
    /// the basic lexer would not read it back as that terminal, or its
    /// placeholder's marker as it is. A text drawn before is not
    /// matched again: what was remembered of it (see
    /// [`ReadingsCache::recall`]) says how much of it the pattern takes.
    fn draw_terminal<D: Draws>(&mut self, d: &mut D, id: TermId) -> Result<Token, Step> {
        let start = self.text.len();
        match &self.spec.terminals[id] {
            TerminalGen::Placeholder(_) => {
                self.draw_text(d, id)?;
                let scans = self
                    .readings
                    .read_token(self.spec, id, &self.text[start..], None);
                Ok(self.token(id, start, scans))
            }
            TerminalGen::Pattern(_) => {
                let mut attempt = 0;
                loop {
                    self.draw_text(d, id)?;
                    attempt += 1;
                    if !self.fits() {
                        self.text.truncate(start);
                        if attempt > LEX_RETRIES {
                            return Err(Step::TooLong);
                        }
                        continue;
                    }
                    let drawn = &self.text[start..];
                    let len = drawn.len();
                    let (end, scans) = match self.readings.recall(id, drawn) {
                        Ok(scans) => (taken(&self.readings.findings[scans].own, len), Some(scans)),
                        Err(hash) => {
                            let (end, own) = self.spec.whole(id, drawn);
                            let scans = (end == len).then(|| {
                                self.readings.learn(self.spec, id, hash, drawn, Some(own))
                            });
                            (end, scans)
                        }
                    };
                    self.text.truncate(start + end);
                    let scans = scans.filter(|_| end == len).unwrap_or_else(|| {
                        self.readings
                            .read_token(self.spec, id, &self.text[start..], None)
                    });
                    let token = self.token(id, start, scans);
                    if token.lexable(&self.readings.findings) || attempt > LEX_RETRIES {
                        return Ok(token);
                    }
                    self.text.truncate(start);
                }
            }
        }
    }

    /// Draw a token of terminal `id` onto the end of the string, with
    /// any ignorables that go before it.
    fn terminal<D: Draws>(&mut self, d: &mut D, id: TermId) -> Result<(), Step> {
        let token = self.draw_terminal(d, id)?;
        self.tokens.push(token);
        self.maybe_ignore(d, 1)
    }

    /// Insert ignorable tokens before the last `pending` tokens of the
    /// string (the token just drawn, or none at its end), reporting
    /// whether any were. An ignorable the lexer would run into the token
    /// after it (a comment, say, which only a newline ends) is followed
    /// by another ignorable drawn to keep the two apart, up to
    /// [`IGNORE_CHAIN`] deep; a chain that still changes the token
    /// sequence is redrawn, without the terminal it started from when
    /// that read fine on its own, until the attempts run out. An
    /// ignorable the lexer merely gives another type is kept, since that
    /// is a property of the grammar no redraw will change.
    fn insert_ignore<D: Draws>(&mut self, d: &mut D, pending: usize) -> Result<bool, Step> {
        let from = (self.tokens.len() - pending).saturating_sub(1);
        self.candidates.clear();
        self.candidates.extend_from_slice(&self.spec.ignore);
        let mut oversize = false;
        for _ in 0..IGNORE_ATTEMPTS {
            let mut draws = IGNORE_CHAIN_DRAWS;
            let index = draw_index(d, self.candidates.len())?;
            let id = self.candidates[index];
            match self.place_ignore(d, id, from, pending, 0, &mut draws)? {
                Placement::Placed(_) => return Ok(true),
                Placement::Rejected { oversize: true, .. } => oversize = true,
                Placement::Rejected { id, lexable, .. } => {
                    if lexable && self.candidates.len() > 1 {
                        self.candidates.retain(|&candidate| candidate != id);
                    }
                }
            }
        }
        if oversize {
            return Err(Step::TooLong);
        }
        Ok(false)
    }

    /// Draw ignorables after the ones inserted since `from` until the
    /// lexer reads the chain and the pending token back as intended, or
    /// `draws` run out; nothing stays inserted when they do. Only
    /// ignorables that could end the last one's match where it is are
    /// drawn (see [`Lexer::can_stop`]): a comment that runs to the end
    /// of its line is followed by something that can hold a newline,
    /// never by spaces it would swallow. An ignorable that could but, as
    /// drawn, keeps failing to is given up on for the rest of the string
    /// (see [`Generation::hopeful`]), since what the analysis cannot
    /// rule out the draws may: a comment can begin with the newline that
    /// ends the one before it, but rarely does.
    fn ignore_chain<D: Draws>(
        &mut self,
        d: &mut D,
        from: usize,
        pending: usize,
        depth: u32,
        draws: &mut u32,
    ) -> Result<Fit, Step> {
        let prev = self.before(pending).map(|prev| (prev.id, prev.scans));
        let base = self.chain_candidates.len();
        for &id in &self.spec.ignore {
            let stoppable = prev.is_none_or(|(_, scans)| {
                let own = &self.readings.findings[scans].own;
                self.spec.lexer.can_stop(own, id)
            });
            if stoppable {
                self.chain_candidates.push(id);
            }
        }
        let fit = self.ignore_chain_from(d, base, from, pending, depth, draws);
        self.chain_candidates.truncate(base);
        fit
    }

    /// [`Generation::ignore_chain`] over the candidates from `base` on
    /// in `chain_candidates`.
    fn ignore_chain_from<D: Draws>(
        &mut self,
        d: &mut D,
        base: usize,
        from: usize,
        pending: usize,
        depth: u32,
        draws: &mut u32,
    ) -> Result<Fit, Step> {
        let prev = self.before(pending).map(|prev| prev.id);
        while *draws > 0 {
            let hopeful = |this: &Self, id: TermId| prev.is_none_or(|prev| this.hopeful(prev, id));
            let candidates = || {
                self.chain_candidates[base..]
                    .iter()
                    .copied()
                    .filter(|&id| hopeful(self, id))
            };
            let count = candidates().count();
            if count == 0 {
                break;
            }
            *draws -= 1;
            let index = draw_index(d, count)?;
            let id = nth_candidate(candidates(), index)?;
            if let Placement::Placed(fit) = self.place_ignore(d, id, from, pending, depth, draws)? {
                return Ok(fit);
            }
        }
        Ok(Fit::Wrong)
    }

    /// Whether drawing the ignorable `id` to end a token of `prev` in a
    /// chain is still worth it: it is until it has failed
    /// [`IGNORE_CHAIN_PATIENCE`] times for each time it has succeeded,
    /// and once more.
    fn hopeful(&self, prev: TermId, id: TermId) -> bool {
        self.chains
            .get(&(prev, id))
            .is_none_or(|&(failed, succeeded)| failed < IGNORE_CHAIN_PATIENCE * (succeeded + 1))
    }

    /// Draw one ignorable before the last `pending` tokens and, when the
    /// lexer would run it into the token after it but not into the
    /// tokens before it, a chain of further ignorables after it; the
    /// whole chain from the token at `from` on is what has to lex. The
    /// tokens stay in the string when the result fits. A draw in a chain
    /// (at a `depth` above zero) counts against drawing that ignorable
    /// after that token again until it is placed, and then for it.
    fn place_ignore<D: Draws>(
        &mut self,
        d: &mut D,
        id: TermId,
        from: usize,
        pending: usize,
        depth: u32,
        draws: &mut u32,
    ) -> Result<Placement, Step> {
        let prev = self.before(pending).map(|prev| prev.id);
        let drawn = self.draw_ignore(d, id, from, pending, depth)?;
        let pair = prev.filter(|_| depth > 0).map(|prev| (prev, id));
        if let Some(pair) = pair {
            self.chains.entry(pair).or_default().0 += 1;
        }
        let placement = match drawn {
            Drawn::Oversize => Placement::Rejected {
                id,
                lexable: false,
                oversize: true,
            },
            Drawn::Fits(fit) if fit > Fit::Wrong => {
                self.unsettled |= fit < Fit::Settled;
                Placement::Placed(fit)
            }
            Drawn::Fits(_) => self.chain_or_remove(d, from, pending, depth, draws, false)?,
            Drawn::Swallowed => self.chain_or_remove(d, from, pending, depth, draws, true)?,
        };
        if let (Some(pair), Placement::Placed(_)) = (pair, &placement) {
            let (failed, succeeded) = self.chains.entry(pair).or_default();
            *failed -= 1;
            *succeeded += 1;
        }
        Ok(placement)
    }

    /// Chain further ignorables after the one just inserted before the
    /// last `pending` tokens, which the lexer does not read back with
    /// its neighbours, when it is read into the token after it but not
    /// into the tokens before it, or remove it. That the token after it
    /// is the trouble is already known when the ignorable before it
    /// `swallowed` that token (see [`Drawn::Swallowed`]).
    fn chain_or_remove<D: Draws>(
        &mut self,
        d: &mut D,
        from: usize,
        pending: usize,
        depth: u32,
        draws: &mut u32,
        swallowed: bool,
    ) -> Result<Placement, Step> {
        if depth < IGNORE_CHAIN && *draws > 0 {
            let after_only = swallowed || {
                let (text, tokens) = self.window(from, pending);
                self.spec.fit(text, tokens, &self.readings.findings) > Fit::Wrong
            };
            if after_only {
                let fit = self.ignore_chain(d, from, pending, depth + 1, draws)?;
                if fit > Fit::Wrong {
                    return Ok(Placement::Placed(fit));
                }
            }
        }
        let token = self.remove(pending);
        Ok(Placement::Rejected {
            id: token.id,
            lexable: token.lexable(&self.readings.findings),
            oversize: false,
        })
    }

    /// Draw an ignorable of terminal `id` into the string before the
    /// last `pending` tokens: how the lexer reads the tokens from `from`
    /// on with it in place, unless that is known without looking (see
    /// [`Drawn`]). In a chain (at a `depth` above zero), the ignorable
    /// before it was drawn to be read into the token after it, and is
    /// matched on over the new text first: when it still takes all of a
    /// token the parser needs, the new text has not freed that token,
    /// and the lexing check with the neighbours is spared.
    fn draw_ignore<D: Draws>(
        &mut self,
        d: &mut D,
        id: TermId,
        from: usize,
        pending: usize,
        depth: u32,
    ) -> Result<Drawn, Step> {
        let at = self.tokens.len() - pending;
        let start = self
            .tokens
            .get(at)
            .map_or(self.text.len(), |next| next.start);
        let drawn = self.text.len();
        self.draw_text(d, id)?;
        let len = self.text.len() - drawn;
        if !self.fits() {
            self.text.truncate(drawn);
            return Ok(Drawn::Oversize);
        }
        self.text[start..].rotate_left(drawn - start);
        let swallowed = depth > 0 && pending > 0 && self.swallowed(at, len);
        let scans = self
            .readings
            .read_token(self.spec, id, &self.text[start..start + len], None);
        for later in &mut self.tokens[at..] {
            later.start += len;
            later.end += len;
        }
        let token = Token {
            id,
            start,
            end: start + len,
            droppable: false,
            needed: false,
            scans,
        };
        self.tokens.insert(at, token);
        if swallowed {
            return Ok(Drawn::Swallowed);
        }
        let (text, tokens) = self.window(from, 0);
        Ok(Drawn::Fits(self.spec.fit(
            text,
            tokens,
            &self.readings.findings,
        )))
    }

    /// Whether the token before index `at`, matched on over the `len`
    /// characters of new text now at the start of the token at `at`,
    /// takes all of that token, one the parser needs.
    fn swallowed(&self, at: usize, len: usize) -> bool {
        let (prev, next) = (&self.tokens[at - 1], &self.tokens[at]);
        if next.droppable || !next.needed {
            return false;
        }
        let through = next.start + len + next.len();
        let own = &self.readings.findings[prev.scans].own;
        let got = self
            .spec
            .lexer
            .extend(own, &self.text[..through], prev.start, prev.len(), false);
        matches!(got, Some(Lexed::Token(_, end)) if end >= through)
    }

    /// Take the token before the last `pending` out of the string.
    fn remove(&mut self, pending: usize) -> Token {
        let at = self.tokens.len() - pending - 1;
        let token = self.tokens.remove(at);
        if self.spec.is_placeholder(token.id) {
            self.markers -= token.len();
        }
        self.text.drain(token.start..token.end);
        for later in &mut self.tokens[at..] {
            later.start -= token.len();
            later.end -= token.len();
        }
        token
    }

    /// Decide whether an ignorable goes before the last `pending` tokens
    /// (the token just drawn, or none at the end of the string): always
    /// when a lexer would otherwise read that token and the one before
    /// it differently, otherwise at the string's ignore rate. A
    /// separator the basic lexer needs but none of the ignorables can
    /// provide is left out; one the dynamic lexer needs too leaves a
    /// string that is not in the grammar, which is discarded for another
    /// attempt, like a string that does not lex back as a whole. A
    /// grammar without ignorables has nothing to insert, and leaves
    /// what the pair checks cannot settle to the lexing of the whole
    /// string.
    fn maybe_ignore<D: Draws>(&mut self, d: &mut D, pending: usize) -> Result<(), Step> {
        let at = self.tokens.len() - pending;
        let adjacent = at.checked_sub(1).filter(|_| pending > 0);
        if self.spec.ignore.is_empty() {
            if let Some(from) = adjacent {
                let (text, tokens) = self.window(from, 0);
                self.unsettled |= !self.spec.settled(text, tokens, &self.readings.findings);
            }
            return Ok(());
        }
        let pair = adjacent.map_or(Fit::Settled, |from| {
            let (text, tokens) = self.window(from, 0);
            self.spec.fit(text, tokens, &self.readings.findings)
        });
        let beside_placeholder = adjacent.is_some_and(|from| {
            self.spec.is_placeholder(self.tokens[from].id)
                || self.spec.is_placeholder(self.tokens[at].id)
        });
        let forced = if pair < Fit::Exact || beside_placeholder {
            Some(true)
        } else {
            None
        };
        if d.weighted_against(self.ignore_rate, self.ignore_threshold, forced)?
            && self.insert_ignore(d, pending)?
        {
            return Ok(());
        }
        if pair == Fit::Wrong || beside_placeholder {
            return Err(Step::Misread);
        }
        self.unsettled |= pair < Fit::Settled;
        Ok(())
    }

    fn run<D: Draws>(&mut self, d: &mut D, budget: u64) -> Result<(), Step> {
        self.rule(d, self.spec.start, budget, 0)?;
        self.maybe_ignore(d, 0)?;
        if !self.unsettled
            || self
                .spec
                .scans(&self.text, &self.tokens, &self.readings.findings)
        {
            Ok(())
        } else {
            Err(Step::Misread)
        }
    }
}

/// Draw a string from `spec` as one choice of `ntc`, in a span of its
/// own: the walk draws its decisions from the test case's RNG alone (see
/// [`NativeTestCase::draw_string_from`]), and the string is recorded
/// under the grammar's constraint, which a replay checks it against.
/// `max_size` caps this draw below the grammar's own cap, for a caller
/// whose placeholder values have to fit alongside the string; one below
/// the smallest string, or above the grammar's cap, is read as that.
/// A replayed string over this draw's cap rejects the test case.
pub fn generate_grammar(
    ntc: &mut NativeTestCase,
    spec: &GrammarSpec,
    max_size: Option<u64>,
) -> Result<String, EngineError> {
    spec.ensure_simplest()?;
    let max_size = max_size.map_or(spec.max_size, |cap| {
        cap.clamp(spec.min_size(), spec.max_size)
    });
    spanned(ntc, LABEL_GRAMMAR, |ntc| {
        let text =
            ntc.draw_string_from(Arc::clone(&spec.kind), |d| draw_grammar(d, spec, max_size))?;
        if max_size < spec.max_size {
            let markers: usize = spec
                .placeholders
                .iter()
                .map(|(_, marker)| text.matches(marker.as_str()).count() * marker.chars().count())
                .sum();
            if (text.chars().count() - markers) as u64 > max_size {
                ntc.conclude(Status::Invalid, None);
                return Err(EngineError::InvalidTestCase);
            }
        }
        Ok(text)
    })
}

/// Draw a string of at most `max_size` characters from `spec`, every
/// decision from `d`.
fn draw_grammar<D: Draws>(
    d: &mut D,
    spec: &GrammarSpec,
    max_size: u64,
) -> Result<Vec<u32>, EngineError> {
    let min = spec.min_size();
    let target = if max_size > min && d.weighted_precise(LARGE_TARGET_PROBABILITY, None)? {
        d.draw_integer_uniform(min, max_size)?
    } else {
        min
    };
    let ignore_rate = if spec.ignore.is_empty() {
        0.0
    } else {
        let step = d.draw_integer_uniform(0, IGNORE_RATE_STEPS - 1)?;
        let fraction = step as f64 / (IGNORE_RATE_STEPS - 1) as f64;
        fraction * fraction
    };
    let mut generation = Generation::new(spec, ignore_rate, max_size);
    let result = generation.attempts(d, target, min);
    generation.recycle();
    result
}

/// A string of the grammar's smallest size, every decision from `d`,
/// with no optional ignorables.
fn smallest_string<D: Draws>(d: &mut D, spec: &GrammarSpec) -> Result<Vec<u32>, EngineError> {
    let min = spec.min_size();
    let mut generation = Generation::new(spec, 0.0, spec.max_size);
    let result = generation.attempts(d, min, min);
    generation.recycle();
    result
}

impl Generation<'_> {
    /// Draw strings of about `target` characters (never below `min`, the
    /// shortest the grammar allows) until one is kept, halving the target
    /// after each one that is too long.
    fn attempts<D: Draws>(
        &mut self,
        d: &mut D,
        mut target: u64,
        min: u64,
    ) -> Result<Vec<u32>, EngineError> {
        let mut attempt = 0;
        loop {
            let budget = token_budget(target, min, self.ignore_rate, self.spec.ignore_cost);
            match self.run(d, budget) {
                Ok(()) => return Ok(self.text.iter().map(|&c| u32::from(c)).collect()),
                Err(Step::Engine(e)) => return Err(e),
                Err(step) => {
                    attempt += 1;
                    if attempt >= MAX_ATTEMPTS {
                        d.conclude(Status::Invalid);
                        return Err(EngineError::InvalidTestCase);
                    }
                    if matches!(step, Step::TooLong) {
                        target = min.max(target / 2);
                    }
                    self.reset();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/lark/generate_tests.rs"]
mod tests;
