// Schema interpreter for the native backend.
//
// Translates CBOR schemas (as sent by hegel generators) into concrete
// values using pbtkit-style choice recording. Only schemas usable from
// pbtkit's core.py are implemented; everything else is `todo!()`.

use crate::cbor_utils::{as_bool, as_text, as_u64, map_get};
use crate::native::core::{ManyState, NativeTestCase, Status, StopTest};
use crate::test_case::StopTestError;
use ciborium::Value;

/// Top-level dispatcher for native request handling.
///
/// Called from TestCase::send_request when the native backend is active.
pub fn dispatch_request(
    ntc: &mut NativeTestCase,
    command: &str,
    payload: &Value,
) -> Result<Value, StopTestError> {
    match command {
        "generate" => {
            let schema = map_get(payload, "schema").expect("generate command missing schema");
            interpret_schema(ntc, schema).map_err(|StopTest| StopTestError)
        }
        "start_span" | "stop_span" => {
            // Spans are tracked locally by TestCase for output purposes.
            // The native backend doesn't need to do anything here yet.
            Ok(Value::Null)
        }
        "new_collection" => {
            let min_size = map_get(payload, "min_size")
                .and_then(as_u64)
                .unwrap_or(0) as usize;
            let max_size = map_get(payload, "max_size").and_then(as_u64).map(|n| n as usize);
            let state = ManyState::new(min_size, max_size);
            let id = ntc.new_collection(state);
            Ok(Value::Integer(id.into()))
        }
        "collection_more" => {
            let id = map_get(payload, "collection_id")
                .map(cbor_to_i64)
                .expect("collection_more missing collection_id");
            let mut state = ntc
                .collections
                .remove(&id)
                .expect("collection_more: unknown collection_id");
            let result = many_more(ntc, &mut state).map_err(|StopTest| StopTestError)?;
            ntc.collections.insert(id, state);
            Ok(Value::Bool(result))
        }
        "collection_reject" => {
            let id = map_get(payload, "collection_id")
                .map(cbor_to_i64)
                .expect("collection_reject missing collection_id");
            let mut state = ntc
                .collections
                .remove(&id)
                .expect("collection_reject: unknown collection_id");
            many_reject(ntc, &mut state).map_err(|StopTest| StopTestError)?;
            ntc.collections.insert(id, state);
            Ok(Value::Null)
        }
        "new_pool" => {
            let pool_id = ntc.variable_pools.len() as i64;
            ntc.variable_pools.push(crate::native::core::NativeVariables::new());
            Ok(Value::Integer(pool_id.into()))
        }
        "pool_add" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_add missing pool_id") as usize;
            let variable_id = ntc.variable_pools[pool_id].next() as i64;
            Ok(Value::Integer(variable_id.into()))
        }
        "pool_consume" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_consume missing pool_id") as usize;
            let variable_id = map_get(payload, "variable_id")
                .map(cbor_to_i64)
                .expect("pool_consume missing variable_id") as i128;
            ntc.variable_pools[pool_id].consume(variable_id);
            Ok(Value::Null)
        }
        "pool_generate" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_generate missing pool_id") as usize;
            let consume = map_get(payload, "consume")
                .and_then(as_bool)
                .unwrap_or(false);

            let active = ntc.variable_pools[pool_id].active();
            if active.is_empty() {
                // No variables available: mark test case as invalid.
                return Err(StopTestError);
            }
            let n = active.len() as i128;
            // Draw index from [0, n-1]. Shrink towards n-1 (last added = most recent)
            // by drawing k from [0, n-1] and using index = n-1-k.
            let k = ntc.draw_integer(0, n - 1).map_err(|StopTest| StopTestError)?;
            let idx = (n - 1 - k) as usize;
            let variable_id = active[idx] as i64;
            if consume {
                ntc.variable_pools[pool_id].consume(variable_id as i128);
            }
            Ok(Value::Integer(variable_id.into()))
        }
        _ => panic!("Unknown native command: {}", command),
    }
}

/// Interpret a CBOR schema and produce a value using the native test case.
fn interpret_schema(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let schema_type = map_get(schema, "type")
        .and_then(as_text)
        .expect("Schema must have a \"type\" field");

    match schema_type {
        "integer" => interpret_integer(ntc, schema),
        "boolean" => interpret_boolean(ntc),
        "constant" => interpret_constant(schema),
        "null" => Ok(Value::Null),
        "tuple" => interpret_tuple(ntc, schema),
        "one_of" => interpret_one_of(ntc, schema),
        "sampled_from" => interpret_sampled_from(ntc, schema),
        "list" => interpret_list(ntc, schema),
        "dict" => interpret_dict(ntc, schema),
        "string" => interpret_string(ntc, schema),
        "binary" => interpret_binary(ntc, schema),

        "float" => interpret_float(ntc, schema),
        "regex" => todo!("Native backend does not yet support regex schema"),
        "email" => todo!("Native backend does not yet support email schema"),
        "url" => todo!("Native backend does not yet support url schema"),
        "domain" => todo!("Native backend does not yet support domain schema"),
        "ipv4" => todo!("Native backend does not yet support ipv4 schema"),
        "ipv6" => todo!("Native backend does not yet support ipv6 schema"),
        "date" => todo!("Native backend does not yet support date schema"),
        "time" => todo!("Native backend does not yet support time schema"),
        "datetime" => todo!("Native backend does not yet support datetime schema"),

        other => panic!("Unknown schema type: {}", other),
    }
}

fn interpret_integer(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let min_cbor = map_get(schema, "min_value").expect("integer schema must have min_value");
    let max_cbor = map_get(schema, "max_value").expect("integer schema must have max_value");
    let min_value = cbor_to_i128(min_cbor);
    let max_value = cbor_to_i128(max_cbor);

    // If max saturated because it exceeded i128::MAX (e.g. u128::MAX), draw using
    // a selector + two 64-bit halves to cover the full u128 range.
    if bignum_overflows_i128(max_cbor) {
        // Selector: 0 = u128::MIN, 1 = u128::MAX, else = random two-halves.
        // Edge case boosting on the selector naturally produces the min (0) often.
        // Selector = 1 gives u128::MAX with ~1% probability.
        let selector = ntc.draw_integer(0, 99)?;
        match selector {
            0 => return Ok(u128_to_cbor(0u128)),
            1 => return Ok(u128_to_cbor(u128::MAX)),
            _ => {}
        }
        let hi = ntc.draw_integer(0, u64::MAX as i128)?;
        let lo = ntc.draw_integer(0, u64::MAX as i128)?;
        let v = ((hi as u128) << 64) | (lo as u128);
        return Ok(u128_to_cbor(v));
    }

    let v = ntc.draw_integer(min_value, max_value)?;
    Ok(i128_to_cbor(v))
}

fn interpret_boolean(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let v = ntc.weighted(0.5, None)?;
    Ok(Value::Bool(v))
}

fn interpret_constant(schema: &Value) -> Result<Value, StopTest> {
    let value = map_get(schema, "value").expect("constant schema must have value");
    Ok(value.clone())
}

fn interpret_tuple(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let elements = match map_get(schema, "elements") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("tuple schema must have elements array"),
    };
    let mut results = Vec::with_capacity(elements.len());
    for element_schema in elements {
        results.push(interpret_schema(ntc, element_schema)?);
    }
    Ok(Value::Array(results))
}

fn interpret_one_of(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let generators = match map_get(schema, "generators") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("one_of schema must have generators array"),
    };
    assert!(!generators.is_empty(), "one_of schema must have at least one generator");
    let idx = ntc.draw_integer(0, generators.len() as i128 - 1)?;
    interpret_schema(ntc, &generators[idx as usize])
}

fn interpret_sampled_from(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let values = match map_get(schema, "values") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("sampled_from schema must have values array"),
    };
    assert!(!values.is_empty(), "sampled_from schema must have at least one value");
    let idx = ntc.draw_integer(0, values.len() as i128 - 1)?;
    Ok(encode_schema_value(&values[idx as usize]))
}

fn interpret_list(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let element_schema = map_get(schema, "elements").expect("list schema must have elements");
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);
    let unique = map_get(schema, "unique")
        .and_then(as_bool)
        .unwrap_or(false);

    let mut state = ManyState::new(min_size, max_size);
    let mut results: Vec<Value> = Vec::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let element = interpret_schema(ntc, element_schema)?;
        if unique && results.iter().any(|existing| existing == &element) {
            many_reject(ntc, &mut state)?;
            continue;
        }
        results.push(element);
    }

    Ok(Value::Array(results))
}

fn interpret_dict(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let key_schema = map_get(schema, "keys").expect("dict schema must have keys");
    let val_schema = map_get(schema, "values").expect("dict schema must have values");
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);

    let mut state = ManyState::new(min_size, max_size);
    let mut pairs: Vec<Value> = Vec::new();
    let mut keys: Vec<Value> = Vec::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let key = interpret_schema(ntc, key_schema)?;
        if keys.iter().any(|existing| existing == &key) {
            many_reject(ntc, &mut state)?;
            continue;
        }
        let value = interpret_schema(ntc, val_schema)?;
        keys.push(key.clone());
        pairs.push(Value::Array(vec![key, value]));
    }

    Ok(Value::Array(pairs))
}

fn interpret_string(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);

    let alphabet = build_string_alphabet(schema);
    assert!(
        alphabet.len() > 0,
        "No valid codepoints for string generation"
    );

    let mut state = ManyState::new(min_size, max_size);
    let mut result = String::new();
    let n = alphabet.len() as i128;

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let idx = ntc.draw_integer(0, n - 1)?;
        result.push(alphabet.char_at(idx as usize));
    }

    Ok(Value::Tag(91, Box::new(Value::Bytes(result.into_bytes()))))
}

/// Alphabet for string generation.
enum StringAlphabet {
    /// Contiguous codepoint range [min, max] with surrogates excluded.
    Range { min: u32, max: u32 },
    /// Explicit list of valid characters, ordered by `codepoint_key`.
    Explicit(Vec<char>),
}

/// Sort key for codepoints: maps '0' (48) to 0, '1' to 1, ..., and
/// reorders low 128 codepoints so '0' is simplest.
/// Non-ASCII codepoints keep their natural order (key = codepoint).
///
/// Port of pbtkit's `_codepoint_key`.
fn codepoint_sort_key(c: u32) -> u32 {
    if c < 128 {
        (c + 80) % 128  // = (c - 48 + 128) % 128
    } else {
        c
    }
}

impl StringAlphabet {
    fn len(&self) -> usize {
        match self {
            StringAlphabet::Range { min, max } => {
                count_valid_codepoints(*min, *max) as usize
            }
            StringAlphabet::Explicit(v) => v.len(),
        }
    }

    /// Return the character at position `idx` in `codepoint_sort_key` order.
    ///
    /// Index 0 returns '0' (codepoint 48) for alphabets that contain it,
    /// matching pbtkit's shrinking behavior where '0' is the simplest char.
    fn char_at(&self, idx: usize) -> char {
        match self {
            StringAlphabet::Range { min, max } => {
                keyed_codepoint_at_index(*min, *max, idx)
            }
            StringAlphabet::Explicit(v) => v[idx],
        }
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
            let c = (ki + 48) % 128;  // key_to_codepoint
            if c >= ascii_start && c <= ascii_end {
                if found == idx {
                    return char::from_u32(c).unwrap();
                }
                found += 1;
            }
        }
        panic!("keyed_codepoint_at_index: ASCII index out of range");
    } else {
        // Non-ASCII chars: natural order, skipping surrogates.
        let non_ascii_start = min.max(128);
        codepoint_at_index(non_ascii_start, max, (idx - ascii_count) as u32)
    }
}

/// Build the effective character alphabet for a string schema.
fn build_string_alphabet(schema: &Value) -> StringAlphabet {
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
    let exclude_categories: Option<Vec<String>> = extract_string_array(schema, "exclude_categories");
    let include_chars: Option<Vec<char>> =
        map_get(schema, "include_characters")
            .and_then(as_text)
            .map(|s| s.chars().collect());
    let exclude_chars: Option<Vec<char>> =
        map_get(schema, "exclude_characters")
            .and_then(as_text)
            .map(|s| s.chars().collect());

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
            if !cats.iter().any(|cat| char_in_category(c, cat)) {
                continue;
            }
        } else if let Some(ref excl_cats) = exclude_categories {
            if excl_cats.iter().any(|cat| cat != "Cs" && char_in_category(c, cat)) {
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

    // Add include_characters (if not already present).
    if let Some(incl) = include_chars {
        for c in incl {
            let cp = c as u32;
            if cp >= cp_min && cp <= cp_max && !is_surrogate(c) && !alphabet.contains(&c) {
                alphabet.push(c);
            }
        }
    }

    // Sort by codepoint_sort_key so index 0 → '0' (simplest under shrinking).
    alphabet.sort_by_key(|c| codepoint_sort_key(*c as u32));

    StringAlphabet::Explicit(alphabet)
}

fn interpret_binary(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);

    let mut state = ManyState::new(min_size, max_size);
    let mut bytes = Vec::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let byte = ntc.draw_integer(0, 255)?;
        bytes.push(byte as u8);
    }

    Ok(Value::Bytes(bytes))
}

/// Extract an array of strings from a schema field.
fn extract_string_array(schema: &Value, key: &str) -> Option<Vec<String>> {
    map_get(schema, key).and_then(|v| {
        if let Value::Array(arr) = v {
            Some(
                arr.iter()
                    .filter_map(as_text)
                    .map(String::from)
                    .collect(),
            )
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

fn is_surrogate_cp(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

/// Check if a character belongs to a Unicode general category.
///
/// Uses Rust's built-in char methods as approximations for common categories.
fn char_in_category(c: char, category: &str) -> bool {
    match category {
        "Lu" => c.is_alphabetic() && c.is_uppercase(),
        "Ll" => c.is_alphabetic() && c.is_lowercase(),
        "Lt" => c.is_alphabetic() && c.is_uppercase(), // Title case approximation
        "L" | "LC" => c.is_alphabetic(),
        "Lm" | "Lo" => c.is_alphabetic() && !c.is_uppercase() && !c.is_lowercase(),
        "Nd" => c.is_ascii_digit(),
        "No" | "Nl" | "N" => c.is_numeric(),
        "Zs" => c == ' ',
        "Z" => c.is_whitespace(),
        "Pc" => c == '_',
        "Pd" => c == '-',
        "P" | "Po" | "Pe" | "Pf" | "Pi" | "Ps" => c.is_ascii_punctuation(),
        "Sm" => matches!(c, '+' | '<' | '=' | '>' | '|' | '~'),
        "S" | "Sc" | "Sk" | "So" => {
            !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control()
        }
        "Cc" | "C" => c.is_control(),
        "Cs" => false, // Surrogates never appear in Rust strings
        _ => false,    // Unknown category
    }
}

/// Advance the many state by one element. Returns true if another element should be drawn.
///
/// Port of pbtkit's `many.more()`.
fn many_more(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<bool, StopTest> {
    let should_continue = if state.min_size as f64 == state.max_size {
        // Fixed size: draw exactly min_size elements.
        state.count < state.min_size
    } else {
        let forced = if state.force_stop {
            Some(false)
        } else if state.count < state.min_size {
            Some(true)
        } else if state.count as f64 >= state.max_size {
            Some(false)
        } else {
            None
        };
        ntc.weighted(state.p_continue, forced)?
    };

    if should_continue {
        state.count += 1;
    }
    Ok(should_continue)
}

/// Reject the last drawn element. Port of pbtkit's `many.reject()`.
fn many_reject(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<(), StopTest> {
    assert!(state.count > 0);
    state.count -= 1;
    state.rejections += 1;
    if state.rejections > std::cmp::max(3, 2 * state.count) {
        if state.count < state.min_size {
            ntc.status = Some(Status::Invalid);
            return Err(StopTest);
        } else {
            state.force_stop = true;
        }
    }
    Ok(())
}

fn interpret_float(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let width: u64 = map_get(schema, "width").and_then(as_u64).unwrap_or(64);
    let min_value = map_get(schema, "min_value")
        .map(cbor_to_f64)
        .unwrap_or(f64::NEG_INFINITY);
    let max_value = map_get(schema, "max_value")
        .map(cbor_to_f64)
        .unwrap_or(f64::INFINITY);
    let allow_nan = map_get(schema, "allow_nan")
        .and_then(as_bool)
        .unwrap_or(true);
    let allow_infinity = map_get(schema, "allow_infinity")
        .and_then(as_bool)
        .unwrap_or(true);
    let exclude_min = map_get(schema, "exclude_min")
        .and_then(as_bool)
        .unwrap_or(false);
    let exclude_max = map_get(schema, "exclude_max")
        .and_then(as_bool)
        .unwrap_or(false);

    // Adjust bounds by one ULP for exclusive boundaries.
    // For f32 schemas (width=32), use f32-precision next_up/next_down so that
    // the adjusted bound is representable as f32 (preventing round-to-boundary bugs).
    let min_value = if exclude_min && min_value.is_finite() {
        if width == 32 {
            (min_value as f32).next_up() as f64
        } else {
            min_value.next_up()
        }
    } else {
        min_value
    };
    let max_value = if exclude_max && max_value.is_finite() {
        if width == 32 {
            (max_value as f32).next_down() as f64
        } else {
            max_value.next_down()
        }
    } else {
        max_value
    };

    let v = ntc.draw_float(min_value, max_value, allow_nan, allow_infinity)?;
    Ok(Value::Float(v))
}

/// Extract an f64 from a CBOR value (Float or Integer).
fn cbor_to_f64(value: &Value) -> f64 {
    match value {
        Value::Float(f) => *f,
        Value::Integer(i) => i128::from(*i) as f64,
        _ => panic!("Expected CBOR float/integer, got {:?}", value),
    }
}

/// Encode a schema value for transport back to the generator.
///
/// Mirrors hegel-core's `_encode_value`: text strings are wrapped in
/// CBOR tag 91 (HEGEL_STRING_TAG) so they can be deserialized by `HegelValue`.
fn encode_schema_value(value: &Value) -> Value {
    match value {
        Value::Text(s) => Value::Tag(91, Box::new(Value::Bytes(s.as_bytes().to_vec()))),
        other => other.clone(),
    }
}

/// Convert a CBOR value to i128, handling bignum tags.
///
/// For positive bignums (tag 2) that exceed i128::MAX (e.g. u128::MAX),
/// we saturate at i128::MAX so the integer range remains valid.
fn cbor_to_i128(value: &Value) -> i128 {
    match value {
        Value::Integer(i) => (*i).into(),
        Value::Tag(2, inner) => {
            // CBOR tag 2: positive bignum (big-endian bytes)
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 2, got {:?}", inner)
            };
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            // Saturating cast: values above i128::MAX (e.g. u128::MAX) cap at i128::MAX.
            i128::try_from(n).unwrap_or(i128::MAX)
        }
        Value::Tag(3, inner) => {
            // CBOR tag 3: negative bignum, value is -1 - n
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 3, got {:?}", inner)
            };
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            // Safe: -1 - n where n <= i128::MAX is always representable.
            -1i128 - i128::try_from(n).unwrap_or(i128::MAX)
        }
        _ => panic!("Expected CBOR integer, got {:?}", value),
    }
}

fn cbor_to_i64(value: &Value) -> i64 {
    let n: i128 = cbor_to_i128(value);
    n as i64
}

/// Return true if the CBOR value is a positive bignum (tag 2) whose value exceeds i128::MAX.
fn bignum_overflows_i128(value: &Value) -> bool {
    match value {
        Value::Tag(2, inner) => {
            let Value::Bytes(bytes) = inner.as_ref() else {
                return false;
            };
            // Value overflows i128 if it needs more than 16 bytes, or if the high bit
            // of a 16-byte value is set (i.e. > i128::MAX).
            if bytes.len() > 16 {
                return true;
            }
            if bytes.len() == 16 && bytes[0] >= 0x80 {
                return true;
            }
            // Also check: if any byte beyond what i128 can hold is non-zero.
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            n > i128::MAX as u128
        }
        _ => false,
    }
}

/// Encode a u128 value as CBOR. Values up to u64::MAX use normal integer encoding;
/// larger values use CBOR positive bignum tag 2 with big-endian bytes.
fn u128_to_cbor(v: u128) -> Value {
    if let Ok(n) = u64::try_from(v) {
        return Value::Integer(n.into());
    }
    // Encode as CBOR tag 2 (positive bignum), big-endian, minimal encoding.
    let bytes = v.to_be_bytes();
    // Strip leading zero bytes for minimal encoding.
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    Value::Tag(2, Box::new(Value::Bytes(bytes[first_nonzero..].to_vec())))
}

/// Convert an i128 to a CBOR value.
///
/// ciborium's Integer type supports up to i64/u64 directly. For values
/// that fit, we use the direct conversion. Values outside that range
/// use serialization via serde.
fn i128_to_cbor(v: i128) -> Value {
    if let Ok(n) = i64::try_from(v) {
        Value::Integer(n.into())
    } else if let Ok(n) = u64::try_from(v) {
        Value::Integer(n.into())
    } else {
        // For values outside i64/u64 range, serialize through serde
        crate::cbor_utils::cbor_serialize(&v)
    }
}
