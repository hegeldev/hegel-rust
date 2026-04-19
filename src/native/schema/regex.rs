// Regex schema interpreter.
//
// Port of Hypothesis's `strategies._internal.regex._strategy`, rewritten
// against our in-tree CPython `_parser` port (`src/native/re`). Walks the
// `SubPattern` AST directly so we match Python's `re` module semantics
// exactly, rather than adapting the subtly-different Rust regex-syntax
// grammar.

use std::collections::HashMap;

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
                AtCode::BeginningString => {
                    if !out.is_empty() {
                        mark_invalid(ntc)?;
                    }
                }
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
            let chars = build_in_set(items, state.flags, alphabet);
            emit_from_chars(ntc, &chars, out)?;
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
        OpCode::AssertNot { .. } | OpCode::Failure => {
            // Negative lookahead/lookbehind and explicit FAILURE: emit
            // nothing. Production stays valid if the surrounding text
            // doesn't violate the assertion.
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

/// Build the set of characters that a `(IN, items)` node can emit, after
/// applying the current flags (IGNORECASE swaps, ASCII restriction) and
/// intersecting with the user-supplied alphabet.
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
        let mut out: Vec<char> = positive
            .into_iter()
            .filter(|c| !ascii_only || (*c as u32) < 128)
            .filter(|c| alphabet_allows(alphabet, *c))
            .collect();
        if !categories.is_empty() {
            let cat_chars = gather_chars(alphabet, |c| {
                categories.iter().any(|cat| in_category(c, *cat))
            });
            for c in cat_chars {
                if ascii_only && (c as u32) >= 128 {
                    continue;
                }
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        dedup(&mut out);
        out
    } else {
        let cat_blocks: Vec<ChCode> = categories;
        gather_chars(alphabet, |c| {
            if ascii_only && (c as u32) >= 128 {
                return false;
            }
            if positive.contains(&c) {
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

fn dedup(v: &mut Vec<char>) {
    let mut seen: Vec<char> = Vec::with_capacity(v.len());
    v.retain(|c| {
        if seen.contains(c) {
            false
        } else {
            seen.push(*c);
            true
        }
    });
}

/// Return whether codepoint `c` is in the given CPython character category.
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

fn emit_from_chars(
    ntc: &mut NativeTestCase,
    chars: &[char],
    out: &mut String,
) -> Result<(), StopTest> {
    if chars.is_empty() {
        mark_invalid(ntc)?;
    }
    let idx = ntc.draw_integer(0, chars.len() as i128 - 1)? as usize;
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/regex_tests.rs"]
mod tests;
