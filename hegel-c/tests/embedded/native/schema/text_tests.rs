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

#[test]
fn build_intervals_default_excludes_surrogates() {
    let s = schema(|_| {});
    let iv = build_intervals(&s).unwrap();
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
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Nd".into())],
    };
    let iv = build_intervals(&s).unwrap();
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
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![
            ciborium::Value::Text("Lu".into()),
            ciborium::Value::Text("Ll".into()),
        ],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(b'A' as u32));
    assert!(iv.contains(b'a' as u32));
    assert!(!iv.contains(b'0' as u32));
}

#[test]
fn build_intervals_category_with_run_into_surrogates() {
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
    let s = cbor_map! {
        "type" => "string",
        "categories" => cbor_array![ciborium::Value::Text("Cn".into())],
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(0xFFFF));
}

#[test]
fn build_intervals_treats_non_array_categories_field_as_absent() {
    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "categories" => "not-an-array",
    };
    let iv = build_intervals(&s).unwrap();
    assert_eq!(iv.len(), 128);
}

#[test]
fn build_intervals_categories_cover_astral_planes() {
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

    let s = cbor_map! {
        "type" => "string",
        "codec" => "ascii",
        "include_characters" => "az"
    };
    let iv = build_intervals(&s).unwrap();
    assert!(iv.contains(b'a' as u32) && iv.contains(b'z' as u32));
}
