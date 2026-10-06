use crate::control::{InternalError, hegel_internal_unwrap};
use crate::native::{HashMap, HashSet};
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, ManyState, NativeTestCase, Status};
use crate::native::intervalsets::IntervalSet;
use crate::native::re::casefold;
use crate::native::re::constants::{
    AtCode, ChCode, SRE_FLAG_ASCII, SRE_FLAG_DOTALL, SRE_FLAG_IGNORECASE, SRE_FLAG_MULTILINE,
};
use crate::native::re::parser::{OpCode, ParsedPattern, SetItem, SubPattern, parse_pattern};
use crate::sys::sync::{Lazy, Mutex};
use crate::unicodedata;

use super::many_more;

fn is_surrogate_cp(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

/// The flag bits that change which characters a literal or class matches.
const CASE_FLAGS: u32 = SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII;

/// Cache key for a `(IN, items)` node's character set: the `SetItem` slice's
/// address and length (stable because the AST is owned by the enclosing
/// [`CompiledRegex`]) plus the active flags (which affect IGNORECASE folding
/// and the ASCII-only categories).
type InKey = (usize, usize, u32);

/// Cache key for the alphabet-constrained `Any` and `NotLiteral` character
/// sets: the excluded codepoint (`u32::MAX` for `Any`, which has none) plus
/// the flag bits the set depends on.
type CharKey = (u32, u32);

/// Cache key for a subpattern's producibility: the address of its op slice
/// (stable, see [`InKey`]) plus the active flags.
type ProducibleKey = (usize, u32);

/// Cross-draw caches of the alphabet-constrained character sets (category
/// classes like `\w` cost a full alphabet scan to materialise) and of which
/// subpatterns the alphabet can produce at all.
#[derive(Debug)]
pub(crate) struct Caches {
    in_cache: Mutex<HashMap<InKey, Arc<[char]>>>,
    char_cache: Mutex<HashMap<CharKey, Arc<[char]>>>,
    producible: Mutex<HashMap<ProducibleKey, bool>>,
}

impl Default for Caches {
    fn default() -> Self {
        Caches {
            in_cache: Mutex::new(HashMap::default()),
            char_cache: Mutex::new(HashMap::default()),
            producible: Mutex::new(HashMap::default()),
        }
    }
}

/// A regex pattern compiled once at string-generator construction time:
/// the parsed AST, the optional user alphabet, and the [`Caches`] shared by
/// every draw from it.
#[derive(Debug)]
pub(crate) struct CompiledRegex {
    parsed: ParsedPattern,
    alphabet: Option<IntervalSet>,
    caches: Caches,
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
            caches: Caches::default(),
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
    Err(mark_invalid(ntc))
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
        groups: HashMap::default(),
        flags: parsed.flags,
        fullmatch,
        pending_anchors: Vec::new(),
        pending_asserts: Vec::new(),
        pending_lookaheads: Vec::new(),
        needs_whole_match: false,
        caches: &re.caches,
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
        if state.needs_whole_match && !whole_match(parsed, fullmatch, &final_chars, &state.groups) {
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
        (0..=chars.len()).any(|start| {
            match_seq(&parsed.pattern.data, start, chars, parsed.flags, groups).is_some()
        })
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
    /// The enclosing [`CompiledRegex`]'s cross-draw caches.
    caches: &'a Caches,
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

/// Draw an index into a non-empty candidate list, without drawing at all
/// when there is only one candidate.
fn pick_index(ntc: &mut NativeTestCase, n: usize) -> Result<usize, EngineError> {
    if n == 1 {
        return Ok(0);
    }
    Ok(ntc
        .draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?
        .to_i128()
        .unwrap() as usize)
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
            let c = codepoint_to_char(*cp)?;
            let candidates = literal_candidates(c, state.flags, alphabet);
            if candidates.is_empty() {
                return Err(mark_invalid(ntc));
            }
            let idx = pick_index(ntc, candidates.len())?;
            out.push(candidates[idx]);
        }
        OpCode::NotLiteral(cp) => {
            let c = codepoint_to_char(*cp)?;
            let chars = not_literal_chars(c, state.flags, alphabet, state.caches);
            emit_from_chars(ntc, &chars, out)?;
        }
        OpCode::Any => {
            let chars = any_chars(state.flags, alphabet, state.caches);
            emit_from_chars(ntc, &chars, out)?;
        }
        OpCode::At(at) => match at {
            AtCode::BeginningString if !out.is_empty() => {
                return Err(mark_invalid(ntc));
            }
            AtCode::BeginningString => {}
            AtCode::Beginning => {
                if state.flags & SRE_FLAG_MULTILINE != 0 {
                    if !out.is_empty() && !out.ends_with('\n') {
                        return Err(mark_invalid(ntc));
                    }
                } else if !out.is_empty() {
                    return Err(mark_invalid(ntc));
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
            let chars = in_chars(items, state.flags, alphabet, state.caches)?;
            emit_from_chars(ntc, &chars, out)?;
        }
        OpCode::Branch(items) => {
            let mut live: Vec<&SubPattern> = Vec::with_capacity(items.len());
            for item in items {
                if producible_sub(item, state.flags, alphabet, state.caches)? {
                    live.push(item);
                }
            }
            if live.is_empty() {
                return Err(mark_invalid(ntc));
            }
            let idx = pick_index(ntc, live.len())?;
            generate_subpattern(ntc, live[idx], state, alphabet, out)?;
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
                return Err(mark_invalid(ntc));
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
                        return Err(mark_invalid(ntc));
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
            return Err(mark_invalid(ntc));
        }
        OpCode::AtomicGroup(p) => {
            state.needs_whole_match = true;
            generate_subpattern(ntc, p, state, alphabet, out)?;
        }
        OpCode::MaxRepeat { min, max, item }
        | OpCode::MinRepeat { min, max, item }
        | OpCode::PossessiveRepeat { min, max, item } => {
            if *min == 0 && !producible_sub(item, state.flags, alphabet, state.caches)? {
                return Ok(());
            }
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

/// Whether the alphabet can supply every character `sp` needs, under
/// `flags`: false when some literal, class or wildcard in it has no
/// candidates, when every branch of an alternation is unproducible, or when
/// a required repetition's body is. Anchors, assertions and back-references
/// are taken as producible: they fail (if at all) on the final string, not
/// for want of characters.
///
/// A repetition whose body is unproducible is generated zero times rather
/// than rejected, and an alternation picks only among its producible
/// branches, so `(?-i:Ā)*k` over an ASCII alphabet produces `k` instead of
/// tripping the filter-too-much health check.
fn producible_sub(
    sp: &SubPattern,
    flags: u32,
    alphabet: &Option<IntervalSet>,
    caches: &Caches,
) -> Result<bool, InternalError> {
    if sp.data.is_empty() {
        return Ok(true);
    }
    let key = (sp.data.as_ptr() as usize, flags);
    if let Some(&cached) = caches.producible.lock().get(&key) {
        return Ok(cached);
    }
    let mut result = true;
    for op in &sp.data {
        if !producible_op(op, flags, alphabet, caches)? {
            result = false;
            break;
        }
    }
    caches.producible.lock().insert(key, result);
    Ok(result)
}

fn producible_op(
    op: &OpCode,
    flags: u32,
    alphabet: &Option<IntervalSet>,
    caches: &Caches,
) -> Result<bool, InternalError> {
    Ok(match op {
        OpCode::Literal(cp) => {
            !literal_candidates(codepoint_to_char(*cp)?, flags, alphabet).is_empty()
        }
        OpCode::NotLiteral(cp) => {
            !not_literal_chars(codepoint_to_char(*cp)?, flags, alphabet, caches).is_empty()
        }
        OpCode::Any => !any_chars(flags, alphabet, caches).is_empty(),
        OpCode::In(items) => !in_chars(items, flags, alphabet, caches)?.is_empty(),
        OpCode::At(_) | OpCode::GroupRef(_) | OpCode::AssertNot { .. } => true,
        OpCode::Failure => false,
        OpCode::Branch(items) => {
            let mut any = false;
            for item in items {
                if producible_sub(item, flags, alphabet, caches)? {
                    any = true;
                    break;
                }
            }
            any
        }
        OpCode::Subpattern {
            add_flags,
            del_flags,
            p,
            ..
        } => producible_sub(p, (flags | *add_flags) & !*del_flags, alphabet, caches)?,
        OpCode::GroupRefExists { yes, no, .. } => {
            producible_sub(yes, flags, alphabet, caches)?
                || match no {
                    Some(no) => producible_sub(no, flags, alphabet, caches)?,
                    None => true,
                }
        }
        OpCode::Assert { p, .. } | OpCode::AtomicGroup(p) => {
            producible_sub(p, flags, alphabet, caches)?
        }
        OpCode::MaxRepeat { min, item, .. }
        | OpCode::MinRepeat { min, item, .. }
        | OpCode::PossessiveRepeat { min, item, .. } => {
            *min == 0 || producible_sub(item, flags, alphabet, caches)?
        }
    })
}

/// The characters a literal `c` may be emitted as, `c` first: its
/// case-equivalents under IGNORECASE, restricted to the alphabet.
fn literal_candidates(c: char, flags: u32, alphabet: &Option<IntervalSet>) -> Vec<char> {
    let mut candidates = Vec::new();
    casefold::push_equivalents(&mut candidates, c, flags);
    let mut seen: HashSet<char> = HashSet::default();
    candidates.retain(|&x| alphabet_allows(alphabet, x) && seen.insert(x));
    candidates
}

/// The characters a `(IN, items)` node may emit under `flags`, restricted to
/// the alphabet; cached per node and flags.
fn in_chars(
    items: &[SetItem],
    flags: u32,
    alphabet: &Option<IntervalSet>,
    caches: &Caches,
) -> Result<Arc<[char]>, InternalError> {
    if alphabet.is_none() {
        return cached_default_in_set(items, flags);
    }
    let key = (items.as_ptr() as usize, items.len(), flags & CASE_FLAGS);
    let cached = caches.in_cache.lock().get(&key).cloned();
    match cached {
        Some(cached) => Ok(cached),
        None => {
            let computed: Arc<[char]> = build_in_set(items, flags, alphabet)?.into();
            caches.in_cache.lock().insert(key, Arc::clone(&computed));
            Ok(computed)
        }
    }
}

/// The characters a `(NOT_LITERAL, c)` node may emit under `flags`: every
/// alphabet character that does not match the literal `c`.
fn not_literal_chars(
    c: char,
    flags: u32,
    alphabet: &Option<IntervalSet>,
    caches: &Caches,
) -> Arc<[char]> {
    if alphabet.is_none() {
        return cached_default_not_literal(c, flags);
    }
    cached_chars(&caches.char_cache, (c as u32, flags & CASE_FLAGS), || {
        gather_chars(alphabet, |x| !casefold::literal_matches(c, x, flags))
    })
}

/// The characters an `ANY` node may emit under `flags`: the alphabet minus
/// the newline unless DOTALL.
fn any_chars(flags: u32, alphabet: &Option<IntervalSet>, caches: &Caches) -> Arc<[char]> {
    let allow_newline = flags & SRE_FLAG_DOTALL != 0;
    if alphabet.is_none() {
        return cached_default_any(allow_newline);
    }
    cached_chars(
        &caches.char_cache,
        (u32::MAX, flags & SRE_FLAG_DOTALL),
        || gather_chars(alphabet, |c| allow_newline || c != '\n'),
    )
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
fn cached_default_in_set(items: &[SetItem], flags: u32) -> Result<Arc<[char]>, InternalError> {
    type Cache = Mutex<HashMap<(Vec<SetItem>, u32), Arc<[char]>>>;
    static CACHE: Lazy<Cache> = Lazy::new(|| Mutex::new(HashMap::default()));
    let cache_key = (items.to_vec(), flags & CASE_FLAGS);
    {
        let guard = CACHE.lock();
        if let Some(cached) = guard.get(&cache_key) {
            return Ok(Arc::clone(cached));
        }
    }
    let computed: Arc<[char]> = build_in_set(items, flags, &None)?.into();
    CACHE.lock().insert(cache_key, Arc::clone(&computed));
    Ok(computed)
}

/// Cached character set for `Any` nodes with the default alphabet: the whole
/// BMP minus surrogates (minus `'\n'` without DOTALL). Same rationale as
/// [`cached_default_in_set`] — the 64K-codepoint scan is too expensive to
/// repeat per drawn character.
fn cached_default_any(allow_newline: bool) -> Arc<[char]> {
    static CACHE: Lazy<[Arc<[char]>; 2]> = Lazy::new(|| {
        [
            gather_chars(&None, |c| c != '\n').into(),
            gather_chars(&None, |_| true).into(),
        ]
    });
    Arc::clone(&CACHE[usize::from(allow_newline)])
}

/// Look up `key` in a per-[`CompiledRegex`] character-set cache, computing
/// and inserting it on a miss.
fn cached_chars<F: FnOnce() -> Vec<char>>(
    cache: &Mutex<HashMap<CharKey, Arc<[char]>>>,
    key: CharKey,
    compute: F,
) -> Arc<[char]> {
    {
        let guard = cache.lock();
        if let Some(cached) = guard.get(&key) {
            return Arc::clone(cached);
        }
    }
    let computed: Arc<[char]> = compute().into();
    cache.lock().insert(key, Arc::clone(&computed));
    computed
}

/// Cached character set for `NotLiteral` nodes with the default alphabet.
/// Same rationale as `cached_default_in_set`: `gather_chars` scans the
/// entire BMP (~64K codepoints) and is too expensive to repeat per draw.
fn cached_default_not_literal(c: char, flags: u32) -> Arc<[char]> {
    type Cache = Mutex<HashMap<(u32, u32), Arc<[char]>>>;
    static CACHE: Lazy<Cache> = Lazy::new(|| Mutex::new(HashMap::default()));
    let cache_key = (c as u32, flags & CASE_FLAGS);
    {
        let guard = CACHE.lock();
        if let Some(cached) = guard.get(&cache_key) {
            return Arc::clone(cached);
        }
    }
    let computed: Arc<[char]> =
        gather_chars(&None, |x| !casefold::literal_matches(c, x, flags)).into();
    CACHE.lock().insert(cache_key, Arc::clone(&computed));
    computed
}

/// Build the set of characters that a `(IN, items)` node can emit under
/// `flags`, intersected with the user-supplied alphabet.
///
/// A negated class is the alphabet filtered through [`char_matches_set`].
/// A positive class lists its literals and ranges in pattern order (each
/// followed by its case-equivalents under IGNORECASE) and then the
/// characters of its categories; the ASCII flag narrows only the
/// categories and the case folding, never the explicit characters — `(?a)Ā`
/// still matches `Ā` in Python.
fn build_in_set(
    items: &[SetItem],
    flags: u32,
    alphabet: &Option<IntervalSet>,
) -> Result<Vec<char>, InternalError> {
    if matches!(items.first(), Some(SetItem::Negate)) {
        return Ok(gather_chars(alphabet, |c| {
            char_matches_set(items, c, flags)
        }));
    }

    let mut out: Vec<char> = Vec::new();
    let mut seen: HashSet<char> = HashSet::default();
    let mut equivalents: Vec<char> = Vec::new();
    let mut categories: Vec<ChCode> = Vec::new();
    let mut add_equivalents = |c: char| {
        equivalents.clear();
        casefold::push_equivalents(&mut equivalents, c, flags);
        for &x in &equivalents {
            if alphabet_allows(alphabet, x) && seen.insert(x) {
                out.push(x);
            }
        }
    };

    for item in items {
        match item {
            SetItem::Negate => {}
            SetItem::Literal(cp) => add_equivalents(codepoint_to_char(*cp)?),
            SetItem::Range(lo, hi) => {
                for cp in *lo..=*hi {
                    if let Some(c) = char::from_u32(cp) {
                        add_equivalents(c);
                    }
                }
            }
            SetItem::Category(cat) => categories.push(*cat),
        }
    }

    if !categories.is_empty() {
        let ascii = flags & SRE_FLAG_ASCII != 0;
        for c in gather_chars(alphabet, |c| {
            categories.iter().any(|cat| in_category(c, *cat, ascii))
        }) {
            if seen.insert(c) {
                out.push(c);
            }
        }
    }
    Ok(out)
}

/// Return whether `c` is in the given CPython character category, with the
/// ASCII flag's narrower definitions (`\d` is `[0-9]`, `\s` is
/// `[ \t\n\r\f\v]`, `\w` is `[a-zA-Z0-9_]`) when `ascii`.
fn in_category(c: char, cat: ChCode, ascii: bool) -> bool {
    match cat {
        ChCode::Digit => {
            if ascii {
                c.is_ascii_digit()
            } else {
                unicodedata::is_in_group_char(c, "Nd")
            }
        }
        ChCode::NotDigit => !in_category(c, ChCode::Digit, ascii),
        ChCode::Space => {
            if ascii {
                matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
            } else {
                is_uni_space(c)
            }
        }
        ChCode::NotSpace => !in_category(c, ChCode::Space, ascii),
        ChCode::Word => is_word(c, ascii),
        ChCode::NotWord => !is_word(c, ascii),
    }
}

fn is_uni_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\x1f' | '\u{85}'
    ) || unicodedata::is_in_group_char(c, "Z")
}

fn is_word(c: char, ascii: bool) -> bool {
    if ascii {
        c == '_' || c.is_ascii_alphanumeric()
    } else {
        c == '_' || unicodedata::is_in_group_char(c, "L") || unicodedata::is_in_group_char(c, "N")
    }
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
            Ok(hegel_internal_unwrap!(
                char::from_u32(cp as u32),
                "draw_any_char: printable-ASCII draw {cp} is not a char"
            ))
        }
        Some(intervals) => {
            let n = intervals.len();
            if n == 0 {
                return Err(mark_invalid(ntc));
            }
            let idx = ntc
                .draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?
                .to_i128()
                .unwrap();
            let cp = hegel_internal_unwrap!(
                intervals.get(idx as isize),
                "draw_any_char: alphabet index {idx} out of bounds"
            );
            Ok(hegel_internal_unwrap!(
                char::from_u32(cp),
                "draw_any_char: alphabet codepoint {cp} is not a char"
            ))
        }
    }
}

fn emit_from_chars(
    ntc: &mut NativeTestCase,
    chars: &[char],
    out: &mut String,
) -> Result<(), EngineError> {
    if chars.is_empty() {
        return Err(mark_invalid(ntc));
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

fn mark_invalid(ntc: &mut NativeTestCase) -> EngineError {
    ntc.conclude(Status::Invalid, None);
    EngineError::InvalidTestCase
}

fn codepoint_to_char(cp: u32) -> Result<char, InternalError> {
    Ok(hegel_internal_unwrap!(
        char::from_u32(cp),
        "invalid codepoint in regex AST: {cp:#x}"
    ))
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
            if casefold::literal_matches(want, got, flags) {
                match_seq(rest, pos + 1, chars, flags, groups)
            } else {
                None
            }
        }
        OpCode::NotLiteral(cp) => {
            let banned = char::from_u32(*cp)?;
            let got = *chars.get(pos)?;
            if casefold::literal_matches(banned, got, flags) {
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
                if !casefold::groupref_eq(chars[pos + i], *vc, flags) {
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

/// Whether `c` matches the character class `items` under `flags`, with
/// Python's semantics: under IGNORECASE a literal or range matches every
/// character case-equal to one of its members (see [`casefold`]), and under
/// the ASCII flag the categories narrow to their ASCII definitions while
/// explicit characters and ranges keep matching outside ASCII.
fn char_matches_set(items: &[SetItem], c: char, flags: u32) -> bool {
    let negate = matches!(items.first(), Some(SetItem::Negate));
    let ascii = flags & SRE_FLAG_ASCII != 0;
    let contained = items.iter().any(|item| match item {
        SetItem::Negate => false,
        SetItem::Literal(cp) => {
            char::from_u32(*cp).is_some_and(|lc| casefold::literal_matches(lc, c, flags))
        }
        SetItem::Range(lo, hi) => casefold::range_matches(*lo, *hi, c, flags),
        SetItem::Category(cat) => in_category(c, *cat, ascii),
    });
    negate != contained
}

fn at_matches(at: &AtCode, chars: &[char], pos: usize, flags: u32) -> bool {
    let ascii = flags & SRE_FLAG_ASCII != 0;
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
        AtCode::Boundary => is_word_boundary(chars, pos, ascii),
        AtCode::NonBoundary => !is_word_boundary(chars, pos, ascii),
    }
}

fn is_word_boundary(chars: &[char], pos: usize, ascii: bool) -> bool {
    let before = pos > 0 && is_word(chars[pos - 1], ascii);
    let after = pos < chars.len() && is_word(chars[pos], ascii);
    before != after
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/regex_tests.rs"]
mod tests;
