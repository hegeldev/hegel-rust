use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans, StringChoice};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::Shrinker;

fn intervals(min: u32, max: u32) -> IntervalSet {
    IntervalSet::new(vec![(min, max)])
}

fn string_node(value: Vec<u32>, min_codepoint: u32, max_codepoint: u32) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: intervals(min_codepoint, max_codepoint),
            min_size: 0,
            max_size: 32,
        }),
        ChoiceValue::String(value),
        false,
    )
}

fn accepting_shrinker(nodes: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

#[test]
fn redistribute_string_pair_moves_entire_value_when_accepted() {
    let initial = vec![
        string_node(vec![b'a' as u32, b'b' as u32, b'c' as u32], 0, 0x10FFFF),
        string_node(vec![b'd' as u32, b'e' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.redistribute_string_pairs().unwrap();
    let (a, b) = match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::String(a), ChoiceValue::String(b)) => (a.clone(), b.clone()),
        _ => unreachable!(),
    };
    assert!(a.is_empty(), "first node not emptied: {a:?}");
    assert_eq!(
        b,
        vec![
            b'a' as u32,
            b'b' as u32,
            b'c' as u32,
            b'd' as u32,
            b'e' as u32
        ]
    );
}

#[test]
fn redistribute_string_pair_partial_move_triggers_bin_search() {
    let initial = vec![
        string_node(vec![b'a' as u32, b'b' as u32, b'c' as u32], 0, 0x10FFFF),
        string_node(vec![b'd' as u32, b'e' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let t_ok = matches!(
                    nodes.get(1).map(|n| &n.value),
                    Some(ChoiceValue::String(s)) if s.len() <= 3
                );
                (t_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.redistribute_string_pairs().unwrap();
    match &shrinker.current_nodes[1].value {
        ChoiceValue::String(s) => assert!(s.len() <= 3, "t exceeded 3 cps: {s:?}"),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_strings_collapses_accepting_run_toward_simplest() {
    let initial = vec![string_node(
        vec![b'a' as u32, b'b' as u32],
        b'a' as u32,
        b'z' as u32,
    )];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.shrink_strings().unwrap();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::String(v) => v.clone(),
        _ => unreachable!(),
    };
    assert!(v.is_empty(), "expected empty shrink, got {v:?}");
}

#[test]
fn shrink_strings_duplicate_pass_bin_search_skips_after_val_eliminated() {
    let initial = vec![string_node(vec![200, 200], 0, 0x10FFFF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let accept = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if s.len() == 2
                            && s[0] == s[1]
                            && (s[0] == 100 || s[0] == 200)
                );
                (accept, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => assert_eq!(s, &vec![100u32, 100u32]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_strings_semantic_candidate_falls_back_to_nfd_base_in_range() {
    let initial = vec![string_node(vec![0x00C0], b'A' as u32, 0x00FF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let accept = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if s.len() == 1
                            && (b'A' as u32..=b'Z' as u32).contains(&s[0])
                );
                (accept, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => {
            assert_eq!(s.len(), 1);
            assert!((b'A' as u32..=b'Z' as u32).contains(&s[0]));
        }
        _ => unreachable!(),
    }
}
