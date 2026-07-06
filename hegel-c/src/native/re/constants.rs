//! Port of CPython's `Lib/re/_constants.py`.
//!
//! Flag bits and opcode / at / category enums used by the `_parser` port.
//! Variant names mirror the Python constants verbatim so cross-referencing
//! between `resources/cpython/Lib/re/_constants.py` and this file stays
//! mechanical.

pub const MAXREPEAT: u32 = u32::MAX;
/// Deliberately far below CPython's `MAXGROUPS` (2^30 - 1): every capture
/// group costs the generator per-draw bookkeeping, so the cap bounds memory
/// for pathological patterns. Real-world patterns with >500 groups are
/// effectively nonexistent.
pub const MAXGROUPS: u32 = 500;

/// Maximum group/alternation nesting depth. CPython relies on the interpreter
/// recursion limit to raise a (catchable) `RecursionError` on pathologically
/// nested patterns; the native parser and generator recurse on the Rust stack,
/// so we bound the depth explicitly and surface a clean parse error instead of
/// overflowing the stack. `parse`/`parse_sub` add ~2 to the depth per nested
/// group, so this allows well over 50 levels of real nesting while staying far
/// below what would exhaust a small (e.g. 2 MiB) thread stack.
pub const MAX_NESTING: u32 = 100;

pub const SRE_FLAG_IGNORECASE: u32 = 2;
pub const SRE_FLAG_LOCALE: u32 = 4;
pub const SRE_FLAG_MULTILINE: u32 = 8;
pub const SRE_FLAG_DOTALL: u32 = 16;
pub const SRE_FLAG_UNICODE: u32 = 32;
pub const SRE_FLAG_VERBOSE: u32 = 64;
pub const SRE_FLAG_ASCII: u32 = 256;

pub const TYPE_FLAGS: u32 = SRE_FLAG_ASCII | SRE_FLAG_LOCALE | SRE_FLAG_UNICODE;

/// Position assertion codes. Matches Python's `ATCODES` in `_constants.py`,
/// restricted to the subset the parser actually emits and the regex
/// generator checks for.
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
