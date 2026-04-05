mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators as gs;

#[test]
fn test_characters_single_char() {
    assert_all_examples(gs::characters(), |s: &String| s.chars().count() == 1);
}

#[test]
fn test_characters_ascii() {
    assert_all_examples(gs::characters().codec("ascii"), |s: &String| s.is_ascii());
}

#[hegel::test]
fn test_characters_codepoint_range(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<u32>().min_value(0).max_value(0x10FFFF));
    let hi = tc.draw(gs::integers::<u32>().min_value(lo).max_value(0x10FFFF));
    let s: String = tc.draw(gs::characters().min_codepoint(lo).max_codepoint(hi));
    let cp = s.chars().next().unwrap() as u32;
    assert!(cp >= lo && cp <= hi);
}

#[test]
fn test_characters_lu() {
    assert_all_examples(gs::characters().categories(&["Lu"]), |s: &String| {
        let c = s.chars().next().unwrap();
        c.is_uppercase()
    });
}

#[test]
fn test_characters_exclude_categories() {
    assert_all_examples(
        gs::characters().exclude_categories(&["Lu"]),
        |s: &String| {
            let c = s.chars().next().unwrap();
            !c.is_uppercase()
        },
    );
}

#[test]
fn test_characters_include_characters() {
    assert_all_examples(
        gs::characters().categories(&[]).include_characters("xyz"),
        |s: &String| {
            let c = s.chars().next().unwrap();
            "xyz".contains(c)
        },
    );
}

#[hegel::test]
fn test_characters_exclude_characters(tc: hegel::TestCase) {
    let exclude = tc.draw(gs::text().codec("ascii"));
    let s = tc.draw(gs::characters().codec("ascii").exclude_characters(&exclude));
    let c = s.chars().next().unwrap();
    assert!(!exclude.contains(c));
}

#[hegel::test]
fn test_text_alphabet(tc: hegel::TestCase) {
    let alphabet = tc.draw(gs::text().codec("ascii").min_size(1));
    let s = tc.draw(gs::text().alphabet(&alphabet));
    assert!(s.chars().all(|c| alphabet.contains(c)));
}

#[test]
fn test_find_all_alphabet() {
    find_any(gs::text().alphabet("abc").min_size(10), |s: &String| {
        s.contains('a') && s.contains('b') && s.contains('c')
    });
}

#[test]
fn test_text_single_char_alphabet() {
    assert_all_examples(
        gs::text().alphabet("x").min_size(1).max_size(5),
        |s: &String| !s.is_empty() && s.chars().all(|c| c == 'x'),
    );
}

#[test]
fn test_text_codec_ascii() {
    assert_all_examples(gs::text().codec("ascii"), |s: &String| s.is_ascii());
}

#[hegel::test]
fn test_text_codepoint_range(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<u32>().min_value(0).max_value(0x10FFFF));
    let hi = tc.draw(gs::integers::<u32>().min_value(lo).max_value(0x10FFFF));
    let s: String = tc.draw(gs::text().min_codepoint(lo).max_codepoint(hi));
    assert!(s.chars().all(|c| {
        let cp = c as u32;
        cp >= lo && cp <= hi
    }));
}

#[test]
fn test_text_categories() {
    assert_all_examples(gs::text().categories(&["Lu"]).max_size(20), |s: &String| {
        s.chars().all(|c| c.is_uppercase())
    });
}

#[test]
fn test_text_exclude_categories() {
    assert_all_examples(
        gs::text().exclude_categories(&["Lu"]).max_size(20),
        |s: &String| s.chars().all(|c| !c.is_uppercase()),
    );
}

#[test]
fn test_text_include_characters() {
    assert_all_examples(
        gs::text()
            .categories(&[])
            .include_characters("xyz")
            .max_size(20),
        |s: &String| s.chars().all(|c| "xyz".contains(c)),
    );
}

#[hegel::test]
fn test_text_exclude_characters(tc: hegel::TestCase) {
    let exclude = tc.draw(gs::text().codec("ascii"));
    let s = tc.draw(gs::text().codec("ascii").exclude_characters(&exclude));
    assert!(!s.chars().any(|c| exclude.contains(c)));
}

#[test]
fn test_regex_with_alphabet() {
    assert_all_examples(
        gs::from_regex("[a-z]+")
            .fullmatch(true)
            .alphabet(gs::characters().max_codepoint(0x7F)),
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase()),
    );
}
