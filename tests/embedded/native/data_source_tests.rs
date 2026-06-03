// Embedded tests for src/native/data_source.rs — exercise each method on
// the DataSource trait implementation and the StopTest-to-abort conversion.

use super::*;
use crate::backend::{DataSource, DataSourceError};
use crate::cbor_utils::{cbor_map, map_get};
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn random_source() -> (NativeDataSource, NativeTestCaseHandle) {
    let ntc = NativeTestCase::new_random(EngineRng::seeded(7));
    NativeDataSource::new(ntc)
}

fn exhausted_source() -> (NativeDataSource, NativeTestCaseHandle) {
    let ntc = NativeTestCase::for_choices(&[], None, None);
    NativeDataSource::new(ntc)
}

#[test]
fn take_nodes_and_take_spans_return_recorded_data() {
    let (ds, handle) = random_source();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 0,
        "max_value" => 10,
    };
    ds.generate(&schema).unwrap();

    let nodes = NativeDataSource::take_nodes(&handle);
    let spans = NativeDataSource::take_spans(&handle);
    assert_eq!(nodes.len(), 1);
    // integer is a leaf schema, so a span is recorded.
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].label, "integer");
}

#[test]
fn start_and_stop_span_return_ok() {
    let (ds, handle) = random_source();
    // Two balanced start/stop pairs, covering both the discarded and
    // non-discarded branches of stop_span. start_span must be matched by
    // a corresponding stop_span — the native backend tracks open spans on
    // a stack and records the completed span on stop.
    ds.start_span(42).unwrap();
    ds.stop_span(false).unwrap();
    ds.start_span(17).unwrap();
    ds.stop_span(true).unwrap();

    // Both spans are recorded even though no choices were drawn between
    // start and stop, mirroring Hypothesis's `ConjectureData.spans` which
    // keeps empty spans (see `test_has_examples_even_when_empty`).
    let spans = NativeDataSource::take_spans(&handle);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].label, "42");
    assert_eq!(spans[0].start, spans[0].end);
    assert!(!spans[0].discarded);
    assert_eq!(spans[1].label, "17");
    assert_eq!(spans[1].start, spans[1].end);
    assert!(spans[1].discarded);
}

#[test]
fn new_collection_returns_sequential_id() {
    let (ds, _handle) = random_source();
    let id_a = ds.new_collection(0, None).unwrap();
    let id_b = ds.new_collection(1, Some(3)).unwrap();
    // IDs are allocated sequentially as i64.
    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);
}

#[test]
fn collection_more_and_reject_round_trip() {
    let (ds, _handle) = random_source();
    let id = ds.new_collection(2, Some(4)).unwrap();
    // First call: min_size not met, so forced true.
    assert!(ds.collection_more(id).unwrap());
    // Reject the element we just "drew" — exercises the no-reason path.
    ds.collection_reject(id, None).unwrap();
    // Another more → reject cycle exercising the "with reason" branch.
    assert!(ds.collection_more(id).unwrap());
    ds.collection_reject(id, Some("nope")).unwrap();
}

#[test]
fn new_pool_pool_add_and_pool_generate_non_consuming() {
    let (ds, _handle) = random_source();
    let pool = ds.new_pool().unwrap();
    let v1 = ds.pool_add(pool).unwrap();
    let v2 = ds.pool_add(pool).unwrap();
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);
    let drawn = ds.pool_generate(pool, false).unwrap();
    assert!(drawn == v1 || drawn == v2);
    // Non-consuming draw leaves both variables active.
    assert_eq!(ds.pool_generate(pool, true).ok().map(|_| ()), Some(()));
}

#[test]
fn pool_generate_on_empty_pool_returns_stop_test() {
    // No `pool_add` calls — the pool has no active variables, so
    // `pool_generate` immediately reports `StopTest`.
    let (ds, _handle) = random_source();
    let pool = ds.new_pool().unwrap();
    assert!(matches!(
        ds.pool_generate(pool, false),
        Err(DataSourceError::StopTest)
    ));
}

#[test]
fn generate_invalid_schema_maps_to_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    let schema = cbor_map! { "type" => "no-such-type" };
    let err = ds.generate(&schema).unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    // A schema error is not data exhaustion: the test case is not latched as
    // aborted, so a (valid) subsequent draw still dispatches normally.
    assert!(!ds.test_aborted());
    let good = cbor_map! { "type" => "integer", "min_value" => 0, "max_value" => 0 };
    assert!(ds.generate(&good).is_ok());
}

#[test]
fn generate_stoptest_sets_aborted_and_short_circuits() {
    let (ds, _handle) = exhausted_source();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 0,
        "max_value" => 10,
    };
    // First call exhausts the empty prefix and sets aborted.
    let err = ds.generate(&schema).unwrap_err();
    assert!(matches!(err, DataSourceError::StopTest));
    assert!(ds.test_aborted());

    // Subsequent calls short-circuit with StopTest without re-dispatching.
    let err2 = ds.generate(&schema).unwrap_err();
    assert!(matches!(err2, DataSourceError::StopTest));
    assert!(ds.start_span(0).is_err());
    assert!(ds.stop_span(false).is_err());
    assert!(ds.new_collection(0, None).is_err());
    assert!(ds.collection_more(0).is_err());
    assert!(ds.collection_reject(0, None).is_err());
    assert!(ds.new_pool().is_err());
    assert!(ds.pool_add(0).is_err());
    assert!(ds.pool_generate(0, false).is_err());
}

#[test]
fn generate_integer_round_trips() {
    let (ds, _handle) = random_source();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 5,
        "max_value" => 5,
    };
    let value = ds.generate(&schema).unwrap();
    let n = map_get(&cbor_map! { "v" => value.clone() }, "v")
        .cloned()
        .unwrap();
    assert_eq!(n, ciborium::Value::Integer(5.into()));
}

// `tc.target()` records observations into per-test-case state on the data
// source handle; the targeting phase reads them back via
// `NativeDataSource::take_target_observations` after the test body returns.

#[test]
fn target_observation_records_finite_score() {
    let (ds, handle) = random_source();
    ds.target_observation(1.5, "x");
    let obs = NativeDataSource::take_target_observations(&handle);
    assert_eq!(obs.get("x"), Some(&1.5));
}

#[test]
fn target_observation_take_drains() {
    let (ds, handle) = random_source();
    ds.target_observation(1.0, "x");
    let first = NativeDataSource::take_target_observations(&handle);
    assert_eq!(first.len(), 1);
    let second = NativeDataSource::take_target_observations(&handle);
    assert!(second.is_empty());
}

#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_rejects_nan() {
    let (ds, _handle) = random_source();
    ds.target_observation(f64::NAN, "x");
}

#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_rejects_infinity() {
    let (ds, _handle) = random_source();
    ds.target_observation(f64::INFINITY, "x");
}

#[test]
#[should_panic(expected = "would overwrite previous")]
fn target_observation_rejects_duplicate_label() {
    let (ds, _handle) = random_source();
    ds.target_observation(1.0, "x");
    ds.target_observation(2.0, "x");
}
