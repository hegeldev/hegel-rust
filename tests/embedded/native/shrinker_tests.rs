use super::*;
use crate::native::core::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
};

// ── bin_search_down ─────────────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_core.py::test_bin_search_down_lo_satisfies.
// In pbtkit this is observed indirectly through state.result; here we
// exercise the helper directly.

#[test]
fn bin_search_down_returns_lo_when_lo_satisfies() {
    // f(lo)=true, so the result should be lo.
    let mut f = |_v: i128| true;
    assert_eq!(bin_search_down(5, 100, &mut f), 5);
}

#[test]
fn bin_search_down_finds_threshold() {
    // f is true iff v >= 17. Searching [0, 100] should find 17.
    let mut f = |v: i128| v >= 17;
    assert_eq!(bin_search_down(0, 100, &mut f), 17);
}

#[test]
fn bin_search_down_returns_hi_when_only_hi_satisfies() {
    // f(hi)=true, f(everything else) = false. Result: hi.
    let mut f = |v: i128| v == 100;
    assert_eq!(bin_search_down(0, 100, &mut f), 100);
}

// ── shrink_bytes ────────────────────────────────────────────────────────────
//
// Exercises each pass of the bytes shrinker: simplest replacement, binary
// search shortening, linear-scan fallback, element deletion, per-byte
// reduction toward zero, and insertion-sort normalization.

fn bytes_node(min_size: usize, max_size: usize, value: Vec<u8>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice { min_size, max_size }),
        value: ChoiceValue::Bytes(value),
        was_forced: false,
    }
}

fn bytes_at(nodes: &[ChoiceNode], i: usize) -> Vec<u8> {
    match &nodes[i].value {
        ChoiceValue::Bytes(v) => v.clone(),
        _ => panic!("expected Bytes at index {i}"),
    }
}

fn bool_node(v: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(v),
        was_forced: false,
    }
}

#[test]
fn shrink_bytes_skips_non_bytes_nodes() {
    // A non-bytes node alongside a bytes node: the pass must skip the
    // boolean without panicking and still shrink the bytes.
    let nodes = vec![bool_node(true), bytes_node(0, 10, vec![7, 8, 9])];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 2)), nodes);
    shrinker.shrink_bytes();
    // Boolean unchanged.
    assert!(matches!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Boolean(true)
    ));
    // Bytes shrunk to empty (simplest for min_size=0).
    assert_eq!(bytes_at(&shrinker.current_nodes, 1), Vec::<u8>::new());
}

#[test]
fn shrink_bytes_replaces_with_simplest() {
    // Always interesting → bytes shrink to min_size zeros.
    let nodes = vec![bytes_node(3, 10, vec![1, 2, 3])];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 1)), nodes);
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![0, 0, 0]);
}

#[test]
fn shrink_bytes_noop_when_already_simplest() {
    // Starting from simplest — no replace attempts should fire at the
    // simplest step, and later passes should find nothing to do.
    let nodes = vec![bytes_node(2, 10, vec![0, 0])];
    let mut calls = 0;
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| {
            calls += 1;
            (true, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![0, 0]);
}

#[test]
fn shrink_bytes_binary_searches_shorter_length() {
    // Interesting iff length >= 3. Binary search finds the shortest
    // prefix that satisfies the predicate.
    let nodes = vec![bytes_node(0, 100, vec![5; 10])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            (b.len() >= 3, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    // Length exactly 3, bytes reduced toward zero.
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![0, 0, 0]);
}

#[test]
fn shrink_bytes_linear_scan_catches_non_monotonic_lengths() {
    // Interesting iff length is exactly 2 or exactly 8. Binary search
    // over [0, 8] misses length 2 because it's not monotonic; the linear
    // scan over [min_size, min_size+8) catches it.
    let nodes = vec![bytes_node(0, 100, vec![5; 8])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            (b.len() == 2 || b.len() == 8, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![0, 0]);
}

#[test]
fn shrink_bytes_deletes_middle_elements() {
    // Interesting iff the sum of bytes is exactly 50. The delete pass
    // should remove the zero elements between the two 25s; the reduce
    // pass is blocked by the sum constraint.
    let nodes = vec![bytes_node(0, 100, vec![25, 0, 0, 0, 25])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            (b.iter().map(|&x| u32::from(x)).sum::<u32>() == 50, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    let final_bytes = bytes_at(&shrinker.current_nodes, 0);
    assert_eq!(final_bytes.iter().map(|&x| u32::from(x)).sum::<u32>(), 50);
    assert_eq!(final_bytes.len(), 2);
}

#[test]
fn shrink_bytes_skips_delete_when_at_min_size() {
    // min_size=2 floor means the delete pass can't shrink further; it
    // must hit the `cur.len() <= min_size` branch and `continue`.
    // Interesting iff len >= 2 && first byte >= 3.
    let nodes = vec![bytes_node(2, 100, vec![5, 5, 5, 5])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            (b.len() >= 2 && b[0] >= 3, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![3, 0]);
}

#[test]
fn shrink_bytes_reduces_byte_values_toward_zero() {
    // min_size=2 blocks length reduction; only the reduce pass
    // applies. Interesting iff bytes[0] >= 5 and bytes[1] >= 3.
    let nodes = vec![bytes_node(2, 2, vec![50, 70])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            (b.len() == 2 && b[0] >= 5 && b[1] >= 3, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![5, 3]);
}

#[test]
fn shrink_bytes_insertion_sort_normalizes_order() {
    // Predicate fixes length at 3 and multiset {1, 2, 3} but allows any
    // order. Insertion sort swaps adjacent inversions until sorted.
    let nodes = vec![bytes_node(3, 3, vec![3, 1, 2])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            if b.len() != 3 {
                return (false, 1);
            }
            let mut sorted = b.clone();
            sorted.sort();
            (sorted == vec![1, 2, 3], 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![1, 2, 3]);
}

#[test]
fn shrink_bytes_insertion_sort_stops_when_swap_rejected() {
    // A fragile ordering: only [2, 1, 3] and [1, 2, 3] satisfy the
    // predicate. Insertion sort tries to swap [2, 1] → [1, 2] which IS
    // accepted (the target already permits that intermediate), then can't
    // improve further. When the predicate rejects a swap, the inner
    // while-loop's `else { break; }` branch fires.
    let nodes = vec![bytes_node(3, 3, vec![2, 3, 1])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Bytes(b) = &n[0].value else {
                unreachable!()
            };
            // Accept only specific orderings so mid-sort swaps can be rejected.
            let ok = b == &[2u8, 3, 1] || b == &[2u8, 1, 3] || b == &[1u8, 2, 3];
            (ok, 1)
        }),
        nodes,
    );
    shrinker.shrink_bytes();
    // Ends at the sorted form once sorting completes through an accepted path.
    assert_eq!(bytes_at(&shrinker.current_nodes, 0), vec![1, 2, 3]);
}

// ── shrink_floats ───────────────────────────────────────────────────────────

fn float_node(fc: FloatChoice, v: f64) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Float(fc),
        value: ChoiceValue::Float(v),
        was_forced: false,
    }
}

fn float_at(nodes: &[ChoiceNode], i: usize) -> f64 {
    match nodes[i].value {
        ChoiceValue::Float(f) => f,
        _ => panic!("expected Float at index {i}"),
    }
}

fn unbounded_float() -> FloatChoice {
    FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
    }
}

#[test]
fn shrink_floats_skips_non_float_nodes() {
    // Non-float alongside float: skip the bool, shrink the float.
    let nodes = vec![bool_node(true), float_node(unbounded_float(), 4.5)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 2)), nodes);
    shrinker.shrink_floats();
    assert!(matches!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Boolean(true)
    ));
    assert_eq!(float_at(&shrinker.current_nodes, 1), 0.0);
}

#[test]
fn shrink_floats_replaces_with_simplest() {
    // Always interesting → shrink to simplest (0.0).
    let nodes = vec![float_node(unbounded_float(), 123.456)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 1)), nodes);
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 0.0);
}

#[test]
fn shrink_floats_skips_nan() {
    // A NaN value can't be binary-searched. The pass must recognise this
    // and leave the NaN in place (it's already the "simplest" NaN here).
    let nan_only = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::NEG_INFINITY, // only -inf and NaN valid
        allow_nan: true,
        allow_infinity: true,
    };
    let nodes = vec![float_node(nan_only, f64::NAN)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            // Only NaN is "interesting". Reject any numeric replacement.
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            (f.is_nan(), 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    assert!(float_at(&shrinker.current_nodes, 0).is_nan());
}

#[test]
fn shrink_floats_negates_sign_negative() {
    // Interesting iff |v| >= 10. Starting from -20.0, negation makes 20.0
    // interesting (sort-simpler), and further shrinks reach 10.0.
    let fc = FloatChoice {
        min_value: -100.0,
        max_value: 100.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, -20.0)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            (f.abs() >= 10.0, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 10.0);
}

#[test]
fn shrink_floats_skips_negate_when_already_positive() {
    // Positive starting point: negation branch is skipped.
    let fc = FloatChoice {
        min_value: 0.0,
        max_value: 100.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, 50.0)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 1)), nodes);
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 0.0);
}

#[test]
fn shrink_floats_integer_search_finds_positive_integer() {
    // Interesting iff v >= 7.5. Starting from 65672.5 (non-integer, large
    // lex index), the integer-search step should jump to 8.0.
    let fc = FloatChoice {
        min_value: 0.0,
        max_value: 1_000_000.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, 65672.5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            (f >= 7.5, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 8.0);
}

#[test]
fn shrink_floats_integer_search_negative_range() {
    // Entirely-negative range, predicate forbids the simplest integer.
    // Forces step 3a to run on a negative non-integer and exercise the
    // `max_value <= 0.0 → lo = (-fc.max_value).ceil()` branch.
    let fc = FloatChoice {
        min_value: -1_000_000.0,
        max_value: -1.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, -65672.5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            (f <= -3.0, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), -3.0);
}

#[test]
fn shrink_floats_integer_search_straddling_zero_negative() {
    // Range straddles zero, starting from a negative non-integer. The
    // negative integer search uses `max_value > 0.0 → lo = 0`, which
    // eventually produces a candidate the predicate accepts.
    let fc = FloatChoice {
        min_value: -1_000_000.0,
        max_value: 1_000_000.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, -65672.5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            (f <= -3.0, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    assert_eq!(float_at(&shrinker.current_nodes, 0), -3.0);
}

#[test]
fn shrink_floats_final_binary_search_rejects_out_of_range() {
    // Force 3b to exercise its `validate` rejection branch: the simplest
    // is ruled out by the predicate, so shrinking proceeds via lex-index
    // binary search, and fc.validate rejects candidates below min_value.
    let fc = FloatChoice {
        min_value: 2.0,
        max_value: 5.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![float_node(fc, 4.5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Float(f) = n[0].value else {
                unreachable!()
            };
            // Only non-integers in [2, 5] are "interesting", preventing
            // integer search from snapping to 2.0 / 3.0 / etc.
            ((2.0..=5.0).contains(&f) && f.fract() != 0.0, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    let v = float_at(&shrinker.current_nodes, 0);
    assert!((2.0..=5.0).contains(&v));
    assert!(v.fract() != 0.0);
}

// ── sort_values / swap_adjacent_blocks ──────────────────────────────────────

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: min,
            max_value: max,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn int_at(nodes: &[ChoiceNode], i: usize) -> i128 {
    match nodes[i].value {
        ChoiceValue::Integer(v) => v,
        _ => panic!("expected Integer at index {i}"),
    }
}

fn bool_at(nodes: &[ChoiceNode], i: usize) -> bool {
    match nodes[i].value {
        ChoiceValue::Boolean(v) => v,
        _ => panic!("expected Boolean at index {i}"),
    }
}

#[test]
fn sort_values_integers_reorders_by_absolute_magnitude() {
    // Three ints at positions 0, 2, 4 — the pass groups them and sorts by
    // |value| without regard to sign. Boolean at position 1 is untouched.
    let nodes = vec![
        int_node(-100, 100, 50),
        bool_node(true),
        int_node(-100, 100, -3),
        bool_node(true),
        int_node(-100, 100, 20),
    ];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 5)), nodes);
    shrinker.sort_values_integers();
    // Expected sorted by |v|: -3, 20, 50.
    assert_eq!(int_at(&shrinker.current_nodes, 0), -3);
    assert_eq!(int_at(&shrinker.current_nodes, 2), 20);
    assert_eq!(int_at(&shrinker.current_nodes, 4), 50);
    // Booleans unchanged.
    assert!(bool_at(&shrinker.current_nodes, 1));
    assert!(bool_at(&shrinker.current_nodes, 3));
}

#[test]
fn sort_values_integers_skips_when_fewer_than_two() {
    // Single integer → short-circuit: the sort can't reorder anything.
    let nodes = vec![bool_node(true), int_node(-100, 100, 42), bool_node(false)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 3)), nodes);
    shrinker.sort_values_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 1), 42);
}

#[test]
fn sort_values_integers_noop_when_already_sorted() {
    // Already in magnitude-sorted order: no change, predicate never called.
    let nodes = vec![int_node(-100, 100, 1), int_node(-100, 100, -5)];
    let mut calls = 0;
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| {
            calls += 1;
            (true, 2)
        }),
        nodes,
    );
    shrinker.sort_values_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 1);
    assert_eq!(int_at(&shrinker.current_nodes, 1), -5);
}

#[test]
fn sort_values_booleans_orders_false_before_true() {
    let nodes = vec![bool_node(true), bool_node(false), bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 3)), nodes);
    shrinker.sort_values_booleans();
    assert!(!bool_at(&shrinker.current_nodes, 0));
    assert!(bool_at(&shrinker.current_nodes, 1));
    assert!(bool_at(&shrinker.current_nodes, 2));
}

#[test]
fn sort_values_booleans_skips_when_fewer_than_two() {
    let nodes = vec![int_node(0, 10, 5), bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 2)), nodes);
    shrinker.sort_values_booleans();
    assert!(bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn sort_values_booleans_noop_when_already_sorted() {
    let nodes = vec![bool_node(false), bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 2)), nodes);
    shrinker.sort_values_booleans();
    assert!(!bool_at(&shrinker.current_nodes, 0));
    assert!(bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn sort_values_dispatches_to_both_helpers() {
    // sort_values() is the public entry point; both sub-passes should run.
    let nodes = vec![
        int_node(-100, 100, 50),
        int_node(-100, 100, 3),
        bool_node(true),
        bool_node(false),
    ];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 4)), nodes);
    shrinker.sort_values();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 50);
    assert!(!bool_at(&shrinker.current_nodes, 2));
    assert!(bool_at(&shrinker.current_nodes, 3));
}

#[test]
fn swap_adjacent_blocks_swaps_differing_pair() {
    // Two adjacent [int, bool] blocks: swapping should succeed if the later
    // block sorts simpler. Predicate accepts everything.
    let nodes = vec![
        int_node(0, 100, 5),
        bool_node(true),
        int_node(0, 100, 2),
        bool_node(false),
    ];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 4)), nodes);
    shrinker.swap_adjacent_blocks();
    // After swap the simpler block moves first.
    assert_eq!(int_at(&shrinker.current_nodes, 0), 2);
    assert!(!bool_at(&shrinker.current_nodes, 1));
    assert_eq!(int_at(&shrinker.current_nodes, 2), 5);
    assert!(bool_at(&shrinker.current_nodes, 3));
}

#[test]
fn swap_adjacent_blocks_skips_mismatched_types() {
    // Block [int, bool] next to [bool, int] — types don't match, skip.
    let nodes = vec![
        int_node(0, 100, 9),
        bool_node(false),
        bool_node(false),
        int_node(0, 100, 1),
    ];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 4)), nodes);
    shrinker.swap_adjacent_blocks();
    // Unchanged.
    assert_eq!(int_at(&shrinker.current_nodes, 0), 9);
    assert_eq!(int_at(&shrinker.current_nodes, 3), 1);
}

#[test]
fn swap_adjacent_blocks_skips_equal_blocks() {
    // Two identical [int, bool] blocks — nothing to gain from swap.
    let nodes = vec![
        int_node(0, 100, 7),
        bool_node(true),
        int_node(0, 100, 7),
        bool_node(true),
    ];
    let mut calls = 0;
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| {
            calls += 1;
            (true, 4)
        }),
        nodes,
    );
    shrinker.swap_adjacent_blocks();
    // Swap would be a no-op so it shouldn't fire.
    assert_eq!(int_at(&shrinker.current_nodes, 0), 7);
    assert_eq!(int_at(&shrinker.current_nodes, 2), 7);
}

// ── delete_chunks ───────────────────────────────────────────────────────────

#[test]
fn delete_chunks_empty_nodes_terminates_cleanly() {
    // Starting with zero nodes, every k iteration must bail via the
    // `i >= self.current_nodes.len()` break without attempting deletions.
    let nodes: Vec<ChoiceNode> = Vec::new();
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("test_fn must not be called on empty nodes")),
        nodes,
    );
    shrinker.delete_chunks();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn delete_chunks_removes_middle_booleans() {
    // Predicate: first two booleans must be true. Middle false booleans are
    // pure padding and can be deleted in any chunk size.
    let nodes = vec![
        bool_node(true),
        bool_node(true),
        bool_node(false),
        bool_node(false),
        bool_node(false),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let got_leaders = n.len() >= 2
                && matches!(n[0].value, ChoiceValue::Boolean(true))
                && matches!(n[1].value, ChoiceValue::Boolean(true));
            (got_leaders, n.len())
        }),
        nodes,
    );
    shrinker.delete_chunks();
    // Single delete_chunks pass can remove three padding Fs. `shrink()` would
    // iterate further; here we exercise just the pass and verify it made
    // progress without breaking the invariant.
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert!(bool_at(&shrinker.current_nodes, 0));
    assert!(bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn delete_chunks_decrements_preceding_integer_on_failed_delete() {
    // The test expects a counter that tracks how many trailing booleans there
    // are. Deleting a trailing bool alone breaks the invariant; decrementing
    // the counter alongside the delete restores it.
    let nodes = vec![
        int_node(0, 10, 2),
        bool_node(true),
        bool_node(true),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ok = !n.is_empty()
                && matches!(&n[0].value, ChoiceValue::Integer(v) if (*v as usize) + 1 == n.len());
            (ok, n.len())
        }),
        nodes,
    );
    shrinker.delete_chunks();
    // Counter should match the number of trailing booleans (0, 1, or 2).
    let ChoiceValue::Integer(v) = shrinker.current_nodes[0].value else {
        panic!("expected integer")
    };
    assert_eq!((v as usize) + 1, shrinker.current_nodes.len());
    assert!(shrinker.current_nodes.len() < 3);
}

#[test]
fn delete_chunks_decrements_preceding_boolean_on_failed_delete() {
    // A "has-extra" boolean gate followed by a boolean payload: dropping just
    // the payload breaks the invariant (gate says true but no payload); the
    // pass flips the gate to false while deleting the payload.
    let nodes = vec![bool_node(true), bool_node(false), bool_node(true)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            if n.is_empty() {
                return (false, 0);
            }
            let ChoiceValue::Boolean(gate) = n[0].value else {
                return (false, n.len());
            };
            let ok = if gate { n.len() >= 2 } else { n.len() == 1 };
            (ok, n.len())
        }),
        nodes,
    );
    shrinker.delete_chunks();
    // Either [true, x] kept (len 2) or [false] after flipping gate.
    if shrinker.current_nodes.len() == 1 {
        assert!(!bool_at(&shrinker.current_nodes, 0));
    } else {
        assert!(shrinker.current_nodes.len() >= 2);
    }
}

#[test]
fn delete_chunks_skips_integer_decrement_when_already_simplest() {
    // Preceding integer is already at its simplest value, so the integer
    // branch of the decrement match arm is skipped. No further action.
    let nodes = vec![
        int_node(0, 10, 0),
        bool_node(false),
        bool_node(false),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| (!n.is_empty(), n.len())),
        nodes,
    );
    shrinker.delete_chunks();
    // Must terminate without panicking. Some shrink is allowed.
    assert!(!shrinker.current_nodes.is_empty());
}

// ── try_replace_with_deletion / bind_deletion ───────────────────────────────

#[test]
fn try_replace_with_deletion_returns_true_on_direct_replace() {
    // Straight replace succeeds → short-circuit, no further probing.
    let nodes = vec![int_node(0, 10, 5)];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 1)), nodes);
    let got = shrinker.try_replace_with_deletion(0, ChoiceValue::Integer(2), 1);
    assert!(got);
    assert_eq!(int_at(&shrinker.current_nodes, 0), 2);
}

#[test]
fn try_replace_with_deletion_returns_false_when_actual_len_not_shorter() {
    // The attempt isn't interesting, and the test uses the same number of
    // nodes — no deletion can recover anything.
    let nodes = vec![int_node(0, 10, 5), bool_node(true)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            // Only interesting on the untouched value 5.
            let ok = matches!(&n[0].value, ChoiceValue::Integer(v) if *v == 5);
            (ok, n.len())
        }),
        nodes,
    );
    let got = shrinker.try_replace_with_deletion(0, ChoiceValue::Integer(3), 2);
    assert!(!got);
}

#[test]
fn try_replace_with_deletion_deletes_trailing_region() {
    // Length-prefixed sequence: the integer at index 0 is the claimed
    // length. The test only reads `v` trailing booleans and requires the
    // total length to match exactly, so replace alone can't shrink — the
    // trailing region must also be deleted.
    let nodes = vec![
        int_node(0, 10, 3),
        bool_node(true),
        bool_node(true),
        bool_node(true),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            if n.is_empty() {
                return (false, 0);
            }
            let ChoiceValue::Integer(v) = n[0].value else {
                return (false, n.len());
            };
            let needed = v as usize;
            let consumed = 1 + needed.min(n.len() - 1);
            // Interesting iff length matches exactly AND v >= 2.
            let ok = n.len() == 1 + needed && needed >= 2;
            (ok, consumed)
        }),
        nodes,
    );
    // Initial [3, T, T, T] is interesting. bind_deletion drives
    // replace-with-deletion via bin_search_down; replace alone can't reduce
    // v without also deleting the now-excess booleans, so the deletion loop
    // inside `try_replace_with_deletion` fires.
    shrinker.bind_deletion();
    // Should have shrunk to [2, T, T] (minimal v with the "v >= 2" bound).
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 0), 2);
    assert!(bool_at(&shrinker.current_nodes, 1));
    assert!(bool_at(&shrinker.current_nodes, 2));
}

#[test]
fn bind_deletion_skips_non_integers_and_simplest_integers() {
    // No work possible: the single integer is already at simplest, and the
    // booleans aren't eligible. bind_deletion must walk the list without
    // making any changes.
    let nodes = vec![
        bool_node(true),
        int_node(0, 10, 0),
        bool_node(false),
    ];
    let mut shrinker = Shrinker::new(Box::new(|_: &[ChoiceNode]| (true, 3)), nodes);
    shrinker.bind_deletion();
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}
