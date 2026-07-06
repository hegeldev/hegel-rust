use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::native::core::EngineError;
use crate::native::intervalsets::IntervalSet;
use crate::unicodedata;

/// Character-alphabet constraints for a text draw, as accepted at the
/// `hegel_string_generator_text` API surface.
pub struct TextAlphabet {
    /// Restrict to a codec's range: `"ascii"`, `"latin-1"` / `"iso-8859-1"`,
    /// or `"utf-8"` (the default full-Unicode range).
    pub codec: Option<String>,
    /// Inclusive codepoint bounds, intersected with the codec range.
    pub min_codepoint: u32,
    pub max_codepoint: u32,
    /// Restrict to the union of these Unicode general categories. `Some`
    /// with an empty list means an empty alphabet.
    pub categories: Option<Vec<String>>,
    /// Remove these Unicode general categories.
    pub exclude_categories: Option<Vec<String>>,
    /// Always include these characters (unioned in last).
    pub include_characters: Option<String>,
    /// Always exclude these characters.
    pub exclude_characters: Option<String>,
}

impl Default for TextAlphabet {
    fn default() -> Self {
        TextAlphabet {
            codec: None,
            min_codepoint: 0,
            max_codepoint: u32::MAX,
            categories: None,
            exclude_categories: None,
            include_characters: None,
            exclude_characters: None,
        }
    }
}

/// Build the effective character alphabet for a text draw. Mirrors
/// Hypothesis's `charmap` handling: codec/codepoint bounds intersect,
/// surrogates are always removed, category constraints apply over the whole
/// codespace, and include/exclude character sets are applied last.
pub fn build_intervals(alphabet: &TextAlphabet) -> Result<IntervalSet, EngineError> {
    let codec = alphabet.codec.as_deref();
    let (codec_min, codec_max): (u32, u32) = match codec {
        Some("ascii") => (0, 127),
        Some("latin-1") | Some("iso-8859-1") => (0, 255),
        Some("utf-8") | None => (0, 0x10FFFF),
        Some(other) => {
            return Err(EngineError::InvalidArgument(format!(
                "invalid codec: {other}"
            )));
        }
    };
    let cp_min = codec_min.max(alphabet.min_codepoint);
    let cp_max = codec_max.min(alphabet.max_codepoint);

    let categories = alphabet.categories.as_ref();
    let exclude_categories = alphabet.exclude_categories.as_ref();
    let include_chars: Option<Vec<char>> = alphabet
        .include_characters
        .as_ref()
        .map(|s| s.chars().collect());
    let exclude_chars: Option<Vec<char>> = alphabet
        .exclude_characters
        .as_ref()
        .map(|s| s.chars().collect());

    for cat in categories
        .iter()
        .flat_map(|c| c.iter())
        .chain(exclude_categories.iter().flat_map(|c| c.iter()))
    {
        if !is_valid_category(cat) {
            return Err(EngineError::InvalidArgument(format!(
                "{cat:?} is not a valid Unicode category"
            )));
        }
    }

    if codec.is_some() {
        if let Some(ref incl) = include_chars {
            let bad: Vec<char> = incl
                .iter()
                .filter(|c| {
                    let cp = **c as u32;
                    cp < codec_min || cp > codec_max
                })
                .copied()
                .collect();
            if !bad.is_empty() {
                let codec_name = codec.unwrap_or_default();
                return Err(EngineError::InvalidArgument(format!(
                    "include_characters {bad:?} cannot be encoded by codec {codec_name:?}"
                )));
            }
        }
    }

    if let (Some(incl), Some(excl)) = (include_chars.as_ref(), exclude_chars.as_ref()) {
        let overlap: Vec<char> = incl.iter().filter(|c| excl.contains(c)).copied().collect();
        if !overlap.is_empty() {
            let incl_str: String = incl.iter().collect();
            let excl_str: String = excl.iter().collect();
            return Err(EngineError::InvalidArgument(format!(
                "characters {overlap:?} are present in both \
                 include_characters={incl_str:?} and exclude_characters={excl_str:?} (overlap)"
            )));
        }
    }

    let base = range_minus_surrogates(cp_min, cp_max);

    let needs_category_filter = categories.is_some()
        || exclude_categories
            .map(|ec| !ec.iter().all(|c| c == "Cs"))
            .unwrap_or(false);

    let mut intervals = if let Some(cats) = categories {
        if cats.is_empty() {
            IntervalSet::new(Vec::new())
        } else {
            let cat_union = categories_union(cats);
            base.intersection(&cat_union)
        }
    } else if let Some(excl_cats) = exclude_categories {
        if needs_category_filter {
            let cat_union = categories_union(
                &excl_cats
                    .iter()
                    .filter(|c| c.as_str() != "Cs")
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            base.difference(&cat_union)
        } else {
            base
        }
    } else {
        base
    };

    if let Some(ref excl) = exclude_chars {
        if !excl.is_empty() {
            let excl_set = chars_to_intervals(excl);
            intervals = intervals.difference(&excl_set);
        }
    }

    if let Some(ref incl) = include_chars {
        if !incl.is_empty() {
            let incl_filtered: Vec<char> = incl
                .iter()
                .filter(|c| !is_surrogate(**c))
                .copied()
                .collect();
            if !incl_filtered.is_empty() {
                let incl_set = chars_to_intervals(&incl_filtered);
                intervals = intervals.union(&incl_set);
            }
        }
    }

    Ok(intervals)
}

/// Build an [`IntervalSet`] containing `[min, max]` with the surrogate block
/// `[0xD800, 0xDFFF]` removed.
fn range_minus_surrogates(min: u32, max: u32) -> IntervalSet {
    if min > max {
        return IntervalSet::new(Vec::new());
    }
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    if max < 0xD800 || min > 0xDFFF {
        ranges.push((min, max));
    } else {
        if min < 0xD800 {
            ranges.push((min, 0xD7FF));
        }
        if max > 0xDFFF {
            ranges.push((0xE000, max));
        }
    }
    IntervalSet::new(ranges)
}

/// Collapse a list of (potentially duplicated, unsorted) chars into an
/// [`IntervalSet`].
fn chars_to_intervals(chars: &[char]) -> IntervalSet {
    let mut cps: Vec<u32> = chars.iter().map(|c| *c as u32).collect();
    cps.sort_unstable();
    cps.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for cp in cps {
        match ranges.last_mut() {
            Some(last) if cp == last.1 + 1 => last.1 = cp,
            _ => ranges.push((cp, cp)),
        }
    }
    IntervalSet::new(ranges)
}

/// Union of the given category abbreviations as an [`IntervalSet`] over the
/// whole codespace `0..=0x10FFFF`, matching Hypothesis's charmap (which is
/// built over `range(sys.maxunicode + 1)`). Cached per category: the scan
/// runs once per process per category name.
fn categories_union(cats: &[String]) -> IntervalSet {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<IntervalSet>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut union: Option<IntervalSet> = None;
    for cat in cats {
        let cached = {
            let map = cache.lock().unwrap();
            map.get(cat).map(Arc::clone)
        };
        let single = match cached {
            Some(s) => s,
            None => {
                let s = Arc::new(category_intervalset(cat));
                cache.lock().unwrap().insert(cat.clone(), Arc::clone(&s));
                s
            }
        };
        union = Some(match union {
            Some(u) => u.union(&single),
            None => (*single).clone(),
        });
    }
    union.unwrap_or_else(|| IntervalSet::new(Vec::new()))
}

/// Build the `IntervalSet` of codepoints (over the full codespace,
/// surrogates excepted) whose `unicodedata.category` matches `cat` (or
/// whose category starts with `cat` when `cat` is a single-letter major
/// class).
fn category_intervalset(cat: &str) -> IntervalSet {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut run_start: Option<u32> = None;
    for cp in 0u32..=0x10FFFF {
        if (0xD800..=0xDFFF).contains(&cp) {
            if let Some(start) = run_start.take() {
                ranges.push((start, cp - 1));
            }
            continue;
        }
        if unicodedata::is_in_group(cp, cat) {
            if run_start.is_none() {
                run_start = Some(cp);
            }
        } else if let Some(start) = run_start.take() {
            ranges.push((start, cp - 1));
        }
    }
    if let Some(start) = run_start {
        ranges.push((start, 0x10FFFF));
    }
    IntervalSet::new(ranges)
}

fn is_surrogate(c: char) -> bool {
    (0xD800..=0xDFFF).contains(&(c as u32))
}

/// Whether `cat` is a valid Unicode general category name. Accepts the seven
/// single-letter major classes (`L`, `M`, `N`, `P`, `S`, `Z`, `C`) and the
/// 29 two-letter codes returned by Python's `unicodedata.category`. Mirrors
/// Hypothesis's validation in `charmap.as_general_categories`.
fn is_valid_category(cat: &str) -> bool {
    matches!(
        cat,
        "L" | "M"
            | "N"
            | "P"
            | "S"
            | "Z"
            | "C"
            | "Lu"
            | "Ll"
            | "Lt"
            | "Lm"
            | "Lo"
            | "Mn"
            | "Mc"
            | "Me"
            | "Nd"
            | "Nl"
            | "No"
            | "Pc"
            | "Pd"
            | "Ps"
            | "Pe"
            | "Pi"
            | "Pf"
            | "Po"
            | "Sm"
            | "Sc"
            | "Sk"
            | "So"
            | "Zs"
            | "Zl"
            | "Zp"
            | "Cc"
            | "Cf"
            | "Cs"
            | "Co"
            | "Cn"
    )
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/text_tests.rs"]
mod tests;
