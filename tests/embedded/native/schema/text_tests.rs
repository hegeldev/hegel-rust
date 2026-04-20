use super::*;

// ── build_intervals_alphabet ──────────────────────────────────────────

#[test]
fn build_intervals_alphabet_basic() {
    let alpha = build_intervals_alphabet(0, 127, &['a', 'b', 'c']);
    assert_eq!(alpha.len(), 128 - 3);
    assert!(alpha.len() > 0);
}

#[test]
fn build_intervals_alphabet_empty_excludes() {
    let alpha = build_intervals_alphabet(65, 90, &[]);
    assert_eq!(alpha.len(), 26); // A-Z
}

#[test]
fn build_intervals_alphabet_all_excluded() {
    let alpha = build_intervals_alphabet(65, 67, &['A', 'B', 'C']);
    assert_eq!(alpha.len(), 0);
}

#[test]
fn build_intervals_alphabet_excludes_surrogates() {
    let alpha = build_intervals_alphabet(0xD700, 0xE100, &[]);
    let expected = (0xD800 - 0xD700) + (0xE100 - 0xE000 + 1);
    assert_eq!(alpha.len(), expected as usize);
}

#[test]
fn build_intervals_alphabet_range_below_surrogates() {
    let alpha = build_intervals_alphabet(0, 0xD7FF, &[]);
    assert_eq!(alpha.len(), 0xD800);
}

#[test]
fn build_intervals_alphabet_range_above_surrogates() {
    let alpha = build_intervals_alphabet(0xE000, 0xFFFF, &[]);
    assert_eq!(alpha.len(), 0xFFFF - 0xE000 + 1);
}

#[test]
fn build_intervals_alphabet_range_entirely_surrogates() {
    let alpha = build_intervals_alphabet(0xD800, 0xDFFF, &[]);
    assert_eq!(alpha.len(), 0);
}

#[test]
fn build_intervals_alphabet_duplicate_excludes() {
    let alpha1 = build_intervals_alphabet(65, 90, &['A', 'A', 'B']);
    let alpha2 = build_intervals_alphabet(65, 90, &['A', 'B']);
    assert_eq!(alpha1.len(), alpha2.len());
}

#[test]
fn build_intervals_alphabet_exclude_outside_range() {
    let alpha = build_intervals_alphabet(65, 90, &['a']);
    assert_eq!(alpha.len(), 26);
}

#[test]
fn build_intervals_alphabet_exclude_surrogate_char() {
    let alpha = build_intervals_alphabet(0xD700, 0xE100, &[]);
    let alpha2 = build_intervals_alphabet(0xD700, 0xE100, &[char::from_u32(0xD700).unwrap()]);
    assert_eq!(alpha2.len(), alpha.len() - 1);
}

#[test]
fn build_intervals_alphabet_ascii_sorted_by_codepoint_key() {
    let alpha = build_intervals_alphabet(0, 127, &[]);
    if let StringAlphabet::Intervals { ascii_sorted, .. } = &alpha {
        assert_eq!(ascii_sorted[0], '0');
        assert_eq!(ascii_sorted.len(), 128);
    } else {
        panic!("expected Intervals variant");
    }
}

#[test]
fn build_intervals_alphabet_excludes_at_boundaries() {
    let alpha = build_intervals_alphabet(65, 70, &['A', 'F']);
    assert_eq!(alpha.len(), 4); // B, C, D, E
}

// ── intervals_non_ascii_at ────────────────────────────────────────────

#[test]
fn intervals_non_ascii_at_single_range() {
    let ranges = vec![(128, 200)];
    assert_eq!(
        intervals_non_ascii_at(&ranges, 0),
        char::from_u32(128).unwrap()
    );
    assert_eq!(
        intervals_non_ascii_at(&ranges, 72),
        char::from_u32(200).unwrap()
    );
}

#[test]
fn intervals_non_ascii_at_multiple_ranges() {
    let ranges = vec![(128, 130), (200, 202)];
    assert_eq!(
        intervals_non_ascii_at(&ranges, 0),
        char::from_u32(128).unwrap()
    );
    assert_eq!(
        intervals_non_ascii_at(&ranges, 2),
        char::from_u32(130).unwrap()
    );
    assert_eq!(
        intervals_non_ascii_at(&ranges, 3),
        char::from_u32(200).unwrap()
    );
    assert_eq!(
        intervals_non_ascii_at(&ranges, 5),
        char::from_u32(202).unwrap()
    );
}

#[test]
fn intervals_non_ascii_at_skips_low_ranges() {
    let ranges = vec![(10, 50), (200, 210)];
    assert_eq!(
        intervals_non_ascii_at(&ranges, 0),
        char::from_u32(200).unwrap()
    );
    assert_eq!(
        intervals_non_ascii_at(&ranges, 10),
        char::from_u32(210).unwrap()
    );
}

#[test]
#[should_panic(expected = "out of range")]
fn intervals_non_ascii_at_out_of_range() {
    let ranges = vec![(200, 202)];
    intervals_non_ascii_at(&ranges, 3);
}

// ── StringAlphabet::Intervals — len, ascii_count, char_at ─────────────

#[test]
fn intervals_len() {
    let alpha = build_intervals_alphabet(48, 122, &['a']);
    assert_eq!(alpha.len(), 122 - 48 + 1 - 1); // range minus one excluded
}

#[test]
fn intervals_ascii_count() {
    let alpha = build_intervals_alphabet(0, 200, &['x']);
    let total_ascii = 128 - 1; // 0-127 minus 'x'
    assert_eq!(alpha.ascii_count(), total_ascii);
}

#[test]
fn intervals_char_at_ascii() {
    let alpha = build_intervals_alphabet(0, 200, &[]);
    assert_eq!(alpha.char_at(0), '0');
}

#[test]
fn intervals_char_at_non_ascii() {
    let alpha = build_intervals_alphabet(0, 200, &[]);
    let ascii_len = alpha.ascii_count();
    let c = alpha.char_at(ascii_len);
    assert_eq!(c, char::from_u32(128).unwrap());
}

// ── StringAlphabet::Range — ascii_count, char_at ──────────────────────

#[test]
fn range_ascii_count_all_ascii() {
    let alpha = StringAlphabet::Range { min: 0, max: 127 };
    assert_eq!(alpha.ascii_count(), 128);
}

#[test]
fn range_ascii_count_above_ascii() {
    let alpha = StringAlphabet::Range { min: 200, max: 300 };
    assert_eq!(alpha.ascii_count(), 0);
}

#[test]
fn range_ascii_count_partial() {
    let alpha = StringAlphabet::Range { min: 100, max: 200 };
    assert_eq!(alpha.ascii_count(), 28);
}

#[test]
fn range_char_at_starts_with_zero() {
    let alpha = StringAlphabet::Range { min: 0, max: 127 };
    assert_eq!(alpha.char_at(0), '0');
}

#[test]
fn range_char_at_non_ascii() {
    let alpha = StringAlphabet::Range { min: 0, max: 200 };
    let c = alpha.char_at(128);
    assert_eq!(c, char::from_u32(128).unwrap());
}

// ── StringAlphabet::Explicit — ascii_count, char_at ───────────────────

#[test]
fn explicit_ascii_count() {
    let alpha = StringAlphabet::Explicit(vec!['A', 'B', 'C']);
    assert_eq!(alpha.ascii_count(), 3);
}

#[test]
fn explicit_ascii_count_with_non_ascii() {
    let alpha = StringAlphabet::Explicit(vec!['A', 'B', char::from_u32(200).unwrap()]);
    assert_eq!(alpha.ascii_count(), 2);
}

#[test]
fn explicit_char_at() {
    let alpha = StringAlphabet::Explicit(vec!['x', 'y', 'z']);
    assert_eq!(alpha.char_at(0), 'x');
    assert_eq!(alpha.char_at(2), 'z');
}

// ── keyed_codepoint_at_index ──────────────────────────────────────────

#[test]
fn keyed_codepoint_at_index_first_is_zero() {
    assert_eq!(keyed_codepoint_at_index(0, 127, 0), '0');
}

#[test]
fn keyed_codepoint_at_index_non_ascii_region() {
    let c = keyed_codepoint_at_index(0, 200, 128);
    assert_eq!(c, char::from_u32(128).unwrap());
}

#[test]
fn keyed_codepoint_at_index_limited_ascii_range() {
    assert_eq!(keyed_codepoint_at_index(65, 90, 0), 'A');
}

#[test]
fn keyed_codepoint_at_index_all_ascii_chars() {
    let mut seen = Vec::new();
    for i in 0..128 {
        seen.push(keyed_codepoint_at_index(0, 127, i));
    }
    seen.sort();
    let expected: Vec<char> = (0u32..128).map(|c| char::from_u32(c).unwrap()).collect();
    assert_eq!(seen, expected);
}

// ── codepoint_at_index ────────────────────────────────────────────────

#[test]
fn codepoint_at_index_simple() {
    assert_eq!(codepoint_at_index(65, 90, 0), 'A');
    assert_eq!(codepoint_at_index(65, 90, 25), 'Z');
}

#[test]
fn codepoint_at_index_across_surrogates() {
    let c = codepoint_at_index(0xD7FE, 0xE002, 0);
    assert_eq!(c, char::from_u32(0xD7FE).unwrap());
    let c2 = codepoint_at_index(0xD7FE, 0xE002, 2);
    assert_eq!(c2, char::from_u32(0xE000).unwrap());
}

// ── extract_string_array ──────────────────────────────────────────────

#[test]
fn extract_string_array_present() {
    let schema = crate::cbor_utils::cbor_map! {
        "cats" => Value::Array(vec![
            Value::Text("Nd".to_string()),
            Value::Text("Lu".to_string()),
        ])
    };
    let result = extract_string_array(&schema, "cats");
    assert_eq!(result, Some(vec!["Nd".to_string(), "Lu".to_string()]));
}

#[test]
fn extract_string_array_missing() {
    let schema = crate::cbor_utils::cbor_map! {};
    assert_eq!(extract_string_array(&schema, "cats"), None);
}

#[test]
fn extract_string_array_not_array() {
    let schema = crate::cbor_utils::cbor_map! {
        "cats" => Value::Text("Nd".to_string())
    };
    assert_eq!(extract_string_array(&schema, "cats"), None);
}

// ── codepoint_sort_key ────────────────────────────────────────────────

#[test]
fn codepoint_sort_key_zero_maps_to_48() {
    assert_eq!(codepoint_sort_key(48), 0);
}

#[test]
fn codepoint_sort_key_non_ascii_identity() {
    assert_eq!(codepoint_sort_key(200), 200);
    assert_eq!(codepoint_sort_key(1000), 1000);
}
