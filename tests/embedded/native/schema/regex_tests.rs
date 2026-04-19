use super::*;

#[test]
fn codepoint_to_char_ascii() {
    assert_eq!(codepoint_to_char(65), 'A');
}

#[test]
#[should_panic(expected = "invalid codepoint")]
fn codepoint_to_char_surrogate_panics() {
    let _ = codepoint_to_char(0xD800);
}

#[test]
fn char_swapcase_lower_to_upper() {
    assert_eq!(char_swapcase('a'), 'A');
}

#[test]
fn char_swapcase_upper_to_lower() {
    assert_eq!(char_swapcase('Z'), 'z');
}

#[test]
fn char_swapcase_digit_is_identity() {
    assert_eq!(char_swapcase('3'), '3');
}

#[test]
fn in_category_digit_matches_ascii_digit() {
    assert!(in_category('7', ChCode::Digit));
    assert!(!in_category('a', ChCode::Digit));
}

#[test]
fn in_category_word_matches_underscore() {
    assert!(in_category('_', ChCode::Word));
    assert!(in_category('a', ChCode::Word));
    assert!(in_category('5', ChCode::Word));
    assert!(!in_category(' ', ChCode::Word));
}

#[test]
fn in_category_space_matches_whitespace() {
    assert!(in_category(' ', ChCode::Space));
    assert!(in_category('\t', ChCode::Space));
    assert!(in_category('\n', ChCode::Space));
    assert!(!in_category('a', ChCode::Space));
}
