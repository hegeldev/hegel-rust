//! Ported from hypothesis-python/tests/cover/test_uuids.py

use crate::common::utils::{
    assert_no_examples, check_can_generate_examples, expect_panic, find_any,
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
