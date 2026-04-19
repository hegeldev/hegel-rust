//! Port of CPython's `Lib/re/_constants.py`.
//!
//! Flag bits and opcode / at / category enums used by the `_parser` port.
//! Variant names mirror the Python constants verbatim so cross-referencing
//! between `resources/cpython/Lib/re/_constants.py` and this file stays
//! mechanical.

// Allowed while the consumer side of the parser port (the regex strategy)
// is still being ported — every item here mirrors a CPython constant we
// expect that port to need.
#![allow(dead_code)]

pub const MAGIC: u32 = 20230612;

pub const MAXREPEAT: u32 = u32::MAX;
pub const MAXGROUPS: u32 = 500;

pub const SRE_FLAG_IGNORECASE: u32 = 2;
pub const SRE_FLAG_LOCALE: u32 = 4;
pub const SRE_FLAG_MULTILINE: u32 = 8;
pub const SRE_FLAG_DOTALL: u32 = 16;
pub const SRE_FLAG_UNICODE: u32 = 32;
pub const SRE_FLAG_VERBOSE: u32 = 64;
pub const SRE_FLAG_DEBUG: u32 = 128;
pub const SRE_FLAG_ASCII: u32 = 256;

pub const TYPE_FLAGS: u32 = SRE_FLAG_ASCII | SRE_FLAG_LOCALE | SRE_FLAG_UNICODE;
pub const GLOBAL_FLAGS: u32 = SRE_FLAG_DEBUG;

pub const SRE_INFO_PREFIX: u32 = 1;
pub const SRE_INFO_LITERAL: u32 = 2;
pub const SRE_INFO_CHARSET: u32 = 4;

/// Position assertion codes. Matches Python's `ATCODES` in `_constants.py`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtCode {
    Beginning,
    BeginningLine,
    BeginningString,
    Boundary,
    NonBoundary,
    End,
    EndLine,
    EndString,
    LocBoundary,
    LocNonBoundary,
    UniBoundary,
    UniNonBoundary,
}

/// Character-class category codes. Matches Python's `CHCODES`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChCode {
    Digit,
    NotDigit,
    Space,
    NotSpace,
    Word,
    NotWord,
    Linebreak,
    NotLinebreak,
    LocWord,
    LocNotWord,
    UniDigit,
    UniNotDigit,
    UniSpace,
    UniNotSpace,
    UniWord,
    UniNotWord,
    UniLinebreak,
    UniNotLinebreak,
}

impl AtCode {
    /// Port of `AT_MULTILINE`: `^$` → `^`/`$` line-anchored variants.
    pub fn as_multiline(self) -> AtCode {
        match self {
            AtCode::Beginning => AtCode::BeginningLine,
            AtCode::End => AtCode::EndLine,
            other => other,
        }
    }

    /// Port of `AT_LOCALE`: boundary → locale-aware boundary.
    pub fn as_locale(self) -> AtCode {
        match self {
            AtCode::Boundary => AtCode::LocBoundary,
            AtCode::NonBoundary => AtCode::LocNonBoundary,
            other => other,
        }
    }

    /// Port of `AT_UNICODE`: boundary → unicode-aware boundary.
    pub fn as_unicode(self) -> AtCode {
        match self {
            AtCode::Boundary => AtCode::UniBoundary,
            AtCode::NonBoundary => AtCode::UniNonBoundary,
            other => other,
        }
    }
}

impl ChCode {
    /// Port of `CH_LOCALE`: rewrite word categories as locale variants.
    pub fn as_locale(self) -> ChCode {
        match self {
            ChCode::Word => ChCode::LocWord,
            ChCode::NotWord => ChCode::LocNotWord,
            other => other,
        }
    }

    /// Port of `CH_UNICODE`: rewrite every category as its unicode variant.
    pub fn as_unicode(self) -> ChCode {
        match self {
            ChCode::Digit => ChCode::UniDigit,
            ChCode::NotDigit => ChCode::UniNotDigit,
            ChCode::Space => ChCode::UniSpace,
            ChCode::NotSpace => ChCode::UniNotSpace,
            ChCode::Word => ChCode::UniWord,
            ChCode::NotWord => ChCode::UniNotWord,
            ChCode::Linebreak => ChCode::UniLinebreak,
            ChCode::NotLinebreak => ChCode::UniNotLinebreak,
            other => other,
        }
    }

    /// Port of `CH_NEGATE`: toggle between positive and negative category.
    pub fn negate(self) -> ChCode {
        match self {
            ChCode::Digit => ChCode::NotDigit,
            ChCode::NotDigit => ChCode::Digit,
            ChCode::Space => ChCode::NotSpace,
            ChCode::NotSpace => ChCode::Space,
            ChCode::Word => ChCode::NotWord,
            ChCode::NotWord => ChCode::Word,
            ChCode::Linebreak => ChCode::NotLinebreak,
            ChCode::NotLinebreak => ChCode::Linebreak,
            ChCode::LocWord => ChCode::LocNotWord,
            ChCode::LocNotWord => ChCode::LocWord,
            ChCode::UniDigit => ChCode::UniNotDigit,
            ChCode::UniNotDigit => ChCode::UniDigit,
            ChCode::UniSpace => ChCode::UniNotSpace,
            ChCode::UniNotSpace => ChCode::UniSpace,
            ChCode::UniWord => ChCode::UniNotWord,
            ChCode::UniNotWord => ChCode::UniWord,
            ChCode::UniLinebreak => ChCode::UniNotLinebreak,
            ChCode::UniNotLinebreak => ChCode::UniLinebreak,
        }
    }
}
