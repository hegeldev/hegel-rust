// String and binary schema interpreters, plus StringAlphabet helpers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::cbor_utils::{as_text, as_u64, map_get};
use crate::native::core::{ManyState, NativeTestCase, StopTest};
use crate::native::unicodedata;
use ciborium::Value;

use super::many_more;

pub(super) fn interpret_string(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    let max_size_opt = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize);

    let alphabet = build_string_alphabet(schema);
    if alphabet.len() == 0 {
        // No valid codepoints in this range (e.g., surrogate-only range like [0xD800, 0xDFFF]).
        // This is a schema-level error, not a per-test-case filter.
        panic!(
            "InvalidArgument: No valid characters in the specified range. \
             The codepoint range contains only surrogate codepoints (U+D800..U+DFFF), \
             which are not valid Unicode characters."
        );
    }

    // Fast path: for a simple contiguous codepoint-range alphabet, emit a
    // single StringChoice node instead of decomposing the string into an
    // alphabet-building phase and one integer-per-char. This makes strings
    // much cheaper to shrink and record.
    if let StringAlphabet::Range { min, max } = alphabet {
        let max_size = max_size_opt.unwrap_or(min_size.max(100));
        let s = ntc.draw_string(min, max, min_size, max_size)?;
        return Ok(Value::Tag(91, Box::new(Value::Bytes(s.into_bytes()))));
    }

    // Filtered alphabets (categories, include/exclude lists, explicit codec):
    // fall back to the decomposed path that loops through individual integer
    // draws per char.
    let n = alphabet.len() as i128;
    let n_ascii = alphabet.ascii_count() as i128;

    // Build a small sub-alphabet (1–10 characters) following pbtkit's approach.
    // This boosts the probability of generating structurally interesting strings
    // (e.g. containing '\n', duplicate characters) by concentrating choices.
    // Each slot has a 20% chance of being drawn from the ASCII sub-range (if any
    // ASCII characters exist in the alphabet), matching pbtkit's `_draw_string`.
    let alpha_size = ntc.draw_integer(1, 10)?;
    let mut sub_alpha: Vec<i128> = Vec::with_capacity(alpha_size as usize);
    for _ in 0..alpha_size {
        // weighted(0.2): simplest() = false, so shrinker converges to no-ASCII bias.
        let use_ascii = n_ascii > 0 && ntc.weighted(0.2, None)?;
        let idx = if use_ascii {
            ntc.draw_integer(0, n_ascii - 1)?
        } else {
            ntc.draw_integer(0, n - 1)?
        };
        sub_alpha.push(idx);
    }

    let mut state = ManyState::new(min_size, max_size_opt);
    let mut result = String::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let sub_idx = ntc.draw_integer(0, alpha_size - 1)?;
        let char_idx = sub_alpha[sub_idx as usize] as usize;
        result.push(alphabet.char_at(char_idx));
    }

    Ok(Value::Tag(91, Box::new(Value::Bytes(result.into_bytes()))))
}

pub(super) fn interpret_binary(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    // If max_size is unbounded in the schema, fall back to a generous cap.
    // pbtkit and the server backend both truncate generation at some finite
    // ceiling; matching that keeps shrinker and cache behavior sensible.
    let max_size = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize)
        .unwrap_or(100);

    let bytes = ntc.draw_bytes(min_size, max_size)?;
    Ok(Value::Bytes(bytes))
}

/// Alphabet for string generation.
#[derive(Clone)]
pub(super) enum StringAlphabet {
    /// Contiguous codepoint range [min, max] with surrogates excluded.
    Range { min: u32, max: u32 },
    /// Interval-based alphabet: a codepoint range minus surrogates and a
    /// small set of excluded characters. Mirrors Hypothesis's `IntervalSet`
    /// approach (`internal/intervalsets.py`) — avoids the O(1M) codepoint
    /// scan that `Explicit` would require for large ranges with few exclusions.
    Intervals {
        ranges: Vec<(u32, u32)>,
        total: usize,
        ascii_sorted: Vec<char>,
    },
    /// Explicit list of valid characters, ordered by `codepoint_key`.
    Explicit(Vec<char>),
}

impl StringAlphabet {
    pub(super) fn len(&self) -> usize {
        match self {
            StringAlphabet::Range { min, max } => count_valid_codepoints(*min, *max) as usize,
            StringAlphabet::Intervals { total, .. } => *total,
            StringAlphabet::Explicit(v) => v.len(),
        }
    }

    /// Count how many characters in this alphabet are ASCII (codepoint < 128).
    ///
    /// These correspond to indices [0, ascii_count) in `char_at` order,
    /// since both `keyed_codepoint_at_index` and the explicit alphabet's
    /// `codepoint_sort_key` ordering put ASCII characters first.
    pub(super) fn ascii_count(&self) -> usize {
        match self {
            StringAlphabet::Range { min, max } => {
                if *min > 127 {
                    0
                } else {
                    ((*max).min(127) - *min + 1) as usize
                }
            }
            StringAlphabet::Intervals { ascii_sorted, .. } => ascii_sorted.len(),
            StringAlphabet::Explicit(v) => v.iter().take_while(|c| (**c as u32) < 128).count(),
        }
    }

    /// Return the character at position `idx` in `codepoint_sort_key` order.
    ///
    /// Index 0 returns '0' (codepoint 48) for alphabets that contain it,
    /// matching pbtkit's shrinking behavior where '0' is the simplest char.
    pub(super) fn char_at(&self, idx: usize) -> char {
        match self {
            StringAlphabet::Range { min, max } => keyed_codepoint_at_index(*min, *max, idx),
            StringAlphabet::Intervals {
                ranges,
                ascii_sorted,
                ..
            } => {
                if idx < ascii_sorted.len() {
                    ascii_sorted[idx]
                } else {
                    intervals_non_ascii_at(ranges, idx - ascii_sorted.len())
                }
            }
            StringAlphabet::Explicit(v) => v[idx],
        }
    }
}

/// Return the `idx`-th non-ASCII (codepoint >= 128) character across the
/// given sorted intervals, in natural codepoint order.
fn intervals_non_ascii_at(ranges: &[(u32, u32)], idx: usize) -> char {
    let mut remaining = idx;
    for &(start, end) in ranges {
        let lo = start.max(128);
        if lo > end {
            continue;
        }
        let count = (end - lo + 1) as usize;
        if remaining < count {
            return char::from_u32(lo + remaining as u32).unwrap();
        }
        remaining -= count;
    }
    panic!("intervals_non_ascii_at: index {idx} out of range");
}

/// Build an `Intervals` alphabet from a codepoint range minus surrogates and
/// a small set of excluded characters. O(|excluded| log |excluded|) instead
/// of the O(range_size) scan that the `Explicit` path would require.
fn build_intervals_alphabet(cp_min: u32, cp_max: u32, exclude_chars: &[char]) -> StringAlphabet {
    let mut excluded: Vec<u32> = exclude_chars
        .iter()
        .map(|c| *c as u32)
        .filter(|&cp| cp >= cp_min && cp <= cp_max && !is_surrogate_cp(cp))
        .collect();
    excluded.sort();
    excluded.dedup();

    let mut base_ranges: Vec<(u32, u32)> = Vec::new();
    if cp_max < 0xD800 || cp_min > 0xDFFF {
        base_ranges.push((cp_min, cp_max));
    } else {
        if cp_min < 0xD800 {
            base_ranges.push((cp_min, 0xD7FF));
        }
        if cp_max > 0xDFFF {
            base_ranges.push((0xE000, cp_max));
        }
    }

    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (start, end) in base_ranges {
        let mut current = start;
        for &ep in &excluded {
            if ep < current {
                continue;
            }
            if ep > end {
                break;
            }
            if current < ep {
                ranges.push((current, ep - 1));
            }
            current = ep + 1;
        }
        if current <= end {
            ranges.push((current, end));
        }
    }

    let mut ascii_sorted: Vec<char> = Vec::new();
    for &(start, end) in &ranges {
        if start > 127 {
            continue;
        }
        let hi = end.min(127);
        for cp in start..=hi {
            ascii_sorted.push(char::from_u32(cp).unwrap());
        }
    }
    ascii_sorted.sort_by_key(|c| codepoint_sort_key(*c as u32));

    let total: usize = ranges.iter().map(|(s, e)| (e - s + 1) as usize).sum();

    StringAlphabet::Intervals {
        ranges,
        total,
        ascii_sorted,
    }
}

/// Build the effective character alphabet for a string schema.
///
/// Category-driven alphabets (e.g. `categories=["Nd"]`) require a full BMP
/// scan with a category lookup per codepoint. The same schema is re-presented
/// once per draw, so we memoise the result globally keyed by the schema's
/// canonical CBOR encoding. Mirrors Hypothesis's `limited_category_index_cache`
/// in `internal/charmap.py`.
// nocov start
pub(super) fn build_string_alphabet(schema: &Value) -> StringAlphabet {
    type Cache = Mutex<HashMap<Vec<u8>, Arc<StringAlphabet>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut key = Vec::new();
    if ciborium::into_writer(schema, &mut key).is_ok() {
        if let Some(cached) = cache.lock().unwrap().get(&key) {
            return (**cached).clone();
        }
        let computed = Arc::new(build_string_alphabet_uncached(schema));
        cache.lock().unwrap().insert(key, Arc::clone(&computed));
        return (*computed).clone();
    }
    build_string_alphabet_uncached(schema)
}
// nocov end

// nocov start
fn build_string_alphabet_uncached(schema: &Value) -> StringAlphabet {
    // Determine codepoint range from codec + min/max codepoint.
    let codec = map_get(schema, "codec").and_then(as_text);
    let (mut cp_min, mut cp_max): (u32, u32) = match codec {
        Some("ascii") => (0, 127),
        Some("latin-1") | Some("iso-8859-1") => (0, 255),
        Some("utf-8") | None => (0, 0x10FFFF),
        Some(other) => panic!("Invalid codec: {}", other),
    };

    if let Some(min_cp) = map_get(schema, "min_codepoint").and_then(as_u64) {
        cp_min = cp_min.max(min_cp as u32);
    }
    if let Some(max_cp) = map_get(schema, "max_codepoint").and_then(as_u64) {
        cp_max = cp_max.min(max_cp as u32);
    }

    // Parse category/character constraints.
    let categories: Option<Vec<String>> = extract_string_array(schema, "categories");
    let exclude_categories: Option<Vec<String>> =
        extract_string_array(schema, "exclude_categories");
    let include_chars: Option<Vec<char>> = map_get(schema, "include_characters")
        .and_then(as_text)
        .map(|s| s.chars().collect());
    let exclude_chars: Option<Vec<char>> = map_get(schema, "exclude_characters")
        .and_then(as_text)
        .map(|s| s.chars().collect());

    // Validate category names. Mirrors Hypothesis `charmap.as_general_categories`.
    for cat in categories
        .iter()
        .flatten()
        .chain(exclude_categories.iter().flatten())
    {
        if !is_valid_category(cat) {
            panic!("InvalidArgument: {cat:?} is not a valid Unicode category.");
        }
    }

    // Validate no overlap between include_characters and exclude_characters.
    // Mirrors Hypothesis `strategies/_internal/core.py::characters`.
    if let (Some(incl), Some(excl)) = (include_chars.as_ref(), exclude_chars.as_ref()) {
        let overlap: Vec<char> = incl.iter().filter(|c| excl.contains(c)).copied().collect();
        if !overlap.is_empty() {
            let incl_str: String = incl.iter().collect();
            let excl_str: String = excl.iter().collect();
            panic!(
                "InvalidArgument: Characters {overlap:?} are present in both \
                 include_characters={incl_str:?} and exclude_characters={excl_str:?} (overlap)"
            );
        }
    }

    // If categories is empty AND include_characters is set: explicit alphabet from include list.
    if let Some(ref cats) = categories {
        if cats.is_empty() {
            let base: Vec<char> = include_chars.unwrap_or_default();
            let mut filtered: Vec<char> = base
                .into_iter()
                .filter(|c| {
                    let cp = *c as u32;
                    cp >= cp_min
                        && cp <= cp_max
                        && !is_surrogate(*c)
                        && !exclude_chars
                            .as_ref()
                            .map(|ec| ec.contains(c))
                            .unwrap_or(false)
                })
                .collect();
            filtered.sort_by_key(|c| codepoint_sort_key(*c as u32));
            return StringAlphabet::Explicit(filtered);
        }
    }

    // Detect "only excludes surrogates" — treat as simple range.
    let needs_category_filter = categories.is_some()
        || exclude_categories
            .as_ref()
            .map(|ec| !ec.iter().all(|c| c == "Cs"))
            .unwrap_or(false);
    let needs_char_filter = include_chars.is_some() || exclude_chars.is_some();

    if !needs_category_filter && !needs_char_filter {
        // Fast path: just use the codepoint range.
        return StringAlphabet::Range {
            min: cp_min,
            max: cp_max,
        };
    }

    // Interval fast path: when only exclude_chars is active (no category
    // filtering beyond surrogates, no include_chars), build intervals from
    // the codepoint range minus the excluded chars. This is O(|excluded|)
    // instead of the O(range_size) scan below.
    if !needs_category_filter && include_chars.is_none() {
        if let Some(ref excl) = exclude_chars {
            return build_intervals_alphabet(cp_min, cp_max, excl);
        }
    }

    // Build explicit alphabet by iterating the effective range.
    // Limit to BMP (0xFFFF) when doing category filtering for performance.
    let scan_max = if needs_category_filter {
        cp_max.min(0xFFFF)
    } else {
        cp_max
    };

    let mut alphabet: Vec<char> = Vec::new();

    for cp in cp_min..=scan_max {
        if is_surrogate_cp(cp) {
            continue;
        }
        let c = match char::from_u32(cp) {
            Some(c) => c,
            None => continue,
        };

        // Apply category filters.
        if let Some(ref cats) = categories {
            if !cats
                .iter()
                .any(|cat| unicodedata::is_in_group(c as u32, cat))
            {
                continue;
            }
        } else if let Some(ref excl_cats) = exclude_categories {
            if excl_cats
                .iter()
                .any(|cat| cat != "Cs" && unicodedata::is_in_group(c as u32, cat))
            {
                continue;
            }
        }

        // Apply explicit exclude_chars filter.
        if let Some(ref excl) = exclude_chars {
            if excl.contains(&c) {
                continue;
            }
        }

        alphabet.push(c);
    }

    // Add include_characters (if not already present). `include_characters`
    // is a union override: it adds characters regardless of the codepoint
    // range. Mirrors Hypothesis `charmap.query` which ORs in the include set.
    if let Some(incl) = include_chars {
        for c in incl {
            if !is_surrogate(c) && !alphabet.contains(&c) {
                alphabet.push(c);
            }
        }
    }

    // Sort by codepoint_sort_key so index 0 → '0' (simplest under shrinking).
    alphabet.sort_by_key(|c| codepoint_sort_key(*c as u32));

    StringAlphabet::Explicit(alphabet)
}
// nocov end

/// Sort key for codepoints: maps '0' (48) to 0, '1' to 1, ..., and
/// reorders low 128 codepoints so '0' is simplest.
/// Non-ASCII codepoints keep their natural order (key = codepoint).
///
/// Port of pbtkit's `_codepoint_key`.
fn codepoint_sort_key(c: u32) -> u32 {
    if c < 128 {
        (c + 80) % 128 // = (c - 48 + 128) % 128
    } else {
        c
    }
}

/// Return the character at `idx` in codepoint_sort_key order within [min, max].
///
/// ASCII chars (0-127) come first, sorted by codepoint_sort_key.
/// Non-ASCII chars (128+) come after, in natural codepoint order.
fn keyed_codepoint_at_index(min: u32, max: u32, idx: usize) -> char {
    // Count ASCII chars in the range.
    let ascii_end = max.min(127);
    let ascii_start = min.min(128);
    let ascii_count = if ascii_start <= ascii_end {
        count_valid_codepoints(ascii_start, ascii_end) as usize
    } else {
        0
    };

    if idx < ascii_count {
        // Find the idx-th ASCII char in codepoint_sort_key order.
        // Iterate all 128 key values; key ki corresponds to codepoint (ki+48)%128.
        let mut found = 0usize;
        for ki in 0u32..128 {
            let c = (ki + 48) % 128; // key_to_codepoint
            if c >= ascii_start && c <= ascii_end {
                if found == idx {
                    return char::from_u32(c).unwrap();
                }
                found += 1;
            }
        }
        unreachable!("keyed_codepoint_at_index: ASCII index out of range");
    } else {
        // Non-ASCII chars: natural order, skipping surrogates.
        let non_ascii_start = min.max(128);
        codepoint_at_index(non_ascii_start, max, (idx - ascii_count) as u32)
    }
}

/// Extract an array of strings from a schema field.
fn extract_string_array(schema: &Value, key: &str) -> Option<Vec<String>> {
    map_get(schema, key).and_then(|v| {
        if let Value::Array(arr) = v {
            Some(arr.iter().filter_map(as_text).map(String::from).collect())
        } else {
            None
        }
    })
}

/// Count valid (non-surrogate) codepoints in the range [min, max].
fn count_valid_codepoints(min: u32, max: u32) -> u32 {
    if min > max {
        return 0;
    }
    let total = max - min + 1;
    let overlap_lo = 0xD800u32.max(min);
    let overlap_hi = 0xDFFFu32.min(max);
    if overlap_lo <= overlap_hi {
        total - (overlap_hi - overlap_lo + 1)
    } else {
        total
    }
}

/// Map a 0-based index to a codepoint in [min, max] excluding surrogates.
fn codepoint_at_index(min: u32, max: u32, idx: u32) -> char {
    // Count codepoints in [min, min(max, 0xD7FF)] (before surrogates).
    let pre_max = 0xD7FFu32.min(max);
    let pre_count = if min <= pre_max { pre_max - min + 1 } else { 0 };

    let cp = if idx < pre_count {
        min + idx
    } else {
        let post_start = 0xE000u32.max(min);
        post_start + (idx - pre_count)
    };

    char::from_u32(cp)
        .unwrap_or_else(|| panic!("codepoint_at_index produced invalid codepoint {:#x}", cp))
}

fn is_surrogate(c: char) -> bool {
    let cp = c as u32;
    is_surrogate_cp(cp)
}

/// Whether `cat` is a valid Unicode general category name.
///
/// Accepts the seven single-letter major classes (`L`, `M`, `N`, `P`, `S`,
/// `Z`, `C`) and the 29 two-letter codes returned by Python's
/// `unicodedata.category`. Mirrors Hypothesis's validation in
/// `charmap.as_general_categories`.
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

pub(super) fn is_surrogate_cp(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/text_tests.rs"]
mod tests;
