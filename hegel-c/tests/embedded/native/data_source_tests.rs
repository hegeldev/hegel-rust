use super::*;
use crate::backend::{DataSource, DataSourceError, TestCaseResult};
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;
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
    ds.generate_integer(&BigInt::from(0), &BigInt::from(10))
        .unwrap();

    let nodes = NativeDataSource::take_nodes(&handle);
    let spans = NativeDataSource::take_spans(&handle);
    assert_eq!(nodes.len(), 1);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].label, "26");
}

#[test]
fn start_and_stop_span_return_ok() {
    let (ds, handle) = random_source();
    ds.start_span(42).unwrap();
    ds.stop_span(false).unwrap();
    ds.start_span(17).unwrap();
    ds.stop_span(true).unwrap();

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
    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);
}

#[test]
fn collection_more_and_reject_round_trip() {
    let (ds, _handle) = random_source();
    let id = ds.new_collection(2, Some(4)).unwrap();
    assert!(ds.collection_more(id).unwrap());
    ds.collection_reject(id, None).unwrap();
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
    assert_eq!(ds.pool_generate(pool, true).ok().map(|_| ()), Some(()));
}

#[test]
fn pool_generate_on_empty_pool_returns_assume() {
    let (ds, _handle) = random_source();
    let pool = ds.new_pool().unwrap();
    assert!(matches!(
        ds.pool_generate(pool, false),
        Err(DataSourceError::Assume)
    ));
}

fn sequential_machine(
    ds: &NativeDataSource,
    rule_names: Vec<String>,
    invariant_names: Vec<String>,
) -> Result<i64, DataSourceError> {
    let rule_groups = vec![0; rule_names.len()];
    ds.new_state_machine(1, rule_names, rule_groups, invariant_names, 1)
}

#[test]
fn new_state_machine_returns_sequential_ids() {
    let (ds, _handle) = random_source();
    assert_eq!(
        sequential_machine(
            &ds,
            vec!["push".into(), "pop".into()],
            vec!["sorted".into()]
        )
        .unwrap(),
        0
    );
    assert_eq!(
        sequential_machine(&ds, vec!["clear".into()], vec![]).unwrap(),
        1
    );
}

#[test]
fn new_state_machine_with_no_rules_is_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    let err = sequential_machine(&ds, vec![], vec![]).unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("no rules"));
    assert!(!ds.test_aborted());
    assert_eq!(
        sequential_machine(&ds, vec!["push".into()], vec![]).unwrap(),
        0
    );
}

#[test]
fn new_state_machine_with_no_groups_is_invalid_argument() {
    let (ds, _handle) = random_source();
    let err = ds
        .new_state_machine(0, vec!["a".into()], vec![0], vec![], 1)
        .unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("no concurrency groups"));
    assert!(!ds.test_aborted());
}

#[test]
fn new_state_machine_with_non_parallel_rule_groups_is_invalid_argument() {
    let (ds, _handle) = random_source();
    let err = ds
        .new_state_machine(1, vec!["a".into()], vec![0, 0], vec![], 1)
        .unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("parallel"));
}

#[test]
fn new_state_machine_with_out_of_range_group_is_invalid_argument() {
    let (ds, _handle) = random_source();
    for group in [-1, 1] {
        let err = ds
            .new_state_machine(1, vec!["a".into()], vec![group], vec![], 1)
            .unwrap_err();
        assert!(matches!(err, DataSourceError::InvalidArgument(_)));
        assert!(err.to_string().contains("rule_groups[0] must be in [0, 1)"));
    }
}

#[test]
fn new_state_machine_with_empty_group_is_invalid_argument() {
    let (ds, _handle) = random_source();
    let err = ds
        .new_state_machine(2, vec!["a".into()], vec![0], vec![], 1)
        .unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("concurrency group 1 has no rules"));
}

#[test]
fn new_state_machine_with_zero_concurrency_is_invalid_argument() {
    let (ds, _handle) = random_source();
    let err = ds
        .new_state_machine(1, vec!["a".into()], vec![0], vec![], 0)
        .unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("concurrency must be at least 1"));
}

#[test]
fn state_machine_next_rule_returns_in_range_indices() {
    let (ds, _handle) = random_source();
    let id = sequential_machine(&ds, vec!["a".into(), "b".into(), "c".into()], vec![]).unwrap();
    assert!(ds.state_machine_next_group(id).unwrap().is_some());
    assert!(ds.state_machine_next_rule(id, 0).unwrap().unwrap() < 3);
    for _ in 0..20 {
        if ds.state_machine_next_group(id).unwrap().is_none() {
            break;
        }
        match ds.state_machine_next_rule(id, 0).unwrap() {
            Some(index) => assert!(index < 3),
            None => break,
        }
    }
}

#[test]
fn new_state_machine_on_exhausted_source_stops_test() {
    let (ds, _handle) = exhausted_source();
    assert!(matches!(
        sequential_machine(&ds, vec!["a".into(), "b".into()], vec![]),
        Err(DataSourceError::StopTest)
    ));
    assert!(matches!(
        ds.state_machine_next_group(0),
        Err(DataSourceError::StopTest)
    ));
    assert!(matches!(
        ds.state_machine_next_rule(0, 0),
        Err(DataSourceError::StopTest)
    ));
}

#[test]
fn generate_concurrency_is_in_range_and_validates_max() {
    let (ds, _handle) = random_source();
    assert_eq!(ds.generate_concurrency(1).unwrap(), 1);
    for _ in 0..20 {
        let level = ds.generate_concurrency(4).unwrap();
        assert!((1..=4).contains(&level));
    }
    let err = ds.generate_concurrency(0).unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(err.to_string().contains("max_value >= 1"));
    assert!(!ds.test_aborted());
}

#[test]
fn generate_concurrency_on_exhausted_source_stops_test() {
    let (ds, _handle) = exhausted_source();
    assert!(matches!(
        ds.generate_concurrency(4),
        Err(DataSourceError::StopTest)
    ));
}

#[test]
fn generate_boolean_forced_returns_forced_value() {
    let (ds, _handle) = random_source();
    assert!(ds.generate_boolean(0.5, Some(true)).unwrap());
    assert!(!ds.generate_boolean(0.5, Some(false)).unwrap());
}

#[test]
fn generate_boolean_boundary_p_auto_forces() {
    let (ds, _handle) = random_source();
    assert!(!ds.generate_boolean(0.0, None).unwrap());
    assert!(ds.generate_boolean(1.0, None).unwrap());
}

#[test]
fn generate_boolean_invalid_p_maps_to_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    for p in [f64::NAN, -0.5, 1.5] {
        let err = ds.generate_boolean(p, None).unwrap_err();
        assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    }
    assert!(!ds.test_aborted());
    assert!(ds.generate_boolean(0.5, None).is_ok());
}

#[test]
fn generate_boolean_forced_contradicting_boundary_is_invalid_argument() {
    let (ds, _handle) = random_source();
    assert!(matches!(
        ds.generate_boolean(0.0, Some(true)),
        Err(DataSourceError::InvalidArgument(_))
    ));
    assert!(matches!(
        ds.generate_boolean(1.0, Some(false)),
        Err(DataSourceError::InvalidArgument(_))
    ));
    assert!(ds.generate_boolean(1.0, Some(true)).unwrap());
    assert!(!ds.generate_boolean(0.0, Some(false)).unwrap());
}

#[test]
fn generate_inverted_bounds_map_to_invalid_argument_without_aborting() {
    let (ds, _handle) = random_source();
    let err = ds
        .generate_integer(&BigInt::from(1), &BigInt::from(0))
        .unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    assert!(!ds.test_aborted());
    assert!(
        ds.generate_integer(&BigInt::from(0), &BigInt::from(0))
            .is_ok()
    );
}

#[test]
fn generate_stoptest_sets_aborted_and_short_circuits() {
    let (ds, _handle) = exhausted_source();
    let err = ds
        .generate_integer(&BigInt::from(0), &BigInt::from(10))
        .unwrap_err();
    assert!(matches!(err, DataSourceError::StopTest));
    assert!(ds.test_aborted());

    let err2 = ds
        .generate_integer(&BigInt::from(0), &BigInt::from(10))
        .unwrap_err();
    assert!(matches!(err2, DataSourceError::StopTest));
    assert!(ds.start_span(0).is_err());
    assert!(ds.stop_span(false).is_err());
    assert!(ds.new_collection(0, None).is_err());
    assert!(ds.collection_more(0).is_err());
    assert!(ds.collection_reject(0, None).is_err());
    assert!(ds.generate_boolean(0.5, None).is_err());
    assert!(ds.new_pool().is_err());
    assert!(ds.pool_add(0).is_err());
    assert!(ds.pool_generate(0, false).is_err());
}

#[test]
fn unknown_handle_ids_map_to_invalid_argument_without_panicking() {
    let (ds, _handle) = random_source();

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
    let sm_past_end = ds.state_machine_next_rule(0, 0).unwrap_err();
    assert!(
        matches!(&sm_past_end, DataSourceError::InvalidArgument(m) if m.contains("unknown state machine id")),
        "{sm_past_end:?}"
    );
    let sm_group_past_end = ds.state_machine_next_group(0).unwrap_err();
    assert!(
        matches!(&sm_group_past_end, DataSourceError::InvalidArgument(m) if m.contains("unknown state machine id")),
        "{sm_group_past_end:?}"
    );
}

#[test]
fn generate_integer_round_trips() {
    let (ds, _handle) = random_source();
    let value = ds
        .generate_integer(&BigInt::from(5), &BigInt::from(5))
        .unwrap();
    assert_eq!(value, BigInt::from(5));
}

#[test]
fn target_observation_records_finite_score() {
    let (ds, handle) = random_source();
    ds.target_observation(1.5, "x").unwrap();
    let obs = NativeDataSource::take_target_observations(&handle);
    assert_eq!(obs.get("x"), Some(&1.5));
}

#[test]
fn target_observation_read_does_not_mutate() {
    let (ds, handle) = random_source();
    ds.target_observation(1.0, "x").unwrap();
    let first = NativeDataSource::take_target_observations(&handle);
    assert_eq!(first.len(), 1);
    let second = NativeDataSource::take_target_observations(&handle);
    assert_eq!(second.len(), 1);
}

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

#[test]
fn clone_stream_draws_independently_and_reassembles() {
    let (ds, handle) = random_source();
    let bounds = (BigInt::from(0), BigInt::from(1000));
    ds.generate_integer(&bounds.0, &bounds.1).unwrap();
    let child = ds.clone_stream().unwrap();
    child.generate_integer(&bounds.0, &bounds.1).unwrap();
    ds.generate_integer(&bounds.0, &bounds.1).unwrap();
    ds.mark_complete(&TestCaseResult::Valid);

    let nodes = NativeDataSource::take_nodes(&handle);
    assert_eq!(nodes.len(), 3);
    let ChoiceValue::Clone(record) = &nodes[1].value else {
        panic!("expected the clone node to carry its stream");
    };
    assert_eq!(record.realized_nodes().unwrap().len(), 1);
}

#[test]
fn pools_are_shared_across_cloned_streams() {
    let (ds, _handle) = random_source();
    let pool = ds.new_pool().unwrap();
    let v1 = ds.pool_add(pool).unwrap();
    let child = ds.clone_stream().unwrap();
    let got = child.pool_generate(pool, true).unwrap();
    assert_eq!(got, v1);
    assert!(matches!(
        ds.pool_generate(pool, false),
        Err(DataSourceError::Assume)
    ));
}

#[test]
fn collections_are_shared_across_cloned_streams() {
    let (ds, _handle) = random_source();
    let collection = ds.new_collection(1, Some(1)).unwrap();
    let child = ds.clone_stream().unwrap();
    assert!(child.collection_more(collection).unwrap());
    assert!(!child.collection_more(collection).unwrap());
}

#[test]
fn state_machines_are_shared_across_cloned_streams() {
    let (ds, _handle) = random_source();
    let machine = ds
        .new_state_machine(
            1,
            vec!["a".into(), "b".into(), "c".into()],
            vec![0, 0, 0],
            vec![],
            2,
        )
        .unwrap();
    assert_eq!(ds.state_machine_next_group(machine).unwrap(), Some(0));
    let child = ds.clone_stream().unwrap();
    assert!(child.state_machine_next_rule(machine, 1).unwrap().unwrap() < 3);
    if let Some(index) = ds.state_machine_next_rule(machine, 0).unwrap() {
        assert!(index < 3);
    }
}

#[test]
fn target_labels_are_unique_across_cloned_streams() {
    let (ds, handle) = random_source();
    let child = ds.clone_stream().unwrap();
    ds.target_observation(1.0, "score").unwrap();
    let err = child.target_observation(2.0, "score").unwrap_err();
    assert!(matches!(err, DataSourceError::InvalidArgument(_)));
    child.target_observation(2.0, "other").unwrap();

    let observations = NativeDataSource::take_target_observations(&handle);
    assert_eq!(observations.len(), 2);
    assert_eq!(observations["score"], 1.0);
    assert_eq!(observations["other"], 2.0);
}

#[test]
fn mark_complete_from_a_clone_concludes_the_family() {
    let (ds, handle) = random_source();
    let child = ds.clone_stream().unwrap();
    child.mark_complete(&TestCaseResult::Invalid);
    assert!(matches!(
        ds.generate_boolean(0.5, None),
        Err(DataSourceError::Assume)
    ));
    assert!(matches!(
        NativeDataSource::take_outcome(&handle),
        TestCaseResult::Invalid
    ));
}

#[test]
fn clone_stream_on_an_exhausted_source_stops_the_test() {
    let (ds, _handle) = exhausted_source();
    assert!(matches!(ds.clone_stream(), Err(DataSourceError::StopTest)));
}
