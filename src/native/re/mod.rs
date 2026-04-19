//! Port of CPython's `Lib/re/` parser.
//!
//! This module mirrors the public shape of CPython's `_parser.py` /
//! `_constants.py` closely enough that
//! `hypothesis.strategies._internal.regex` can be ported without papering
//! over Python-vs-Rust regex semantic differences. Only the parser is
//! ported; matching/execution remains delegated to the `regex` crate at
//! test-runtime — test generation only needs to *produce* matching
//! strings, not match them.
//!
//! Vendored sources for cross-reference live under
//! `resources/cpython/Lib/re/`.

pub mod constants;
pub mod parser;

pub use constants::{AtCode, ChCode};
pub use parser::{OpCode, ParseError, ParsedPattern, SetItem, SubPattern, parse_pattern};
