//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_composite_kwonlyargs.py
//!
//! Tests that composite generators with parameters work when used in collection generators.
//! Python's keyword-only args have no Rust counterpart; regular function parameters
//! cover the same semantics.

use crate::common::utils::check_can_generate_examples;
use hegel::generators as gs;
use hegel::TestCase;

#[hegel::composite]
fn kwonlyargs_composites(tc: TestCase, kwarg1: &'static str) -> (String, i64) {
    let i = tc.draw(gs::integers::<i64>());
    (kwarg1.to_string(), i)
}

#[test]
fn test_composite_with_keyword_only_args() {
    check_can_generate_examples(gs::vecs(kwonlyargs_composites("test")));
}
