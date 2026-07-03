use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, CloneRecord, Status};
use crate::native::rng::EngineRng;

fn int_kind(min: i128, max: i128) -> ChoiceKind {
    ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(min),
        max_value: BigInt::from(max),
        shrink_towards: BigInt::from(0),
    })
}

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode::new(
        int_kind(min, max),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

fn forced_bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        true,
    )
}

#[test]
fn record_tree_forced_position_counts_as_complete_for_exhaustion() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[forced_bool_node(true)], Status::Valid, &[]);
    assert!(root.is_exhausted);
}

#[test]
fn generate_novel_prefix_replays_forced_values_and_descends() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[forced_bool_node(true), bool_node(false)],
        Status::Valid,
        &[],
    );
    let mut rng = EngineRng::seeded(0);
    for _ in 0..50 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        assert_eq!(
            prefix,
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
            "prefix must pass through the forced value to the novel position"
        );
    }
}

#[test]
fn record_tree_exhaustion_check_handles_deep_paths_without_recursion() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut root = DataTreeNode::default();
            let nodes: Vec<ChoiceNode> = (0..20_000).map(|_| forced_bool_node(true)).collect();
            record_tree(&mut root, &nodes, Status::Valid, &[]);
            assert!(root.is_exhausted);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn record_tree_reports_kind_mismatch() {
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &[int_node(0, 10, 0)], Status::Valid, &[]).is_none());
    let msg = record_tree(&mut root, &[bool_node(false)], Status::Valid, &[])
        .expect("kind mismatch should be reported");
    assert!(msg.contains("non-deterministic"), "{msg}");
}

#[test]
fn record_tree_kill_depths_marks_inner_nodes_exhausted() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[int_node(0, 10, 0), int_node(0, 10, 1)],
        Status::Valid,
        &[1],
    );
    let mut rng = EngineRng::seeded(0);
    for _ in 0..50 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        assert!(
            prefix.is_empty()
                || prefix.first() != Some(&ChoiceValue::Integer(BigInt::from(0)))
                || prefix.len() == 1
        );
    }
}

#[test]
fn record_tree_kill_depths_out_of_range_is_a_no_op() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 10, 0)], Status::Valid, &[99]);
    assert!(root.kind.is_some() || !root.is_exhausted);
}

#[test]
fn generate_novel_prefix_returns_empty_for_exhausted_root() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[], Status::Valid, &[0]);
    assert!(root.is_exhausted);
    let mut rng = EngineRng::seeded(0);
    assert!(generate_novel_prefix(&root, &mut rng).is_empty());
}

#[test]
fn generate_novel_prefix_terminates_when_subtree_exhausted() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Invalid, &[]);
    record_tree(&mut root, &[bool_node(true)], Status::Invalid, &[]);

    let mut rng = EngineRng::seeded(0);
    let prefix = generate_novel_prefix(&root, &mut rng);
    assert!(prefix.is_empty());
}

#[test]
fn simulate_unseen_on_empty_tree() {
    let root = DataTreeNode::default();
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(false)]), None);
    assert_eq!(simulate(&root, &[]), None);
}

#[test]
fn simulate_returns_recorded_conclusion_for_exact_match() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Valid, &[]);
    assert_eq!(
        simulate(&root, &[ChoiceValue::Boolean(false)]),
        Some(Status::Valid)
    );
}

#[test]
fn simulate_ignores_trailing_choices_past_a_conclusion() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Invalid, &[]);
    assert_eq!(
        simulate(
            &root,
            &[
                ChoiceValue::Boolean(false),
                ChoiceValue::Boolean(true),
                ChoiceValue::Boolean(true),
            ],
        ),
        Some(Status::Invalid)
    );
}

#[test]
fn simulate_unseen_when_path_diverges() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[bool_node(false), bool_node(false)],
        Status::Valid,
        &[],
    );
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(true)]), None);
}

#[test]
fn simulate_unseen_when_choices_run_out_mid_path() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[bool_node(false), bool_node(true)],
        Status::Valid,
        &[],
    );
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(false)]), None);
}

#[test]
fn simulate_returns_interesting_conclusion() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 100, 7)], Status::Interesting, &[]);
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(7))]),
        Some(Status::Interesting)
    );
}

#[test]
fn simulate_follows_forced_value_ignoring_prefix() {
    let forced_true = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        true,
    );
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[forced_true], Status::Valid, &[]);

    assert_eq!(
        simulate(&root, &[ChoiceValue::Boolean(false)]),
        Some(Status::Valid)
    );
}

#[test]
fn simulate_puns_out_of_range_prefix_to_unit() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 10, 1)], Status::Valid, &[]);

    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(999))]),
        Some(Status::Valid)
    );
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(7))]),
        None
    );
}

#[test]
fn generate_novel_prefix_replays_forced_values_of_every_kind() {
    use crate::native::core::choices::{BytesChoice, FloatChoice, StringChoice};
    use crate::native::intervalsets::IntervalSet;
    let forced = |kind: ChoiceKind, value: ChoiceValue| ChoiceNode::new(kind, value, true);
    let nodes = vec![
        forced(int_kind(0, 100), ChoiceValue::Integer(BigInt::from(42))),
        forced(
            ChoiceKind::Float(FloatChoice {
                min_value: 0.0,
                max_value: 10.0,
                allow_nan: false,
                allow_infinity: false,
                smallest_nonzero_magnitude: 5e-324,
            }),
            ChoiceValue::Float(2.5),
        ),
        forced(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 0,
                max_size: 4,
            }),
            ChoiceValue::Bytes(vec![7, 8]),
        ),
        forced(
            ChoiceKind::String(StringChoice {
                intervals: IntervalSet::new(vec![(b'a' as u32, b'z' as u32)]),
                min_size: 0,
                max_size: 4,
            }),
            ChoiceValue::String(vec![b'h' as u32, b'i' as u32]),
        ),
        bool_node(false),
    ];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &nodes, Status::Valid, &[]);
    let mut rng = EngineRng::seeded(0);
    for _ in 0..20 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        assert_eq!(prefix[0], ChoiceValue::Integer(BigInt::from(42)));
        assert_eq!(prefix[1], ChoiceValue::Float(2.5));
        assert_eq!(prefix[2], ChoiceValue::Bytes(vec![7, 8]));
        assert_eq!(
            prefix[3],
            ChoiceValue::String(vec![b'h' as u32, b'i' as u32])
        );
    }
}

fn realized_clone_node(children: Vec<ChoiceNode>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Clone,
        ChoiceValue::Clone(Arc::new(CloneRecord::from_run(
            children,
            Vec::new(),
            Vec::new(),
        ))),
        false,
    )
}

fn clone_prefix_value(values: Vec<ChoiceValue>) -> ChoiceValue {
    ChoiceValue::Clone(Arc::new(CloneRecord::from_values(values)))
}

#[test]
fn record_and_simulate_roundtrip_with_clone_nodes() {
    let nodes = vec![
        int_node(0, 10, 3),
        realized_clone_node(vec![bool_node(true), int_node(0, 5, 2)]),
        bool_node(false),
    ];
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &nodes, Status::Interesting, &[]).is_none());

    let values: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
    let outcome = simulate_full(&root, &values).unwrap();
    assert_eq!(outcome.status, Status::Interesting);
    assert_eq!(outcome.nodes.len(), 3);
    let ChoiceValue::Clone(record) = &outcome.nodes[1].value else {
        panic!("simulated clone node lost its record");
    };
    let realized = record.realized_nodes().unwrap();
    assert_eq!(realized.len(), 2);
    assert_eq!(*realized[0].kind, ChoiceKind::Boolean(BooleanChoice));
    assert_eq!(realized[1].value, ChoiceValue::Integer(BigInt::from(2)));

    let values_only: Vec<ChoiceValue> = vec![
        ChoiceValue::Integer(BigInt::from(3)),
        clone_prefix_value(vec![
            ChoiceValue::Boolean(true),
            ChoiceValue::Integer(BigInt::from(2)),
        ]),
        ChoiceValue::Boolean(false),
    ];
    assert_eq!(simulate(&root, &values_only), Some(Status::Interesting));
}

#[test]
fn simulate_puns_invalid_values_inside_clone_streams() {
    let nodes = vec![realized_clone_node(vec![int_node(0, 10, 1)])];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &nodes, Status::Valid, &[]);

    let punned = vec![clone_prefix_value(vec![ChoiceValue::Integer(
        BigInt::from(999),
    )])];
    assert_eq!(simulate(&root, &punned), Some(Status::Valid));

    let divergent = vec![clone_prefix_value(vec![ChoiceValue::Integer(
        BigInt::from(7),
    )])];
    assert_eq!(simulate(&root, &divergent), None);
}

#[test]
fn simulate_ignores_trailing_unread_child_values() {
    let nodes = vec![realized_clone_node(vec![bool_node(false)])];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &nodes, Status::Valid, &[]);

    let longer = vec![clone_prefix_value(vec![
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(true),
    ])];
    let outcome = simulate_full(&root, &longer).unwrap();
    assert_eq!(outcome.status, Status::Valid);
    let ChoiceValue::Clone(record) = &outcome.nodes[0].value else {
        panic!("expected a clone node");
    };
    assert_eq!(record.len(), 1);
}

#[test]
fn simulate_unseen_when_child_values_run_out_mid_stream() {
    let nodes = vec![realized_clone_node(vec![bool_node(false), bool_node(true)])];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &nodes, Status::Valid, &[]);

    let shorter = vec![clone_prefix_value(vec![ChoiceValue::Boolean(false)])];
    assert_eq!(simulate(&root, &shorter), None);
}

#[test]
fn simulate_puns_non_clone_candidates_to_the_empty_clone() {
    let empty_child = vec![realized_clone_node(Vec::new()), bool_node(true)];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &empty_child, Status::Valid, &[]);
    assert_eq!(
        simulate(
            &root,
            &[
                ChoiceValue::Integer(BigInt::from(5)),
                ChoiceValue::Boolean(true),
            ]
        ),
        Some(Status::Valid)
    );

    let nonempty_child = vec![realized_clone_node(vec![bool_node(false)])];
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &nonempty_child, Status::Valid, &[]);
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(5))]),
        None
    );
}

#[test]
fn clone_streams_with_context_dependent_children_disable_prediction_quietly() {
    let run1 = vec![
        realized_clone_node(vec![bool_node(true)]),
        int_node(0, 10, 1),
    ];
    let run2 = vec![
        realized_clone_node(vec![int_node(0, 5, 2)]),
        int_node(0, 10, 1),
    ];
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &run1, Status::Valid, &[]).is_none());
    assert!(record_tree(&mut root, &run2, Status::Valid, &[]).is_none());

    let exact: Vec<ChoiceValue> = run1.iter().map(|n| n.value.clone()).collect();
    assert_eq!(simulate(&root, &exact), Some(Status::Valid));

    let punnable = vec![
        clone_prefix_value(vec![ChoiceValue::Boolean(true)]),
        ChoiceValue::Integer(BigInt::from(999)),
    ];
    assert_eq!(simulate(&root, &punnable), Some(Status::Valid));

    let punned_inside = vec![
        clone_prefix_value(vec![ChoiceValue::Integer(BigInt::from(999))]),
        ChoiceValue::Integer(BigInt::from(1)),
    ];
    assert_eq!(simulate(&root, &punned_inside), None);
}

#[test]
fn record_reports_kind_mismatch_at_clone_positions() {
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &[int_node(0, 10, 0)], Status::Valid, &[]).is_none());
    let msg = record_tree(
        &mut root,
        &[realized_clone_node(Vec::new())],
        Status::Valid,
        &[],
    )
    .expect("clone-vs-integer at one position is non-deterministic generation");
    assert!(msg.contains("non-deterministic"), "{msg}");
}

#[test]
fn novel_prefix_explores_inside_clone_subtrees() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[realized_clone_node(vec![bool_node(false)])],
        Status::Valid,
        &[],
    );
    let mut rng = EngineRng::seeded(0);
    for _ in 0..20 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        assert_eq!(
            prefix,
            vec![clone_prefix_value(vec![ChoiceValue::Boolean(true)])]
        );
    }
}

#[test]
fn novel_prefix_descends_recorded_continuations_or_recurses() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[
            realized_clone_node(vec![bool_node(false)]),
            bool_node(false),
        ],
        Status::Valid,
        &[],
    );
    let inside = vec![clone_prefix_value(vec![ChoiceValue::Boolean(true)])];
    let continuation = vec![
        clone_prefix_value(vec![ChoiceValue::Boolean(false)]),
        ChoiceValue::Boolean(true),
    ];
    let mut rng = EngineRng::seeded(0);
    let mut seen_inside = false;
    let mut seen_continuation = false;
    for _ in 0..100 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        if prefix == inside {
            seen_inside = true;
        } else if prefix == continuation {
            seen_continuation = true;
        } else {
            panic!("unexpected novel prefix: {prefix:?}");
        }
    }
    assert!(seen_inside);
    assert!(seen_continuation);
}

#[test]
fn novel_prefix_stops_before_a_fully_explored_clone_node() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[realized_clone_node(vec![bool_node(false)])],
        Status::Valid,
        &[],
    );
    record_tree(
        &mut root,
        &[realized_clone_node(vec![bool_node(true)])],
        Status::Valid,
        &[],
    );
    let mut rng = EngineRng::seeded(0);
    for _ in 0..20 {
        assert!(generate_novel_prefix(&root, &mut rng).is_empty());
    }
    assert!(!root.is_exhausted);
}

#[test]
fn a_stream_that_previously_ended_but_now_continues_disables_prediction() {
    let mut root = DataTreeNode::default();
    assert!(
        record_tree(
            &mut root,
            &[realized_clone_node(Vec::new()), bool_node(true)],
            Status::Valid,
            &[],
        )
        .is_none()
    );
    assert!(
        record_tree(
            &mut root,
            &[realized_clone_node(vec![bool_node(false)]), bool_node(true)],
            Status::Valid,
            &[],
        )
        .is_none()
    );
    let exact = vec![
        clone_prefix_value(vec![ChoiceValue::Boolean(false)]),
        ChoiceValue::Boolean(true),
    ];
    assert_eq!(simulate(&root, &exact), Some(Status::Valid));
    let punned_inside = vec![
        clone_prefix_value(vec![ChoiceValue::Integer(BigInt::from(999))]),
        ChoiceValue::Boolean(true),
    ];
    assert_eq!(simulate(&root, &punned_inside), None);
}

#[test]
fn values_only_clone_records_disable_prediction_quietly() {
    let node = ChoiceNode::new(
        ChoiceKind::Clone,
        clone_prefix_value(vec![ChoiceValue::Boolean(true)]),
        false,
    );
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &[node], Status::Valid, &[]).is_none());
    assert_eq!(
        simulate(
            &root,
            &[clone_prefix_value(vec![ChoiceValue::Boolean(true)])]
        ),
        Some(Status::Valid)
    );
    assert_eq!(
        simulate(
            &root,
            &[clone_prefix_value(vec![ChoiceValue::Integer(
                BigInt::from(999)
            )])]
        ),
        None
    );
}

#[test]
fn forced_nodes_inside_clone_streams_replay_their_recorded_value() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[realized_clone_node(vec![forced_bool_node(true)])],
        Status::Valid,
        &[],
    );
    let outcome = simulate_full(
        &root,
        &[clone_prefix_value(vec![ChoiceValue::Boolean(false)])],
    )
    .unwrap();
    assert_eq!(outcome.status, Status::Valid);
    let ChoiceValue::Clone(record) = &outcome.nodes[0].value else {
        panic!("expected a clone node");
    };
    assert_eq!(record.value_at(0), &ChoiceValue::Boolean(true));
}

#[test]
fn nested_clones_inside_clone_streams_simulate_recursively() {
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[realized_clone_node(vec![realized_clone_node(vec![
            bool_node(false),
        ])])],
        Status::Valid,
        &[],
    );
    let candidate = clone_prefix_value(vec![clone_prefix_value(vec![
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
    ])]);
    let outcome = simulate_full(&root, &[candidate]).unwrap();
    assert_eq!(outcome.status, Status::Valid);
    let ChoiceValue::Clone(record) = &outcome.nodes[0].value else {
        panic!("expected a clone node");
    };
    let ChoiceValue::Clone(inner) = record.value_at(0) else {
        panic!("expected a nested clone value");
    };
    assert_eq!(inner.len(), 1);
}

#[test]
fn span_events_inside_clone_streams_are_reconstructed() {
    use crate::native::core::{Span, SpanEvent};
    let child_record = CloneRecord::from_run(
        vec![bool_node(true)],
        vec![Span {
            start: 0,
            end: 1,
            label: "9".to_string(),
            depth: 0,
            parent: None,
            discarded: false,
        }],
        vec![
            (0, SpanEvent::Open { label: 9 }),
            (1, SpanEvent::Close { discarded: false }),
        ],
    );
    let node = ChoiceNode::new(
        ChoiceKind::Clone,
        ChoiceValue::Clone(Arc::new(child_record)),
        false,
    );
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[node], Status::Valid, &[]);
    let outcome = simulate_full(
        &root,
        &[clone_prefix_value(vec![ChoiceValue::Boolean(true)])],
    )
    .unwrap();
    let ChoiceValue::Clone(record) = &outcome.nodes[0].value else {
        panic!("expected a clone node");
    };
    assert_eq!(record.spans().len(), 1);
    assert_eq!(record.spans()[0].label, "9");
    assert_eq!(record.spans()[0].start, 0);
    assert_eq!(record.spans()[0].end, 1);
    assert_eq!(record.span_events().len(), 2);
}
