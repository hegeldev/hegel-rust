//! Unit tests for `lower_duplicated_characters` and
//! `normalize_unicode_chars`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::StringChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn string_node_with(min_cp: u32, max_cp: u32, value: Vec<u32>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(min_cp, max_cp)]),
            min_size: 0,
            max_size: 32,
        }),
        ChoiceValue::String(value),
        false,
    )
}

#[test]
fn lower_duplicated_characters_lowers_shared_codepoint_in_pair() {
    // Both strings contain 'b' (codepoint 98).  With the lowercase
    // alphabet [a-z], the shared char should be reduced to 'a'.
    let initial = vec![
        string_node_with(b'a' as u32, b'z' as u32, vec![b'b' as u32, b'c' as u32]),
        string_node_with(b'a' as u32, b'z' as u32, vec![b'b' as u32, b'd' as u32]),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.lower_duplicated_characters().unwrap();
    // The 'b' shared codepoint should be lowered to the smallest in the
    // shrink order ('a').
    if let ChoiceValue::String(s0) = &shrinker.current_nodes[0].value {
        assert!(s0.contains(&(b'a' as u32)) || !s0.contains(&(b'b' as u32)));
    }
    if let ChoiceValue::String(s1) = &shrinker.current_nodes[1].value {
        assert!(s1.contains(&(b'a' as u32)) || !s1.contains(&(b'b' as u32)));
    }
}

#[test]
fn lower_duplicated_characters_skips_non_string_neighbour() {
    use crate::native::core::choices::IntegerChoice;
    let initial = vec![
        string_node_with(b'a' as u32, b'z' as u32, vec![b'b' as u32]),
        ChoiceNode::new(
            ChoiceKind::Integer(IntegerChoice {
                min_value: BigInt::from(0),
                max_value: BigInt::from(100),
                shrink_towards: BigInt::from(0),
            }),
            ChoiceValue::Integer(BigInt::from(7)),
            false,
        ),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.lower_duplicated_characters().unwrap();
    // No second string to pair with → no change.
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(s, &vec![b'b' as u32]);
    }
}

#[test]
fn normalize_unicode_chars_replaces_accented_letter_with_base() {
    // 'À' (U+00C0) has NFD base 'A' (U+0041); the alphabet covers both.
    let initial = vec![string_node_with(b'A' as u32, 0x00FF, vec![0x00C0])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        // Either lowered to 'A' (NFD) or 'a' (case map) — both fine.
        assert!(s == &vec![b'A' as u32] || s == &vec![b'a' as u32]);
    } else {
        unreachable!();
    }
}

#[test]
fn normalize_unicode_chars_skips_when_no_simpler_chars() {
    // 'A' has no simpler natural form within the [A-Z] alphabet.
    let initial = vec![string_node_with(
        b'A' as u32,
        b'Z' as u32,
        vec![b'A' as u32],
    )];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(s, &vec![b'A' as u32]);
    } else {
        unreachable!();
    }
}

#[test]
fn normalize_unicode_chars_handles_string_truncated_by_closure() {
    // Closure truncates the realised string to length 1 — after the
    // first position is normalised, current[i].value becomes shorter
    // than the originally-captured `value`.  The outer loop's
    // `pos >= cur.len()` continue at strings.rs:~414 is exercised
    // when the loop reaches the now-out-of-bounds position.
    let initial = vec![string_node_with(b'A' as u32, 0x00FF, vec![0x00C0, 0x00C0])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Closure: accept the candidate but return an actual
                // sequence with the string truncated to length 1.
                let mut actual: Vec<ChoiceNode> = nodes.to_vec();
                if let Some(node) = actual.first_mut() {
                    if let ChoiceValue::String(s) = &mut node.value {
                        s.truncate(1);
                    }
                }
                (true, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    // No panic on the second iteration; current_nodes converged on a
    // length-1 string.
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(s.len(), 1);
    } else {
        unreachable!();
    }
}

#[test]
fn normalize_unicode_chars_does_nothing_on_non_string() {
    use crate::native::core::choices::BooleanChoice;
    let initial = vec![ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        false,
    )];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Boolean(b) => assert!(b),
        _ => unreachable!(),
    }
}

/// When the string's allowed alphabet excludes the simpler ASCII form
/// (e.g. the range [0xC0, 0xFF] contains 'À' but not 'A'), the pass
/// must not produce out-of-alphabet replacements. 'À' should stay 'À'.
#[test]
fn normalize_unicode_chars_respects_intervals() {
    let initial = vec![string_node_with(0xC0, 0xFF, vec![0xC0])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        // The 'A' base (0x41) sits outside the allowed range; 'À' (0xC0)
        // remains unchanged.
        assert!(
            s.iter().all(|&cp| (0xC0..=0xFF).contains(&cp)),
            "produced out-of-alphabet codepoints: {:?}",
            s
        );
    }
}

/// ß (U+00DF) case-folds to "ss", so 's' is a natural-simpler candidate —
/// reachable only via casefold, since to_lowercase('ß') = 'ß' and
/// to_uppercase('ß') = "SS" whose 'S' lies outside this lowercase-plus
/// alphabet. This is Hypothesis's own motivating example for including
/// casefold alongside the case mappings.
#[test]
fn normalize_unicode_chars_casefolds_sharp_s_to_s() {
    let initial = vec![string_node_with(b'a' as u32, 0xFF, vec![0x00DF])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(s, &vec![b's' as u32]);
    } else {
        unreachable!();
    }
}

/// ① (U+2460) has a *compatibility* decomposition to '1' — NFKD-only, no
/// canonical decomposition and no case mappings. The pass must reach '1'.
#[test]
fn normalize_unicode_chars_uses_nfkd_decomposition() {
    let initial = vec![string_node_with(0x20, 0x2460, vec![0x2460])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.normalize_unicode_chars().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(s, &vec![b'1' as u32]);
    } else {
        unreachable!();
    }
}

#[test]
fn lower_duplicated_characters_handles_mismatched_alphabets() {
    // The two string nodes share 'b', but their alphabets differ: lowering
    // 'b' in the first node's [a-z] alphabet proposes 'a', which the second
    // node's [b-z] alphabet does not contain. The attempt must be rejected
    // gracefully (Hypothesis's choice_permitted silently rejects it), not
    // trip a debug assertion.
    let initial = vec![
        string_node_with(b'a' as u32, b'z' as u32, vec![b'b' as u32]),
        string_node_with(b'b' as u32, b'z' as u32, vec![b'b' as u32]),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.lower_duplicated_characters().unwrap();
    // The second node must still hold a value its own alphabet permits.
    if let (ChoiceKind::String(k1), ChoiceValue::String(s1)) = (
        shrinker.current_nodes[1].kind.as_ref(),
        &shrinker.current_nodes[1].value,
    ) {
        assert!(k1.validate(s1), "node 1 left with out-of-alphabet value");
    }
}

/// The gap-preserving reorder from Hypothesis's Ordering shrinker
/// (`sort_regions_with_gaps`): the middle character is pinned by the
/// predicate and adjacent swaps would move it, so only sorting the
/// elements *around* the fixed middle reaches the minimal permutation.
#[test]
fn shrink_strings_sorts_around_fixed_middle() {
    let initial = vec![string_node_with(
        b'a' as u32,
        b'z' as u32,
        vec![b'b' as u32, b'z' as u32, b'a' as u32],
    )];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if s.len() == 3 && s[1] == 'z' as u32 && {
                            let mut rest = vec![s[0], s[2]];
                            rest.sort_unstable();
                            rest == vec![b'a' as u32, b'b' as u32]
                        }
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(
            s,
            &vec![b'a' as u32, b'z' as u32, b'b' as u32],
            "expected the gap-sort to produce \"azb\""
        );
    } else {
        unreachable!();
    }
}

/// The collection driver's read closure copes with the node being punned
/// to a different kind by the realised run mid-shrink.
#[test]
fn shrink_strings_handles_node_punned_by_closure() {
    use crate::native::core::choices::BooleanChoice;
    let punned = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(false),
        false,
    );
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                // Reject the all-simplest short circuit (empty) so the pass
                // keeps going before the pun is realised.
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s)) if !s.is_empty()
                );
                (ok, vec![punned.clone()], Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![string_node_with(
            b'a' as u32,
            b'z' as u32,
            vec![b'c' as u32, b'b' as u32],
        )],
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    assert!(matches!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Boolean(_)
    ));
}

/// The improvement cap unwinds out of the collection driver mid-pass.
#[test]
fn shrink_strings_stops_at_improvement_cap() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Reject the all-simplest short circuit (empty) so the
                // pass keeps probing past its first accepted improvement.
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s)) if !s.is_empty()
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![string_node_with(
            b'a' as u32,
            b'z' as u32,
            vec![b'c' as u32, b'b' as u32],
        )],
        Spans::new(),
    );
    shrinker.max_improvements = 1;
    assert!(shrinker.shrink_strings().is_err());
}

/// The duplicated-codepoint semantic loop: the Integer key moves cannot
/// land on the NFD base when the alphabet places it at an unreachable
/// key, so only the semantic candidate collapses the duplicates; a
/// realised run that rewrites the *other* duplicate value mid-loop makes
/// its replacement a no-op.
#[test]
fn shrink_strings_duplicate_semantic_loop_edges() {
    // Alphabet with digits first: 'A' sits at key 10, which none of the
    // Integer moves from key(À)/key(Â) produce, so the semantic NFD
    // candidates are the only route down.
    let kind = StringChoice {
        intervals: IntervalSet::new(vec![(b'0' as u32, b'9' as u32), (b'A' as u32, 0xFF)]),
        min_size: 0,
        max_size: 8,
    };
    let node = |v: Vec<u32>| {
        ChoiceNode::new(
            ChoiceKind::String(kind.clone()),
            ChoiceValue::String(v),
            false,
        )
    };
    let a_up = b'A' as u32;
    let initial = vec![node(vec![0xC0, 0xC0, 0xC2, 0xC2])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::String(s))
                        if s.len() == 4
                            && s[0] == s[1]
                            && s[2] == s[3]
                            && s.iter().all(|&c| c == 0xC0 || c == 0xC2 || c == a_up)
                );
                // Once the first pair collapses to 'A', the realised run
                // rewrites the second pair too — so the loop's later
                // replacement attempt for 'Â' finds nothing to change.
                let mut actual = nodes.to_vec();
                if ok {
                    if let Some(n) = actual.first_mut() {
                        if let ChoiceValue::String(s) = &mut n.value {
                            if s[0] == a_up {
                                for c in s.iter_mut() {
                                    *c = a_up;
                                }
                            }
                        }
                    }
                }
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_strings().unwrap();
    if let ChoiceValue::String(s) = &shrinker.current_nodes[0].value {
        assert_eq!(
            s,
            &vec![a_up; 4],
            "collapsed via the semantic candidates: {s:?}"
        );
    } else {
        unreachable!();
    }
}

/// `semantic_candidates` offers the in-alphabet NFD base for non-ASCII
/// codepoints.
#[test]
fn semantic_candidates_include_nfd_base() {
    let kind = StringChoice {
        intervals: IntervalSet::new(vec![(b'A' as u32, 0xFF)]),
        min_size: 0,
        max_size: 8,
    };
    let cands = super::semantic_candidates(0xC0, &kind);
    assert!(
        cands.contains(&(b'A' as u32)),
        "expected the NFD base 'A' in {cands:?}"
    );
}
