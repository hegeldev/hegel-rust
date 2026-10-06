//! Python `re`'s case-insensitive matching rules.
//!
//! Under `re.IGNORECASE`, CPython's `_sre` compares characters by their
//! simple lowercase mapping (`_sre.unicode_tolower`, or `_sre.ascii_tolower`
//! under `re.ASCII`), only for characters that have a case at all
//! (`_sre.unicode_iscased`), and additionally treats the lowercase pairs in
//! `Lib/re/_casefix.py` as equal. This module ports those three pieces and
//! the queries the generator and matcher need on top of them.

use alloc::vec::Vec;

use super::constants::{SRE_FLAG_ASCII, SRE_FLAG_IGNORECASE};
use crate::native::HashMap;
use crate::sys::sync::Lazy;

fn ignorecase(flags: u32) -> bool {
    flags & SRE_FLAG_IGNORECASE != 0
}

fn ascii(flags: u32) -> bool {
    flags & SRE_FLAG_ASCII != 0
}

/// `_sre.unicode_tolower` (or `_sre.ascii_tolower` under the ASCII flag):
/// the first character of the full lowercase mapping, which is the simple
/// mapping for every character but U+0130.
pub fn lower(c: char, flags: u32) -> char {
    if ascii(flags) {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// `_sre.unicode_iscased` / `_sre.ascii_iscased`: whether `c` has a
/// lowercase or uppercase form other than itself. An uncased literal matches
/// itself alone even under `re.IGNORECASE`.
pub fn is_cased(c: char, flags: u32) -> bool {
    if ascii(flags) {
        c.is_ascii_alphabetic()
    } else {
        lower(c, flags) != c || c.to_uppercase().next().unwrap_or(c) != c
    }
}

/// `_casefix._EXTRA_CASES`: the lowercase characters Python treats as equal
/// to the lowercase character `c` beyond sharing its lowercase form, e.g.
/// `s` and `ſ`. Symmetric, and every entry is its own lowercase.
pub fn extra_cases(c: char) -> &'static [char] {
    match c {
        '\u{0069}' => &['\u{0131}'],
        '\u{0073}' => &['\u{017f}'],
        '\u{00b5}' => &['\u{03bc}'],
        '\u{0131}' => &['\u{0069}'],
        '\u{017f}' => &['\u{0073}'],
        '\u{0345}' => &['\u{03b9}', '\u{1fbe}'],
        '\u{0390}' => &['\u{1fd3}'],
        '\u{03b0}' => &['\u{1fe3}'],
        '\u{03b2}' => &['\u{03d0}'],
        '\u{03b5}' => &['\u{03f5}'],
        '\u{03b8}' => &['\u{03d1}'],
        '\u{03b9}' => &['\u{0345}', '\u{1fbe}'],
        '\u{03ba}' => &['\u{03f0}'],
        '\u{03bc}' => &['\u{00b5}'],
        '\u{03c0}' => &['\u{03d6}'],
        '\u{03c1}' => &['\u{03f1}'],
        '\u{03c2}' => &['\u{03c3}'],
        '\u{03c3}' => &['\u{03c2}'],
        '\u{03c6}' => &['\u{03d5}'],
        '\u{03d0}' => &['\u{03b2}'],
        '\u{03d1}' => &['\u{03b8}'],
        '\u{03d5}' => &['\u{03c6}'],
        '\u{03d6}' => &['\u{03c0}'],
        '\u{03f0}' => &['\u{03ba}'],
        '\u{03f1}' => &['\u{03c1}'],
        '\u{03f5}' => &['\u{03b5}'],
        '\u{0432}' => &['\u{1c80}'],
        '\u{0434}' => &['\u{1c81}'],
        '\u{043e}' => &['\u{1c82}'],
        '\u{0441}' => &['\u{1c83}'],
        '\u{0442}' => &['\u{1c84}', '\u{1c85}'],
        '\u{044a}' => &['\u{1c86}'],
        '\u{0463}' => &['\u{1c87}'],
        '\u{1c80}' => &['\u{0432}'],
        '\u{1c81}' => &['\u{0434}'],
        '\u{1c82}' => &['\u{043e}'],
        '\u{1c83}' => &['\u{0441}'],
        '\u{1c84}' => &['\u{0442}', '\u{1c85}'],
        '\u{1c85}' => &['\u{0442}', '\u{1c84}'],
        '\u{1c86}' => &['\u{044a}'],
        '\u{1c87}' => &['\u{0463}'],
        '\u{1c88}' => &['\u{a64b}'],
        '\u{1e61}' => &['\u{1e9b}'],
        '\u{1e9b}' => &['\u{1e61}'],
        '\u{1fbe}' => &['\u{0345}', '\u{03b9}'],
        '\u{1fd3}' => &['\u{0390}'],
        '\u{1fe3}' => &['\u{03b0}'],
        '\u{a64b}' => &['\u{1c88}'],
        '\u{fb05}' => &['\u{fb06}'],
        '\u{fb06}' => &['\u{fb05}'],
        _ => &[],
    }
}

const ASCII_UPPER: [char; 26] = {
    let mut table = ['A'; 26];
    let mut i = 0;
    while i < 26 {
        table[i] = (b'A' + i as u8) as char;
        i += 1;
    }
    table
};

/// Every character other than `c` whose [`lower`] is `c`, in codepoint
/// order: `K` and the Kelvin sign U+212A for `k`, nothing for an uncased or
/// an uppercase character.
pub fn uppers_of(c: char, flags: u32) -> &'static [char] {
    if ascii(flags) {
        return if c.is_ascii_lowercase() {
            let i = (c as u8 - b'a') as usize;
            &ASCII_UPPER[i..=i]
        } else {
            &[]
        };
    }
    static UPPERS: Lazy<HashMap<char, Vec<char>>> = Lazy::new(|| {
        let mut table: HashMap<char, Vec<char>> = HashMap::default();
        for cp in 0..=0x10FFFF {
            if let Some(c) = char::from_u32(cp) {
                let l = lower(c, 0);
                if l != c {
                    table.entry(l).or_default().push(c);
                }
            }
        }
        table
    });
    Lazy::force(&UPPERS).get(&c).map_or(&[], Vec::as_slice)
}

/// The lowercase forms a character must fold to in order to match the
/// cased literal with lowercase form `l`: `l` itself and its extra cases.
fn fold_targets(l: char, flags: u32) -> impl Iterator<Item = char> {
    let extras = if ascii(flags) {
        &[][..]
    } else {
        extra_cases(l)
    };
    core::iter::once(l).chain(extras.iter().copied())
}

/// Whether the literal `want` matches `got` under `flags`: equality, or
/// under `re.IGNORECASE` a cased `want` whose lowercase form `got` folds to
/// directly or through an extra case.
pub fn literal_matches(want: char, got: char, flags: u32) -> bool {
    if got == want {
        return true;
    }
    if !ignorecase(flags) || !is_cased(want, flags) {
        return false;
    }
    let got = lower(got, flags);
    fold_targets(lower(want, flags), flags).any(|t| t == got)
}

/// Whether the character-class range `lo..=hi` matches `c` under `flags`:
/// membership, or under `re.IGNORECASE` membership of some character that
/// `c` is case-equal to.
pub fn range_matches(lo: u32, hi: u32, c: char, flags: u32) -> bool {
    let in_range = |x: char| (lo..=hi).contains(&(x as u32));
    if !ignorecase(flags) {
        return in_range(c);
    }
    fold_targets(lower(c, flags), flags)
        .any(|t| in_range(t) || uppers_of(t, flags).iter().copied().any(in_range))
}

/// Whether two characters of a back-reference compare equal under `flags`:
/// `GROUPREF_UNI_IGNORE` compares lowercase forms without the extra cases.
pub fn groupref_eq(a: char, b: char, flags: u32) -> bool {
    a == b || (ignorecase(flags) && lower(a, flags) == lower(b, flags))
}

/// Append to `out` every character the literal `c` matches under `flags`,
/// `c` first and possibly with repeats: `c` alone without `re.IGNORECASE`
/// or when `c` is uncased, otherwise every character whose lowercase form is
/// one of `c`'s fold targets.
pub fn push_equivalents(out: &mut Vec<char>, c: char, flags: u32) {
    out.push(c);
    if !ignorecase(flags) || !is_cased(c, flags) {
        return;
    }
    for t in fold_targets(lower(c, flags), flags) {
        out.push(t);
        out.extend_from_slice(uppers_of(t, flags));
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/re_casefold_tests.rs"]
mod tests;
