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
fn pool_generate_on_empty_pool_returns_assume() {
    // No `pool_add` calls — the pool has no active variables, so
    // `pool_generate` rejects the test case as invalid.
    let (ds, _handle) = random_source();
    let pool = ds.new_pool().unwrap();
    assert!(matches!(
        ds.pool_generate(pool, false),
        Err(DataSourceError::Assume)
    ));
}

#[test]
fn new_state_machine_returns_sequential_ids() {
    let (ds, _handle) = random_source();
    assert_eq!(
        ds.new_state_machine(&["push", "pop"], &["sorted"]).unwrap(),
        0
    );
    assert_eq!(ds.new_state_machine(&["clear"], &[]).unwrap(), 1);
}

#[test]
fn new_state_machine_with_no_rules_is_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    let err = ds.new_state_machine(&[], &[]).unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("no rules"));
    // A usage error is not data exhaustion: the test case is not latched
    // as aborted, so a valid registration still succeeds.
    assert!(!ds.test_aborted());
    assert_eq!(ds.new_state_machine(&["push"], &[]).unwrap(), 0);
}

#[test]
fn state_machine_next_rule_returns_in_range_indices() {
    let (ds, _handle) = random_source();
    let id = ds.new_state_machine(&["a", "b", "c"], &[]).unwrap();
    for _ in 0..20 {
        assert!(ds.state_machine_next_rule(id).unwrap() < 3);
    }
}

#[test]
fn state_machine_next_rule_on_exhausted_source_stops_test() {
    let (ds, _handle) = exhausted_source();
    // Registration makes no draws, so it succeeds even with no data.
    let id = ds.new_state_machine(&["a", "b"], &[]).unwrap();
    assert!(matches!(
        ds.state_machine_next_rule(id),
        Err(DataSourceError::StopTest)
    ));
    // Subsequent state-machine calls short-circuit on the latched abort.
    assert!(ds.state_machine_next_rule(id).is_err());
    assert!(ds.new_state_machine(&["a"], &[]).is_err());
}

#[test]
fn primitive_boolean_forced_returns_forced_value() {
    let (ds, _handle) = random_source();
    assert!(ds.primitive_boolean(0.5, Some(true)).unwrap());
    assert!(!ds.primitive_boolean(0.5, Some(false)).unwrap());
}

#[test]
fn primitive_boolean_boundary_p_auto_forces() {
    let (ds, _handle) = random_source();
    assert!(!ds.primitive_boolean(0.0, None).unwrap());
    assert!(ds.primitive_boolean(1.0, None).unwrap());
}

#[test]
fn primitive_boolean_invalid_p_maps_to_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    for p in [f64::NAN, -0.5, 1.5] {
        let err = ds.primitive_boolean(p, None).unwrap_err();
        assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    }
    // An argument error is not data exhaustion: the test case is not latched
    // as aborted, so a (valid) subsequent draw still dispatches normally.
    assert!(!ds.test_aborted());
    assert!(ds.primitive_boolean(0.5, None).is_ok());
}

#[test]
fn primitive_boolean_forced_contradicting_boundary_is_invalid_argument() {
    let (ds, _handle) = random_source();
    assert!(matches!(
        ds.primitive_boolean(0.0, Some(true)),
        Err(DataSourceError::InvalidArgument(_))
    ));
    assert!(matches!(
        ds.primitive_boolean(1.0, Some(false)),
        Err(DataSourceError::InvalidArgument(_))
    ));
    // Forcing in the same direction as a boundary p is consistent and allowed.
    assert!(ds.primitive_boolean(1.0, Some(true)).unwrap());
    assert!(!ds.primitive_boolean(0.0, Some(false)).unwrap());
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
    assert!(ds.primitive_boolean(0.5, None).is_err());
    assert!(ds.new_pool().is_err());
    assert!(ds.pool_add(0).is_err());
    assert!(ds.pool_generate(0, false).is_err());
}

// On a live (non-aborted) test case, an opaque handle id libhegel never
// issued is a caller usage error, not a panic: it comes back as
// `InvalidArgument` (→ `HEGEL_E_INVALID_ARG`) so the C ABI stays panic-free
// and libhegel remains correct under `panic = "abort"`.
#[test]
fn unknown_handle_ids_map_to_invalid_argument_without_panicking() {
    let (ds, _handle) = random_source();

    // Collections are keyed in a map; an unissued id is simply absent.
    let more = ds.collection_more(999).unwrap_err();
    assert!(
        matches!(&more, DataSourceError::InvalidArgument(m) if m.contains("unknown collection id")),
        "{more:?}"
    );
    let reject = ds.collection_reject(999, None).unwrap_err();
    assert!(
        matches!(&reject, DataSourceError::InvalidArgument(m) if m.contains("unknown collection id")),
        "{reject:?}"
    );

    // Pools / state machines index a `Vec`. Cover both arms of the bounds
    // check: a negative id (fails the `usize` conversion) and an id past the
    // end (fails the range check).
    let pool_negative = ds.pool_add(-1).unwrap_err();
    assert!(
        matches!(&pool_negative, DataSourceError::InvalidArgument(m) if m.contains("unknown variable pool id")),
        "{pool_negative:?}"
    );
    let pool_past_end = ds.pool_generate(0, false).unwrap_err();
    assert!(
        matches!(&pool_past_end, DataSourceError::InvalidArgument(m) if m.contains("unknown variable pool id")),
        "{pool_past_end:?}"
    );
    let sm_past_end = ds.state_machine_next_rule(0).unwrap_err();
    assert!(
        matches!(&sm_past_end, DataSourceError::InvalidArgument(m) if m.contains("unknown state machine id")),
        "{sm_past_end:?}"
    );
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
    ds.target_observation(1.5, "x").unwrap();
    let obs = NativeDataSource::take_target_observations(&handle);
    assert_eq!(obs.get("x"), Some(&1.5));
}

#[test]
fn target_observation_read_does_not_mutate() {
    // Reading the observations is a non-mutating clone: the handle may still be
    // shared with a run-owned test case, so a read must not empty it. A second
    // read returns the same data.
    let (ds, handle) = random_source();
    ds.target_observation(1.0, "x").unwrap();
    let first = NativeDataSource::take_target_observations(&handle);
    assert_eq!(first.len(), 1);
    let second = NativeDataSource::take_target_observations(&handle);
    assert_eq!(second.len(), 1);
}

// A non-finite score / a repeated label are caller usage errors. libhegel
// must surface them as `InvalidArgument` (→ `HEGEL_E_INVALID_ARG`), never a
// panic — it has to stay correct under `panic = "abort"`.

#[test]
fn target_observation_rejects_nan() {
    let (ds, _handle) = random_source();
    let err = ds.target_observation(f64::NAN, "x").unwrap_err();
    assert!(
        matches!(&err, DataSourceError::InvalidArgument(m) if m.contains("requires a finite score")),
        "{err:?}"
    );
}

#[test]
fn target_observation_rejects_infinity() {
    let (ds, _handle) = random_source();
    let err = ds.target_observation(f64::INFINITY, "x").unwrap_err();
    assert!(
        matches!(&err, DataSourceError::InvalidArgument(m) if m.contains("requires a finite score")),
        "{err:?}"
    );
}

#[test]
fn target_observation_rejects_duplicate_label() {
    let (ds, _handle) = random_source();
    ds.target_observation(1.0, "x").unwrap();
    let err = ds.target_observation(2.0, "x").unwrap_err();
    assert!(
        matches!(&err, DataSourceError::InvalidArgument(m) if m.contains("would overwrite previous")),
        "{err:?}"
    );
}
