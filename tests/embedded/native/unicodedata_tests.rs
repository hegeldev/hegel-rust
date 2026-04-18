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
