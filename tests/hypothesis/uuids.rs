//! Ported from hypothesis-python/tests/cover/test_uuids.py
//!
//! Also ports test_can_generate_specified_version from
//! hypothesis-python/tests/nocover/test_uuids.py, since the uuids()
//! version path would otherwise be uncovered.

use crate::common::utils::{assert_all_examples, assert_no_examples};
use hegel::generators as gs;

const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

#[test]
fn test_no_nil_uuid_by_default() {
    assert_no_examples(gs::uuids(), |s: &String| s == NIL_UUID);
}

#[test]
fn test_uuids_have_canonical_form() {
    assert_all_examples(gs::uuids(), |s: &String| {
        let bytes = s.as_bytes();
        bytes.len() == 36
            && bytes[8] == b'-'
            && bytes[13] == b'-'
            && bytes[18] == b'-'
            && bytes[23] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| matches!(i, 8 | 13 | 18 | 23) || b.is_ascii_hexdigit())
    });
}

#[test]
fn test_can_generate_specified_version() {
    for version in 1u8..=5 {
        assert_all_examples(gs::uuids().version(version), move |s: &String| {
            // Version digit is the 15th character (index 14) of the canonical form.
            // RFC 4122 variant: byte at index 19 has top two bits 10, so the hex
            // digit is one of 8, 9, a, b.
            let bytes = s.as_bytes();
            let version_digit = (bytes[14] as char).to_digit(16);
            let variant_digit = (bytes[19] as char).to_digit(16);
            version_digit == Some(u32::from(version)) && matches!(variant_digit, Some(0x8..=0xb))
        });
    }
}
