use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;

#[test]
fn test_minimize_string_to_empty() {
    let s: String = minimal(gs::text(), |_: &String| true);
    assert_eq!(s, "");
}

#[test]
fn test_minimize_longer_string() {
    let s = minimal(gs::text().max_size(50), |x: &String| {
        x.chars().count() >= 10
    });
    assert_eq!(s, "0".repeat(10));
}

#[test]
fn test_minimize_longer_list_of_strings() {
    let v = minimal(gs::vecs(gs::text()), |x: &Vec<String>| x.len() >= 10);
    assert_eq!(v, vec![String::new(); 10]);
}

#[test]
fn test_string_sorts_characters_when_possible() {
    // String shrinking should sort characters by codepoint.
    // Sorting "0e0" produces "00e" (smaller codepoints first).
    let s = Minimal::new(
        gs::text().min_codepoint(32).max_codepoint(126).max_size(20),
        |v0: &String| v0.chars().count() >= 3 && v0.contains('e'),
    )
    .test_cases(1000)
    .run();
    assert_eq!(s, "00e");
}

#[test]
fn test_string_insertion_sort_swap_succeeds() {
    // Fixed-length 2-char string over {'a','b'} where the condition requires
    // both letters. Starting from "ba" the insertion-sort swap produces "ab".
    let s = Minimal::new(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'b' as u32)
            .min_size(2)
            .max_size(2),
        |s: &String| s.contains('a') && s.contains('b'),
    )
    .test_cases(1000)
    .run();
    assert_eq!(s, "ab");
}

// Port of `tests/quality/test_shrink_quality.py::test_minimize_duplicated_characters_within_a_choice`.
//
// Strings that contain at least 3 of the same character *and* at least
// two different characters should collapse to "000A"-style minimal
// forms after `shrink_strings`' duplicate-codepoint pass fires.
#[test]
fn test_minimize_duplicated_characters_within_a_choice() {
    let s = Minimal::new(gs::text().min_size(1).max_size(20), |s: &String| {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 4 {
            return false;
        }
        // At least one character appearing ≥3 times.
        let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
        for c in &chars {
            *counts.entry(*c).or_default() += 1;
        }
        let has_triple = counts.values().any(|&n| n >= 3);
        // At least two distinct characters.
        let distinct = counts.len() >= 2;
        has_triple && distinct
    })
    .test_cases(5000)
    .run();
    // The shrinker should land on length 4 with three of one char and
    // one of another, both in the simplest part of the alphabet.
    assert_eq!(s.chars().count(), 4);
    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_default() += 1;
    }
    assert!(counts.values().any(|&n| n >= 3));
    assert!(counts.len() >= 2);
}

// Port of `tests/quality/test_shrink_quality.py::test_shrink_strips_accent_to_ascii_letter`.
//
// `normalize_unicode_chars` should peel accents off latin letters when
// the predicate is satisfied by the base form.
#[test]
fn test_shrink_strips_accent_to_ascii_letter() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        s.to_lowercase().contains('e')
    })
    .test_cases(5000)
    .run();
    // After unicode normalization the canonical 1-char counterexample
    // is "E" (or any equivalent that still satisfies the predicate).
    let lower = s.to_lowercase();
    assert!(lower.contains('e'));
    assert!(s.chars().count() == 1);
}

// Port of `tests/quality/test_shrink_quality.py::test_shrink_text_differs_from_lower_to_ascii`.
// Hypothesis regression: text shrinking previously got stuck on a
// high-codepoint accented letter rather than converging to ASCII 'A'.
#[test]
fn test_shrink_text_differs_from_lower_to_ascii() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        *s != s.to_lowercase()
    })
    .test_cases(5000)
    .run();
    // Counterexample: a single-character string that is not equal to its
    // lowercased form.  The shortest with the simplest codepoint is "A".
    assert_eq!(s.chars().count(), 1);
    assert!(s != s.to_lowercase());
    // Strongest assertion: the canonical answer.
    assert_eq!(s, "A");
}

// Port of `tests/quality/test_shrink_quality.py::test_shrink_text_differs_from_upper_to_ascii`.
#[test]
fn test_shrink_text_differs_from_upper_to_ascii() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        *s != s.to_uppercase()
    })
    .test_cases(5000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(s != s.to_uppercase());
    assert_eq!(s, "a");
}

// Port of `tests/quality/test_shrink_quality.py::test_shrink_decomposes_compatibility_form_to_ascii`.
// Codepoints that NFKD-decompose to ASCII letters (e.g. Mathematical
// Bold Capital T) should reduce to the bare letter when the
// predicate also matches it.
#[test]
fn test_shrink_decomposes_compatibility_form_to_ascii() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        s.chars().any(|c| c.eq_ignore_ascii_case(&'t'))
    })
    .test_cases(5000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(s.chars().any(|c| c.eq_ignore_ascii_case(&'t')));
    assert_eq!(s, "T");
}

// Port of `tests/quality/test_shrink_quality.py::test_shrink_ligature_to_base_character`.
// 'fi' (U+FB01) NFKD-decomposes to "fi"; the shrinker should land on
// plain 'F' (a single ASCII letter) when the predicate accepts any
// 'f'-like character.
#[test]
fn test_shrink_ligature_to_base_character() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        s.chars().any(|c| c.eq_ignore_ascii_case(&'f'))
    })
    .test_cases(5000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(s.chars().any(|c| c.eq_ignore_ascii_case(&'f')));
    assert_eq!(s, "F");
}
