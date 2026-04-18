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
#[should_panic(expected = "non-deterministic")]
fn tree_detects_kind_mismatch_at_different_values() {
    // The schema at a given choice position must be consistent across
    // runs, regardless of which value was drawn. If the same prefix
    // produces draws with different kinds (here: different min_value
    // constraints), that's non-deterministic data generation — even
    // though the drawn values differ.
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
    ctf.record(&nodes_b);
}

#[test]
fn tree_allows_different_kinds_at_different_prefixes() {
    // Different prefixes mean different tree positions, so kinds are
    // tracked independently. Here both paths start with the same bool,
    // but since the second draw only happens after descending into one
    // branch of the root, its kind can differ between branches.
    let mut ctf = CachedTestFunction::new(dummy_test);
    let path_true = vec![
        ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(true),
            was_forced: false,
        },
        ChoiceNode {
            kind: ChoiceKind::Integer(IntegerChoice {
                min_value: 0,
                max_value: 100,
            }),
            value: ChoiceValue::Integer(42),
            was_forced: false,
        },
    ];
    let path_false = vec![
        ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(false),
            was_forced: false,
        },
        ChoiceNode {
            kind: ChoiceKind::Bytes(BytesChoice {
                min_size: 0,
                max_size: 10,
            }),
            value: ChoiceValue::Bytes(vec![]),
            was_forced: false,
        },
    ];
    ctf.record(&path_true);
    ctf.record(&path_false);
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

// ── cache key float-bit sensitivity ─────────────────────────────────────────
//
// Ports of pbtkit/tests/test_core.py::test_cache_key_distinguishes_negative_zero,
// test_cache_key_distinguishes_nan_variants, and
// test_cache_distinguishes_negative_zero_in_lookup.
//
// In pbtkit these directly test the private `_cache_key`. Here we exercise
// the same behavior via the public-to-tests `cache_lookup`/`cache_store`
// surface: distinct float bit patterns must produce distinct cache entries.

#[test]
fn cache_key_distinguishes_negative_zero() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let pos_zero = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(0.0),
        was_forced: false,
    }];
    let neg_zero = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(-0.0),
        was_forced: false,
    }];
    ctf.cache_store(&pos_zero, (true, 1));
    ctf.cache_store(&neg_zero, (false, 1));
    // If the cache key conflated 0.0 and -0.0, the second store would
    // overwrite the first.
    assert_eq!(ctf.cache_lookup(&pos_zero), Some((true, 1)));
    assert_eq!(ctf.cache_lookup(&neg_zero), Some((false, 1)));
}

#[test]
fn cache_key_distinguishes_nan_variants() {
    let mut ctf = CachedTestFunction::new(dummy_test);
    let nan1 = f64::NAN;
    // Construct a NaN with a different bit pattern.
    let nan2 = f64::from_bits(nan1.to_bits() ^ 1);
    assert!(nan1.is_nan() && nan2.is_nan());
    assert_ne!(nan1.to_bits(), nan2.to_bits());

    let nodes_a = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(nan1),
        was_forced: false,
    }];
    let nodes_b = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(nan2),
        was_forced: false,
    }];
    ctf.cache_store(&nodes_a, (true, 1));
    ctf.cache_store(&nodes_b, (false, 1));
    assert_eq!(ctf.cache_lookup(&nodes_a), Some((true, 1)));
    assert_eq!(ctf.cache_lookup(&nodes_b), Some((false, 1)));
}

#[test]
fn cache_distinguishes_negative_zero_in_lookup() {
    // Same intent as the previous: looking up 0.0 must not return the
    // entry stored for -0.0.
    let mut ctf = CachedTestFunction::new(dummy_test);
    let pos_zero = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(0.0),
        was_forced: false,
    }];
    let neg_zero = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Float(-0.0),
        was_forced: false,
    }];
    ctf.cache_store(&pos_zero, (true, 1));
    // Lookup with -0.0 should miss because we only stored +0.0.
    assert!(ctf.cache_lookup(&neg_zero).is_none());
}
