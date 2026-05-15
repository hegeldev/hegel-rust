use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, StringChoice};
use crate::native::shrinker::Shrinker;

fn string_node(value: Vec<u32>, min_codepoint: u32, max_codepoint: u32) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            min_codepoint,
            max_codepoint,
            min_size: 0,
            max_size: 32,
        }),
        value: ChoiceValue::String(value),
        was_forced: false,
    }
}

fn accepting_shrinker(nodes: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        nodes,
    )
}

#[test]
fn redistribute_string_pair_moves_entire_value_when_accepted() {
    // Accepting predicate lets the first step (move everything from `s`
    // into `t`) succeed and triggers the early `return` after the
    // `combined` candidate is accepted.
    let initial = vec![
        string_node(vec![b'a' as u32, b'b' as u32, b'c' as u32], 0, 0x10FFFF),
        string_node(vec![b'd' as u32, b'e' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.redistribute_string_pairs();
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
    // Reach the bin_search arm: the full-move candidate is rejected (because
    // `nodes[1]` would exceed 3 codepoints), the single-codepoint move is
    // accepted (so the second early-return doesn't fire), and bin_search
    // then probes the remaining suffixes.
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
                (t_ok, nodes.to_vec())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.redistribute_string_pairs();
    match &shrinker.current_nodes[1].value {
        ChoiceValue::String(s) => assert!(s.len() <= 3, "t exceeded 3 cps: {s:?}"),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_strings_collapses_accepting_run_toward_simplest() {
    // Accepting test_fn drives the shrinker to settle on the simplest
    // (length-1 '0') choice.
    let initial = vec![string_node(
        vec![b'a' as u32, b'b' as u32],
        b'a' as u32,
        b'z' as u32,
    )];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.shrink_strings();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::String(v) => v.clone(),
        _ => unreachable!(),
    };
    // 'a' is the simplest codepoint in the alphabet (codepoint_key('a') is
    // the smallest in [a,z]); the shrinker reduces toward `[]` then climbs
    // back up to length 0 because `min_size = 0`.
    assert!(v.is_empty(), "expected empty shrink, got {v:?}");
}

#[test]
fn shrink_strings_semantic_candidate_falls_back_to_nfd_base_in_range() {
    // 'À' (U+00C0) has NFD base 'A' (U+0041). With `categories=[Lu]`-style
    // logic stubbed out and a wide range, the surrogate-skip filter in
    // `semantic_candidates` is exercised when the engine considers `'A'` as
    // a smaller-key replacement for the non-ASCII 'À'.
    let initial = vec![string_node(vec![0x00C0], b'A' as u32, 0x00FF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                // Accept any single uppercase Latin letter (codepoint range A..=Z).
                let accept = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if s.len() == 1
                            && (b'A' as u32..=b'Z' as u32).contains(&s[0])
                );
                (accept, nodes.to_vec())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.shrink_strings();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => {
            assert_eq!(s.len(), 1);
            // The shrinker should have lowered the codepoint to 'A'
            // (NFD base of 'À') or another in-range candidate.
            assert!((b'A' as u32..=b'Z' as u32).contains(&s[0]));
        }
        _ => unreachable!(),
    }
}
