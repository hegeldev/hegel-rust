use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::serde_json as json_gs;
use hegel::generators as gs;
use serde_json::{Map, Value};

#[test]
fn test_serde_json_values_default() {
    check_can_generate_examples(json_gs::values());
}

#[test]
fn test_serde_json_values_serialize_to_valid_json() {
    // Every drawn Value must serialize to text that re-parses as JSON.
    // Note: serde_json's float parser does not always preserve f64 precision
    // exactly for very large numbers, so we don't assert structural equality.
    assert_all_examples(json_gs::values(), |v| {
        let s = serde_json::to_string(v).unwrap();
        serde_json::from_str::<Value>(&s).is_ok()
    });
}

#[test]
fn test_serde_json_values_in_vec() {
    assert_all_examples(gs::vecs(json_gs::values()).max_size(3), |v| {
        v.iter()
            .all(|val| serde_json::to_string(val).is_ok_and(|s| !s.is_empty()))
    });
}

#[test]
fn test_serde_json_value_default_generator() {
    check_can_generate_examples(gs::default::<Value>());
}

#[test]
fn test_serde_json_map_default_generator() {
    check_can_generate_examples(gs::default::<Map<String, Value>>());
}

#[test]
fn test_serde_json_map_default_serializes_to_valid_json() {
    assert_all_examples(gs::default::<Map<String, Value>>(), |m| {
        let v = Value::Object(m.clone());
        let s = serde_json::to_string(&v).unwrap();
        serde_json::from_str::<Value>(&s).is_ok()
    });
}
