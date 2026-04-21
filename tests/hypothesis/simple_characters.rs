//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_simple_characters.py
//!
//! Individually-skipped tests:
//! - `test_include_exclude_with_multiple_chars_is_invalid` — Python passes a
//!   list-of-strings where each element must be a single character; in Rust
//!   `include_characters` / `exclude_characters` take `&str` (each codepoint
//!   is a char), so the "more than one character" element is unrepresentable.
//! - `test_whitelisted_characters_alone` — Python raises when
//!   `include_characters` is the only constraint (no categories/bounds). The
//!   hegel-rust client always sends `exclude_categories=["Cs"]` so Rust
//!   strings can't hold surrogates, which means the "include alone" failure
//!   mode is unreachable through the public Rust API.

use crate::common::utils::{assert_no_examples, expect_panic, find_any, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

fn expect_generator_panic<T, G>(generator: G, pattern: &str)
where
    G: Generator<T> + 'static + std::panic::UnwindSafe,
    T: std::fmt::Debug + Send + 'static,
{
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                tc.draw(&generator);
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        pattern,
    );
}

#[test]
fn test_nonexistent_category_argument() {
    expect_generator_panic(
        gs::characters().exclude_categories(&["foo"]),
        "(?i)(invalid|foo|categor|no valid)",
    );
}

#[test]
fn test_bad_codepoint_arguments() {
    expect_generator_panic(
        gs::characters().min_codepoint(42).max_codepoint(24),
        "(?i)(invalid|min_codepoint|max_codepoint|no valid)",
    );
}

#[test]
fn test_exclude_all_available_range() {
    expect_generator_panic(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'0' as u32)
            .exclude_characters("0"),
        "(?i)(invalid|no valid|empty)",
    );
}

#[test]
fn test_when_nothing_could_be_produced() {
    expect_generator_panic(
        gs::characters()
            .categories(&["Cc"])
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32),
        "(?i)(invalid|no valid|empty)",
    );
}

#[cfg(feature = "native")]
fn category_of(c: char) -> &'static str {
    hegel::__native_test_internals::unicodedata::general_category(c as u32).as_str()
}

#[cfg(feature = "native")]
#[test]
fn test_characters_of_specific_groups() {
    find_any(gs::characters().categories(&["Lu", "Nd"]), |c: &char| {
        category_of(*c) == "Lu"
    });
    find_any(gs::characters().categories(&["Lu", "Nd"]), |c: &char| {
        category_of(*c) == "Nd"
    });
    assert_no_examples(gs::characters().categories(&["Lu", "Nd"]), |c: &char| {
        !matches!(category_of(*c), "Lu" | "Nd")
    });
}

#[cfg(feature = "native")]
#[test]
fn test_characters_of_major_categories() {
    find_any(gs::characters().categories(&["L", "N"]), |c: &char| {
        category_of(*c).starts_with('L')
    });
    find_any(gs::characters().categories(&["L", "N"]), |c: &char| {
        category_of(*c).starts_with('N')
    });
    assert_no_examples(gs::characters().categories(&["L", "N"]), |c: &char| {
        let first = category_of(*c).chars().next().unwrap();
        first != 'L' && first != 'N'
    });
}

#[cfg(feature = "native")]
#[test]
fn test_exclude_characters_of_specific_groups() {
    find_any(
        gs::characters().exclude_categories(&["Lu", "Nd"]),
        |c: &char| category_of(*c) != "Lu",
    );
    find_any(
        gs::characters().exclude_categories(&["Lu", "Nd"]),
        |c: &char| category_of(*c) != "Nd",
    );
    assert_no_examples(
        gs::characters().exclude_categories(&["Lu", "Nd"]),
        |c: &char| matches!(category_of(*c), "Lu" | "Nd"),
    );
}

#[cfg(feature = "native")]
#[test]
fn test_exclude_characters_of_major_categories() {
    find_any(
        gs::characters().exclude_categories(&["L", "N"]),
        |c: &char| !category_of(*c).starts_with('L'),
    );
    find_any(
        gs::characters().exclude_categories(&["L", "N"]),
        |c: &char| !category_of(*c).starts_with('N'),
    );
    assert_no_examples(
        gs::characters().exclude_categories(&["L", "N"]),
        |c: &char| {
            let first = category_of(*c).chars().next().unwrap();
            first == 'L' || first == 'N'
        },
    );
}

#[test]
fn test_find_one() {
    let c = minimal(
        gs::characters().min_codepoint(48).max_codepoint(48),
        |_: &char| true,
    );
    assert_eq!(c, '0');
}

#[cfg(feature = "native")]
#[test]
fn test_find_something_rare() {
    find_any(
        gs::characters().categories(&["Zs"]).min_codepoint(12288),
        |c: &char| category_of(*c) == "Zs",
    );
    assert_no_examples(
        gs::characters().categories(&["Zs"]).min_codepoint(12288),
        |c: &char| category_of(*c) != "Zs",
    );
}

#[test]
fn test_whitelisted_characters_overlap_blacklisted_characters() {
    expect_generator_panic(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .include_characters("te02тест49st")
            .exclude_characters("ts94тсет"),
        "(?i)(invalid|overlap|both)",
    );
}

#[test]
fn test_whitelisted_characters_override() {
    let good = "teтестst";
    let good_owned = good.to_string();
    find_any(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .include_characters(good),
        move |c: &char| good_owned.contains(*c),
    );
    find_any(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .include_characters(good),
        |c: &char| "0123456789".contains(*c),
    );
    let combined = format!("{good}0123456789");
    assert_no_examples(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .include_characters(good),
        move |c: &char| !combined.contains(*c),
    );
}

#[test]
fn test_blacklisted_characters() {
    let bad = "te02тест49st";
    let c = minimal(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .exclude_characters(bad),
        |_: &char| true,
    );
    assert_eq!(c, '1');

    let bad_owned = bad.to_string();
    assert_no_examples(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .exclude_characters(bad),
        move |c: &char| bad_owned.contains(*c),
    );
}

#[test]
fn test_whitelist_characters_disjoint_blacklist_characters() {
    let bad = "456def";
    let bad_owned = bad.to_string();
    assert_no_examples(
        gs::characters()
            .min_codepoint(b'0' as u32)
            .max_codepoint(b'9' as u32)
            .exclude_characters(bad)
            .include_characters("123abc"),
        move |c: &char| bad_owned.contains(*c),
    );
}
