use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::serde_json as json_gs;
use hegel::generators as gs;
use serde_json::Number;

#[test]
fn test_serde_json_numbers_default() {
    check_can_generate_examples(json_gs::numbers());
}

#[test]
fn test_serde_json_numbers_are_finite() {
    // Every drawn Number must be representable: it has a valid i64, u64,
    // or finite f64 view.
    assert_all_examples(json_gs::numbers(), |n| {
        n.as_i64().is_some() || n.as_u64().is_some() || n.as_f64().is_some_and(|f| f.is_finite())
    });
}

#[test]
fn test_serde_json_numbers_in_vec() {
    assert_all_examples(gs::vecs(json_gs::numbers()).max_size(5), |v| {
        v.iter().all(|n| {
            n.as_i64().is_some()
                || n.as_u64().is_some()
                || n.as_f64().is_some_and(|f| f.is_finite())
        })
    });
}

#[test]
fn test_serde_json_number_default_generator() {
    check_can_generate_examples(gs::default::<Number>());
}
