//! Port of Python's `unicodedata` module for General Category queries.
//!
//! Backed by two text data files generated from
//! `src/unicodedata/UnicodeData.txt` (vendored Unicode 15.1.0 UCD, matching
//! Python 3.13's `unicodedata.unidata_version`). The files are embedded with
//! `include_str!` and parsed once on first lookup into a `OnceLock`-backed
//! `Vec`, keeping `cargo build` from having to parse ~6000 tuple literals
//! per compile.
//!
//! - `categories.txt`: contiguous category runs covering every codepoint in
//!   `0..=0x10FFFF`. Codepoints not listed in UnicodeData.txt are reported
//!   as `Cn` (Unassigned), matching Python.
//! - `nfd_bases.txt`: recursive NFD base for each canonically-decomposable
//!   codepoint.
//!
//! To refresh the vendored data, replace `src/unicodedata/UnicodeData.txt`
//! and run `python scripts/generate_unicodedata_tables.py`.

use std::sync::OnceLock;

/// Unicode General Category.
///
/// Names match the two-character abbreviations returned by Python's
/// `unicodedata.category(c)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Category {
    Lu,
    Ll,
    Lt,
    Lm,
    Lo,
    Mn,
    Mc,
    Me,
    Nd,
    Nl,
    No,
    Pc,
    Pd,
    Ps,
    Pe,
    Pi,
    Pf,
    Po,
    Sm,
    Sc,
    Sk,
    So,
    Zs,
    Zl,
    Zp,
    Cc,
    Cf,
    Cs,
    Co,
    Cn,
}

impl Category {
    /// Two-character category code, e.g. `"Lu"` or `"Cn"`. Matches Python's
    /// `unicodedata.category` string.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Lu => "Lu",
            Category::Ll => "Ll",
            Category::Lt => "Lt",
            Category::Lm => "Lm",
            Category::Lo => "Lo",
            Category::Mn => "Mn",
            Category::Mc => "Mc",
            Category::Me => "Me",
            Category::Nd => "Nd",
            Category::Nl => "Nl",
            Category::No => "No",
            Category::Pc => "Pc",
            Category::Pd => "Pd",
            Category::Ps => "Ps",
            Category::Pe => "Pe",
            Category::Pi => "Pi",
            Category::Pf => "Pf",
            Category::Po => "Po",
            Category::Sm => "Sm",
            Category::Sc => "Sc",
            Category::Sk => "Sk",
            Category::So => "So",
            Category::Zs => "Zs",
            Category::Zl => "Zl",
            Category::Zp => "Zp",
            Category::Cc => "Cc",
            Category::Cf => "Cf",
            Category::Cs => "Cs",
            Category::Co => "Co",
            Category::Cn => "Cn",
        }
    }

    /// Inverse of [`as_str`]. Returns `None` for unknown codes.
    fn from_code(s: &str) -> Option<Category> {
        Some(match s {
            "Lu" => Category::Lu,
            "Ll" => Category::Ll,
            "Lt" => Category::Lt,
            "Lm" => Category::Lm,
            "Lo" => Category::Lo,
            "Mn" => Category::Mn,
            "Mc" => Category::Mc,
            "Me" => Category::Me,
            "Nd" => Category::Nd,
            "Nl" => Category::Nl,
            "No" => Category::No,
            "Pc" => Category::Pc,
            "Pd" => Category::Pd,
            "Ps" => Category::Ps,
            "Pe" => Category::Pe,
            "Pi" => Category::Pi,
            "Pf" => Category::Pf,
            "Po" => Category::Po,
            "Sm" => Category::Sm,
            "Sc" => Category::Sc,
            "Sk" => Category::Sk,
            "So" => Category::So,
            "Zs" => Category::Zs,
            "Zl" => Category::Zl,
            "Zp" => Category::Zp,
            "Cc" => Category::Cc,
            "Cf" => Category::Cf,
            "Cs" => Category::Cs,
            "Co" => Category::Co,
            "Cn" => Category::Cn,
            _ => return None,
        })
    }
}

/// Embedded raw data; parsed lazily into [`ranges`] / [`nfd_bases`].
const CATEGORIES_DATA: &str = include_str!("categories.txt");
const NFD_BASES_DATA: &str = include_str!("nfd_bases.txt");

/// Run-length-encoded General Category table covering all codepoints
/// in `0..=0x10FFFF`. Entries are non-overlapping, contiguous, and sorted
/// by `end`; lookup is binary search for the first entry with `end >= cp`.
fn ranges() -> &'static [(u32, Category)] {
    static RANGES: OnceLock<Vec<(u32, Category)>> = OnceLock::new();
    RANGES.get_or_init(|| {
        CATEGORIES_DATA
            .lines()
            .map(|line| {
                let (end, cat) = line
                    .split_once(' ')
                    .unwrap_or_else(|| panic!("malformed categories.txt line: {line:?}"));
                let end = u32::from_str_radix(end, 16)
                    .unwrap_or_else(|_| panic!("bad hex in categories.txt: {end:?}"));
                let cat = Category::from_code(cat)
                    .unwrap_or_else(|| panic!("unknown category in categories.txt: {cat:?}"));
                (end, cat)
            })
            .collect()
    })
}

/// Recursive NFD base for codepoints that have a canonical decomposition,
/// sorted by codepoint. Codepoints not in this table either have no
/// canonical decomposition or are already their own base.
fn nfd_bases() -> &'static [(u32, u32)] {
    static BASES: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    BASES.get_or_init(|| {
        NFD_BASES_DATA
            .lines()
            .map(|line| {
                let (cp, base) = line
                    .split_once(' ')
                    .unwrap_or_else(|| panic!("malformed nfd_bases.txt line: {line:?}"));
                let cp = u32::from_str_radix(cp, 16)
                    .unwrap_or_else(|_| panic!("bad hex in nfd_bases.txt: {cp:?}"));
                let base = u32::from_str_radix(base, 16)
                    .unwrap_or_else(|_| panic!("bad hex in nfd_bases.txt: {base:?}"));
                (cp, base)
            })
            .collect()
    })
}

/// Return the Unicode General Category for `cp`.
///
/// Matches Python's `unicodedata.category(chr(cp))` for every codepoint in
/// `0..=0x10FFFF`. Codepoints outside that range panic, mirroring Python's
/// `chr(cp)` which rejects `cp > 0x10FFFF`.
pub fn general_category(cp: u32) -> Category {
    assert!(cp <= 0x10FFFF, "codepoint {:#x} out of range", cp);
    let table = ranges();
    // Binary search for the first range with `end >= cp`. The table is
    // contiguous and sorted by `end`, so this uniquely identifies the run.
    let idx = table
        .binary_search_by(|&(end, _)| {
            if end < cp {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        })
        .unwrap_err();
    table[idx].1
}

/// Test whether `cp` belongs to the category `group`.
///
/// `group` may be a full two-character category code (`"Lu"`, `"Cn"`, ...)
/// or a single-letter major class (`"L"`, `"N"`, ...) in which case all
/// subcategories with that prefix match. Mirrors Hypothesis's
/// `as_general_categories` prefix semantics (see
/// `hypothesis/internal/charmap.py`).
///
/// Unknown `group` strings match nothing.
pub fn is_in_group(cp: u32, group: &str) -> bool {
    let cat = general_category(cp).as_str();
    match group.len() {
        2 => cat == group,
        1 => cat.starts_with(group) && MAJOR_CLASSES.contains(&group),
        _ => false,
    }
}

const MAJOR_CLASSES: &[&str] = &["L", "M", "N", "P", "S", "Z", "C"];

/// Return the recursive NFD base codepoint for `cp`, if it has a canonical
/// decomposition that resolves to a different starting codepoint.
///
/// For example, `À` (U+00C0) → `A` (U+0041), `Ǻ` (U+01FA) → `A`, but `A`
/// itself returns `None`. The returned codepoint is itself non-decomposable
/// (i.e. applying `nfd_base` to the result yields `None`).
///
/// Codepoints with no canonical decomposition (including emoji, CJK
/// ideographs, and ASCII) return `None`. Compatibility decompositions
/// (NFKD, e.g. `Ⅰ` → `I`) are *not* applied — they're not part of NFD.
pub fn nfd_base(cp: u32) -> Option<u32> {
    let table = nfd_bases();
    table
        .binary_search_by_key(&cp, |&(c, _)| c)
        .ok()
        .map(|idx| table[idx].1)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/unicodedata_tests.rs"]
mod tests;
