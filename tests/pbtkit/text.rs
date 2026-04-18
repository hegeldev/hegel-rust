//! Ported from resources/pbtkit/tests/test_text.py.
//!
//! Individually omitted:
//! - `test_string_single_codepoint_unit`, `test_string_validate`: already
//!   ported as embedded tests in tests/embedded/native/choices_tests.rs.
//! - `test_string_from_index_out_of_range`, `test_string_from_index_past_end`,
//!   `test_string_codepoint_rank_with_surrogates`: exercise pbtkit's
//!   index-based shortlex enumeration (`to_index`/`from_index`/
//!   `_codepoint_rank`), which hegel-rust deliberately does not implement
//!   (see SKIPPED.md for test_choice_index.py).
//! - `test_string_sort_key_type_mismatch`: exercises Python's dynamically-typed
//!   `sort_key(non-string)`; Rust's `sort_key` is type-safe.
//! - `test_truncated_string_database_entry`: tests pbtkit's
//!   `SerializationTag.STRING` byte layout; hegel-rust's database format
//!   differs.
//! - `test_draw_string_invalid_range`: exercises the Python `TC.for_choices`
//!   zero-draw harness, which has no hegel-rust counterpart.
//! - `test_text_database_round_trip`: the native round-trip is covered by the
//!   existing tests/test_database_key.rs pattern; not re-ported here.

use crate::common::utils::{assert_all_examples, minimal};
use hegel::generators::{self as gs, Generator};

#[test]
fn test_text_basic() {
    assert_all_examples(gs::text().min_size(1).max_size(5), |s: &String| {
        let len = s.chars().count();
        (1..=5).contains(&len)
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
fn test_text_shrinks_to_short() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32),
        |s: &String| !s.is_empty(),
    );
    assert_eq!(s, "a");
}

#[test]
fn test_text_shrinks_characters() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .min_size(1)
            .max_size(5),
        |s: &String| s.contains('z'),
    );
    assert_eq!(s, "z");
}

#[test]
fn test_text_no_surrogates() {
    assert_all_examples(
        gs::text().min_codepoint(0xD700).max_codepoint(0xE000),
        |s: &String| s.chars().all(|c| !(0xD800..=0xDFFF).contains(&(c as u32))),
    );
}

#[test]
fn test_text_unicode_shrinks() {
    let s = minimal(
        gs::text()
            .min_codepoint(128)
            .max_codepoint(256)
            .min_size(1)
            .max_size(3),
        |s: &String| s.chars().any(|c| (c as u32) >= 200),
    );
    // Shrinks to a single high-codepoint character at the boundary.
    assert_eq!(s.chars().count(), 1);
    assert!(s.chars().all(|c| (c as u32) >= 200));
}

#[test]
fn test_text_shrinks_to_simplest() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .max_size(5),
        |_: &String| true,
    );
    assert_eq!(s, "");
}

#[test]
fn test_text_sorts_characters() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .min_size(3)
            .max_size(5),
        |s: &String| {
            let chars: Vec<char> = s.chars().collect();
            chars.len() >= 3 && chars.windows(2).all(|w| w[0] > w[1])
        },
    );
    let chars: Vec<char> = s.chars().collect();
    assert!(chars.len() >= 3);
    assert!(chars.windows(2).all(|w| w[0] > w[1]));
}

#[test]
fn test_text_redistributes_to_empty() {
    let (s1, s2) = minimal(
        gs::tuples!(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .max_size(10),
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .max_size(10),
        ),
        |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 3,
    );
    assert!(s1.is_empty() || s2.is_empty());
}

#[test]
fn test_text_redistributes_pair() {
    let (s1, s2) = minimal(
        gs::tuples!(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(1)
                .max_size(10),
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(1)
                .max_size(10),
        ),
        |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 5,
    );
    assert!(!s1.is_empty());
    assert!(!s2.is_empty());
}
