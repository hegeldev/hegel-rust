//! Ported from hypothesis-python/tests/cover/test_uuids.py
//!
//! Also ports test_can_generate_specified_version from
//! hypothesis-python/tests/nocover/test_uuids.py, since the uuids()
//! version path would otherwise be uncovered.

use crate::common::utils::{
    assert_all_examples, assert_no_examples, check_can_generate_examples, expect_panic, find_any,
};
use hegel::generators as gs;

#[test]
fn test_no_nil_uuid_by_default() {
    assert_no_examples(gs::uuids(), |x: &u128| *x == 0);
}

#[test]
fn test_can_generate_nil_uuid() {
    find_any(gs::uuids().allow_nil(true), |x: &u128| *x == 0);
}

#[test]
fn test_can_only_allow_nil_uuid_with_none_version() {
    check_can_generate_examples(gs::uuids().allow_nil(true));
    expect_panic(
        || check_can_generate_examples(gs::uuids().version(4).allow_nil(true)),
        "nil UUID",
    );
}

fn uuid_version(uuid: u128) -> u8 {
    ((uuid >> 76) & 0xF) as u8
}

fn uuid_variant_rfc4122(uuid: u128) -> bool {
    ((uuid >> 62) & 0x3) == 0x2
}

#[test]
fn test_can_generate_specified_version() {
    for version in 1u8..=5 {
        assert_all_examples(gs::uuids().version(version), move |u: &u128| {
            uuid_version(*u) == version && uuid_variant_rfc4122(*u)
        });
    }
}
