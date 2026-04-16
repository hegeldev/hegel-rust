use super::*;
use crate::native::core::*;
use crate::test_case::TestCase;

fn dummy_test(_tc: TestCase) {}

#[test]
fn tree_records_and_accepts_consistent_kinds() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nodes = vec![ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 100,
        }),
        value: ChoiceValue::Integer(42),
        was_forced: false,
    }];
    ctf.record(&nodes);
    // Same nodes again — should not panic.
    ctf.record(&nodes);
}

#[test]
#[should_panic(expected = "non-deterministic")]
fn tree_detects_kind_mismatch() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nodes_a = vec![ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 100,
        }),
        value: ChoiceValue::Integer(42),
        was_forced: false,
    }];
    ctf.record(&nodes_a);

    // Same value, different kind (min_value changed).
    let nodes_b = vec![ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 5,
            max_value: 100,
        }),
        value: ChoiceValue::Integer(42),
        was_forced: false,
    }];
    ctf.record(&nodes_b);
}

#[test]
fn tree_allows_different_kinds_at_different_values() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nodes_a = vec![ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 100,
        }),
        value: ChoiceValue::Integer(42),
        was_forced: false,
    }];
    let nodes_b = vec![ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 5,
            max_value: 100,
        }),
        value: ChoiceValue::Integer(77),
        was_forced: false,
    }];
    ctf.record(&nodes_a);
    // Different value → different tree branch → no conflict.
    ctf.record(&nodes_b);
}

#[test]
fn cache_miss_returns_none() {
    let ctf = CachedTestFunction::new(dummy_test);
    let nodes = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    }];
    assert!(ctf.cache_lookup(&nodes).is_none());
}

#[test]
fn cache_hit_returns_stored_result() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nodes = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    }];
    ctf.cache_store(&nodes, (true, 1));
    assert_eq!(ctf.cache_lookup(&nodes), Some((true, 1)));
}

#[test]
fn cache_distinguishes_different_sequences() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nodes_a = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    }];
    let nodes_b = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(false),
        was_forced: false,
    }];
    ctf.cache_store(&nodes_a, (true, 1));
    ctf.cache_store(&nodes_b, (false, 1));
    assert_eq!(ctf.cache_lookup(&nodes_a), Some((true, 1)));
    assert_eq!(ctf.cache_lookup(&nodes_b), Some((false, 1)));
}
