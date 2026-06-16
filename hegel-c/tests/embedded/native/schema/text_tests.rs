use super::*;
use crate::cbor_utils::{cbor_array, cbor_map};

fn schema(builder: fn(&mut Vec<(ciborium::Value, ciborium::Value)>)) -> ciborium::Value {
    let mut pairs: Vec<(ciborium::Value, ciborium::Value)> = vec![(
        ciborium::Value::Text("type".to_string()),
        ciborium::Value::Text("string".to_string()),
    )];
    builder(&mut pairs);
    ciborium::Value::Map(pairs)
}

// ── build_intervals ─────────────────────────────────────────────────────

#[test]
fn build_intervals_default_excludes_surrogates() {
    let s = schema(|_| {});
    let iv = build_intervals(&s).unwrap();
    // 0..=0x10FFFF minus the 2048-codepoint surrogate block.
    assert_eq!(iv.len(), 0x110000 - 2048);
    assert!(!iv.contains(0xD800));
    assert!(iv.contains(b'0' as u32));
}

#[test]
fn build_intervals_codec_ascii() {
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 128);
    assert!(iv.contains(0));
    assert!(iv.contains(127));
    assert!(!iv.contains(128));
}

#[test]
fn build_intervals_codec_latin1() {
    let s = cbor_map! {
        "type" => "string",
        "codec" => "latin-1",
    };
    assert_eq!(build_intervals(&s).unwrap().len(), 256);
}

#[test]
fn build_intervals_unknown_codec_is_invalid_argument() {
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ebcdic",
    };
    let err = build_intervals(&s).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("invalid codec"));
}

#[test]
fn build_intervals_codepoint_range() {
    let s = cbor_map! {
        "type" => "string",
        "min_codepoint" => b'a' as u64,
        "max_codepoint" => b'z' as u64,
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 26);
}

#[test]
fn build_intervals_range_straddles_surrogates() {
    let s = cbor_map! {
        "type" => "string",
        "min_codepoint" => 0xD700u64,
        "max_codepoint" => 0xE100u64,
    };
    let iv = build_intervals(&s).unwrap();
    // [0xD700..=0xD7FF] ∪ [0xE000..=0xE100] = 0x100 + 0x101 codepoints.
    assert_eq!(iv.len(), 0x100 + 0x101);
}

#[test]
fn build_intervals_range_entirely_in_surrogates_is_empty() {
    let s = cbor_map! {
        "type" => "string",
        "min_codepoint" => 0xD800u64,
        "max_codepoint" => 0xDFFFu64,
    };
    assert_eq!(build_intervals(&s).unwrap().len(), 0);
}

#[test]
fn build_intervals_categories_subset_intersects_base() {
    // categories=["Nd"]: decimal digits.
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Nd".into())],
    };
    let iv = build_intervals(&s).unwrap();
    // BMP has multiple Nd ranges; at minimum '0'..='9' are present.
    assert!(iv.contains(b'0' as u32));
    assert!(iv.contains(b'9' as u32));
    assert!(!iv.contains(b'a' as u32));
}

#[test]
fn build_intervals_exclude_categories_subtracts_from_base() {
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "exclude_categories" => cbor_array![ciborium::Value::Text("Cc".into())],
    };
    let iv = build_intervals(&s).unwrap();
    // ASCII (128) minus control characters (33: 0x00..=0x1F + 0x7F).
    assert_eq!(iv.len(), 128 - 33);
    assert!(!iv.contains(0));
    assert!(iv.contains(b' ' as u32));
}

#[test]
fn build_intervals_exclude_characters_subtracts() {
    let s = cbor_map! {
        "type" => "string",
        "min_codepoint" => b'a' as u64,
        "max_codepoint" => b'z' as u64,
        "exclude_characters" => "aeiou",
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 21);
    assert!(!iv.contains(b'a' as u32));
    assert!(iv.contains(b'b' as u32));
}

#[test]
fn build_intervals_include_characters_unions_in() {
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![],
        "include_characters" => "xyz",
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 3);
    assert!(iv.contains(b'x' as u32));
}

#[test]
fn build_intervals_include_characters_drops_surrogates() {
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![],
        "include_characters" => "ab",
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 2);
}

#[test]
fn build_intervals_invalid_category_is_invalid_argument() {
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Xx".into())],
    };
    let err = build_intervals(&s).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("valid Unicode category"));
}

#[test]
fn build_intervals_overlap_between_include_and_exclude_is_invalid_argument() {
    let s = cbor_map! {
        "type" => "string",
        "include_characters" => "abc",
        "exclude_characters" => "bcd",
    };
    let err = build_intervals(&s).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("overlap"));
}

#[test]
fn build_intervals_caches_repeated_schema() {
    let s = cbor_map! {
        "type" => "string",
        "min_codepoint" => b'a' as u64,
        "max_codepoint" => b'z' as u64,
    };
    let a = build_intervals(&s).unwrap();
    let b = build_intervals(&s).unwrap();
    assert_eq!(a.len(), b.len());
}

#[test]
fn build_intervals_unions_multiple_categories() {
    // categories with > 1 entry exercises the union path in
    // `categories_union`.
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![
            ciborium::Value::Text("Lu".into()),
            ciborium::Value::Text("Ll".into()),
        ],
    };
    let iv = build_intervals(&s).unwrap();
    // Both 'A' (Lu) and 'a' (Ll) are present.
    assert!(iv.contains(b'A' as u32));
    assert!(iv.contains(b'a' as u32));
    // Digits ('0', Nd) and punctuation are not.
    assert!(!iv.contains(b'0' as u32));
}

#[test]
fn build_intervals_category_with_run_into_surrogates() {
    // `Lo` (Other Letter) includes Hangul syllables that extend right up to
    // 0xD7A3 — the open run hits the surrogate-block early-return when the
    // BMP scan reaches 0xD800, exercising the "close-run-at-surrogate"
    // branch.
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Lo".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(0xD7A3));
    assert!(!iv.contains(0xD800));
}

#[test]
fn build_intervals_category_running_to_bmp_end() {
    // `Cn` (Unassigned) extends through 0xFFFF — the open run survives to
    // the end of the BMP scan, exercising the post-loop `if let Some(start)`
    // arm in `category_intervalset`.
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Cn".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(0xFFFF));
}

#[test]
fn build_intervals_treats_non_array_categories_field_as_absent() {
    // `extract_string_array` returns `None` on a non-array value: a schema
    // that mistakenly passes a scalar for `categories` falls back to the
    // codec-default alphabet rather than panicking.
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "categories" => "not-an-array",
    };
    let iv = build_intervals(&s).unwrap();
    // 128 ASCII codepoints (no category filter applied).
    assert_eq!(iv.len(), 128);
}

#[test]
fn build_intervals_categories_cover_astral_planes() {
    // Hypothesis's charmap spans the whole codespace (0..=0x10FFFF), not
    // just the BMP: `categories=["So"]` must include emoji (U+1F600 is So)
    // and `categories=["Lo"]` must include CJK Extension B (U+20000 is Lo).
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("So".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(0x1F600), "emoji U+1F600 (So) missing");

    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Lo".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(0x20000), "CJK Ext B U+20000 (Lo) missing");
}

#[test]
fn build_intervals_exclude_categories_excludes_astral_members() {
    // `exclude_categories=["Co"]` must remove the astral private-use planes
    // (15-16), not just the BMP private-use area: a user excluding a
    // category must never be handed an astral member of it.
    let s = cbor_map! {
        "type" => "string",
        "exclude_categories" => cbor_array![ciborium::Value::Text("Co".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(!iv.contains(0xE000), "BMP private-use must be excluded");
    assert!(
        !iv.contains(0xF0000),
        "plane-15 private-use must be excluded"
    );
    assert!(
        !iv.contains(0x10FFFD),
        "plane-16 private-use must be excluded"
    );
    assert!(iv.contains(b'a' as u32));
}

#[test]
fn build_intervals_rejects_include_characters_outside_codec() {
    // Hypothesis raises InvalidArgument for include characters the codec
    // cannot encode; silently generating them would violate the codec
    // constraint.
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "include_characters" => "é"
    };
    let err = build_intervals(&s).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("cannot be encoded"), "{err}");

    let s = cbor_map! {
        "type" => "string",
        "codec" => "latin-1",
        "include_characters" => "☃"
    };
    assert!(build_intervals(&s).is_err());

    // In-range include characters stay fine.
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "include_characters" => "az"
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(b'a' as u32) && iv.contains(b'z' as u32));
}
