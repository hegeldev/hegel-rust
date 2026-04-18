//! Port of Python's `unicodedata` module for General Category queries.
//!
//! Backed by a run-length-encoded table generated from
//! `src/native/unicodedata/UnicodeData.txt` (vendored Unicode 15.1.0 UCD,
//! matching Python 3.13's `unicodedata.unidata_version`). The table covers
//! every codepoint in `0..=0x10FFFF`: codepoints not listed in
//! UnicodeData.txt are reported as `Cn` (Unassigned), matching Python.
//!
//! To refresh the vendored data, replace
//! `src/native/unicodedata/UnicodeData.txt` and run
//! `python scripts/generate_unicodedata_tables.py`.

mod tables;

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
}

/// Return the Unicode General Category for `cp`.
///
/// Matches Python's `unicodedata.category(chr(cp))` for every codepoint in
/// `0..=0x10FFFF`. Codepoints outside that range panic, mirroring Python's
/// `chr(cp)` which rejects `cp > 0x10FFFF`.
pub fn general_category(cp: u32) -> Category {
    assert!(cp <= 0x10FFFF, "codepoint {:#x} out of range", cp);
    // Binary search for the first range with `end >= cp`. The table is
    // contiguous and sorted by `end`, so this uniquely identifies the run.
    let idx = tables::RANGES
        .binary_search_by(|&(end, _)| {
            if end < cp {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        })
        .unwrap_err();
    tables::RANGES[idx].1
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

#[cfg(test)]
#[path = "../../tests/embedded/native/unicodedata_tests.rs"]
mod tests;
