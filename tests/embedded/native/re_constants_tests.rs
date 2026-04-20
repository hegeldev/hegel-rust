//! Embedded tests for `src/native/re/constants.rs`.
//!
//! The rewrite helpers (`AtCode::as_multiline`/`as_locale`/`as_unicode`
//! and `ChCode::as_locale`/`as_unicode`/`negate`) aren't exercised yet
//! by the parser — they're provided for the regex-strategy port that's
//! still landing. Test them directly so the constants file stays
//! covered in the meantime.

use super::*;

#[test]
fn atcode_as_multiline_rewrites_beginning_and_end() {
    assert_eq!(AtCode::Beginning.as_multiline(), AtCode::BeginningLine);
    assert_eq!(AtCode::End.as_multiline(), AtCode::EndLine);
}

#[test]
fn atcode_as_multiline_leaves_other_variants_unchanged() {
    for at in [
        AtCode::BeginningLine,
        AtCode::BeginningString,
        AtCode::Boundary,
        AtCode::NonBoundary,
        AtCode::EndLine,
        AtCode::EndString,
        AtCode::LocBoundary,
        AtCode::LocNonBoundary,
        AtCode::UniBoundary,
        AtCode::UniNonBoundary,
    ] {
        assert_eq!(at.as_multiline(), at);
    }
}

#[test]
fn atcode_as_locale_rewrites_boundary_variants() {
    assert_eq!(AtCode::Boundary.as_locale(), AtCode::LocBoundary);
    assert_eq!(AtCode::NonBoundary.as_locale(), AtCode::LocNonBoundary);
}

#[test]
fn atcode_as_locale_leaves_other_variants_unchanged() {
    for at in [
        AtCode::Beginning,
        AtCode::BeginningLine,
        AtCode::BeginningString,
        AtCode::End,
        AtCode::EndLine,
        AtCode::EndString,
        AtCode::LocBoundary,
        AtCode::LocNonBoundary,
        AtCode::UniBoundary,
        AtCode::UniNonBoundary,
    ] {
        assert_eq!(at.as_locale(), at);
    }
}

#[test]
fn atcode_as_unicode_rewrites_boundary_variants() {
    assert_eq!(AtCode::Boundary.as_unicode(), AtCode::UniBoundary);
    assert_eq!(AtCode::NonBoundary.as_unicode(), AtCode::UniNonBoundary);
}

#[test]
fn atcode_as_unicode_leaves_other_variants_unchanged() {
    for at in [
        AtCode::Beginning,
        AtCode::BeginningLine,
        AtCode::BeginningString,
        AtCode::End,
        AtCode::EndLine,
        AtCode::EndString,
        AtCode::LocBoundary,
        AtCode::LocNonBoundary,
        AtCode::UniBoundary,
        AtCode::UniNonBoundary,
    ] {
        assert_eq!(at.as_unicode(), at);
    }
}

#[test]
fn chcode_as_locale_rewrites_word_variants() {
    assert_eq!(ChCode::Word.as_locale(), ChCode::LocWord);
    assert_eq!(ChCode::NotWord.as_locale(), ChCode::LocNotWord);
}

#[test]
fn chcode_as_locale_leaves_other_variants_unchanged() {
    for ch in [
        ChCode::Digit,
        ChCode::NotDigit,
        ChCode::Space,
        ChCode::NotSpace,
        ChCode::Linebreak,
        ChCode::NotLinebreak,
        ChCode::LocWord,
        ChCode::LocNotWord,
        ChCode::UniDigit,
        ChCode::UniNotDigit,
        ChCode::UniSpace,
        ChCode::UniNotSpace,
        ChCode::UniWord,
        ChCode::UniNotWord,
        ChCode::UniLinebreak,
        ChCode::UniNotLinebreak,
    ] {
        assert_eq!(ch.as_locale(), ch);
    }
}

#[test]
fn chcode_as_unicode_rewrites_all_base_variants() {
    assert_eq!(ChCode::Digit.as_unicode(), ChCode::UniDigit);
    assert_eq!(ChCode::NotDigit.as_unicode(), ChCode::UniNotDigit);
    assert_eq!(ChCode::Space.as_unicode(), ChCode::UniSpace);
    assert_eq!(ChCode::NotSpace.as_unicode(), ChCode::UniNotSpace);
    assert_eq!(ChCode::Word.as_unicode(), ChCode::UniWord);
    assert_eq!(ChCode::NotWord.as_unicode(), ChCode::UniNotWord);
    assert_eq!(ChCode::Linebreak.as_unicode(), ChCode::UniLinebreak);
    assert_eq!(ChCode::NotLinebreak.as_unicode(), ChCode::UniNotLinebreak);
}

#[test]
fn chcode_as_unicode_leaves_locale_and_unicode_variants_unchanged() {
    for ch in [
        ChCode::LocWord,
        ChCode::LocNotWord,
        ChCode::UniDigit,
        ChCode::UniNotDigit,
        ChCode::UniSpace,
        ChCode::UniNotSpace,
        ChCode::UniWord,
        ChCode::UniNotWord,
        ChCode::UniLinebreak,
        ChCode::UniNotLinebreak,
    ] {
        assert_eq!(ch.as_unicode(), ch);
    }
}

#[test]
fn chcode_negate_is_involutive_on_every_variant() {
    for ch in [
        ChCode::Digit,
        ChCode::NotDigit,
        ChCode::Space,
        ChCode::NotSpace,
        ChCode::Word,
        ChCode::NotWord,
        ChCode::Linebreak,
        ChCode::NotLinebreak,
        ChCode::LocWord,
        ChCode::LocNotWord,
        ChCode::UniDigit,
        ChCode::UniNotDigit,
        ChCode::UniSpace,
        ChCode::UniNotSpace,
        ChCode::UniWord,
        ChCode::UniNotWord,
        ChCode::UniLinebreak,
        ChCode::UniNotLinebreak,
    ] {
        assert_ne!(ch.negate(), ch);
        assert_eq!(ch.negate().negate(), ch);
    }
}

#[test]
fn chcode_negate_pairs_expected_variants() {
    assert_eq!(ChCode::Digit.negate(), ChCode::NotDigit);
    assert_eq!(ChCode::Space.negate(), ChCode::NotSpace);
    assert_eq!(ChCode::Word.negate(), ChCode::NotWord);
    assert_eq!(ChCode::Linebreak.negate(), ChCode::NotLinebreak);
    assert_eq!(ChCode::LocWord.negate(), ChCode::LocNotWord);
    assert_eq!(ChCode::UniDigit.negate(), ChCode::UniNotDigit);
    assert_eq!(ChCode::UniSpace.negate(), ChCode::UniNotSpace);
    assert_eq!(ChCode::UniWord.negate(), ChCode::UniNotWord);
    assert_eq!(ChCode::UniLinebreak.negate(), ChCode::UniNotLinebreak);
}
