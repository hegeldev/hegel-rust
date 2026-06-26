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

#[test]
fn test_minimize_duplicated_characters_within_a_choice() {
    let s = Minimal::new(gs::text().min_size(1).max_size(20), |s: &String| {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 4 {
            return false;
        }
        let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
        for c in &chars {
            *counts.entry(*c).or_default() += 1;
        }
        let has_triple = counts.values().any(|&n| n >= 3);
        let distinct = counts.len() >= 2;
        has_triple && distinct
    })
    .test_cases(5000)
    .run();
    assert_eq!(s.chars().count(), 4);
    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_default() += 1;
    }
    assert!(counts.values().any(|&n| n >= 3));
    assert!(counts.len() >= 2);
}

#[test]
fn test_shrink_strips_accent_to_ascii_letter() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        s.to_lowercase().contains('e')
    })
    .test_cases(5000)
    .run();
    let lower = s.to_lowercase();
    assert!(lower.contains('e'));
    assert!(s.chars().count() == 1);
}

#[test]
fn test_shrink_text_differs_from_lower_to_ascii() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        *s != s.to_lowercase()
    })
    .test_cases(10000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(s != s.to_lowercase());
}

#[test]
fn test_shrink_text_differs_from_upper_to_ascii() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        *s != s.to_uppercase()
    })
    .test_cases(10000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(s != s.to_uppercase());
}

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

#[test]
fn test_shrink_ligature_to_base_character() {
    let s = Minimal::new(gs::text().min_size(1).max_size(8), |s: &String| {
        s.chars().any(|c| c.eq_ignore_ascii_case(&'f'))
    })
    .test_cases(5000)
    .run();
    assert_eq!(s.chars().count(), 1);
    assert!(matches!(s.as_str(), "F" | "f"), "got {s:?}");
}
