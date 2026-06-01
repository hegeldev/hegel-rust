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
    shrinker.lower_duplicated_characters();
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
    shrinker.lower_duplicated_characters();
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
    shrinker.normalize_unicode_chars();
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
    shrinker.normalize_unicode_chars();
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
    shrinker.normalize_unicode_chars();
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
    shrinker.normalize_unicode_chars();
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
    shrinker.normalize_unicode_chars();
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
