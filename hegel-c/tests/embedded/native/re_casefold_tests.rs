//! Embedded tests for `src/native/re/casefold.rs`. The expectations were
//! checked against CPython 3.12's `_sre.unicode_tolower`,
//! `_sre.unicode_iscased`, `re._casefix._EXTRA_CASES` and `re.fullmatch`.

use super::*;
use crate::native::re::constants::{SRE_FLAG_ASCII, SRE_FLAG_IGNORECASE};
use alloc::vec;

const IC: u32 = SRE_FLAG_IGNORECASE;
const AIC: u32 = SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII;

const EXTRA_CASE_KEYS: &[char] = &[
    '\u{0069}', '\u{0073}', '\u{00b5}', '\u{0131}', '\u{017f}', '\u{0345}', '\u{0390}', '\u{03b0}',
    '\u{03b2}', '\u{03b5}', '\u{03b8}', '\u{03b9}', '\u{03ba}', '\u{03bc}', '\u{03c0}', '\u{03c1}',
    '\u{03c2}', '\u{03c3}', '\u{03c6}', '\u{03d0}', '\u{03d1}', '\u{03d5}', '\u{03d6}', '\u{03f0}',
    '\u{03f1}', '\u{03f5}', '\u{0432}', '\u{0434}', '\u{043e}', '\u{0441}', '\u{0442}', '\u{044a}',
    '\u{0463}', '\u{1c80}', '\u{1c81}', '\u{1c82}', '\u{1c83}', '\u{1c84}', '\u{1c85}', '\u{1c86}',
    '\u{1c87}', '\u{1c88}', '\u{1e61}', '\u{1e9b}', '\u{1fbe}', '\u{1fd3}', '\u{1fe3}', '\u{a64b}',
    '\u{fb05}', '\u{fb06}',
];

#[test]
fn lower_is_the_first_char_of_the_full_mapping() {
    assert_eq!(lower('A', 0), 'a');
    assert_eq!(lower('\u{130}', 0), 'i');
    assert_eq!(lower('\u{212A}', 0), 'k');
    assert_eq!(lower('1', 0), '1');
    assert_eq!(lower('A', SRE_FLAG_ASCII), 'a');
    assert_eq!(lower('À', SRE_FLAG_ASCII), 'À');
}

#[test]
fn is_cased_follows_sre() {
    assert!(is_cased('a', 0));
    assert!(is_cased('ß', 0));
    assert!(is_cased('\u{1F80}', 0));
    assert!(is_cased('ſ', 0));
    assert!(!is_cased('1', 0));
    assert!(!is_cased('\u{4E00}', 0));
    assert!(is_cased('a', SRE_FLAG_ASCII));
    assert!(!is_cased('À', SRE_FLAG_ASCII));
}

#[test]
fn extra_cases_table_is_symmetric_and_lowercase() {
    assert_eq!(extra_cases('s'), &['ſ']);
    assert_eq!(extra_cases('\u{0345}'), &['\u{03b9}', '\u{1fbe}']);
    assert!(extra_cases('x').is_empty());
    for &k in EXTRA_CASE_KEYS {
        assert!(!extra_cases(k).is_empty(), "{k:?}");
        assert_eq!(lower(k, 0), k, "{k:?}");
        for &v in extra_cases(k) {
            assert!(extra_cases(v).contains(&k), "{k:?} -> {v:?}");
            assert_eq!(lower(v, 0), v, "{v:?}");
        }
    }
}

#[test]
fn uppers_of_lists_every_char_lowering_to_the_argument() {
    assert_eq!(uppers_of('k', 0), &['K', '\u{212A}']);
    assert_eq!(uppers_of('i', 0), &['I', '\u{130}']);
    assert_eq!(uppers_of('\u{1F80}', 0), &['\u{1F88}']);
    assert!(uppers_of('K', 0).is_empty());
    assert!(uppers_of('1', 0).is_empty());
    assert_eq!(uppers_of('\u{10428}', 0), &['\u{10400}']);
    assert_eq!(uppers_of('k', SRE_FLAG_ASCII), &['K']);
    assert!(uppers_of('K', SRE_FLAG_ASCII).is_empty());
    assert!(uppers_of('à', SRE_FLAG_ASCII).is_empty());
}

#[test]
fn literal_matches_follows_python_re() {
    let cases: &[(char, char, u32, bool)] = &[
        ('a', 'a', 0, true),
        ('a', 'A', 0, false),
        ('a', 'A', IC, true),
        ('ß', '\u{1E9E}', IC, true),
        ('\u{1E9E}', 'ß', IC, true),
        ('À', 'à', AIC, false),
        ('À', 'à', IC, true),
        ('k', '\u{212A}', IC, true),
        ('k', '\u{212A}', AIC, false),
        ('i', '\u{130}', IC, true),
        ('i', '\u{131}', IC, true),
        ('I', '\u{131}', IC, true),
        ('\u{130}', 'I', IC, true),
        ('ſ', 'S', IC, true),
        ('s', 'ſ', IC, true),
        ('S', 'ſ', IC, true),
        ('\u{3A3}', '\u{3C2}', IC, true),
        ('\u{1F80}', '\u{1F88}', IC, true),
        ('\u{1C5}', '\u{1C4}', IC, true),
        ('\u{1C5}', '\u{1C6}', IC, true),
        ('\u{345}', '\u{399}', IC, true),
        ('\u{399}', '\u{345}', IC, true),
        ('1', '1', IC, true),
        ('1', '2', IC, false),
        ('\u{10400}', '\u{10428}', IC, true),
    ];
    for &(want, got, flags, expected) in cases {
        assert_eq!(
            literal_matches(want, got, flags),
            expected,
            "{want:?} vs {got:?} under {flags}"
        );
    }
}

#[test]
fn range_matches_follows_python_re() {
    let lo = 'a' as u32;
    let hi = 'z' as u32;
    assert!(range_matches(lo, hi, 'm', 0));
    assert!(!range_matches(lo, hi, 'M', 0));
    assert!(range_matches(lo, hi, 'M', IC));
    assert!(range_matches(lo, hi, '\u{212A}', IC));
    assert!(!range_matches(lo, hi, '\u{212A}', AIC));
    assert!(range_matches(lo, hi, 'ſ', IC));
    assert!(!range_matches(lo, hi, 'ſ', AIC));
    assert!(range_matches(0x131, 0x131, 'I', IC));
    assert!(range_matches(0x131, 0x131, 'i', IC));
    assert!(!range_matches('À' as u32, 'Ö' as u32, 'à', AIC));
    assert!(range_matches('À' as u32, 'Ö' as u32, 'à', IC));
    assert!(range_matches('À' as u32, 'Ö' as u32, 'Ð', SRE_FLAG_ASCII));
    assert!(range_matches(0x10400, 0x10401, '\u{10428}', IC));
}

#[test]
fn groupref_eq_compares_lowercase_forms_only() {
    assert!(groupref_eq('a', 'a', 0));
    assert!(!groupref_eq('a', 'A', 0));
    assert!(groupref_eq('a', 'A', IC));
    assert!(!groupref_eq('s', 'ſ', IC));
    assert!(!groupref_eq('à', 'À', AIC));
}

#[test]
fn push_equivalents_lists_the_literal_first() {
    let mut out = Vec::new();
    push_equivalents(&mut out, 'K', IC);
    assert_eq!(out[0], 'K');
    assert!(out.contains(&'k') && out.contains(&'\u{212A}'));
    let mut out = Vec::new();
    push_equivalents(&mut out, 'k', 0);
    assert_eq!(out, vec!['k']);
    let mut out = Vec::new();
    push_equivalents(&mut out, '1', IC);
    assert_eq!(out, vec!['1']);
    let mut out = Vec::new();
    push_equivalents(&mut out, 'à', AIC);
    assert_eq!(out, vec!['à']);
    let mut out = Vec::new();
    push_equivalents(&mut out, 'ſ', IC);
    assert_eq!(out, vec!['ſ', 'ſ', 's', 'S']);
}
