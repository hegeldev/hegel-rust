//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_simple_strings.py

use crate::common::utils::{assert_all_examples, minimal};
use hegel::generators::{self as gs};

#[test]
fn test_can_minimize_up_to_zero() {
    let s = minimal(gs::text(), |s: &String| s.chars().any(|c| c <= '0'));
    assert_eq!(s, "0");
}

#[test]
fn test_minimizes_towards_ascii_zero() {
    let s = minimal(gs::text(), |s: &String| s.chars().any(|c| c < '0'));
    // Native engine's codepoint_key ordering makes NUL (key 80) simpler
    // than '/' (key 127); Hypothesis's ordering spirals around '0'.
    #[cfg(feature = "native")]
    assert_eq!(s, "\0");
    #[cfg(not(feature = "native"))]
    assert_eq!(s, "/");
}

#[test]
fn test_can_handle_large_codepoints() {
    let s = minimal(gs::text(), |s: &String| s.as_str() >= "\u{2603}");
    assert_eq!(s, "\u{2603}");
}

#[test]
fn test_can_find_mixed_ascii_and_non_ascii_strings() {
    let s = minimal(gs::text(), |s: &String| {
        s.chars().any(|c| c >= '\u{2603}') && s.chars().any(|c| c as u32 <= 127)
    });
    assert_eq!(s.chars().count(), 2);
    let mut chars: Vec<char> = s.chars().collect();
    chars.sort();
    assert_eq!(chars, vec!['0', '\u{2603}']);
}

#[test]
fn test_will_find_ascii_examples_given_the_chance() {
    let s = minimal(
        gs::tuples!(gs::text().max_size(1), gs::text().max_size(1)),
        |s: &(String, String)| !s.0.is_empty() && s.0 < s.1,
    );
    let c0 = s.0.chars().next().unwrap();
    let c1 = s.1.chars().next().unwrap();
    assert_eq!(c1 as u32, c0 as u32 + 1);
    assert!(s.0 == "0" || s.1 == "0");
}

#[test]
fn test_minimisation_consistent_with_characters() {
    let s = minimal(gs::text().alphabet("FEDCBA").min_size(3), |_: &String| true);
    assert_eq!(s, "AAA");
}

#[test]
fn test_finds_single_element_strings() {
    let s = minimal(gs::text(), |s: &String| !s.is_empty());
    assert_eq!(s, "0");
}

#[test]
fn test_binary_respects_max_size() {
    assert_all_examples(gs::binary().max_size(5), |x: &Vec<u8>| x.len() <= 5);
}

#[test]
fn test_does_not_simplify_into_surrogates() {
    let f = minimal(gs::text(), |s: &String| s.as_str() >= "\u{e000}");
    assert_eq!(f, "\u{e000}");

    let size: usize = 2;
    let f = minimal(gs::text().min_size(size), move |s: &String| {
        s.chars().filter(|&c| c >= '\u{e000}').count() >= size
    });
    assert_eq!(f, "\u{e000}".repeat(size));
}

#[test]
fn test_respects_alphabet_if_list() {
    assert_all_examples(gs::text().alphabet("ab"), |s: &String| {
        s.chars().all(|c| c == 'a' || c == 'b')
    });
}

#[test]
fn test_respects_alphabet_if_string() {
    assert_all_examples(gs::text().alphabet("cdef"), |s: &String| {
        s.chars().all(|c| "cdef".contains(c))
    });
}

#[test]
fn test_can_encode_as_utf8() {
    assert_all_examples(gs::text(), |s: &String| {
        std::str::from_utf8(s.as_bytes()).is_ok()
    });
}

#[test]
fn test_can_blacklist_newlines() {
    assert_all_examples(gs::text().exclude_characters("\n"), |s: &String| {
        !s.contains('\n')
    });
}

#[test]
fn test_can_exclude_newlines_by_category() {
    assert_all_examples(
        gs::text().exclude_categories(&["Cc", "Cs"]),
        |s: &String| !s.contains('\n'),
    );
}

#[test]
fn test_can_restrict_to_ascii_only() {
    assert_all_examples(gs::text().max_codepoint(127), |s: &String| s.is_ascii());
}

#[cfg(feature = "native")]
#[test]
fn test_fixed_size_bytes_just_draw_bytes() {
    use hegel::__native_test_internals::{ChoiceValue, NativeTestCase};
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Bytes(b"foo".to_vec())], None, None);
    let result = ntc.draw_bytes(3, 3).ok().unwrap();
    assert_eq!(result, b"foo");
}

#[test]
fn test_can_set_max_size_large() {
    assert_all_examples(gs::text().max_size(1_000_000), |_: &String| true);
}
