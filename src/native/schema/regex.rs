// Regex schema interpreter.
//
// Port of Hypothesis's `strategies._internal.regex._strategy`, rewritten
// against our in-tree CPython `_parser` port (`src/native/re`). Walks the
// `SubPattern` AST directly so we match Python's `re` module semantics
// exactly, rather than adapting the subtly-different Rust regex-syntax
// grammar.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

use crate::cbor_utils::{as_bool, as_text, map_get};
use crate::native::core::{ManyState, NativeTestCase, Status, StopTest};
use crate::native::re::constants::{
    AtCode, ChCode, SRE_FLAG_ASCII, SRE_FLAG_DOTALL, SRE_FLAG_IGNORECASE, SRE_FLAG_MULTILINE,
};
use crate::native::re::parser::{OpCode, ParsedPattern, SetItem, SubPattern, parse_pattern};
use crate::native::unicodedata;
use ciborium::Value;

use super::many_more;
use super::text::{StringAlphabet, build_string_alphabet, is_surrogate_cp};

pub(super) fn interpret_regex(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let pattern = map_get(schema, "pattern")
        .and_then(as_text)
        .expect("regex schema must have pattern");
    let fullmatch = map_get(schema, "fullmatch")
        .and_then(as_bool)
        .unwrap_or(false);
    let alphabet_schema = map_get(schema, "alphabet");
    let alphabet = alphabet_schema.map(build_string_alphabet);

    let parsed = parse_pattern(pattern, 0)
        .unwrap_or_else(|e| panic!("invalid regex pattern {:?}: {}", pattern, e));

    let mut state = GenState {
        groups: HashMap::new(),
        flags: parsed.flags,
        pending_lookaheads: Vec::new(),
        in_cache: HashMap::new(),
    };
    let mut result = String::new();

    if parsed.pattern.is_empty() {
        // Empty regex: emit a padded arbitrary string when fullmatch is
        // false (matches hypothesis's `st.text(alphabet=alphabet)` path),
        // or an empty string when fullmatch is true.
        if !fullmatch {
            draw_pad(ntc, &alphabet, &mut result)?;
        }
        return Ok(encode(result));
    }

    if !fullmatch {
        draw_prefix(ntc, &parsed, &alphabet, &mut result)?;
    }
    generate_subpattern(ntc, &parsed.pattern, &mut state, &alphabet, &mut result)?;
    if !fullmatch {
        draw_suffix(ntc, &parsed, &alphabet, &mut result)?;
    }

    // Validate any deferred negative-lookahead assertions against the final
    // output. This mirrors Python's `.filter(regex.search)` for `(?!...)`
    // bodies — we only check the assertion position, but that's enough to
    // reject the impossible/violating cases the test suite cares about.
    if !state.pending_lookaheads.is_empty() {
        let final_chars: Vec<char> = result.chars().collect();
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
                mark_invalid(ntc)?;
            }
        }
    }

    Ok(encode(result))
}

fn encode(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

/// Mutable state threaded through generation: captured groups (for
/// back-references) and the active regex flags (which change as we descend
/// into `SUBPATTERN` nodes with inline flag modifiers).
struct GenState {
    groups: HashMap<u32, String>,
    flags: u32,
    /// Negative-lookahead assertions recorded during generation. Each entry
    /// captures the assertion body, the active flags, and a snapshot of the
    /// groups at the point of the assertion. We check them against the final
    /// output string in [`interpret_regex`].
    pending_lookaheads: Vec<PendingAssertNot>,
    /// Memoised character sets for `OpCode::In` nodes. Categories like `\w`
    /// require a full BMP scan; without caching, a `\w+` match would redo
    /// that work for every emitted character. Keyed by the slice pointer
    /// of the SetItem list (stable across one walk of the parsed AST) plus
    /// the active flags (which affect IGNORECASE swaps and ASCII-only
    /// filtering).
    in_cache: HashMap<(*const SetItem, usize, u32), Rc<[char]>>,
}

#[derive(Clone)]
struct PendingAssertNot {
    char_pos: usize,
    pattern: SubPattern,
    flags: u32,
    groups: HashMap<u32, String>,
}

/// Draw 0..10 arbitrary characters from `alphabet` (or ASCII 32..126 when
/// no alphabet is given). Used for prefix/suffix padding and for the
/// empty-pattern case.
fn draw_pad(
    ntc: &mut NativeTestCase,
    alphabet: &Option<StringAlphabet>,
    out: &mut String,
) -> Result<(), StopTest> {
    let n = ntc.draw_integer(0, 10)?;
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
    alphabet: &Option<StringAlphabet>,
    out: &mut String,
) -> Result<(), StopTest> {
    // Mirror of hypothesis.regex_strategy's left-pad logic.
    if let Some(OpCode::At(at)) = effective_first(&parsed.pattern) {
        match at {
            AtCode::BeginningString => return Ok(()),
            AtCode::Beginning => {
                if parsed.flags & SRE_FLAG_MULTILINE != 0 {
                    draw_pad(ntc, alphabet, out)?;
                    if !out.is_empty() && ntc.weighted(0.5, None)? {
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
    alphabet: &Option<StringAlphabet>,
    out: &mut String,
) -> Result<(), StopTest> {
    // Mirror of hypothesis.regex_strategy's right-pad logic.
    if let Some(OpCode::At(at)) = effective_last(&parsed.pattern) {
        match at {
            AtCode::EndString => return Ok(()),
            AtCode::End => {
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
    alphabet: &Option<StringAlphabet>,
    out: &mut String,
) -> Result<(), StopTest> {
    for op in &sp.data {
        generate_op(ntc, op, state, alphabet, out)?;
    }
    Ok(())
}

// nocov start
fn generate_op(
    ntc: &mut NativeTestCase,
    op: &OpCode,
    state: &mut GenState,
    alphabet: &Option<StringAlphabet>,
    out: &mut String,
) -> Result<(), StopTest> {
    match op {
        OpCode::Literal(cp) => {
            let c = codepoint_to_char(*cp);
            if state.flags & SRE_FLAG_IGNORECASE != 0 {
                let sw = char_swapcase(c);
                if sw != c {
                    let which = ntc.draw_integer(0, 1)?;
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
                let c = codepoint_to_char(*cp);
                let mut blacklist: Vec<char> = vec![c];
                if state.flags & SRE_FLAG_IGNORECASE != 0 {
                    let sw = char_swapcase(c);
                    if sw != c && !blacklist.contains(&sw) {
                        blacklist.push(sw);
                    }
                }
                let chars = gather_chars(alphabet, |c| !blacklist.contains(&c));
                emit_from_chars(ntc, &chars, out)?;
            }
        }
        OpCode::Any => {
            let allow_newline = state.flags & SRE_FLAG_DOTALL != 0;
            let chars = gather_chars(alphabet, |c| allow_newline || c != '\n');
            emit_from_chars(ntc, &chars, out)?;
        }
        OpCode::At(at) => {
            // Zero-width anchors don't emit a character, but some of them
            // constrain the current position in `out`. Python's
            // `regex_strategy` handles this by filtering the final result
            // through `regex.search`; we don't have a Python-compatible
            // matcher, so we validate inline for the position anchors we
            // can check against the partial output.
            match at {
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
                _ => {}
            }
        }
        OpCode::In(items) => {
            if alphabet.is_none() {
                let chars = cached_default_in_set(items, state.flags);
                emit_from_chars(ntc, &chars, out)?;
            } else {
                let key = (items.as_ptr(), items.len(), state.flags);
                let chars = match state.in_cache.get(&key) {
                    Some(cached) => Rc::clone(cached),
                    None => {
                        let computed: Rc<[char]> =
                            build_in_set(items, state.flags, alphabet).into();
                        state.in_cache.insert(key, Rc::clone(&computed));
                        computed
                    }
                };
                emit_from_chars(ntc, &chars, out)?;
            }
        }
        OpCode::Branch(items) => {
            let idx = ntc.draw_integer(0, items.len() as i128 - 1)? as usize;
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
                mark_invalid(ntc)?;
                return Ok(());
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
        OpCode::Assert { p, .. } => {
            // Positive lookahead/lookbehind: emit the asserted content so
            // the surrounding context sees it. (Hypothesis's strategy does
            // this too.)
            generate_subpattern(ntc, p, state, alphabet, out)?;
        }
        OpCode::AssertNot { direction, p } => {
            // Negative lookaround emits nothing itself, but we still have to
            // enforce the assertion. Python relies on `.filter(regex.search)`
            // to reject violating outputs; we don't have a Python-compatible
            // matcher to filter against, so we run a small SRE interpreter
            // (`match_seq`) against the output instead.
            //
            // Lookbehind (`dir < 0`) fires immediately: `p` must not match as
            // a suffix of what's been emitted so far. Lookahead defers until
            // after generation so we can check the body against what comes
            // next in the final string.
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
                    pattern: p.clone(),
                    flags: state.flags,
                    groups: state.groups.clone(),
                });
            }
        }
        OpCode::Failure => {
            // Explicit FAILURE is emitted for empty negative lookahead `(?!)`:
            // a position that can never match. Reject the generation rather
            // than producing a value that the regex wouldn't accept.
            mark_invalid(ntc)?;
        }
        OpCode::AtomicGroup(p) => {
            generate_subpattern(ntc, p, state, alphabet, out)?;
        }
        OpCode::MaxRepeat { min, max, item }
        | OpCode::MinRepeat { min, max, item }
        | OpCode::PossessiveRepeat { min, max, item } => {
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
// nocov end

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
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&(items.to_vec(), flags)) {
            return Arc::clone(cached);
        }
    }
    let computed: Arc<[char]> = build_in_set(items, flags, &None).into();
    cache
        .lock()
        .unwrap()
        .insert((items.to_vec(), flags), Arc::clone(&computed));
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
    let mut blacklist: Vec<char> = vec![c];
    if flags & SRE_FLAG_IGNORECASE != 0 {
        let sw = char_swapcase(c);
        if sw != c {
            blacklist.push(sw);
        }
    }
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
// nocov start
fn build_in_set(items: &[SetItem], flags: u32, alphabet: &Option<StringAlphabet>) -> Vec<char> {
    let negate = matches!(items.first(), Some(SetItem::Negate));

    // Characters explicitly listed (positive-set case) or to blacklist
    // (negated case).
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
        // Use a HashSet for O(1) deduplication. Category gathers can yield
        // tens of thousands of characters (e.g. `\w`), so any linear-scan
        // membership check turns this into an O(n²) hot loop.
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
        let positive_set: HashSet<char> = positive.into_iter().collect();
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
// nocov end

fn add_with_swapcase(v: &mut Vec<char>, c: char, flags: u32) {
    if !v.contains(&c) {
        v.push(c);
    }
    if flags & SRE_FLAG_IGNORECASE != 0 {
        let sw = char_swapcase(c);
        if sw != c && !v.contains(&sw) {
            v.push(sw);
        }
    }
}

/// Return whether codepoint `c` is in the given CPython character category.
// nocov start
fn in_category(c: char, cat: ChCode) -> bool {
    let cp = c as u32;
    match cat {
        ChCode::Digit | ChCode::UniDigit => unicodedata::is_in_group(cp, "Nd"),
        ChCode::NotDigit | ChCode::UniNotDigit => !unicodedata::is_in_group(cp, "Nd"),
        ChCode::Space | ChCode::UniSpace => is_uni_space(c),
        ChCode::NotSpace | ChCode::UniNotSpace => !is_uni_space(c),
        ChCode::Word | ChCode::UniWord | ChCode::LocWord => is_uni_word(c),
        ChCode::NotWord | ChCode::UniNotWord | ChCode::LocNotWord => !is_uni_word(c),
        ChCode::Linebreak | ChCode::UniLinebreak => c == '\n',
        ChCode::NotLinebreak | ChCode::UniNotLinebreak => c != '\n',
    }
}
// nocov end

fn is_uni_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\x1f' | '\u{85}'
    ) || unicodedata::is_in_group(c as u32, "Z")
}

fn is_uni_word(c: char) -> bool {
    c == '_' || unicodedata::is_in_group(c as u32, "L") || unicodedata::is_in_group(c as u32, "N")
}

/// Gather all characters in `alphabet` (or a default scan range) that
/// satisfy `predicate`.
// nocov start
fn gather_chars<F: Fn(char) -> bool>(alphabet: &Option<StringAlphabet>, predicate: F) -> Vec<char> {
    match alphabet {
        None => {
            // Default scan: BMP minus surrogates. Gives us reasonable
            // coverage for `.`, `\w`, `[^a]`, etc. without enumerating the
            // full 0x10FFFF range.
            let mut out = Vec::new();
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
            out
        }
        Some(StringAlphabet::Range { min, max }) => {
            let mut out = Vec::new();
            for cp in *min..=*max {
                if is_surrogate_cp(cp) {
                    continue;
                }
                if let Some(c) = char::from_u32(cp) {
                    if predicate(c) {
                        out.push(c);
                    }
                }
            }
            out
        }
        Some(StringAlphabet::Explicit(chars)) => {
            chars.iter().copied().filter(|c| predicate(*c)).collect()
        }
    }
}
// nocov end

fn alphabet_allows(alphabet: &Option<StringAlphabet>, c: char) -> bool {
    match alphabet {
        None => !is_surrogate_cp(c as u32),
        Some(StringAlphabet::Range { min, max }) => {
            let cp = c as u32;
            cp >= *min && cp <= *max && !is_surrogate_cp(cp)
        }
        Some(StringAlphabet::Explicit(chars)) => chars.contains(&c),
    }
}

/// Pick an arbitrary character from the alphabet. Used for prefix/suffix
/// padding — must never fail if the alphabet is non-empty.
// nocov start
fn draw_any_char(
    ntc: &mut NativeTestCase,
    alphabet: &Option<StringAlphabet>,
) -> Result<char, StopTest> {
    match alphabet {
        None => {
            let cp = ntc.draw_integer(32, 126)?;
            Ok(char::from_u32(cp as u32).expect("ASCII codepoint"))
        }
        Some(StringAlphabet::Range { min, max }) => {
            let lo = *min as i128;
            let hi = *max as i128;
            loop {
                let cp = ntc.draw_integer(lo, hi)? as u32;
                if !is_surrogate_cp(cp) {
                    if let Some(c) = char::from_u32(cp) {
                        return Ok(c);
                    }
                }
            }
        }
        Some(StringAlphabet::Explicit(chars)) => {
            if chars.is_empty() {
                mark_invalid(ntc)?;
            }
            let idx = ntc.draw_integer(0, chars.len() as i128 - 1)?;
            Ok(chars[idx as usize])
        }
    }
}
// nocov end

fn emit_from_chars(
    ntc: &mut NativeTestCase,
    chars: &[char],
    out: &mut String,
) -> Result<(), StopTest> {
    if chars.is_empty() {
        mark_invalid(ntc)?;
    }
    // Mirror `HypothesisProvider.draw_string`: when the pool is larger than
    // 256, pick from the first 256 entries 80% of the time. `gather_chars`
    // returns codepoints in ascending order, so the low-index slice covers
    // the low codepoints (ASCII / control chars). Without this bias,
    // interesting characters like '\n' are astronomically rare draws out of
    // the full BMP alphabet.
    let n = chars.len() as i128;
    let idx = if n > 256 && ntc.weighted(0.8, None)? {
        ntc.draw_integer(0, 255)? as usize
    } else if n > 256 {
        ntc.draw_integer(256, n - 1)? as usize
    } else {
        ntc.draw_integer(0, n - 1)? as usize
    };
    out.push(chars[idx]);
    Ok(())
}

fn mark_invalid(ntc: &mut NativeTestCase) -> Result<(), StopTest> {
    ntc.status = Some(Status::Invalid);
    Err(StopTest)
}

fn codepoint_to_char(cp: u32) -> char {
    char::from_u32(cp).unwrap_or_else(|| panic!("invalid codepoint in regex AST: {:#x}", cp))
}

/// Python's `str.swapcase()` on a single char.  Python's definition differs
/// from Rust's only for a handful of title-case codepoints; for the regex
/// strategy the straightforward case-flipping-or-identity behaviour is
/// sufficient, matching what `re.IGNORECASE` does on single-character
/// matches.
fn char_swapcase(c: char) -> char {
    if c.is_lowercase() {
        c.to_uppercase().next().unwrap_or(c)
    } else if c.is_uppercase() {
        c.to_lowercase().next().unwrap_or(c)
    } else {
        c
    }
}

// -- SRE-style matcher -------------------------------------------------------
//
// Minimal backtracking matcher over our parsed `SubPattern` AST, used to
// validate `AssertNot` bodies during generation. Python's `regex_strategy`
// does this externally via `regex.search` on the final output; we don't have a
// Python-compatible matcher to filter against, so we roll a small one that
// understands the subset of `OpCode`s that legitimately appear in lookaround
// bodies.
//
// `match_seq` tries to match the opcode sequence `ops` starting at `pos` in
// `chars` and returns `Some(end_pos)` on success. Backtracking is handled
// inline for `Branch` and the repeat opcodes; `Subpattern` scoped flags are
// approximated by matching the body with its modified flags and then
// continuing the tail with the outer flags.

// nocov start
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
                    _ => break,
                }
            }
            if positions.len() - 1 < mn {
                return None;
            }
            for i in (mn..positions.len()).rev() {
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
                    return None;
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
                    _ => break,
                }
            }
            if count < mn {
                return None;
            }
            match_seq(rest, cur, chars, flags, groups)
        }
    }
}
// nocov end

// nocov start
fn chars_eq(a: char, b: char, flags: u32) -> bool {
    if a == b {
        return true;
    }
    if flags & SRE_FLAG_IGNORECASE != 0 {
        char_swapcase(a) == b || a == char_swapcase(b)
    } else {
        false
    }
}
// nocov end

// nocov start
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
                    let sw = char_swapcase(c) as u32;
                    if sw >= *lo && sw <= *hi {
                        contained = true;
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
// nocov end

// nocov start
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
        AtCode::BeginningLine => pos == 0 || (pos > 0 && chars[pos - 1] == '\n'),
        AtCode::End => {
            if flags & SRE_FLAG_MULTILINE != 0 {
                pos == chars.len() || chars[pos] == '\n'
            } else {
                pos == chars.len() || (pos + 1 == chars.len() && chars[pos] == '\n')
            }
        }
        AtCode::EndLine => pos == chars.len() || chars[pos] == '\n',
        AtCode::EndString => pos == chars.len(),
        AtCode::Boundary | AtCode::UniBoundary | AtCode::LocBoundary => {
            is_word_boundary(chars, pos)
        }
        AtCode::NonBoundary | AtCode::UniNonBoundary | AtCode::LocNonBoundary => {
            !is_word_boundary(chars, pos)
        }
    }
}
// nocov end

// nocov start
fn is_word_boundary(chars: &[char], pos: usize) -> bool {
    let before = pos > 0 && is_uni_word(chars[pos - 1]);
    let after = pos < chars.len() && is_uni_word(chars[pos]);
    before != after
}
// nocov end

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/regex_tests.rs"]
mod tests;
