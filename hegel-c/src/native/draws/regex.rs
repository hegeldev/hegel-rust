use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, ManyState, NativeTestCase, Status};
use crate::native::intervalsets::IntervalSet;
use crate::native::re::constants::{
    AtCode, ChCode, SRE_FLAG_ASCII, SRE_FLAG_DOTALL, SRE_FLAG_IGNORECASE, SRE_FLAG_MULTILINE,
};
use crate::native::re::parser::{OpCode, ParsedPattern, SetItem, SubPattern, parse_pattern};
use crate::unicodedata;

use super::many_more;

fn is_surrogate_cp(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

/// Cache key for a `(IN, items)` node's character set: the `SetItem` slice's
/// address and length (stable because the AST is owned by the enclosing
/// [`CompiledRegex`]) plus the active flags (which affect IGNORECASE swaps
/// and ASCII-only filtering).
type InKey = (usize, usize, u32);

/// A regex pattern compiled once at string-generator construction time:
/// the parsed AST, the optional user alphabet, and a cross-draw cache of
/// the alphabet-constrained `IN`-node character sets (category classes like
/// `\w` cost a full alphabet scan to materialise).
/// Cache key for the alphabet-constrained `Any` and `NotLiteral` character
/// sets: the excluded codepoint (`u32::MAX` for `Any`, which has none) plus
/// the flag bits the set depends on.
type CharKey = (u32, u32);

#[derive(Debug)]
pub(crate) struct CompiledRegex {
    parsed: ParsedPattern,
    alphabet: Option<IntervalSet>,
    in_cache: Mutex<HashMap<InKey, Arc<[char]>>>,
    char_cache: Mutex<HashMap<CharKey, Arc<[char]>>>,
}

impl CompiledRegex {
    /// Parse `pattern`, reporting an invalid-argument diagnostic so a bad
    /// pattern surfaces at construction time rather than mid-draw.
    pub(crate) fn compile(
        pattern: &str,
        alphabet: Option<IntervalSet>,
    ) -> Result<Self, EngineError> {
        let parsed = parse_pattern(pattern, 0).map_err(|e| {
            EngineError::InvalidArgument(format!("invalid regex pattern {pattern:?}: {e}"))
        })?;
        Ok(CompiledRegex {
            parsed,
            alphabet,
            in_cache: Mutex::new(HashMap::new()),
            char_cache: Mutex::new(HashMap::new()),
        })
    }
}

/// Draw a string matching `re`, anchored at both ends when `fullmatch`
/// and otherwise padded with draws from the compiled alphabet (or the full
/// codespace when it has none).
///
/// Candidates whose deferred checks fail (a `\b` that the padding broke, a
/// possessive repeat whose committed count the pattern can't actually
/// match, ...) are retried a few times within the same test case — the
/// analogue of Hypothesis's strategy-level `.filter(re.search)` retries —
/// before the whole test case is marked invalid.
pub(crate) fn generate_regex(
    ntc: &mut NativeTestCase,
    re: &CompiledRegex,
    fullmatch: bool,
) -> Result<String, EngineError> {
    const MAX_ATTEMPTS: usize = 5;
    for _ in 0..MAX_ATTEMPTS {
        if let Some(s) = generate_regex_attempt(ntc, re, fullmatch)? {
            return Ok(s);
        }
    }
    mark_invalid(ntc)?;
    unreachable!("mark_invalid returns Err — control flow does not reach here")
}

/// One generation attempt. Returns `Ok(None)` when the candidate was
/// generated but failed a deferred check against the final string.
fn generate_regex_attempt(
    ntc: &mut NativeTestCase,
    re: &CompiledRegex,
    fullmatch: bool,
) -> Result<Option<String>, EngineError> {
    let parsed = &re.parsed;
    let alphabet = &re.alphabet;

    let mut state = GenState {
        groups: HashMap::new(),
        flags: parsed.flags,
        fullmatch,
        pending_anchors: Vec::new(),
        pending_asserts: Vec::new(),
        pending_lookaheads: Vec::new(),
        needs_whole_match: false,
        in_cache: &re.in_cache,
        char_cache: &re.char_cache,
    };
    let mut result = String::new();

    if parsed.pattern.is_empty() {
        if !fullmatch {
            draw_pad(ntc, alphabet, &mut result)?;
        }
        return Ok(Some(result));
    }

    if !fullmatch {
        draw_prefix(ntc, parsed, alphabet, &mut result)?;
    }
    generate_subpattern(ntc, &parsed.pattern, &mut state, alphabet, &mut result)?;
    if !fullmatch {
        draw_suffix(ntc, parsed, alphabet, &mut result)?;
    }

    let needs_final_checks = !state.pending_anchors.is_empty()
        || !state.pending_asserts.is_empty()
        || !state.pending_lookaheads.is_empty()
        || state.needs_whole_match;
    if needs_final_checks {
        let final_chars: Vec<char> = result.chars().collect();
        for anchor in &state.pending_anchors {
            if !at_matches(&anchor.at, &final_chars, anchor.char_pos, anchor.flags) {
                return Ok(None);
            }
        }
        for pending in &state.pending_asserts {
            let holds = if pending.direction >= 0 {
                match_seq(
                    &pending.pattern.data,
                    pending.char_pos,
                    &final_chars,
                    pending.flags,
                    &pending.groups,
                )
                .is_some()
            } else {
                (0..=pending.char_pos).any(|start| {
                    match_seq(
                        &pending.pattern.data,
                        start,
                        &final_chars,
                        pending.flags,
                        &pending.groups,
                    ) == Some(pending.char_pos)
                })
            };
            if !holds {
                return Ok(None);
            }
        }
        for pending in &state.pending_lookaheads {
            if match_seq(
                &pending.pattern.data,
                pending.char_pos,
                &final_chars,
                pending.flags,
                &pending.groups,
            )
            .is_some()
            {
                return Ok(None);
            }
        }
        if state.needs_whole_match && !whole_match(parsed, fullmatch, &final_chars, &state.groups)
        {
            return Ok(None);
        }
    }

    Ok(Some(result))
}

/// Whether the pattern matches `chars` — anywhere for search semantics, or
/// spanning the whole string for `fullmatch`. Used as a post-generation
/// filter (Hypothesis filters every candidate through `re.search`) for the
/// constructs whose generated output is not a match by construction: atomic
/// groups and possessive repeats commit to a repetition count during
/// generation that the pattern's real (non-backtracking) semantics may not
/// admit.
fn whole_match(
    parsed: &ParsedPattern,
    fullmatch: bool,
    chars: &[char],
    groups: &HashMap<u32, String>,
) -> bool {
    if fullmatch {
        let mut anchored = parsed.pattern.data.clone();
        anchored.push(OpCode::At(AtCode::EndString));
        match_seq(&anchored, 0, chars, parsed.flags, groups).is_some()
    } else {
        (0..=chars.len())
            .any(|start| match_seq(&parsed.pattern.data, start, chars, parsed.flags, groups).is_some())
    }
}

/// Mutable state threaded through generation: captured groups (for
/// back-references) and the active regex flags (which change as we descend
/// into `SUBPATTERN` nodes with inline flag modifiers).
struct GenState<'a> {
    groups: HashMap<u32, String>,
    flags: u32,
    /// Whether the draw is anchored at both ends. Affects how lookaround
    /// assertions are generated: in fullmatch mode their bodies must not be
    /// emitted (the pattern has to consume the entire output), so they become
    /// deferred checks instead.
    fullmatch: bool,
    /// Zero-width anchors (`\b`, `\B`, and `$`/`\Z` in non-final positions)
    /// recorded during generation and checked against the final output
    /// string, since the content that follows them isn't known yet when they
    /// are reached.
    pending_anchors: Vec<PendingAnchor>,
    /// Positive lookaround assertions deferred in fullmatch mode.
    pending_asserts: Vec<PendingAssertNot>,
    /// Negative-lookahead assertions recorded during generation. Each entry
    /// captures the assertion body, the active flags, and a snapshot of the
    /// groups at the point of the assertion. We check them against the final
    /// output string in [`generate_regex`].
    pending_lookaheads: Vec<PendingAssertNot>,
    /// Set when the pattern contains an atomic group or possessive repeat,
    /// whose generated output must be re-validated against the whole pattern.
    needs_whole_match: bool,
    /// The enclosing [`CompiledRegex`]'s cross-draw cache of
    /// alphabet-constrained `IN`-node character sets.
    in_cache: &'a Mutex<HashMap<InKey, Arc<[char]>>>,
    /// The enclosing [`CompiledRegex`]'s cross-draw cache of
    /// alphabet-constrained `Any` / `NotLiteral` character sets.
    char_cache: &'a Mutex<HashMap<CharKey, Arc<[char]>>>,
}

struct PendingAnchor {
    char_pos: usize,
    at: AtCode,
    flags: u32,
}

#[derive(Clone)]
struct PendingAssertNot {
    char_pos: usize,
    direction: i32,
    pattern: SubPattern,
    flags: u32,
    groups: HashMap<u32, String>,
}

/// Draw 0..10 arbitrary characters from `alphabet` (or ASCII 32..126 when
/// no alphabet is given). Used for prefix/suffix padding and for the
/// empty-pattern case.
fn draw_pad(
    ntc: &mut NativeTestCase,
    alphabet: &Option<IntervalSet>,
    out: &mut String,
) -> Result<(), EngineError> {
    let n = ntc
        .draw_integer(BigInt::from(0), BigInt::from(10))?
        .to_i128()
        .unwrap();
    for _ in 0..n {
        let c = draw_any_char(ntc, alphabet)?;
        out.push(c);
    }
    Ok(())
}

/// Return the first non-grouping OpCode in `sp`, descending through SUBPATTERN
/// and ATOMIC_GROUP nodes (which don't consume characters themselves).
///
/// Python's `regex_strategy` doesn't descend like this — it relies on
/// `regex.search` as a post-generation filter. We don't have a Python-compatible
/// regex matcher to filter against, so we peek through non-consuming wrappers
/// instead, which handles the common `(\Afoo\Z)` shape.
fn effective_first(sp: &SubPattern) -> Option<&OpCode> {
    let first = sp.data.first()?;
    match first {
        OpCode::Subpattern { p, .. } | OpCode::AtomicGroup(p) => effective_first(p),
        _ => Some(first),
    }
}

fn effective_last(sp: &SubPattern) -> Option<&OpCode> {
    let last = sp.data.last()?;
    match last {
        OpCode::Subpattern { p, .. } | OpCode::AtomicGroup(p) => effective_last(p),
        _ => Some(last),
    }
}

fn draw_prefix(
    ntc: &mut NativeTestCase,
    parsed: &ParsedPattern,
    alphabet: &Option<IntervalSet>,
    out: &mut String,
) -> Result<(), EngineError> {
    if let Some(OpCode::At(at)) = effective_first(&parsed.pattern) {
        match at {
            AtCode::BeginningString => return Ok(()),
            AtCode::Beginning => {
                if parsed.flags & SRE_FLAG_MULTILINE != 0 && alphabet_allows(alphabet, '\n') {
                    draw_pad(ntc, alphabet, out)?;
                    if !out.is_empty() {
                        out.push('\n');
                    }
                }
                return Ok(());
            }
            _ => {}
        }
    }
    draw_pad(ntc, alphabet, out)
}

fn draw_suffix(
    ntc: &mut NativeTestCase,
    parsed: &ParsedPattern,
    alphabet: &Option<IntervalSet>,
    out: &mut String,
) -> Result<(), EngineError> {
    if let Some(OpCode::At(at)) = effective_last(&parsed.pattern) {
        match at {
            AtCode::EndString => return Ok(()),
            AtCode::End => {
                if !alphabet_allows(alphabet, '\n') {
                    return Ok(());
                }
                if parsed.flags & SRE_FLAG_MULTILINE != 0 {
                    if ntc.weighted(0.5, None)? {
                        out.push('\n');
                        draw_pad(ntc, alphabet, out)?;
                    }
                } else if ntc.weighted(0.5, None)? {
                    out.push('\n');
                }
                return Ok(());
            }
            _ => {}
        }
    }
    draw_pad(ntc, alphabet, out)
}

/// Recursively generate a string from a `SubPattern`, appending to `out`.
fn generate_subpattern(
    ntc: &mut NativeTestCase,
    sp: &SubPattern,
    state: &mut GenState,
    alphabet: &Option<IntervalSet>,
    out: &mut String,
) -> Result<(), EngineError> {
    for op in &sp.data {
        generate_op(ntc, op, state, alphabet, out)?;
    }
    Ok(())
}
fn generate_op(
    ntc: &mut NativeTestCase,
    op: &OpCode,
    state: &mut GenState,
    alphabet: &Option<IntervalSet>,
    out: &mut String,
) -> Result<(), EngineError> {
    match op {
        OpCode::Literal(cp) => {
            let c = codepoint_to_char(*cp);
            if state.flags & SRE_FLAG_IGNORECASE != 0 {
                if let Some(sw) = char_swapcase(c) {
                    let which = ntc
                        .draw_integer(BigInt::from(0), BigInt::from(1))?
                        .to_i128()
                        .unwrap();
                    let pick = if which == 0 { c } else { sw };
                    if !alphabet_allows(alphabet, pick) {
                        mark_invalid(ntc)?;
                    }
                    out.push(pick);
                    return Ok(());
                }
            }
            if !alphabet_allows(alphabet, c) {
                mark_invalid(ntc)?;
            }
            out.push(c);
        }
        OpCode::NotLiteral(cp) => {
            if alphabet.is_none() {
                let chars = cached_default_not_literal(*cp, state.flags);
                emit_from_chars(ntc, &chars, out)?;
            } else {
                let chars = cached_chars(
                    state.char_cache,
                    (*cp, state.flags & SRE_FLAG_IGNORECASE),
                    || {
                        let c = codepoint_to_char(*cp);
                        let blacklist = swapcase_blacklist(c, state.flags);
                        gather_chars(alphabet, |c| !blacklist.contains(&c))
                    },
                );
                emit_from_chars(ntc, &chars, out)?;
            }
        }
        OpCode::Any => {
            let allow_newline = state.flags & SRE_FLAG_DOTALL != 0;
            let chars = if alphabet.is_none() {
                cached_default_any(allow_newline)
            } else {
                cached_chars(
                    state.char_cache,
                    (u32::MAX, state.flags & SRE_FLAG_DOTALL),
                    || gather_chars(alphabet, |c| allow_newline || c != '\n'),
                )
            };
            emit_from_chars(ntc, &chars, out)?;
        }
        OpCode::At(at) => match at {
            AtCode::BeginningString if !out.is_empty() => {
                mark_invalid(ntc)?;
            }
            AtCode::BeginningString => {}
            AtCode::Beginning => {
                if state.flags & SRE_FLAG_MULTILINE != 0 {
                    if !out.is_empty() && !out.ends_with('\n') {
                        mark_invalid(ntc)?;
                    }
                } else if !out.is_empty() {
                    mark_invalid(ntc)?;
                }
            }
            AtCode::End | AtCode::EndString | AtCode::Boundary | AtCode::NonBoundary => {
                state.pending_anchors.push(PendingAnchor {
                    char_pos: out.chars().count(),
                    at: *at,
                    flags: state.flags,
                });
            }
        },
        OpCode::In(items) => {
            if alphabet.is_none() {
                let chars = cached_default_in_set(items, state.flags);
                emit_from_chars(ntc, &chars, out)?;
            } else {
                let key = (items.as_ptr() as usize, items.len(), state.flags);
                let cached = state.in_cache.lock().unwrap().get(&key).cloned();
                let chars = match cached {
                    Some(cached) => cached,
                    None => {
                        let computed: Arc<[char]> =
                            build_in_set(items, state.flags, alphabet).into();
                        state
                            .in_cache
                            .lock()
                            .unwrap()
                            .insert(key, Arc::clone(&computed));
                        computed
                    }
                };
                emit_from_chars(ntc, &chars, out)?;
            }
        }
        OpCode::Branch(items) => {
            let idx = ntc
                .draw_integer(BigInt::from(0), BigInt::from(items.len() as i64 - 1))?
                .to_i128()
                .unwrap() as usize;
            generate_subpattern(ntc, &items[idx], state, alphabet, out)?;
        }
        OpCode::Subpattern {
            group,
            add_flags,
            del_flags,
            p,
        } => {
            let saved_flags = state.flags;
            state.flags = (state.flags | *add_flags) & !*del_flags;
            let before = out.len();
            generate_subpattern(ntc, p, state, alphabet, out)?;
            state.flags = saved_flags;
            if let Some(gid) = group {
                state.groups.insert(*gid, out[before..].to_string());
            }
        }
        OpCode::GroupRef(gid) => {
            let Some(val) = state.groups.get(gid).cloned() else {
                return mark_invalid(ntc);
            };
            out.push_str(&val);
        }
        OpCode::GroupRefExists {
            cond_group,
            yes,
            no,
        } => {
            if state.groups.contains_key(cond_group) {
                generate_subpattern(ntc, yes, state, alphabet, out)?;
            } else if let Some(no) = no {
                generate_subpattern(ntc, no, state, alphabet, out)?;
            }
        }
        OpCode::Assert { direction, p } => {
            if state.fullmatch {
                state.pending_asserts.push(PendingAssertNot {
                    char_pos: out.chars().count(),
                    direction: *direction,
                    pattern: p.clone(),
                    flags: state.flags,
                    groups: state.groups.clone(),
                });
            } else {
                generate_subpattern(ntc, p, state, alphabet, out)?;
            }
        }
        OpCode::AssertNot { direction, p } => {
            if *direction < 0 {
                let out_chars: Vec<char> = out.chars().collect();
                let end = out_chars.len();
                for start in 0..=end {
                    if match_seq(&p.data, start, &out_chars, state.flags, &state.groups)
                        == Some(end)
                    {
                        mark_invalid(ntc)?;
                    }
                }
            } else {
                state.pending_lookaheads.push(PendingAssertNot {
                    char_pos: out.chars().count(),
                    direction: *direction,
                    pattern: p.clone(),
                    flags: state.flags,
                    groups: state.groups.clone(),
                });
            }
        }
        OpCode::Failure => {
            mark_invalid(ntc)?;
        }
        OpCode::AtomicGroup(p) => {
            state.needs_whole_match = true;
            generate_subpattern(ntc, p, state, alphabet, out)?;
        }
        OpCode::MaxRepeat { min, max, item }
        | OpCode::MinRepeat { min, max, item }
        | OpCode::PossessiveRepeat { min, max, item } => {
            if matches!(op, OpCode::PossessiveRepeat { .. }) {
                state.needs_whole_match = true;
            }
            let min = *min as usize;
            let max = if *max == u32::MAX {
                None
            } else {
                Some(*max as usize)
            };
            let mut ms = ManyState::new(min, max);
            loop {
                if !many_more(ntc, &mut ms)? {
                    break;
                }
                generate_subpattern(ntc, item, state, alphabet, out)?;
            }
        }
    }
    Ok(())
}
/// Cached version of [`build_in_set`] for the default (no-alphabet) case.
///
/// Category-driven classes like `\w`, `\s`, `[^a-z0-9_]` require a full
/// 65 536-codepoint BMP scan to compute, and the parser yields distinct
/// `SetItem` slices for distinct regex patterns — so the state-level
/// pointer cache only helps within one draw. Patterns like `\w` alone
/// cost ~35ms per draw in debug; 10 draws trips the 1s TooSlow health
/// check on slower CI runners. Since the default alphabet is fixed, we
/// can memoise across draws (and across patterns).
fn cached_default_in_set(items: &[SetItem], flags: u32) -> Arc<[char]> {
    type Cache = Mutex<HashMap<(Vec<SetItem>, u32), Arc<[char]>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache_key = (items.to_vec(), flags & (SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII));
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&cache_key) {
            return Arc::clone(cached);
        }
    }
    let computed: Arc<[char]> = build_in_set(items, flags, &None).into();
    cache
        .lock()
        .unwrap()
        .insert(cache_key, Arc::clone(&computed));
    computed
}

/// Cached character set for `Any` nodes with the default alphabet: the whole
/// BMP minus surrogates (minus `'\n'` without DOTALL). Same rationale as
/// [`cached_default_in_set`] — the 64K-codepoint scan is too expensive to
/// repeat per drawn character.
fn cached_default_any(allow_newline: bool) -> Arc<[char]> {
    static CACHE: OnceLock<[Arc<[char]>; 2]> = OnceLock::new();
    let both = CACHE.get_or_init(|| {
        [
            gather_chars(&None, |c| c != '\n').into(),
            gather_chars(&None, |_| true).into(),
        ]
    });
    Arc::clone(&both[usize::from(allow_newline)])
}

/// Look up `key` in a per-[`CompiledRegex`] character-set cache, computing
/// and inserting it on a miss.
fn cached_chars<F: FnOnce() -> Vec<char>>(
    cache: &Mutex<HashMap<CharKey, Arc<[char]>>>,
    key: CharKey,
    compute: F,
) -> Arc<[char]> {
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&key) {
            return Arc::clone(cached);
        }
    }
    let computed: Arc<[char]> = compute().into();
    cache.lock().unwrap().insert(key, Arc::clone(&computed));
    computed
}

/// Cached character set for `NotLiteral` nodes with the default alphabet.
/// Same rationale as `cached_default_in_set`: `gather_chars` scans the
/// entire BMP (~64K codepoints) and is too expensive to repeat per draw.
fn cached_default_not_literal(cp: u32, flags: u32) -> Arc<[char]> {
    type Cache = Mutex<HashMap<(u32, u32), Arc<[char]>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache_key = (cp, flags & SRE_FLAG_IGNORECASE);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&cache_key) {
            return Arc::clone(cached);
        }
    }
    let c = codepoint_to_char(cp);
    let blacklist = swapcase_blacklist(c, flags);
    let computed: Arc<[char]> = gather_chars(&None, |c| !blacklist.contains(&c)).into();
    cache
        .lock()
        .unwrap()
        .insert(cache_key, Arc::clone(&computed));
    computed
}

/// Build the set of characters that a `(IN, items)` node can emit, after
/// applying the current flags (IGNORECASE swaps, ASCII restriction) and
/// intersecting with the user-supplied alphabet.
fn build_in_set(items: &[SetItem], flags: u32, alphabet: &Option<IntervalSet>) -> Vec<char> {
    let negate = matches!(items.first(), Some(SetItem::Negate));

    let mut positive: Vec<char> = Vec::new();
    let mut categories: Vec<ChCode> = Vec::new();

    for item in items {
        match item {
            SetItem::Negate => {}
            SetItem::Literal(cp) => {
                let c = codepoint_to_char(*cp);
                add_with_swapcase(&mut positive, c, flags);
            }
            SetItem::Range(lo, hi) => {
                for cp in *lo..=*hi {
                    if let Some(c) = char::from_u32(cp) {
                        add_with_swapcase(&mut positive, c, flags);
                    }
                }
            }
            SetItem::Category(cat) => {
                categories.push(*cat);
            }
        }
    }

    let ascii_only = flags & SRE_FLAG_ASCII != 0;

    if !negate {
        let mut out: Vec<char> = Vec::new();
        let mut seen: HashSet<char> = HashSet::new();
        for c in positive {
            if ascii_only && (c as u32) >= 128 {
                continue;
            }
            if !alphabet_allows(alphabet, c) {
                continue;
            }
            if seen.insert(c) {
                out.push(c);
            }
        }
        if !categories.is_empty() {
            let cat_chars = gather_chars(alphabet, |c| {
                categories.iter().any(|cat| in_category(c, *cat))
            });
            for c in cat_chars {
                if ascii_only && (c as u32) >= 128 {
                    continue;
                }
                if seen.insert(c) {
                    out.push(c);
                }
            }
        }
        out
    } else {
        let cat_blocks: Vec<ChCode> = categories;
        let mut positive_set: HashSet<char> = HashSet::new();
        for c in positive {
            positive_set.extend(swapcase_blacklist(c, flags));
        }
        gather_chars(alphabet, |c| {
            if ascii_only && (c as u32) >= 128 {
                return false;
            }
            if positive_set.contains(&c) {
                return false;
            }
            if cat_blocks.iter().any(|cat| in_category(c, *cat)) {
                return false;
            }
            true
        })
    }
}
fn add_with_swapcase(v: &mut Vec<char>, c: char, flags: u32) {
    v.push(c);
    if flags & SRE_FLAG_IGNORECASE != 0 {
        if let Some(sw) = char_swapcase(c) {
            v.push(sw);
        }
    }
}

/// Return whether codepoint `c` is in the given CPython character category.
fn in_category(c: char, cat: ChCode) -> bool {
    let cp = c as u32;
    match cat {
        ChCode::Digit => unicodedata::is_in_group(cp, "Nd"),
        ChCode::NotDigit => !unicodedata::is_in_group(cp, "Nd"),
        ChCode::Space => is_uni_space(c),
        ChCode::NotSpace => !is_uni_space(c),
        ChCode::Word => is_uni_word(c),
        ChCode::NotWord => !is_uni_word(c),
    }
}
fn is_uni_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\x1f' | '\u{85}'
    ) || unicodedata::is_in_group(c as u32, "Z")
}

fn is_uni_word(c: char) -> bool {
    c == '_' || unicodedata::is_in_group(c as u32, "L") || unicodedata::is_in_group(c as u32, "N")
}

/// Gather all characters in `alphabet` (or BMP-minus-surrogates when no
/// alphabet is given) that satisfy `predicate`. The default scan is bounded
/// to the BMP to keep `.`, `\w`, `[^a]`, etc. tractable on the unconstrained
/// path.
fn gather_chars<F: Fn(char) -> bool>(alphabet: &Option<IntervalSet>, predicate: F) -> Vec<char> {
    let mut out = Vec::new();
    match alphabet {
        None => {
            for cp in 0u32..=0xFFFF {
                if is_surrogate_cp(cp) {
                    continue;
                }
                if let Some(c) = char::from_u32(cp) {
                    if predicate(c) {
                        out.push(c);
                    }
                }
            }
        }
        Some(intervals) => {
            for &(start, end) in &intervals.intervals {
                for cp in start..=end {
                    if let Some(c) = char::from_u32(cp) {
                        if predicate(c) {
                            out.push(c);
                        }
                    }
                }
            }
        }
    }
    out
}

fn alphabet_allows(alphabet: &Option<IntervalSet>, c: char) -> bool {
    match alphabet {
        None => !is_surrogate_cp(c as u32),
        Some(intervals) => intervals.contains(c as u32),
    }
}

/// Pick an arbitrary character from the alphabet. Used for prefix/suffix
/// padding — must never fail if the alphabet is non-empty.
fn draw_any_char(
    ntc: &mut NativeTestCase,
    alphabet: &Option<IntervalSet>,
) -> Result<char, EngineError> {
    match alphabet {
        None => {
            let cp = ntc
                .draw_integer(BigInt::from(32), BigInt::from(126))?
                .to_i128()
                .unwrap();
            Ok(char::from_u32(cp as u32).expect("ASCII codepoint"))
        }
        Some(intervals) => {
            let n = intervals.len();
            if n == 0 {
                mark_invalid(ntc)?;
                unreachable!("mark_invalid returns Err — control flow does not reach here")
            }
            let idx = ntc
                .draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?
                .to_i128()
                .unwrap();
            let cp = intervals
                .get(idx as isize)
                .expect("draw_integer respects len bound");
            Ok(char::from_u32(cp).expect("IntervalSet excludes surrogates"))
        }
    }
}
fn emit_from_chars(
    ntc: &mut NativeTestCase,
    chars: &[char],
    out: &mut String,
) -> Result<(), EngineError> {
    if chars.is_empty() {
        mark_invalid(ntc)?;
    }
    let n = chars.len();
    let idx = if n > 256 && ntc.weighted(0.8, None)? {
        ntc.draw_integer(BigInt::from(0), BigInt::from(255))?
            .to_i128()
            .unwrap() as usize
    } else if n > 256 {
        ntc.draw_integer(BigInt::from(256), BigInt::from(n as i64 - 1))?
            .to_i128()
            .unwrap() as usize
    } else {
        ntc.draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?
            .to_i128()
            .unwrap() as usize
    };
    out.push(chars[idx]);
    Ok(())
}

fn mark_invalid(ntc: &mut NativeTestCase) -> Result<(), EngineError> {
    ntc.conclude(Status::Invalid, None);
    Err(EngineError::InvalidTestCase)
}

fn codepoint_to_char(cp: u32) -> char {
    char::from_u32(cp).unwrap_or_else(|| panic!("invalid codepoint in regex AST: {:#x}", cp))
}

/// Python's `str.swapcase()` on a single char.  Python's definition differs
/// from Rust's only for a handful of title-case codepoints; for the regex
/// strategy the straightforward case-flipping-or-identity behaviour is
/// sufficient, matching what `re.IGNORECASE` does on single-character
/// matches.
///
/// Returns `None` when there is no usable swap: the character is uncased,
/// maps to itself, or its case mapping is more than one codepoint (e.g.
/// `'ß'.to_uppercase()` is `"SS"`, which `re.IGNORECASE` does *not* treat
/// as equal to `'ß'` — Hypothesis guards this with an explicit `re.match`
/// check before offering the swapped form).
fn char_swapcase(c: char) -> Option<char> {
    match swapcase_chars(c).as_slice() {
        [sw] if *sw != c => Some(*sw),
        _ => None,
    }
}

/// All codepoints of `c`'s swapped-case form (possibly several, e.g.
/// `'İ'.to_lowercase()` is `"i\u{307}"`); empty for uncased characters.
fn swapcase_chars(c: char) -> Vec<char> {
    if c.is_lowercase() {
        c.to_uppercase().collect()
    } else if c.is_uppercase() {
        c.to_lowercase().collect()
    } else {
        Vec::new()
    }
}

/// The set of characters `re.IGNORECASE` may treat as equal to `c`: chain
/// `swapcase` to a fixpoint, expanding multi-character results, so e.g.
/// `(?i)[^İ]` excludes {İ, i, U+0307, I}. Port of Hypothesis's fix for
/// issue #2657 ("patterns such as r\"[^\\u0130]+\" where \"i\\u0307\"
/// matches").
fn swapcase_blacklist(c: char, flags: u32) -> Vec<char> {
    let mut blacklist = vec![c];
    if flags & SRE_FLAG_IGNORECASE == 0 {
        return blacklist;
    }
    let mut stack = swapcase_chars(c);
    while let Some(ch) = stack.pop() {
        if !blacklist.contains(&ch) {
            blacklist.push(ch);
            stack.extend(swapcase_chars(ch));
        }
    }
    blacklist
}

fn match_seq(
    ops: &[OpCode],
    pos: usize,
    chars: &[char],
    flags: u32,
    groups: &HashMap<u32, String>,
) -> Option<usize> {
    let Some((first, rest)) = ops.split_first() else {
        return Some(pos);
    };
    match first {
        OpCode::Literal(cp) => {
            let want = char::from_u32(*cp)?;
            let got = *chars.get(pos)?;
            if chars_eq(got, want, flags) {
                match_seq(rest, pos + 1, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::NotLiteral(cp) => {
            let banned = char::from_u32(*cp)?;
            let got = *chars.get(pos)?;
            if chars_eq(got, banned, flags) {
                None
            } else {
                match_seq(rest, pos + 1, chars, flags, groups)
            }
        }
        OpCode::Any => {
            let got = *chars.get(pos)?;
            if got == '\n' && flags & SRE_FLAG_DOTALL == 0 {
                None
            } else {
                match_seq(rest, pos + 1, chars, flags, groups)
            }
        }
        OpCode::In(items) => {
            let got = *chars.get(pos)?;
            if char_matches_set(items, got, flags) {
                match_seq(rest, pos + 1, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::At(at) => {
            if at_matches(at, chars, pos, flags) {
                match_seq(rest, pos, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::Branch(branches) => {
            for br in branches {
                let mut combined = br.data.clone();
                combined.extend_from_slice(rest);
                if let Some(end) = match_seq(&combined, pos, chars, flags, groups) {
                    return Some(end);
                }
            }
            None
        }
        OpCode::Subpattern {
            add_flags,
            del_flags,
            p,
            ..
        } => {
            let inner_flags = (flags | *add_flags) & !*del_flags;
            let end = match_seq(&p.data, pos, chars, inner_flags, groups)?;
            match_seq(rest, end, chars, flags, groups)
        }
        OpCode::AtomicGroup(p) => {
            let end = match_seq(&p.data, pos, chars, flags, groups)?;
            match_seq(rest, end, chars, flags, groups)
        }
        OpCode::GroupRef(gid) => {
            let val = groups.get(gid)?;
            let vcs: Vec<char> = val.chars().collect();
            if pos + vcs.len() > chars.len() {
                return None;
            }
            for (i, vc) in vcs.iter().enumerate() {
                if !chars_eq(chars[pos + i], *vc, flags) {
                    return None;
                }
            }
            match_seq(rest, pos + vcs.len(), chars, flags, groups)
        }
        OpCode::GroupRefExists {
            cond_group,
            yes,
            no,
        } => {
            let mut combined = if groups.contains_key(cond_group) {
                yes.data.clone()
            } else if let Some(n) = no {
                n.data.clone()
            } else {
                Vec::new()
            };
            combined.extend_from_slice(rest);
            match_seq(&combined, pos, chars, flags, groups)
        }
        OpCode::Assert { p, .. } => {
            if match_seq(&p.data, pos, chars, flags, groups).is_some() {
                match_seq(rest, pos, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::AssertNot { p, .. } => {
            if match_seq(&p.data, pos, chars, flags, groups).is_none() {
                match_seq(rest, pos, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::Failure => None,
        OpCode::MaxRepeat { min, max, item } => {
            let mn = *min as usize;
            let mx = if *max == u32::MAX {
                None
            } else {
                Some(*max as usize)
            };
            let mut positions = vec![pos];
            let mut cur = pos;
            let mut zero_width = false;
            loop {
                if let Some(m) = mx {
                    if positions.len() > m {
                        break;
                    }
                }
                match match_seq(&item.data, cur, chars, flags, groups) {
                    Some(next) if next > cur => {
                        cur = next;
                        positions.push(cur);
                    }
                    Some(_) => {
                        zero_width = true;
                        break;
                    }
                    None => break,
                }
            }
            let mn_eff = if zero_width { 0 } else { mn };
            if positions.len() - 1 < mn_eff {
                return None;
            }
            for i in (mn_eff..positions.len()).rev() {
                if let Some(end) = match_seq(rest, positions[i], chars, flags, groups) {
                    return Some(end);
                }
            }
            None
        }
        OpCode::MinRepeat { min, max, item } => {
            let mn = *min as usize;
            let mx = if *max == u32::MAX {
                None
            } else {
                Some(*max as usize)
            };
            let mut cur = pos;
            let mut count = 0usize;
            while count < mn {
                let next = match_seq(&item.data, cur, chars, flags, groups)?;
                if next <= cur {
                    count = mn;
                    break;
                }
                cur = next;
                count += 1;
            }
            loop {
                if let Some(end) = match_seq(rest, cur, chars, flags, groups) {
                    return Some(end);
                }
                if let Some(m) = mx {
                    if count >= m {
                        return None;
                    }
                }
                let next = match_seq(&item.data, cur, chars, flags, groups)?;
                if next <= cur {
                    return None;
                }
                cur = next;
                count += 1;
            }
        }
        OpCode::PossessiveRepeat { min, max, item } => {
            let mn = *min as usize;
            let mx = if *max == u32::MAX {
                None
            } else {
                Some(*max as usize)
            };
            let mut cur = pos;
            let mut count = 0usize;
            let mut zero_width = false;
            loop {
                if let Some(m) = mx {
                    if count >= m {
                        break;
                    }
                }
                match match_seq(&item.data, cur, chars, flags, groups) {
                    Some(next) if next > cur => {
                        cur = next;
                        count += 1;
                    }
                    Some(_) => {
                        zero_width = true;
                        break;
                    }
                    None => break,
                }
            }
            if count < mn && !zero_width {
                return None;
            }
            match_seq(rest, cur, chars, flags, groups)
        }
    }
}
fn chars_eq(a: char, b: char, flags: u32) -> bool {
    if a == b {
        return true;
    }
    if flags & SRE_FLAG_IGNORECASE != 0 {
        char_swapcase(a) == Some(b) || char_swapcase(b) == Some(a)
    } else {
        false
    }
}
fn char_matches_set(items: &[SetItem], c: char, flags: u32) -> bool {
    let negate = matches!(items.first(), Some(SetItem::Negate));
    let mut contained = false;
    for item in items {
        match item {
            SetItem::Negate => {}
            SetItem::Literal(cp) => {
                if let Some(lc) = char::from_u32(*cp) {
                    if chars_eq(c, lc, flags) {
                        contained = true;
                    }
                }
            }
            SetItem::Range(lo, hi) => {
                let cp = c as u32;
                if cp >= *lo && cp <= *hi {
                    contained = true;
                } else if flags & SRE_FLAG_IGNORECASE != 0 {
                    if let Some(sw) = char_swapcase(c) {
                        if (sw as u32) >= *lo && (sw as u32) <= *hi {
                            contained = true;
                        }
                    }
                }
            }
            SetItem::Category(cat) => {
                if in_category(c, *cat) {
                    contained = true;
                }
            }
        }
    }
    if negate { !contained } else { contained }
}
fn at_matches(at: &AtCode, chars: &[char], pos: usize, flags: u32) -> bool {
    match at {
        AtCode::BeginningString => pos == 0,
        AtCode::Beginning => {
            if flags & SRE_FLAG_MULTILINE != 0 {
                pos == 0 || chars[pos - 1] == '\n'
            } else {
                pos == 0
            }
        }
        AtCode::End => {
            if flags & SRE_FLAG_MULTILINE != 0 {
                pos == chars.len() || chars[pos] == '\n'
            } else {
                pos == chars.len() || (pos + 1 == chars.len() && chars[pos] == '\n')
            }
        }
        AtCode::EndString => pos == chars.len(),
        AtCode::Boundary => is_word_boundary(chars, pos),
        AtCode::NonBoundary => !is_word_boundary(chars, pos),
    }
}
fn is_word_boundary(chars: &[char], pos: usize) -> bool {
    let before = pos > 0 && is_uni_word(chars[pos - 1]);
    let after = pos < chars.len() && is_uni_word(chars[pos]);
    before != after
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/regex_tests.rs"]
mod tests;
