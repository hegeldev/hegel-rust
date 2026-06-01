//! Unit tests for `Shrinker::minimize_individual_choices`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn forced_int_node(value: i128) -> ChoiceNode {
    let mut n = int_node(value);
    n.was_forced = true;
    n
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_drives_int_to_simplest_when_predicate_admits() {
    // Accepting predicate: the bin_search loop drives the integer all
    // the way to zero.
    let initial = vec![int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

#[test]
fn minimize_individual_choices_skips_forced_nodes() {
    let initial = vec![forced_int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 7);
}

#[test]
fn minimize_individual_choices_invokes_span_delete_fallback() {
    // Set up a "size-controlling" integer: the first node decides how
    // many of the following nodes will be drawn.  When the integer is
    // lowered by 1, the realised actual_nodes is shorter — the fallback
    // tries deleting one of the trailing spans / nodes.  Predicate
    // accepts iff the integer is >= 1 *and* there's a trailing pair of
    // ones.
    //
    // Initial value: integer = 3, followed by three 1s.  Lowering to 2
    // produces a shorter actual_nodes (since the test "would" draw only
    // 2 elements).  The fallback should delete one of the trailing 1s
    // to make the candidate match.
    let initial = vec![int_node(3), int_node(1), int_node(1), int_node(1)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Read the integer at index 0.
                let count = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1 + count;
                let actual_len = needed_len.min(nodes.len());
                let actual_nodes = nodes[..actual_len].to_vec();
                // Build a single span around the trailing region for the
                // fallback to splice out.
                let mut spans = Spans::new();
                if actual_len > 1 {
                    spans.push(Span {
                        start: 1,
                        end: actual_len,
                        label: "list".to_string(),
                        depth: 0,
                        parent: None,
                        discarded: false,
                    });
                }
                let ok = actual_len >= 2 && count >= 1;
                (ok, actual_nodes, spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    // After convergence the integer is at its minimum admissible value
    // (1) and the trailing region is the matching size (1 item).
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn minimize_individual_choices_truncates_misaligned_string() {
    // Lowering the integer at index 0 forces the trailing string to be
    // truncated by the closure (mimicking a min_size that depends on
    // the integer).  Direct replacement is rejected — the realised
    // actual_str shorter than the candidate's string is the signal the
    // misalignment-truncation retry needs.
    use crate::native::core::choices::StringChoice;
    use crate::native::intervalsets::IntervalSet;

    let initial = vec![
        int_node(3),
        ChoiceNode::new(
            ChoiceKind::String(StringChoice {
                intervals: IntervalSet::new(vec![(b'a' as u32, b'z' as u32)]),
                min_size: 0,
                max_size: 16,
            }),
            ChoiceValue::String(vec![b'a' as u32, b'a' as u32, b'a' as u32]),
            false,
        ),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Integer at index 0 caps the string length to N.  The
                // closure auto-truncates the string to N.  Predicate:
                // accept iff the candidate's *full* string length
                // equals N (so a too-long candidate is rejected unless
                // truncation lines them up).
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let candidate_str_len = match nodes.get(1).map(|n| &n.value) {
                    Some(ChoiceValue::String(s)) => s.len(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let mut actual: Vec<ChoiceNode> = nodes.to_vec();
                if let Some(node) = actual.get_mut(1) {
                    if let ChoiceValue::String(s) = &mut node.value {
                        s.truncate(n);
                    }
                }
                // Reject any candidate whose original string is longer
                // than the int — only the misalignment-truncation
                // retry, which shortens the string to match `n`, can
                // produce an accepted candidate.
                let ok = n >= 1 && candidate_str_len == n;
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    // The misalignment retry should leave the integer lower than the
    // original 3 and the string truncated to match.
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
    match &shrinker.current_nodes[1].value {
        ChoiceValue::String(s) => {
            assert_eq!(
                s.len() as i128,
                int_value(&shrinker.current_nodes[0]),
                "string length should match the lowered integer"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_size_dep_single_node_delete_succeeds() {
    // Bin_search rejects every direct integer replacement.  Lowering
    // the integer by one yields a shorter realised sequence — and
    // splicing out a single trailing node turns the lowered
    // candidate into an interesting (and shortlex-smaller) result.
    // Exercises the size-dependency single-node delete break at
    // deletion.rs:~280.
    let initial = vec![int_node(2), int_node(7), int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let int_v = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                // Auto-truncate: actual length = 1 + int_v.
                let needed_len = 1usize.saturating_add(int_v as usize);
                let actual_len = needed_len.min(nodes.len());
                let actual: Vec<ChoiceNode> = nodes[..actual_len].to_vec();
                // Predicate: original state (int=2, len=3) OR the
                // post-delete shape (int=1, len=1).  Lowering the int
                // alone to 1 yields (int=1, len=2) which does not
                // match.
                let ok = (int_v == 2 && actual.len() == 3) || (int_v == 1 && actual.len() == 1);
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn minimize_individual_choices_size_dep_span_delete_succeeds() {
    // As above but the closure also reports a span covering the
    // trailing nodes; the span-delete branch (deletion.rs:~269)
    // succeeds before the single-node fallback is tried.
    use crate::native::core::Span;
    let initial = vec![int_node(2), int_node(7), int_node(7), int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let int_v = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1usize.saturating_add(int_v as usize);
                let actual_len = needed_len.min(nodes.len());
                let actual: Vec<ChoiceNode> = nodes[..actual_len].to_vec();
                let mut spans = Spans::new();
                if actual.len() >= 2 {
                    // A "list contents" span covering the trailing
                    // elements.  Splicing it out yields just the
                    // integer node.
                    spans.push(Span {
                        start: 1,
                        end: actual.len(),
                        label: "list".to_string(),
                        depth: 0,
                        parent: None,
                        discarded: false,
                    });
                }
                // Predicate: original (int=2, len=3) OR the
                // post-span-delete shape (int=1, len=1).
                let ok = (int_v == 2 && actual.len() == 3) || (int_v == 1 && actual.len() == 1);
                (ok, actual, spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn minimize_individual_choices_truncates_misaligned_bytes() {
    // Bytes variant of the misaligned-string test, with a `Bytes` node
    // downstream of the integer.
    use crate::native::core::choices::BytesChoice;

    let initial = vec![
        int_node(3),
        ChoiceNode::new(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 0,
                max_size: 16,
            }),
            ChoiceValue::Bytes(vec![1, 2, 3]),
            false,
        ),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let candidate_len = match nodes.get(1).map(|n| &n.value) {
                    Some(ChoiceValue::Bytes(b)) => b.len(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let mut actual: Vec<ChoiceNode> = nodes.to_vec();
                if let Some(node) = actual.get_mut(1) {
                    if let ChoiceValue::Bytes(b) = &mut node.value {
                        b.truncate(n);
                    }
                }
                let ok = n >= 1 && candidate_len == n;
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
    match &shrinker.current_nodes[1].value {
        ChoiceValue::Bytes(b) => {
            assert_eq!(b.len() as i128, int_value(&shrinker.current_nodes[0]));
        }
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_no_op_on_already_simplest_node() {
    let initial = vec![int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

// `test_can_shrink_variable_draws_with_just_deletion` is **DEFERRED**:
// the predicate requires `minimize_individual_choices` to make a
// combined move (lower the count `n` AND bump a trailing zero to
// nonzero, while truncating the tail), chained with the lower-and-bump
// / span-delete fallback that our native equivalent doesn't replicate
// one-to-one. Left for follow-up.
