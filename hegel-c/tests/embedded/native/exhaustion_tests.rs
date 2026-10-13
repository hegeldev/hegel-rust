use super::*;
use crate::native::core::choices::{BooleanChoice, BytesChoice, IntegerChoice};

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn forced_bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 1.0 }, value, true)
}

fn bytes_node() -> ChoiceNode {
    ChoiceNode::bytes(
        BytesChoice {
            min_size: 0,
            max_size: 1,
        },
        alloc::vec![],
        false,
    )
}

#[test]
fn a_space_is_exhausted_once_every_input_has_concluded() {
    let mut space = SearchSpace::default();
    space.record(Status::Invalid, &[bool_node(false), bool_node(false)]);
    space.record(Status::Invalid, &[bool_node(false), bool_node(true)]);
    space.record(Status::Valid, &[bool_node(true), int_node(-1, 1, 0)]);
    space.record(Status::Valid, &[bool_node(true), int_node(-1, 1, -1)]);
    assert!(!space.is_exhausted());
    space.record(Status::Interesting, &[bool_node(true), int_node(-1, 1, 1)]);
    assert!(space.is_exhausted());
}

#[test]
fn a_repeated_input_does_not_count_twice() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[bool_node(false)]);
    space.record(Status::Valid, &[bool_node(false)]);
    assert!(!space.is_exhausted());
    space.record(Status::Valid, &[bool_node(true)]);
    assert!(space.is_exhausted());
}

#[test]
fn a_test_that_draws_nothing_exhausts_at_once() {
    let mut space = SearchSpace::default();
    space.record(Status::Invalid, &[]);
    assert!(space.is_exhausted());
}

#[test]
fn a_forced_value_is_the_only_one_at_its_position() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[forced_bool_node(true), bool_node(false)]);
    space.record(Status::Valid, &[forced_bool_node(true), bool_node(true)]);
    assert!(space.is_exhausted());
}

#[test]
fn a_wide_draw_abandons_tracking() {
    for wide in [
        bytes_node(),
        int_node(0, i128::from(MAX_ARITY), 0),
        int_node(-(1 << 70), 1 << 70, 0),
    ] {
        let mut space = SearchSpace::default();
        space.record(Status::Valid, &[bool_node(false)]);
        space.record(Status::Valid, &[bool_node(true), wide]);
        assert!(!space.tracking);
        space.record(Status::Valid, &[bool_node(true)]);
        assert!(!space.is_exhausted());
    }
}

#[test]
fn the_widest_tracked_range_is_exhaustible() {
    let max = i128::from(MAX_ARITY) - 1;
    let mut space = SearchSpace::default();
    for value in 0..=max {
        space.record(Status::Valid, &[int_node(0, max, value)]);
    }
    assert!(space.is_exhausted());
}

#[test]
fn an_overrun_abandons_tracking() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[bool_node(false)]);
    space.record(Status::EarlyStop, &[bool_node(true)]);
    assert!(!space.tracking);
    space.record(Status::Valid, &[bool_node(true)]);
    assert!(!space.is_exhausted());
}

#[test]
fn a_run_that_draws_past_a_conclusion_abandons_tracking() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[bool_node(false)]);
    space.record(Status::Valid, &[bool_node(false), bool_node(true)]);
    assert!(!space.tracking);
}

#[test]
fn a_run_that_concludes_where_another_drew_abandons_tracking() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[bool_node(false), bool_node(true)]);
    space.record(Status::Valid, &[bool_node(false)]);
    assert!(!space.tracking);
}

#[test]
fn a_changed_domain_at_a_position_abandons_tracking() {
    let mut space = SearchSpace::default();
    space.record(Status::Valid, &[int_node(0, 1, 0)]);
    space.record(Status::Valid, &[int_node(0, 2, 1)]);
    assert!(!space.tracking);
}

#[test]
fn a_trie_past_the_node_cap_abandons_tracking() {
    let mut space = SearchSpace::default();
    let chain: Vec<ChoiceNode> = (0..MAX_NODES - 1).map(|_| bool_node(false)).collect();
    space.record(Status::Valid, &chain);
    assert!(space.tracking);
    assert_eq!(space.nodes.len(), MAX_NODES);
    space.record(Status::Valid, &[bool_node(true)]);
    assert!(!space.tracking);
    assert!(!space.is_exhausted());
}
