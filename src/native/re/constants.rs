//! Port of CPython's `Lib/re/_constants.py`.
//!
//! Flag bits and opcode / at / category enums used by the `_parser` port.
//! Variant names mirror the Python constants verbatim so cross-referencing
//! between `resources/cpython/Lib/re/_constants.py` and this file stays
//! mechanical.

pub const MAXREPEAT: u32 = u32::MAX;
pub const MAXGROUPS: u32 = 500;

pub const SRE_FLAG_IGNORECASE: u32 = 2;
pub const SRE_FLAG_LOCALE: u32 = 4;
pub const SRE_FLAG_MULTILINE: u32 = 8;
pub const SRE_FLAG_DOTALL: u32 = 16;
pub const SRE_FLAG_UNICODE: u32 = 32;
pub const SRE_FLAG_VERBOSE: u32 = 64;
pub const SRE_FLAG_ASCII: u32 = 256;

pub const TYPE_FLAGS: u32 = SRE_FLAG_ASCII | SRE_FLAG_LOCALE | SRE_FLAG_UNICODE;

/// Position assertion codes. Matches Python's `ATCODES` in `_constants.py`,
/// restricted to the subset the parser actually emits and the schema
/// interpreter checks for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtCode {
    Beginning,
    BeginningString,
    Boundary,
    NonBoundary,
    End,
    EndString,
}

/// Character-class category codes. Matches Python's `CHCODES`, restricted
/// to the subset the parser actually emits.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChCode {
    Digit,
    NotDigit,
    Space,
    NotSpace,
    Word,
    NotWord,
}
