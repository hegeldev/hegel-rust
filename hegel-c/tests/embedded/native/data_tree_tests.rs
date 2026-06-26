use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};
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
