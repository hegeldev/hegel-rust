// Tests ported from Hypothesis's shrink quality test suite.
//
// Source files:
//   - hypothesis-python/tests/quality/test_shrink_quality.py
//   - hypothesis-python/tests/quality/test_float_shrinking.py
//   - hypothesis-python/tests/nocover/test_flatmap.py
//   - hypothesis-python/tests/nocover/test_find.py
//   - hypothesis-python/tests/nocover/test_collective_minimization.py

#[path = "../common/mod.rs"]
mod common;

mod collections;
mod composite;
mod flatmap;
mod floats;
mod integers;
mod mixed_types;
mod strings;
