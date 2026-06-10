use super::*;

// Run-length-encoded reference table dumped from Python's
// `unicodedata.category` (regenerate with
// `scripts/generate_unicodedata_python_reference.py`).
const PYTHON_RANGES: &[(u32, &str)] = &include!("unicodedata_python_reference_data.rs");

#[test]
fn matches_python_for_every_codepoint() {
    // Walk the Python-sourced RLE table and compare each codepoint's
    // category against ours. This catches both generator bugs (wrong
    // tables.rs) and lookup bugs (binary search off-by-one at range
    // boundaries).
    let mut start: u32 = 0;
    for &(end, expected) in PYTHON_RANGES {
        for cp in start..=end {
            let got = general_category(cp).as_str();
            assert_eq!(
                got, expected,
                "category mismatch at U+{cp:05X}: got {got}, expected {expected}"
            );
        }
        start = end + 1;
    }
    assert_eq!(
        start,
        0x10FFFF + 1,
        "python reference does not cover full range"
    );
}

#[test]
fn category_as_str_is_two_chars() {
    // Every returned code is a two-letter General Category, matching Python.
    use Category::*;
    for cat in [
        Lu, Ll, Lt, Lm, Lo, Mn, Mc, Me, Nd, Nl, No, Pc, Pd, Ps, Pe, Pi, Pf, Po, Sm, Sc, Sk, So, Zs,
        Zl, Zp, Cc, Cf, Cs, Co, Cn,
    ] {
        assert_eq!(cat.as_str().len(), 2);
    }
}

#[test]
fn edge_codepoints() {
    assert_eq!(general_category(0x0000), Category::Cc);
    assert_eq!(general_category(0x0020), Category::Zs);
    assert_eq!(general_category(0x0030), Category::Nd);
    assert_eq!(general_category(0x005F), Category::Pc); // '_'
    assert_eq!(general_category(0x002D), Category::Pd); // '-'
    assert_eq!(general_category(0x002B), Category::Sm); // '+'
    assert_eq!(general_category(0xD800), Category::Cs);
    assert_eq!(general_category(0xDFFF), Category::Cs);
    assert_eq!(general_category(0xE000), Category::Co);
    assert_eq!(general_category(0xFDD0), Category::Cn); // noncharacter
    assert_eq!(general_category(0x10FFFF), Category::Cn);
}

#[test]
#[should_panic(expected = "out of range")]
fn beyond_max_codepoint_panics() {
    let _ = general_category(0x110000);
}

#[test]
fn is_in_group_two_char_matches_exactly() {
    assert!(is_in_group('A' as u32, "Lu"));
    assert!(!is_in_group('A' as u32, "Ll"));
    assert!(is_in_group('a' as u32, "Ll"));
    assert!(is_in_group('_' as u32, "Pc"));
    assert!(!is_in_group('_' as u32, "Po"));
}

#[test]
fn is_in_group_major_class_matches_all_subclasses() {
    // All letter codepoints match "L".
    for &cp in &[
        'A' as u32, 'a' as u32, 0x01C5, /* Lt */
        0x02B0, /* Lm */
        0x00AA, /* Lo */
    ] {
        assert!(is_in_group(cp, "L"), "U+{cp:04X} should match L");
    }
    // Non-letters should not.
    assert!(!is_in_group(' ' as u32, "L"));
    assert!(!is_in_group('0' as u32, "L"));

    // Numbers.
    assert!(is_in_group('0' as u32, "N"));
    assert!(is_in_group(0x2160, "N")); // Roman numeral I -> Nl
    assert!(!is_in_group('A' as u32, "N"));

    // Punctuation major class covers all Pc/Pd/Ps/Pe/Pi/Pf/Po.
    for &cp in &[
        '_' as u32, '-' as u32, '(' as u32, ')' as u32, 0x00AB, 0x00BB, '.' as u32,
    ] {
        assert!(is_in_group(cp, "P"), "U+{cp:04X} should match P");
    }

    // Separators.
    assert!(is_in_group(' ' as u32, "Z"));
    assert!(is_in_group(0x2028, "Z"));
    assert!(is_in_group(0x2029, "Z"));
}

#[test]
fn is_in_group_unknown_or_invalid_matches_nothing() {
    // Unknown two-letter code.
    assert!(!is_in_group('A' as u32, "Xx"));
    // One-letter code that is not a major class.
    assert!(!is_in_group('A' as u32, "X"));
    // Empty or longer strings are rejected.
    assert!(!is_in_group('A' as u32, ""));
    assert!(!is_in_group('A' as u32, "Lux"));
}

#[test]
fn nfd_base_decomposes_diacritic_letters_to_ascii() {
    // À, Á, Â, Ã, Ä, Å all decompose to A.
    for cp in [0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5] {
        assert_eq!(nfd_base(cp), Some('A' as u32), "U+{cp:04X} → A");
    }
    // à and friends decompose to a.
    for cp in [0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5] {
        assert_eq!(nfd_base(cp), Some('a' as u32), "U+{cp:04X} → a");
    }
    // Ñ → N, ñ → n.
    assert_eq!(nfd_base(0x00D1), Some('N' as u32));
    assert_eq!(nfd_base(0x00F1), Some('n' as u32));
}

#[test]
fn nfd_base_resolves_recursively() {
    // Ǻ (U+01FA) decomposes to Å + acute, and Å decomposes to A + ring.
    // The recursive base is A.
    assert_eq!(nfd_base(0x01FA), Some('A' as u32));
    // Å (U+00C5) directly decomposes to A.
    assert_eq!(nfd_base(0x00C5), Some('A' as u32));
}

#[test]
fn nfd_base_returns_none_for_non_decomposable() {
    // ASCII letters and digits don't decompose.
    assert_eq!(nfd_base('A' as u32), None);
    assert_eq!(nfd_base('0' as u32), None);
    // ß (German sharp s) has no canonical decomposition.
    assert_eq!(nfd_base(0x00DF), None);
    // Roman numeral I uses *compatibility* decomposition (NFKD only),
    // not canonical, so NFD leaves it alone.
    assert_eq!(nfd_base(0x2160), None);
    // High-codepoint characters with no decomposition.
    assert_eq!(nfd_base(0x1F600), None); // 😀
    assert_eq!(nfd_base(0x82535), None);
}

#[test]
fn decomposition_chars_covers_canonical_forms() {
    // NFD('À') = 'A' + combining grave: both characters of the normalized
    // form are candidates, matching Python's
    // `candidates.update(unicodedata.normalize(form, c))`.
    assert_eq!(decomposition_chars(0x00C0), &[0x41, 0x300]);
    // Recursive: Ǻ (U+01FA) fully decomposes to A + ring + acute.
    assert_eq!(decomposition_chars(0x01FA), &[0x41, 0x301, 0x30A]);
}

#[test]
fn decomposition_chars_covers_compatibility_forms() {
    // Roman numeral Ⅰ (U+2160) has only a *compatibility* decomposition,
    // so it is reachable via NFKD but not NFD.
    assert_eq!(decomposition_chars(0x2160), &['I' as u32]);
    // Circled digit one ① → '1'.
    assert_eq!(decomposition_chars(0x2460), &['1' as u32]);
    // Ligature ﬁ (U+FB01) → "fi".
    assert_eq!(decomposition_chars(0xFB01), &['f' as u32, 'i' as u32]);
}

#[test]
fn decomposition_chars_covers_hangul_syllables() {
    // Hangul syllables decompose algorithmically (not via UnicodeData.txt
    // decomposition fields): 가 (U+AC00) = ᄀ (U+1100) + ᅡ (U+1161).
    assert_eq!(decomposition_chars(0xAC00), &[0x1100, 0x1161]);
    // 각 (U+AC01) adds the trailing consonant ᆨ (U+11A8).
    assert_eq!(decomposition_chars(0xAC01), &[0x1100, 0x1161, 0x11A8]);
}

#[test]
fn decomposition_chars_empty_for_non_decomposable() {
    assert!(decomposition_chars('A' as u32).is_empty());
    assert!(decomposition_chars(0x1F600).is_empty()); // 😀
}

#[test]
fn casefold_chars_full_folding() {
    // ß case-folds to "ss" — this is the canonical example of full folding
    // reaching a character ('s') that to_lowercase/to_uppercase cannot.
    assert_eq!(casefold_chars(0x00DF), &['s' as u32, 's' as u32]);
    // Simple foldings are present too.
    assert_eq!(casefold_chars('A' as u32), &['a' as u32]);
    // Already-folded characters map to nothing.
    assert!(casefold_chars('a' as u32).is_empty());
    assert!(casefold_chars('0' as u32).is_empty());
    // ΐ (U+0390) full-folds to ι + diaeresis + acute (3 chars).
    assert_eq!(casefold_chars(0x0390), &[0x3B9, 0x308, 0x301]);
}

#[test]
fn category_from_code_round_trips_every_variant() {
    // Every code emitted by `as_str` parses back to the same variant.
    let all = [
        Category::Lu,
        Category::Ll,
        Category::Lt,
        Category::Lm,
        Category::Lo,
        Category::Mn,
        Category::Mc,
        Category::Me,
        Category::Nd,
        Category::Nl,
        Category::No,
        Category::Pc,
        Category::Pd,
        Category::Ps,
        Category::Pe,
        Category::Pi,
        Category::Pf,
        Category::Po,
        Category::Sm,
        Category::Sc,
        Category::Sk,
        Category::So,
        Category::Zs,
        Category::Zl,
        Category::Zp,
        Category::Cc,
        Category::Cf,
        Category::Cs,
        Category::Co,
        Category::Cn,
    ];
    for cat in all {
        assert_eq!(Category::from_code(cat.as_str()), Some(cat));
    }
}

#[test]
fn category_from_code_rejects_unknown() {
    assert_eq!(Category::from_code(""), None);
    assert_eq!(Category::from_code("Xx"), None);
    assert_eq!(Category::from_code("lu"), None); // wrong case
    assert_eq!(Category::from_code("Luu"), None);
}
