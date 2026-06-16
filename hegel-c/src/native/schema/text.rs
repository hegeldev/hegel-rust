// String schema interpreter. Mirrors Hypothesis's
// `strategies/_internal/strings.py` + `internal/charmap.py`: turn the schema's
// codec / codepoint range / category / include-exclude character constraints
// into a single [`IntervalSet`], then hand it to `draw_string`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::cbor_utils::{as_text, as_u64, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use crate::native::intervalsets::IntervalSet;
use crate::unicodedata;
use ciborium::Value;

/// Default upper bound for `max_size` when the schema doesn't set one.
/// Matches the cap Hypothesis uses in its server-side `text` strategy so
/// generation doesn't run away on unbounded sizes.
const DEFAULT_MAX_SIZE: usize = 100;

pub(super) fn interpret_string(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize)
        .unwrap_or(min_size.max(DEFAULT_MAX_SIZE));

    let intervals = build_intervals(schema)?;
    if intervals.is_empty() && max_size > 0 {
        // Empty alphabets are a schema-level error — Hypothesis raises
        // `InvalidArgument` at strategy-construction time. The Hegel protocol
        // can't catch it that early, so surface it as an error at draw time.
        // The "InvalidArgument" token matches the cross-backend (server)
        // wording so backend-agnostic tests recognise the same failure.
        return Err(EngineError::InvalidArgument(
            "InvalidArgument: No valid characters in the specified range. \
             The schema's codec/codepoint/category/include/exclude constraints \
             leave no characters available."
                .to_string(),
        ));
    }

    let s = ntc.draw_string(intervals, min_size, max_size)?;
    Ok(Value::Tag(91, Box::new(Value::Bytes(s.into_bytes()))))
}

/// Build the effective character alphabet for a string schema, memoised by
/// the schema's canonical CBOR encoding. Mirrors Hypothesis's
/// `limited_category_index_cache` in `internal/charmap.py`.
pub(super) fn build_intervals(schema: &Value) -> Result<IntervalSet, EngineError> {
    type Cache = Mutex<HashMap<Vec<u8>, Arc<IntervalSet>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut key = Vec::new();
    // CBOR serialization into a `Vec<u8>` is infallible: ciborium only fails
    // on the writer (which doesn't fault for in-memory buffers) or on
    // un-encodable values (which `Value` rules out by construction).
    ciborium::into_writer(schema, &mut key).expect("CBOR encoding of schema cannot fail");
    if let Some(cached) = cache.lock().unwrap().get(&key) {
        return Ok((**cached).clone());
    }
    // Only successful results are cached; an invalid schema returns the error
    // and leaves the cache untouched.
    let computed = Arc::new(build_intervals_uncached(schema)?);
    cache.lock().unwrap().insert(key, Arc::clone(&computed));
    Ok((*computed).clone())
}

fn build_intervals_uncached(schema: &Value) -> Result<IntervalSet, EngineError> {
    let codec = map_get(schema, "codec").and_then(as_text);
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
    let (mut cp_min, mut cp_max) = (codec_min, codec_max);

    if let Some(min_cp) = map_get(schema, "min_codepoint").and_then(as_u64) {
        cp_min = cp_min.max(min_cp as u32);
    }
    if let Some(max_cp) = map_get(schema, "max_codepoint").and_then(as_u64) {
        cp_max = cp_max.min(max_cp as u32);
    }

    let categories: Option<Vec<String>> = extract_string_array(schema, "categories");
    let exclude_categories: Option<Vec<String>> =
        extract_string_array(schema, "exclude_categories");
    let include_chars: Option<Vec<char>> = map_get(schema, "include_characters")
        .and_then(as_text)
        .map(|s| s.chars().collect());
    let exclude_chars: Option<Vec<char>> = map_get(schema, "exclude_characters")
        .and_then(as_text)
        .map(|s| s.chars().collect());

    for cat in categories
        .iter()
        .flatten()
        .chain(exclude_categories.iter().flatten())
    {
        if !is_valid_category(cat) {
            return Err(EngineError::InvalidArgument(format!(
                "{cat:?} is not a valid Unicode category"
            )));
        }
    }

    // `include_characters` deliberately bypass the min/max codepoint bounds,
    // but not the codec: Hypothesis raises InvalidArgument for include
    // characters the codec cannot encode.
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

    // Start with the codec/codepoint range minus surrogates.
    let base = range_minus_surrogates(cp_min, cp_max);

    // Apply category filters. `categories=[]` together with
    // `include_characters` is the alphabet-from-include-only case: start
    // from an empty interval set rather than `base`.
    let needs_category_filter = categories.is_some()
        || exclude_categories
            .as_ref()
            .map(|ec| !ec.iter().all(|c| c == "Cs"))
            .unwrap_or(false);

    let mut intervals = if let Some(ref cats) = categories {
        if cats.is_empty() {
            // categories=[] + include_characters: alphabet is whatever
            // include_characters provides (validated against the codec
            // above, minus exclude_characters), with no codec-driven base.
            IntervalSet::new(Vec::new())
        } else {
            // categories=[...]: intersect base with the union of these
            // categories.
            let cat_union = categories_union(cats);
            base.intersection(&cat_union)
        }
    } else if let Some(ref excl_cats) = exclude_categories {
        if needs_category_filter {
            // exclude_categories with at least one non-`Cs` entry: subtract
            // the union of the excluded categories (ignoring `Cs`, which
            // is already absent from `base`).
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

    // Subtract exclude_characters.
    if let Some(ref excl) = exclude_chars {
        if !excl.is_empty() {
            let excl_set = chars_to_intervals(excl);
            intervals = intervals.difference(&excl_set);
        }
    }

    // Union in include_characters (filtered to non-surrogates).
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

fn extract_string_array(schema: &Value, key: &str) -> Option<Vec<String>> {
    map_get(schema, key).and_then(|v| {
        if let Value::Array(arr) = v {
            Some(arr.iter().filter_map(as_text).map(String::from).collect())
        } else {
            None
        }
    })
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
#[path = "../../../tests/embedded/native/schema/text_tests.rs"]
mod tests;
