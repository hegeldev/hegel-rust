//! Ported from resources/pbtkit/tests/test_text.py.
//!
//! A handful of upstream tests are ported elsewhere rather than duplicated here:
//! - Tests on `StringChoice` internals (`test_string_single_codepoint_unit`,
//!   `test_string_validate`, `test_string_from_index_out_of_range`,
//!   `test_string_from_index_past_end`, `test_string_codepoint_rank_with_surrogates`)
//!   are embedded tests in `tests/embedded/native/choices_tests.rs`, where the
//!   crate-internal `StringChoice` type is accessible.
//! - Tests on the database byte layout (`test_truncated_string_database_entry`)
//!   are covered by `tests/embedded/native/database_tests.rs`
//!   (`test_deserialize_truncated_string_length_returns_none`,
//!   `test_deserialize_truncated_string_payload_returns_none`,
//!   `test_database_load_corrupt_file_returns_none`).
//! - `test_text_database_round_trip` is covered by `tests/test_database_key.rs`.
//!
//! `test_string_sort_key_type_mismatch` is listed in `SKIPPED.md`: Rust's typed
//! `sort_key(&str)` makes the "non-string argument" case unrepresentable.

use crate::common::utils::{assert_all_examples, expect_panic, minimal};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

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

#[test]
fn test_draw_string_invalid_range() {
    // Python: `tc.draw_string(min_codepoint=200, max_codepoint=100)` raises
    // ValueError. In hegel-rust drawing from such a generator panics: the
    // server returns an InvalidArgument error, and the native backend panics
    // with a similar message from `schema::text::interpret_string`.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: String = tc.draw(gs::text().min_codepoint(200).max_codepoint(100));
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        "InvalidArgument",
    );
}
