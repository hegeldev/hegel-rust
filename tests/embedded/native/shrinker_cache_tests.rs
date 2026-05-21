//! Tests for the `consider_cache` insertion-order eviction (Step 6 of
//! the audit cleanup).  Previously the cache used `HashSet::iter().next()`
//! to pick an eviction victim, which is implementation-defined; with
//! `VecDeque + HashSet` the oldest entry is the one dropped.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN + 1,
            max_value: i128::MAX,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

#[test]
fn consider_cache_evicts_oldest_entry_first() {
    use std::cell::RefCell;
    use std::rc::Rc;

    // Track which values the closure actually saw.
    let seen = Rc::new(RefCell::new(Vec::<i128>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let v = match nodes[0].value {
                    ChoiceValue::Integer(v) => v,
                    _ => unreachable!(),
                };
                seen_clone.borrow_mut().push(v);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(0)],
        Spans::new(),
    );
    // Disable max_stall so cache lookups, not stall, gate calls.
    shrinker.max_stall = usize::MAX;

    // Fill the cache: 4097 distinct uninteresting candidates.  The
    // 4097th insert triggers eviction of the first.
    for v in 1..=4097_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    let first_round = seen.borrow().len();
    assert_eq!(first_round, 4097);

    // Re-asking for v=2..=4097 should hit the cache and skip the
    // closure entirely.
    for v in 2..=4097_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    // No new closure invocations from cached hits.
    assert_eq!(seen.borrow().len(), 4097);

    // v=1 was the first inserted; it should have been the one evicted.
    // Re-asking for v=1 should now hit the closure again.
    shrinker.consider(&[int_node(1)]);
    assert_eq!(
        seen.borrow().len(),
        4098,
        "v=1 should have been evicted; expected closure to fire again"
    );
}
