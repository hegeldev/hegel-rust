// Regex schema interpreter.

use crate::cbor_utils::{as_bool, as_text, map_get};
use crate::native::core::{ManyState, NativeTestCase, Status, StopTest};
use ciborium::Value;

use super::many_more;
use super::text::{StringAlphabet, build_string_alphabet, is_surrogate_cp};

/// Translate Python regex escapes that regex-syntax doesn't understand.
///
/// `\Z` is Python's end-of-string anchor; regex-syntax uses `\z`.
/// We scan character-by-character so that `\\Z` (escaped backslash + literal Z)
/// is left alone.
fn translate_python_escapes(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Z') => {
                out.push('\\');
                out.push('z');
            }
            Some(next) => {
                out.push('\\');
                out.push(next);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub(super) fn interpret_regex(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let pattern = map_get(schema, "pattern")
        .and_then(as_text)
        .expect("regex schema must have pattern");
    let fullmatch = map_get(schema, "fullmatch")
        .and_then(as_bool)
        .unwrap_or(false);
    let alphabet_schema = map_get(schema, "alphabet");

    let translated = translate_python_escapes(pattern);

    // Parse the regex to HIR using regex-syntax.
    let hir = regex_syntax::Parser::new()
        .parse(&translated)
        .unwrap_or_else(|e| panic!("invalid regex pattern {:?}: {}", pattern, e));

    // Build the alphabet constraint (if any) from the alphabet sub-schema.
    let alphabet_filter = alphabet_schema.map(build_string_alphabet);

    let mut result = String::new();

    if fullmatch {
        generate_hir_string(ntc, &hir, &alphabet_filter, &mut result)?;
    } else {
        // For partial match, wrap the pattern in an arbitrary string on either side.
        // Generate prefix (arbitrary ASCII text), then the pattern, then suffix.
        let prefix_len = ntc.draw_integer(0, 10)?;
        for _ in 0..prefix_len {
            let c = ntc.draw_integer(32, 126)?;
            result.push(char::from_u32(c as u32).expect("valid ASCII"));
        }
        generate_hir_string(ntc, &hir, &alphabet_filter, &mut result)?;
        let suffix_len = ntc.draw_integer(0, 10)?;
        for _ in 0..suffix_len {
            let c = ntc.draw_integer(32, 126)?;
            result.push(char::from_u32(c as u32).expect("valid ASCII"));
        }
    }

    Ok(Value::Tag(91, Box::new(Value::Bytes(result.into_bytes()))))
}

/// Recursively generate a string from a regex HIR node, appending to `result`.
///
/// Characters are filtered through `alphabet` (if Some); if a required
/// character is not in the alphabet, the test case is marked invalid.
fn generate_hir_string(
    ntc: &mut NativeTestCase,
    hir: &regex_syntax::hir::Hir,
    alphabet: &Option<StringAlphabet>,
    result: &mut String,
) -> Result<(), StopTest> {
    use regex_syntax::hir::{Class, HirKind};

    match hir.kind() {
        HirKind::Empty => {
            // Nothing to generate.
        }
        HirKind::Literal(lit) => {
            let s = std::str::from_utf8(&lit.0).expect("regex literal should be valid UTF-8");
            for c in s.chars() {
                if !regex_alphabet_allows(alphabet, c) {
                    ntc.status = Some(Status::Invalid);
                    return Err(StopTest);
                }
                result.push(c);
            }
        }
        HirKind::Class(Class::Unicode(cls)) => {
            let chars: Vec<char> = cls
                .iter()
                .flat_map(|r| {
                    let start = r.start() as u32;
                    let end = r.end() as u32;
                    (start..=end).filter_map(char::from_u32)
                })
                .filter(|c| regex_alphabet_allows(alphabet, *c))
                .collect();
            if chars.is_empty() {
                ntc.status = Some(Status::Invalid);
                return Err(StopTest);
            }
            let idx = ntc.draw_integer(0, chars.len() as i128 - 1)?;
            result.push(chars[idx as usize]);
        }
        HirKind::Class(Class::Bytes(cls)) => {
            let chars: Vec<char> = cls
                .iter()
                .flat_map(|r| (r.start()..=r.end()).map(|b| b as char))
                .filter(|c| regex_alphabet_allows(alphabet, *c))
                .collect();
            if chars.is_empty() {
                ntc.status = Some(Status::Invalid);
                return Err(StopTest);
            }
            let idx = ntc.draw_integer(0, chars.len() as i128 - 1)?;
            result.push(chars[idx as usize]);
        }
        HirKind::Look(_) => {
            // Anchors and word boundaries don't consume characters during generation.
        }
        HirKind::Repetition(rep) => {
            let min = rep.min as usize;
            let max = rep.max.map(|m| m as usize);
            let mut state = ManyState::new(min, max);
            loop {
                if !many_more(ntc, &mut state)? {
                    break;
                }
                generate_hir_string(ntc, &rep.sub, alphabet, result)?;
            }
        }
        HirKind::Capture(cap) => {
            generate_hir_string(ntc, &cap.sub, alphabet, result)?;
        }
        HirKind::Concat(hirs) => {
            for sub in hirs {
                generate_hir_string(ntc, sub, alphabet, result)?;
            }
        }
        HirKind::Alternation(hirs) => {
            let idx = ntc.draw_integer(0, hirs.len() as i128 - 1)?;
            generate_hir_string(ntc, &hirs[idx as usize], alphabet, result)?;
        }
    }
    Ok(())
}

/// Check whether a character is permitted by the optional alphabet constraint.
fn regex_alphabet_allows(alphabet: &Option<StringAlphabet>, c: char) -> bool {
    match alphabet {
        None => true,
        Some(StringAlphabet::Range { min, max }) => {
            let cp = c as u32;
            cp >= *min && cp <= *max && !is_surrogate_cp(cp)
        }
        Some(StringAlphabet::Explicit(chars)) => chars.contains(&c),
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/regex_tests.rs"]
mod tests;
