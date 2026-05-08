use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::serde_json as json_gs;
use hegel::generators as gs;
use serde_json::Value;
use serde_json::value::RawValue;

#[test]
fn test_serde_json_raw_values_default() {
    check_can_generate_examples(json_gs::raw_values());
}

#[test]
fn test_serde_json_raw_values_are_valid_json() {
    // The text inside a generated RawValue must always parse as valid JSON.
    assert_all_examples(json_gs::raw_values(), |r| {
        serde_json::from_str::<Value>(r.get()).is_ok()
    });
}

#[test]
fn test_serde_json_raw_values_in_vec() {
    assert_all_examples(gs::vecs(json_gs::raw_values()).max_size(3), |v| {
        v.iter()
            .all(|r| serde_json::from_str::<Value>(r.get()).is_ok())
    });
}

#[test]
fn test_serde_json_raw_value_default_generator() {
    check_can_generate_examples(gs::default::<Box<RawValue>>());
}
