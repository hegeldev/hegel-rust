//! Ported from pbtkit/tests/test_text.py

use crate::common::utils::{assert_all_examples, minimal};
use hegel::generators as gs;

#[test]
fn test_text_basic() {
    assert_all_examples(gs::text().min_size(1).max_size(5), |s: &String| {
        let n = s.chars().count();
        (1..=5).contains(&n)
    });
}

#[test]
fn test_text_ascii() {
    assert_all_examples(
        gs::text().min_codepoint(32).max_codepoint(126),
        |s: &String| s.chars().all(|c| (32..=126).contains(&(c as u32))),
    );
}

#[test]
fn test_text_no_surrogates() {
    assert_all_examples(
        gs::text().min_codepoint(0xD700).max_codepoint(0xE000),
        |s: &String| s.chars().all(|c| !(0xD800..=0xDFFF).contains(&(c as u32))),
    );
}

#[test]
fn test_text_shrinks_to_short() {
    // Any non-empty string with chars a..z should shrink to "a".
    let result = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32),
        |s: &String| !s.is_empty(),
    );
    assert_eq!(result, "a");
}

#[test]
fn test_text_shrinks_characters() {
    // Condition: string contains 'z'. Shrinker should find "z" itself.
    let result = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .min_size(1)
            .max_size(5),
        |s: &String| s.contains('z'),
    );
    assert_eq!(result, "z");
}

#[test]
fn test_text_unicode_shrinks() {
    // Strings with high codepoints shrink toward the lowest in range.
    // Condition: contains a char with codepoint >= 200.
    let result = minimal(
        gs::text()
            .min_codepoint(128)
            .max_codepoint(256)
            .min_size(1)
            .max_size(3),
        |s: &String| s.chars().any(|c| (c as u32) >= 200),
    );
    // Should shrink to a single char that is exactly 200 (the minimal
    // codepoint satisfying the condition within the range).
    assert_eq!(result.chars().count(), 1);
    let c = result.chars().next().unwrap();
    assert_eq!(c as u32, 200);
}

#[test]
fn test_text_shrinks_to_simplest() {
    // Any condition that matches everything should shrink to the empty string.
    let result = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .max_size(5),
        |_: &String| true,
    );
    assert_eq!(result, "");
}
