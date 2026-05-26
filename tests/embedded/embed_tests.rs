use super::*;
use crate::backend::TestCaseResult;
use crate::cbor_utils::cbor_map;
use crate::runner::{Database, Settings};
use std::sync::atomic::{AtomicUsize, Ordering};

fn quiet_settings(test_cases: u64) -> Settings {
    let mut s = Settings::new().test_cases(test_cases);
    s.database = Database::Disabled;
    s
}

#[test]
fn run_native_invokes_callback_and_returns_passing_result() {
    let calls = AtomicUsize::new(0);
    let result = run_native(&quiet_settings(5), None, |ds, _is_final| {
        calls.fetch_add(1, Ordering::SeqCst);
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(result.passed);
    assert!(result.failures.is_empty());
    assert!(calls.load(Ordering::SeqCst) >= 1);
}

#[test]
fn run_native_callback_can_generate_via_data_source() {
    let result = run_native(&quiet_settings(3), None, |ds, _is_final| {
        let schema = cbor_map! {"type" => "boolean"};
        let value = ds.generate(&schema).expect("generate succeeded");
        assert!(matches!(value, ciborium::Value::Bool(_)));
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(result.passed);
}
