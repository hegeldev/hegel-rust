use super::*;

#[test]
fn build_intervals_default_excludes_surrogates() {
    let iv = build_intervals(&TextAlphabet::default()).unwrap();
    assert_eq!(iv.len(), 0x110000 - 2048);
    assert!(!iv.contains(0xD800));
    assert!(iv.contains(b'0' as u32));
}

#[test]
fn build_intervals_codec_ascii() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 128);
    assert!(iv.contains(0));
    assert!(iv.contains(127));
    assert!(!iv.contains(128));
}

#[test]
fn build_intervals_codec_latin1() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("latin-1".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 256);
}

#[test]
fn build_intervals_codec_utf8_is_explicit_default() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("utf-8".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 0x110000 - 2048);
}

#[test]
fn build_intervals_unknown_codec_is_invalid_argument() {
    let err = build_intervals(&TextAlphabet {
        codec: Some("ebcdic".to_string()),
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("invalid codec"));
}

#[test]
fn build_intervals_codepoint_range() {
    let iv = build_intervals(&TextAlphabet {
        min_codepoint: b'a' as u32,
        max_codepoint: Some(b'z' as u32),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 26);
}

#[test]
fn build_intervals_range_straddles_surrogates() {
    let iv = build_intervals(&TextAlphabet {
        min_codepoint: 0xD700,
        max_codepoint: Some(0xE100),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 0x100 + 0x101);
}

#[test]
fn build_intervals_range_entirely_in_surrogates_is_empty() {
    let iv = build_intervals(&TextAlphabet {
        min_codepoint: 0xD800,
        max_codepoint: Some(0xDFFF),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 0);
}

#[test]
fn build_intervals_inverted_codepoint_range_is_empty() {
    let iv = build_intervals(&TextAlphabet {
        min_codepoint: b'z' as u32,
        max_codepoint: Some(b'a' as u32),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 0);
}

#[test]
fn build_intervals_categories_subset_intersects_base() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["Nd".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(b'0' as u32));
    assert!(iv.contains(b'9' as u32));
    assert!(!iv.contains(b'a' as u32));
}

#[test]
fn build_intervals_exclude_categories_subtracts_from_base() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        exclude_categories: Some(vec!["Cc".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 128 - 33);
    assert!(!iv.contains(0));
    assert!(iv.contains(b' ' as u32));
}

#[test]
fn build_intervals_exclude_only_cs_skips_category_scan() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        exclude_categories: Some(vec!["Cs".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 128);
}

#[test]
fn build_intervals_exclude_characters_subtracts() {
    let iv = build_intervals(&TextAlphabet {
        min_codepoint: b'a' as u32,
        max_codepoint: Some(b'z' as u32),
        exclude_characters: Some("aeiou".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 21);
    assert!(!iv.contains(b'a' as u32));
    assert!(iv.contains(b'b' as u32));
}

#[test]
fn build_intervals_include_characters_unions_in() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec![]),
        include_characters: Some("xyz".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 3);
    assert!(iv.contains(b'x' as u32));
}

#[test]
fn build_intervals_empty_include_and_exclude_are_no_ops() {
    let iv = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        include_characters: Some(String::new()),
        exclude_characters: Some(String::new()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(iv.len(), 128);
}

#[test]
fn build_intervals_invalid_category_is_invalid_argument() {
    let err = build_intervals(&TextAlphabet {
        categories: Some(vec!["Xx".to_string()]),
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("valid Unicode category"));
}

#[test]
fn build_intervals_invalid_exclude_category_is_invalid_argument() {
    let err = build_intervals(&TextAlphabet {
        exclude_categories: Some(vec!["Zz9".to_string()]),
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
}

#[test]
fn build_intervals_overlap_between_include_and_exclude_is_invalid_argument() {
    let err = build_intervals(&TextAlphabet {
        include_characters: Some("abc".to_string()),
        exclude_characters: Some("bcd".to_string()),
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("overlap"));
}

#[test]
fn build_intervals_unions_multiple_categories() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["Lu".to_string(), "Ll".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(b'A' as u32));
    assert!(iv.contains(b'a' as u32));
    assert!(!iv.contains(b'0' as u32));
}

#[test]
fn build_intervals_category_with_run_into_surrogates() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["Lo".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(0xD7A3));
    assert!(!iv.contains(0xD800));
}

#[test]
fn build_intervals_category_running_to_bmp_end() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["Cn".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(0xFFFF));
}

#[test]
fn build_intervals_categories_cover_astral_planes() {
    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["So".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(0x1F600), "emoji U+1F600 (So) missing");

    let iv = build_intervals(&TextAlphabet {
        categories: Some(vec!["Lo".to_string()]),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(0x20000), "CJK Ext B U+20000 (Lo) missing");
}

#[test]
fn build_intervals_exclude_categories_excludes_astral_members() {
    let iv = build_intervals(&TextAlphabet {
        exclude_categories: Some(vec!["Co".to_string()]),
        ..Default::default()
    })
    .unwrap();
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
    let err = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        include_characters: Some("é".to_string()),
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("cannot be encoded"), "{err}");

    assert!(
        build_intervals(&TextAlphabet {
            codec: Some("latin-1".to_string()),
            include_characters: Some("☃".to_string()),
            ..Default::default()
        })
        .is_err()
    );

    let iv = build_intervals(&TextAlphabet {
        codec: Some("ascii".to_string()),
        include_characters: Some("az".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert!(iv.contains(b'a' as u32) && iv.contains(b'z' as u32));
}
