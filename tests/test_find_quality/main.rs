// Tests ported from Hypothesis's findability / discovery test suite.
//
// These test that generators CAN produce particular kinds of values.
//
// Source files:
//   - hypothesis-python/tests/quality/test_discovery_ability.py
//   - hypothesis-python/tests/nocover/test_floating.py
//   - hypothesis-python/tests/nocover/test_simple_numbers.py

#[path = "../common/mod.rs"]
mod common;

mod collections;
mod floats;
mod integers;
mod one_of;
mod strings;
