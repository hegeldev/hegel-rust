use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans, StringChoice};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::Shrinker;

fn intervals(min: u32, max: u32) -> IntervalSet {
    IntervalSet::new(vec![(min, max)])
}

fn string_node(value: Vec<u32>, min_codepoint: u32, max_codepoint: u32) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: intervals(min_codepoint, max_codepoint).into(),
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
fn redistribute_string_pair_moves_several_elements_in_one_invocation() {
    let initial = vec![
        string_node("abcdefgh".chars().map(|c| c as u32).collect(), 0, 0x10FFFF),
        string_node(vec![b'z' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let s_ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s)) if !s.is_empty()
                );
                (s_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.redistribute_string_pairs().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => assert_eq!(
            s,
            &vec![b'a' as u32],
            "one invocation should move every element the predicate allows"
        ),
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

/// A truncation accepted during the bounded linear scan shortens the current
/// value below the next candidate length, which must end the scan rather
/// than slice out of bounds.
#[test]
fn shrink_strings_linear_scan_breaks_when_replace_shortens_below_target() {
    let original: Vec<u32> = "abcdefghi".chars().map(|c| c as u32).collect();
    let prefix6 = original[..6].to_vec();
    let prefix3 = original[..3].to_vec();
    let initial = vec![string_node(original.clone(), 0, 0x10FFFF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if *s == original || *s == prefix6 || *s == prefix3
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => {
            assert_eq!(
                s.len(),
                3,
                "expected the shortest allowed prefix, got {s:?}"
            )
        }
        _ => unreachable!(),
    }
}
