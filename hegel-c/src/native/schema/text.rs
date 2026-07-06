use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::cbor_utils::{as_text, as_u64, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use crate::native::draws::text::TextAlphabet;
use crate::native::intervalsets::IntervalSet;
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
/// the schema's canonical CBOR encoding. The actual interval computation
/// lives in [`crate::native::draws::text::build_intervals`]; this wrapper
/// only translates the CBOR fields and caches per schema.
pub(super) fn build_intervals(schema: &Value) -> Result<IntervalSet, EngineError> {
    type Cache = Mutex<HashMap<Vec<u8>, Arc<IntervalSet>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut key = Vec::new();
    ciborium::into_writer(schema, &mut key).expect("CBOR encoding of schema cannot fail");
    if let Some(cached) = cache.lock().unwrap().get(&key) {
        return Ok((**cached).clone());
    }
    let computed = Arc::new(crate::native::draws::text::build_intervals(
        &alphabet_from_schema(schema),
    )?);
    cache.lock().unwrap().insert(key, Arc::clone(&computed));
    Ok((*computed).clone())
}

fn alphabet_from_schema(schema: &Value) -> TextAlphabet {
    TextAlphabet {
        codec: map_get(schema, "codec").and_then(as_text).map(String::from),
        min_codepoint: map_get(schema, "min_codepoint")
            .and_then(as_u64)
            .unwrap_or(0) as u32,
        max_codepoint: map_get(schema, "max_codepoint")
            .and_then(as_u64)
            .map(|n| n as u32),
        categories: extract_string_array(schema, "categories"),
        exclude_categories: extract_string_array(schema, "exclude_categories"),
        include_characters: map_get(schema, "include_characters")
            .and_then(as_text)
            .map(String::from),
        exclude_characters: map_get(schema, "exclude_characters")
            .and_then(as_text)
            .map(String::from),
    }
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
