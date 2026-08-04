use super::*;
use alloc::string::ToString;

const PYTHON_RANGES: &[(u32, &str)] = &include!("unicodedata_python_reference_data.rs");

#[test]
fn matches_python_for_every_codepoint() {
    let mut start: u32 = 0;
    for &(end, expected) in PYTHON_RANGES {
        for cp in start..=end {
            let got = general_category(cp).unwrap().as_str();
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
    assert_eq!(general_category(0x0000).unwrap(), Category::Cc);
    assert_eq!(general_category(0x0020).unwrap(), Category::Zs);
    assert_eq!(general_category(0x0030).unwrap(), Category::Nd);
    assert_eq!(general_category(0x005F).unwrap(), Category::Pc);
    assert_eq!(general_category(0x002D).unwrap(), Category::Pd);
    assert_eq!(general_category(0x002B).unwrap(), Category::Sm);
    assert_eq!(general_category(0xD800).unwrap(), Category::Cs);
    assert_eq!(general_category(0xDFFF).unwrap(), Category::Cs);
    assert_eq!(general_category(0xE000).unwrap(), Category::Co);
    assert_eq!(general_category(0xFDD0).unwrap(), Category::Cn);
    assert_eq!(general_category(0x10FFFF).unwrap(), Category::Cn);
}

#[test]
fn beyond_max_codepoint_is_an_internal_error() {
    let msg = general_category(0x110000).unwrap_err().to_string();
    assert!(msg.contains("out of range"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn char_variants_agree_with_the_u32_lookups() {
    assert_eq!(general_category_char('A'), Category::Lu);
    assert_eq!(
        general_category_char('\u{10FFFF}'),
        general_category(0x10FFFF).unwrap()
    );
    assert!(is_in_group_char('A', "Lu"));
    assert!(is_in_group_char('A', "L"));
    assert!(!is_in_group_char('A', "N"));
}

#[test]
fn is_in_group_two_char_matches_exactly() {
    assert!(is_in_group('A' as u32, "Lu").unwrap());
    assert!(!is_in_group('A' as u32, "Ll").unwrap());
    assert!(is_in_group('a' as u32, "Ll").unwrap());
    assert!(is_in_group('_' as u32, "Pc").unwrap());
    assert!(!is_in_group('_' as u32, "Po").unwrap());
}

#[test]
fn is_in_group_major_class_matches_all_subclasses() {
    for &cp in &['A' as u32, 'a' as u32, 0x01C5, 0x02B0, 0x00AA] {
        assert!(is_in_group(cp, "L").unwrap(), "U+{cp:04X} should match L");
    }
    assert!(!is_in_group(' ' as u32, "L").unwrap());
    assert!(!is_in_group('0' as u32, "L").unwrap());

    assert!(is_in_group('0' as u32, "N").unwrap());
    assert!(is_in_group(0x2160, "N").unwrap());
    assert!(!is_in_group('A' as u32, "N").unwrap());

    for &cp in &[
        '_' as u32, '-' as u32, '(' as u32, ')' as u32, 0x00AB, 0x00BB, '.' as u32,
    ] {
        assert!(is_in_group(cp, "P").unwrap(), "U+{cp:04X} should match P");
    }

    assert!(is_in_group(' ' as u32, "Z").unwrap());
    assert!(is_in_group(0x2028, "Z").unwrap());
    assert!(is_in_group(0x2029, "Z").unwrap());
}

#[test]
fn is_in_group_unknown_or_invalid_matches_nothing() {
    assert!(!is_in_group('A' as u32, "Xx").unwrap());
    assert!(!is_in_group('A' as u32, "X").unwrap());
    assert!(!is_in_group('A' as u32, "").unwrap());
    assert!(!is_in_group('A' as u32, "Lux").unwrap());
}

#[test]
fn nfd_base_decomposes_diacritic_letters_to_ascii() {
    for cp in [0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5] {
        assert_eq!(nfd_base(cp), Some('A' as u32), "U+{cp:04X} → A");
    }
    for cp in [0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5] {
        assert_eq!(nfd_base(cp), Some('a' as u32), "U+{cp:04X} → a");
    }
    assert_eq!(nfd_base(0x00D1), Some('N' as u32));
    assert_eq!(nfd_base(0x00F1), Some('n' as u32));
}

#[test]
fn nfd_base_resolves_recursively() {
    assert_eq!(nfd_base(0x01FA), Some('A' as u32));
    assert_eq!(nfd_base(0x00C5), Some('A' as u32));
}

#[test]
fn nfd_base_returns_none_for_non_decomposable() {
    assert_eq!(nfd_base('A' as u32), None);
    assert_eq!(nfd_base('0' as u32), None);
    assert_eq!(nfd_base(0x00DF), None);
    assert_eq!(nfd_base(0x2160), None);
    assert_eq!(nfd_base(0x1F600), None);
    assert_eq!(nfd_base(0x82535), None);
}

#[test]
fn category_from_code_round_trips_every_variant() {
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
    assert_eq!(Category::from_code("lu"), None);
    assert_eq!(Category::from_code("Luu"), None);
}
