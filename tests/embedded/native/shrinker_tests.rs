use super::strings::key_to_codepoint_in_range;
use super::*;
use crate::native::core::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
    StringChoice,
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
        Box::new(|n: &[ChoiceNode]| {
            calls += 1;
            (true, n.to_vec())
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
            (b.len() >= 3, n.to_vec())
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
            (b.len() == 2 || b.len() == 8, n.to_vec())
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
            (
                b.iter().map(|&x| u32::from(x)).sum::<u32>() == 50,
                n.to_vec(),
            )
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
            (b.len() >= 2 && b[0] >= 3, n.to_vec())
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
            (b.len() == 2 && b[0] >= 5 && b[1] >= 3, n.to_vec())
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
                return (false, n.to_vec());
            }
            let mut sorted = b.clone();
            sorted.sort();
            (sorted == vec![1, 2, 3], n.to_vec())
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
            (ok, n.to_vec())
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
            (f.is_nan(), n.to_vec())
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
            (f.abs() >= 10.0, n.to_vec())
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
            (f >= 7.5, n.to_vec())
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
            (f <= -3.0, n.to_vec())
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
            (f <= -3.0, n.to_vec())
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
            ((2.0..=5.0).contains(&f) && f.fract() != 0.0, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_floats();
    let v = float_at(&shrinker.current_nodes, 0);
    assert!((2.0..=5.0).contains(&v));
    assert!(v.fract() != 0.0);
}

#[test]
fn shrink_floats_mantissa_reduction_converges() {
    // Port of pbtkit/test_floats.py::test_mantissa_reduction_search.
    // Starting from (x=1.0, y=-3.0000000136813605) with predicate
    // `x + (y - x) != y`, the shrinker must converge to a y whose mantissa
    // sits ~30M ULPs lower, close to -3.0. Without lex-index binary search
    // this crawls at 1 ULP / iteration; with it, convergence is immediate.
    let fc = FloatChoice {
        min_value: -1e100,
        max_value: 1e100,
        allow_nan: false,
        allow_infinity: false,
    };
    let nodes = vec![
        float_node(fc.clone(), 1.0),
        float_node(fc, -3.0000000136813605),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            if n.len() < 2 {
                return (false, n.to_vec());
            }
            let ChoiceValue::Float(x) = n[0].value else {
                return (false, n.to_vec());
            };
            let ChoiceValue::Float(y) = n[1].value else {
                return (false, n.to_vec());
            };
            (x + (y - x) != y, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink();
    // pbtkit lands on -3.0000000000000004; hegel-rust's lex-index binary
    // search shrinks further to -1.0 - 1 ULP (≈ -1.0000000000000002), which
    // has a smaller lex magnitude and still breaks `x + (y - x) == y`. The
    // upstream point is convergence (not crawling at 1 ULP/iter), so assert
    // y landed close to a simple integer and still satisfies the predicate.
    let x = float_at(&shrinker.current_nodes, 0);
    let y = float_at(&shrinker.current_nodes, 1);
    assert_eq!(x, 1.0);
    assert!(
        y.abs() <= 3.0 + f64::EPSILON && y != -3.0000000136813605,
        "y did not converge: {y}"
    );
    assert!(x + (y - x) != y);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.sort_values_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 1), 42);
}

#[test]
fn sort_values_integers_noop_when_already_sorted() {
    // Already in magnitude-sorted order: no change, predicate never called.
    let nodes = vec![int_node(-100, 100, 1), int_node(-100, 100, -5)];
    let mut calls = 0;
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            calls += 1;
            (true, n.to_vec())
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.sort_values_booleans();
    assert!(!bool_at(&shrinker.current_nodes, 0));
    assert!(bool_at(&shrinker.current_nodes, 1));
    assert!(bool_at(&shrinker.current_nodes, 2));
}

#[test]
fn sort_values_booleans_skips_when_fewer_than_two() {
    let nodes = vec![int_node(0, 10, 5), bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.sort_values_booleans();
    assert!(bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn sort_values_booleans_noop_when_already_sorted() {
    let nodes = vec![bool_node(false), bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
        Box::new(|n: &[ChoiceNode]| {
            calls += 1;
            (true, n.to_vec())
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
            (got_leaders, n.to_vec())
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
    let nodes = vec![int_node(0, 10, 2), bool_node(true), bool_node(true)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ok = !n.is_empty()
                && matches!(&n[0].value, ChoiceValue::Integer(v) if (*v as usize) + 1 == n.len());
            (ok, n.to_vec())
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
                return (false, n.to_vec());
            }
            let ChoiceValue::Boolean(gate) = n[0].value else {
                return (false, n.to_vec());
            };
            let ok = if gate { n.len() >= 2 } else { n.len() == 1 };
            (ok, n.to_vec())
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
    let nodes = vec![int_node(0, 10, 0), bool_node(false), bool_node(false)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| (!n.is_empty(), n.to_vec())),
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
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
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
            (ok, n.to_vec())
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
                return (false, n.to_vec());
            }
            let ChoiceValue::Integer(v) = n[0].value else {
                return (false, n.to_vec());
            };
            let needed = v as usize;
            let consumed = 1 + needed.min(n.len() - 1);
            // Interesting iff length matches exactly AND v >= 2.
            let ok = n.len() == 1 + needed && needed >= 2;
            (ok, n[..consumed].to_vec())
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
    let nodes = vec![bool_node(true), int_node(0, 10, 0), bool_node(false)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.bind_deletion();
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

// ── key_to_codepoint_in_range ───────────────────────────────────────────────
//
// The helper powers step 4 of shrink_strings (reduce codepoints). It converts
// a sort-key back into the raw codepoint that produces it, filtering out
// anything outside the alphabet or in the surrogate block.

fn sc(min_cp: u32, max_cp: u32) -> StringChoice {
    StringChoice {
        min_codepoint: min_cp,
        max_codepoint: max_cp,
        min_size: 0,
        max_size: 10,
    }
}

#[test]
fn key_to_codepoint_in_range_ascii_maps_through_key_to_codepoint() {
    // k < 128 → cp = (k + '0') % 128. For k=0 → '0' (48); for k=80 the
    // sum wraps back to 0.
    let kind = sc(0, 127);
    assert_eq!(key_to_codepoint_in_range(0, &kind), Some(b'0' as u32));
    assert_eq!(key_to_codepoint_in_range(80, &kind), Some(0));
}

#[test]
fn key_to_codepoint_in_range_ascii_rejected_when_below_min() {
    // k=0 → cp=48. If min_cp excludes 48, returns None.
    let kind = sc(50, 127);
    assert_eq!(key_to_codepoint_in_range(0, &kind), None);
    // k=2 → cp=50, at the boundary: accepted.
    assert_eq!(key_to_codepoint_in_range(2, &kind), Some(50));
}

#[test]
fn key_to_codepoint_in_range_high_codepoint_identity() {
    // k ≥ 128 → cp = k.
    let kind = sc(0, 0x10000);
    assert_eq!(key_to_codepoint_in_range(200, &kind), Some(200));
    assert_eq!(key_to_codepoint_in_range(0xE000, &kind), Some(0xE000));
}

#[test]
fn key_to_codepoint_in_range_rejects_surrogates() {
    // Both endpoints of the surrogate block map to None even though the
    // alphabet nominally contains them.
    let kind = sc(0, 0xFFFF);
    assert_eq!(key_to_codepoint_in_range(0xD800, &kind), None);
    assert_eq!(key_to_codepoint_in_range(0xDFFF, &kind), None);
}

#[test]
fn key_to_codepoint_in_range_rejects_above_max() {
    let kind = sc(0, 100);
    assert_eq!(key_to_codepoint_in_range(200, &kind), None);
}

// ── shrink_strings ──────────────────────────────────────────────────────────
//
// Exercises each pass: the non-string skip, the simplest replacement, the
// linear-scan shortener, per-codepoint deletion, reduction toward the
// simplest codepoint, and the insertion-sort normaliser. Also covers the
// `current_key == 0 → continue` branch of the reduce pass.

fn string_node(
    min_size: usize,
    max_size: usize,
    min_cp: u32,
    max_cp: u32,
    value: Vec<u32>,
) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            min_codepoint: min_cp,
            max_codepoint: max_cp,
            min_size,
            max_size,
        }),
        value: ChoiceValue::String(value),
        was_forced: false,
    }
}

fn string_at(nodes: &[ChoiceNode], i: usize) -> Vec<u32> {
    match &nodes[i].value {
        ChoiceValue::String(v) => v.clone(),
        _ => panic!("expected String at index {i}"),
    }
}

#[test]
fn shrink_strings_skips_non_string_nodes() {
    // A non-string node alongside a string: the pass must skip the boolean
    // without panicking and still shrink the string.
    let nodes = vec![
        bool_node(true),
        string_node(0, 10, 0x30, 0x7A, vec![b'a' as u32, b'b' as u32]),
    ];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_strings();
    assert!(matches!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Boolean(true)
    ));
    // String shrunk to empty (simplest for min_size=0).
    assert_eq!(string_at(&shrinker.current_nodes, 1), Vec::<u32>::new());
}

#[test]
fn shrink_strings_replaces_with_simplest() {
    // Always interesting → string shrinks to min_size copies of the simplest
    // codepoint ('0' = 48).
    let nodes = vec![string_node(
        3,
        10,
        0x30,
        0x7A,
        vec![b'a' as u32, b'b' as u32, b'c' as u32],
    )];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_strings();
    assert_eq!(string_at(&shrinker.current_nodes, 0), vec![48, 48, 48]);
}

#[test]
fn shrink_strings_linear_scan_catches_non_monotonic_lengths() {
    // Interesting iff length is exactly 2 or exactly 7. Step 2's linear scan
    // from min_size=0 hits length 2 before reaching the original length 7.
    let nodes = vec![string_node(0, 100, 0x30, 0x7A, vec![b'a' as u32; 7])];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::String(s) = &n[0].value else {
                unreachable!()
            };
            (s.len() == 2 || s.len() == 7, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_strings();
    // Reduces to ['0', '0'] after shortening + reducing.
    assert_eq!(string_at(&shrinker.current_nodes, 0), vec![48, 48]);
}

#[test]
fn shrink_strings_deletes_middle_codepoints() {
    // Predicate needs first char 'X' and last char 'Y', length ≥ 2. Step 2's
    // prefix shortening can never satisfy "ends in Y", so only step 3 (delete
    // by index) can shrink length.
    let nodes = vec![string_node(
        0,
        100,
        0x30,
        0x7A,
        vec![
            b'X' as u32,
            b'a' as u32,
            b'b' as u32,
            b'c' as u32,
            b'Y' as u32,
        ],
    )];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::String(s) = &n[0].value else {
                unreachable!()
            };
            let ok = s.len() >= 2 && s[0] == b'X' as u32 && *s.last().unwrap() == b'Y' as u32;
            (ok, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_strings();
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'X' as u32, b'Y' as u32]
    );
}

#[test]
fn shrink_strings_delete_skips_when_at_min_size() {
    // min_size=2 with a length-3 input whose first two codepoints are fixed
    // and whose third is arbitrary. After one successful delete the length
    // hits min_size=2; subsequent delete iterations must hit the
    // `cur.len() <= min_size → continue` branch.
    let nodes = vec![string_node(
        2,
        10,
        0x30,
        0x7A,
        vec![b'A' as u32, b'B' as u32, b'C' as u32],
    )];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::String(s) = &n[0].value else {
                unreachable!()
            };
            // Interesting iff length ≥ 2 and starts 'A','B'. The delete pass
            // can drop the trailing 'C' once, then further deletes are
            // blocked by min_size.
            let ok = s.len() >= 2 && s[0] == b'A' as u32 && s[1] == b'B' as u32;
            (ok, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_strings();
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'A' as u32, b'B' as u32]
    );
}

#[test]
fn shrink_strings_reduce_skips_simplest_codepoint() {
    // Start already at the simplest value (['0','0']). Step 4 visits each
    // position, computes current_key = 0, and hits the `continue` branch
    // for both positions without calling the predicate.
    let nodes = vec![string_node(2, 2, 0x30, 0x7A, vec![48, 48])];
    let mut calls = 0;
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            calls += 1;
            (true, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_strings();
    assert_eq!(string_at(&shrinker.current_nodes, 0), vec![48, 48]);
}

#[test]
fn shrink_strings_reduce_advances_past_none_candidates() {
    // min_codepoint=50 rules out candidate keys 0 and 1 (which would be
    // codepoints 48 and 49), forcing the reduce loop to iterate past the
    // `None` arm of `key_to_codepoint_in_range` before finding cp=50.
    let nodes = vec![string_node(1, 1, 50, 100, vec![80])];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_strings();
    assert_eq!(string_at(&shrinker.current_nodes, 0), vec![50]);
}

#[test]
fn shrink_strings_insertion_sort_stops_when_swap_rejected() {
    // Exercises step 5's `else { break }` path. Predicate accepts only two
    // orderings so the insertion-sort swap at one position is rejected.
    let nodes = vec![string_node(
        3,
        3,
        0x30,
        0x7A,
        vec![b'2' as u32, b'3' as u32, b'1' as u32],
    )];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::String(s) = &n[0].value else {
                unreachable!()
            };
            let a = b'1' as u32;
            let b = b'2' as u32;
            let c = b'3' as u32;
            let ok = s == &[b, c, a] || s == &[b, a, c] || s == &[a, b, c];
            (ok, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_strings();
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'1' as u32, b'2' as u32, b'3' as u32]
    );
}

// ── redistribute_string_pairs ───────────────────────────────────────────────

#[test]
fn redistribute_string_pairs_moves_everything_s_to_t() {
    // Two adjacent strings, predicate only cares about the concatenation.
    // Moving s entirely into t succeeds on the first try and shrinks s to
    // empty (the minimal sort_key).
    let nodes = vec![
        string_node(0, 10, 0x30, 0x7A, vec![b'a' as u32, b'b' as u32]),
        string_node(0, 10, 0x30, 0x7A, vec![b'c' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::String(s), ChoiceValue::String(t)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
            (
                combined == vec![b'a' as u32, b'b' as u32, b'c' as u32],
                n.to_vec(),
            )
        }),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    assert_eq!(string_at(&shrinker.current_nodes, 0), Vec::<u32>::new());
    assert_eq!(
        string_at(&shrinker.current_nodes, 1),
        vec![b'a' as u32, b'b' as u32, b'c' as u32]
    );
}

#[test]
fn redistribute_string_pairs_moves_last_codepoint_when_move_all_rejected() {
    // Predicate needs s non-empty and t length ≥ 2. "Move everything" leaves
    // s empty (rejected); "move just last" succeeds, shrinking s by one and
    // growing t. bin_search_down then probes f(1), which matches the current
    // sort_key and returns immediately without further iteration.
    let nodes = vec![
        string_node(
            0,
            10,
            0x30,
            0x7A,
            vec![b'a' as u32, b'b' as u32, b'c' as u32],
        ),
        string_node(0, 10, 0x30, 0x7A, vec![b'd' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::String(s), ChoiceValue::String(t)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            (!s.is_empty() && t.len() >= 2, n.to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'a' as u32, b'b' as u32]
    );
    assert_eq!(
        string_at(&shrinker.current_nodes, 1),
        vec![b'c' as u32, b'd' as u32]
    );
}

#[test]
fn redistribute_string_pairs_aborts_when_single_move_rejected() {
    // Predicate needs s ≥ 2 codepoints. Moving everything fails (leaves s
    // empty), moving just the last codepoint also fails (leaves s with 1),
    // so the pair is abandoned without running the binary search.
    let nodes = vec![
        string_node(0, 10, 0x30, 0x7A, vec![b'a' as u32, b'b' as u32]),
        string_node(0, 10, 0x30, 0x7A, vec![b'c' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::String(s) = &n[0].value else {
                unreachable!()
            };
            (s.len() >= 2, n.to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    // Unchanged.
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'a' as u32, b'b' as u32]
    );
    assert_eq!(string_at(&shrinker.current_nodes, 1), vec![b'c' as u32]);
}

#[test]
fn redistribute_string_pairs_skips_empty_s() {
    // s is already empty → the pair is skipped without any predicate calls.
    let nodes = vec![
        string_node(0, 10, 0x30, 0x7A, Vec::<u32>::new()),
        string_node(0, 10, 0x30, 0x7A, vec![b'x' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    assert_eq!(string_at(&shrinker.current_nodes, 0), Vec::<u32>::new());
    assert_eq!(string_at(&shrinker.current_nodes, 1), vec![b'x' as u32]);
}

#[test]
fn redistribute_string_pairs_rejects_when_target_max_size_exceeded() {
    // j's max_size=2 blocks any move that would grow t beyond 2 codepoints.
    // try_redistribute's validate check returns false, so the pair is
    // skipped despite the predicate being permissive.
    let nodes = vec![
        string_node(
            0,
            10,
            0x30,
            0x7A,
            vec![b'a' as u32, b'b' as u32, b'c' as u32],
        ),
        string_node(0, 2, 0x30, 0x7A, vec![b'd' as u32, b'e' as u32]),
    ];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.redistribute_string_pairs();
    // Both strings unchanged: every move would push t past max_size=2.
    assert_eq!(
        string_at(&shrinker.current_nodes, 0),
        vec![b'a' as u32, b'b' as u32, b'c' as u32]
    );
    assert_eq!(
        string_at(&shrinker.current_nodes, 1),
        vec![b'd' as u32, b'e' as u32]
    );
}

#[test]
fn redistribute_string_pairs_gap_of_two_matches_skip_one_adjacent_strings() {
    // Three strings with a non-string between them exercises the gap=2
    // iteration: position 0 pairs with position 2 (indices after the
    // boolean skip).
    let nodes = vec![
        string_node(0, 10, 0x30, 0x7A, vec![b'a' as u32]),
        bool_node(true),
        string_node(0, 10, 0x30, 0x7A, vec![b'b' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::String(s), ChoiceValue::String(t)) = (&n[0].value, &n[2].value)
            else {
                unreachable!()
            };
            let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
            (combined == vec![b'a' as u32, b'b' as u32], n.to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    assert_eq!(string_at(&shrinker.current_nodes, 0), Vec::<u32>::new());
    assert_eq!(
        string_at(&shrinker.current_nodes, 2),
        vec![b'a' as u32, b'b' as u32]
    );
}

#[test]
fn redistribute_string_pairs_no_op_with_single_string() {
    // Only one string → no pair exists at any gap.
    let nodes = vec![
        bool_node(true),
        string_node(0, 10, 0x30, 0x7A, vec![b'z' as u32]),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.redistribute_string_pairs();
    assert_eq!(string_at(&shrinker.current_nodes, 1), vec![b'z' as u32]);
}

// ── zero_choices ────────────────────────────────────────────────────────────
//
// Exercises each branch of the integer-level zero_choices pass: the empty
// short-circuit, advance-past-simplest, block replacement, and fall-back to
// smaller k when the big block is rejected.

#[test]
fn zero_choices_empty_nodes_is_noop() {
    // len=0 → outer `while k > 0` is immediately false.
    let nodes: Vec<ChoiceNode> = Vec::new();
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.zero_choices();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn zero_choices_replaces_single_block_with_simplest() {
    let nodes = vec![int_node(0, 10, 5)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.zero_choices();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn zero_choices_advances_past_simplest_node() {
    // First node already simplest → `i += 1` branch fires. Second node then
    // gets its own block replacement at k=1.
    let nodes = vec![int_node(0, 10, 0), int_node(0, 10, 5)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.zero_choices();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

#[test]
fn zero_choices_replaces_multi_node_block_simultaneously() {
    // Block of size 2: both replaced in a single step.
    let nodes = vec![int_node(0, 10, 7), int_node(0, 10, 3)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.zero_choices();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

#[test]
fn zero_choices_falls_back_to_smaller_k_when_big_block_rejected() {
    // Predicate requires first int >= 5. The k=2 block tries [0, 0] which
    // fails; `i += k` advances. k halves to 1, and the individual replace
    // succeeds on the second node only.
    let nodes = vec![int_node(0, 10, 8), int_node(0, 10, 3)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v >= 5, n.to_vec())
        }),
        nodes,
    );
    shrinker.zero_choices();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 8);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

// ── swap_integer_sign ──────────────────────────────────────────────────────
//
// Five branches: non-integer skip, already-simplest skip, negative flips to
// positive, negative where -v is out of range, and positive reduced to
// simplest without a flip.

#[test]
fn swap_integer_sign_skips_non_integer_nodes() {
    let nodes = vec![bool_node(true), int_node(-10, 10, -5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[1].value else {
                unreachable!()
            };
            (v != 0, n.to_vec())
        }),
        nodes,
    );
    shrinker.swap_integer_sign();
    assert!(bool_at(&shrinker.current_nodes, 0));
    // simplest=0 rejected, -5 flipped to 5.
    assert_eq!(int_at(&shrinker.current_nodes, 1), 5);
}

#[test]
fn swap_integer_sign_skips_when_already_simplest() {
    // v == simplest → outer replace branch skipped; re-read still runs but
    // v < 0 is false for v=0.
    let nodes = vec![int_node(-10, 10, 0)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.swap_integer_sign();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn swap_integer_sign_negative_flips_to_positive() {
    let nodes = vec![int_node(-10, 10, -5)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v != 0, n.to_vec())
        }),
        nodes,
    );
    shrinker.swap_integer_sign();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 5);
}

#[test]
fn swap_integer_sign_skips_flip_when_negated_out_of_range() {
    // Range [-10, 5], v=-7. -(-7)=7 > 5, so validate(-v)=false; flip skipped.
    let nodes = vec![int_node(-10, 5, -7)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v <= -6, n.to_vec())
        }),
        nodes,
    );
    shrinker.swap_integer_sign();
    assert_eq!(int_at(&shrinker.current_nodes, 0), -7);
}

#[test]
fn swap_integer_sign_positive_reduced_to_simplest() {
    // Positive → simplest accepted. Re-read sees 0 so flip branch is skipped.
    let nodes = vec![int_node(-10, 10, 5)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.swap_integer_sign();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

// ── binary_search_integer_towards_zero ─────────────────────────────────────
//
// Exercises every branch of both sign cases: non-integer skip, v=0 skip,
// positive with small/large ranges, positive with min<0 negative probe,
// positive where the probe is skipped because cur_v≤0 or upper<1, and the
// negative-branch mirrors of each.

#[test]
fn binary_search_integer_skips_non_integer_nodes() {
    let nodes = vec![bool_node(true)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.binary_search_integer_towards_zero();
    assert!(bool_at(&shrinker.current_nodes, 0));
}

#[test]
fn binary_search_integer_skips_value_zero() {
    // v=0: neither v>0 nor v<0 branches fire.
    let nodes = vec![int_node(-10, 10, 0)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn binary_search_integer_positive_small_range_min_non_negative() {
    // Small range (≤128), min≥0. bin search + linear scan with
    // scan_count=min(range,32); negative-probe branch skipped.
    let nodes = vec![int_node(0, 100, 50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v >= 7, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 7);
}

#[test]
fn binary_search_integer_positive_large_range_uses_scan_count_8() {
    // Range > 128: scan_count = 8.
    let nodes = vec![int_node(0, 10_000, 1000)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn binary_search_integer_positive_probes_negatives_when_min_below_zero() {
    // v=10 with min<0. Predicate accepts v=10 or v<=-3. The positive bin
    // search can't shrink 10 (no other positive accepted); the negative
    // probe then reaches -3 via bin_search_down on a=3.
    let nodes = vec![int_node(-100, 100, 10)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v == 10 || v <= -3, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), -3);
}

#[test]
fn binary_search_integer_positive_skips_negative_probe_when_cur_nonpositive() {
    // min<0 but after bin_search cur_v=0. `if cur_v > 0` is false.
    let nodes = vec![int_node(-100, 100, 10)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn binary_search_integer_positive_skips_negative_probe_when_upper_lt_1() {
    // cur_v=1, -min=100. upper = min(cur_v-1, -min) = 0 < 1, so skip.
    let nodes = vec![int_node(-100, 100, 1)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v >= 1, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 1);
}

#[test]
fn binary_search_integer_negative_small_range_max_nonpositive() {
    // Small range, max≤0. v<0 bin search + small-range neg_scan.
    let nodes = vec![int_node(-100, 0, -50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v <= -7, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), -7);
}

#[test]
fn binary_search_integer_negative_large_range_uses_neg_scan_8() {
    // Range > 128: neg_scan = 8 in the else arm.
    let nodes = vec![int_node(-10_000, 0, -1000)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v <= -3, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), -3);
}

#[test]
fn binary_search_integer_negative_probes_positives_when_max_above_zero() {
    // v=-10, max>0. Predicate accepts only {-10, 3}: positive probe finds 3.
    let nodes = vec![int_node(-100, 100, -10)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v == 3 || v == -10, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
}

#[test]
fn binary_search_integer_negative_skips_positive_probe_when_cur_nonnegative() {
    // After bin_search, cur_v=0. `if cur_v < 0` is false; skip positive probe.
    let nodes = vec![int_node(-100, 100, -10)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
}

#[test]
fn binary_search_integer_negative_skips_positive_probe_when_upper_lt_1() {
    // cur_v=-1, max=100. upper = min(-cur_v - 1, max) = 0 < 1, skip.
    let nodes = vec![int_node(-100, 100, -1)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v <= -1, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), -1);
}

#[test]
fn binary_search_integer_negative_positive_probe_large_range() {
    // Negative branch with positive-probe large-range path: scan_count=8.
    let nodes = vec![int_node(-10_000, 10_000, -10)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v == 3 || v == -10, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
}

#[test]
fn binary_search_integer_negative_positive_probe_small_range() {
    // Negative branch, positive-probe small-range (range_size ≤ 128). Covers
    // the `range_size.min(32)` scan_count branch.
    let nodes = vec![int_node(-50, 50, -10)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(v) = n[0].value else {
                unreachable!()
            };
            (v == 3 || v == -10, n.to_vec())
        }),
        nodes,
    );
    shrinker.binary_search_integer_towards_zero();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
}

// ── redistribute_integers ──────────────────────────────────────────────────

#[test]
fn redistribute_integers_noop_with_no_integers() {
    // Empty int_indices → max_gap=0; outer for-loop empty.
    let nodes = vec![bool_node(true), bool_node(false)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.redistribute_integers();
    assert!(bool_at(&shrinker.current_nodes, 0));
    assert!(!bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn redistribute_integers_noop_with_single_integer() {
    // 1 integer → max_gap=1 → `for gap in 1..1` is empty.
    let nodes = vec![int_node(0, 10, 5)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.redistribute_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 5);
}

#[test]
fn redistribute_integers_reduces_positive_pair() {
    // prev_i>0 branch. bin_search pulls value out of a into b, keeping
    // a+b constant at 130.
    let nodes = vec![int_node(0, 200, 80), int_node(0, 200, 50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            (a + b >= 100, n.to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 130);
}

#[test]
fn redistribute_integers_reduces_negative_pair() {
    // prev_i<0 branch. bin_search drives a toward 0 while keeping a+b = -30.
    let nodes = vec![int_node(-200, 200, -80), int_node(-200, 200, 50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            (a + b == -30, n.to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), -30);
}

#[test]
fn redistribute_integers_skips_when_prev_i_at_simplest() {
    // prev_i==simplest: neither prev_i>0 nor prev_i<0 branch fires.
    let nodes = vec![int_node(0, 10, 0), int_node(0, 10, 5)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.redistribute_integers();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 5);
}

#[test]
fn redistribute_integers_walks_reverse_pair_order() {
    // Three positive ints — gap=1 iterates pairs (1,2) then (0,1), exercising
    // the pair_idx decrement branch.
    let nodes = vec![
        int_node(0, 100, 10),
        int_node(0, 100, 20),
        int_node(0, 100, 30),
    ];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.redistribute_integers();
    // Sum preserved (60); progress was made.
    let a = int_at(&shrinker.current_nodes, 0);
    let b = int_at(&shrinker.current_nodes, 1);
    let c = int_at(&shrinker.current_nodes, 2);
    assert_eq!(a + b + c, 60);
    assert!(a <= 10);
}

// ── shrink_duplicates ──────────────────────────────────────────────────────

#[test]
fn shrink_duplicates_noop_without_duplicates() {
    let nodes = vec![int_node(0, 10, 3), int_node(0, 10, 7)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.shrink_duplicates();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 7);
}

#[test]
fn shrink_duplicates_skips_non_integer_kinds() {
    // Two booleans: the outer grouping only collects integer nodes.
    let nodes = vec![bool_node(true), bool_node(true)];
    let mut shrinker = Shrinker::new(
        Box::new(|_: &[ChoiceNode]| panic!("predicate should not be called")),
        nodes,
    );
    shrinker.shrink_duplicates();
    assert!(bool_at(&shrinker.current_nodes, 0));
    assert!(bool_at(&shrinker.current_nodes, 1));
}

#[test]
fn shrink_duplicates_reduces_positive_pair_to_simplest() {
    // Predicate permissive → simplest(0) replaces both simultaneously.
    let nodes = vec![int_node(0, 100, 50), int_node(0, 100, 50)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_duplicates();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

#[test]
fn shrink_duplicates_reduces_negative_pair_to_simplest() {
    let nodes = vec![int_node(-100, 100, -50), int_node(-100, 100, -50)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_duplicates();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

#[test]
fn shrink_duplicates_positive_bin_search_makes_partial_progress() {
    // Predicate requires both equal AND both >= 10: simplest(0) rejected,
    // forcing the positive bin_search branch. After the first successful
    // candidate, cur_value no longer matches current_nodes so the
    // `current_valid.len() < 2 → return false` branch also fires.
    let nodes = vec![int_node(0, 100, 50), int_node(0, 100, 50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            (a == b && *a >= 10, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_duplicates();
    let a = int_at(&shrinker.current_nodes, 0);
    let b = int_at(&shrinker.current_nodes, 1);
    assert_eq!(a, b);
    assert!(a >= 10);
    assert!(a < 50);
}

#[test]
fn shrink_duplicates_negative_bin_search_makes_partial_progress() {
    let nodes = vec![int_node(-100, 100, -50), int_node(-100, 100, -50)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) = (&n[0].value, &n[1].value)
            else {
                unreachable!()
            };
            (a == b && *a <= -10, n.to_vec())
        }),
        nodes,
    );
    shrinker.shrink_duplicates();
    let a = int_at(&shrinker.current_nodes, 0);
    let b = int_at(&shrinker.current_nodes, 1);
    assert_eq!(a, b);
    assert!(a <= -10);
    assert!(a > -50);
}

#[test]
fn shrink_duplicates_skips_bin_search_when_already_simplest() {
    // cur_value==0: neither v>0 nor v<0 branch runs.
    let nodes = vec![int_node(0, 10, 0), int_node(0, 10, 0)];
    let mut shrinker = Shrinker::new(Box::new(|n: &[ChoiceNode]| (true, n.to_vec())), nodes);
    shrinker.shrink_duplicates();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 0);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 0);
}

// ── Additional per-pass regressions ported from pbtkit/test_core.py ─────────

#[test]
fn delete_chunks_guard_fires_after_shortening() {
    // Port of pbtkit/tests/test_core.py::test_delete_chunks_guard_after_decrement.
    // Exercises the `i >= self.current_nodes.len()` guard in delete_chunks:
    // when a deletion succeeds and shortens the result, the loop's i pointer
    // may now be past the new end; the guard bails out cleanly.
    //
    // Upstream shape: `while tc.weighted(0.9) { tc.draw_integer(0, 10) }`
    // with the body counting iterations; interesting iff count >= 5.
    // Initial: 10 (True, value) pairs + trailing False = 21 nodes.
    let mut nodes = Vec::new();
    for v in [7, 3, 0, 5, 2, 8, 1, 4, 6, 9] {
        nodes.push(bool_node(true));
        nodes.push(int_node(0, 10, v));
    }
    nodes.push(bool_node(false));

    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let mut count = 0;
            let mut i = 0;
            while i < n.len() {
                match n[i].value {
                    ChoiceValue::Boolean(true) => {
                        if i + 1 >= n.len() || !matches!(n[i + 1].value, ChoiceValue::Integer(_)) {
                            return (false, n.to_vec());
                        }
                        count += 1;
                        i += 2;
                    }
                    ChoiceValue::Boolean(false) => {
                        i += 1;
                        break;
                    }
                    _ => return (false, n.to_vec()),
                }
            }
            (count >= 5, n[..i].to_vec())
        }),
        nodes,
    );

    shrinker.delete_chunks();
    // Must terminate without panicking and make some progress below 21 nodes.
    assert!(shrinker.current_nodes.len() < 21);
}

#[test]
fn redistribute_integers_handles_stale_indices() {
    // Port of pbtkit/tests/test_core.py::test_redistribute_integers_stale_indices.
    // redistribute_integers must not panic when redistribution between a
    // loop-controlling value and a later value shortens consumption.
    let nodes = vec![
        int_node(2, 8, 4),
        int_node(0, 100, 20),
        int_node(0, 100, 30),
        int_node(0, 100, 25),
        int_node(0, 100, 35),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(count) = n[0].value else {
                return (false, n.to_vec());
            };
            if !(2..=8).contains(&count) {
                return (false, n.to_vec());
            }
            let count = count as usize;
            if n.len() < 1 + count {
                return (false, n.to_vec());
            }
            let sum: i128 = (1..=count)
                .map(|j| match n[j].value {
                    ChoiceValue::Integer(v) => v,
                    _ => 0,
                })
                .sum();
            (sum >= 50, n[..1 + count].to_vec())
        }),
        nodes,
    );
    shrinker.redistribute_integers();
    assert!(shrinker.current_nodes.len() <= 5);
}

#[test]
fn bind_deletion_try_deletions_recovers_interesting() {
    // Port of pbtkit/tests/test_core.py::test_bind_deletion_try_deletions_succeeds.
    // bind_deletion + try_replace_with_deletion recovers an interesting result
    // by deleting excess choices after reducing a loop-count value.
    //
    // Seed: [n=3, 8, 1, 4], sum=13 >= 10 → interesting. Reducing n to 2 alone
    // leaves the low-value node orphaned; deletion of the low-value (1) yields
    // [n=2, 8, 4] sum=12 still interesting.
    let nodes = vec![
        int_node(1, 5, 3),
        int_node(0, 10, 8),
        int_node(0, 10, 1),
        int_node(0, 10, 4),
    ];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let ChoiceValue::Integer(count) = n[0].value else {
                return (false, n.to_vec());
            };
            if !(1..=5).contains(&count) {
                return (false, n.to_vec());
            }
            let count = count as usize;
            if n.len() < 1 + count {
                return (false, n.to_vec());
            }
            let sum: i128 = (1..=count)
                .map(|j| match n[j].value {
                    ChoiceValue::Integer(v) => v,
                    _ => 0,
                })
                .sum();
            (count >= 2 && sum >= 10, n[..1 + count].to_vec())
        }),
        nodes,
    );
    shrinker.bind_deletion();
    assert!(shrinker.current_nodes.len() < 4);
}

#[test]
fn sort_values_full_sort_fails_preserves_order() {
    // Port of pbtkit/tests/test_core.py::test_sort_values_full_sort_fails.
    // sort_values must leave the sequence untouched when sorting would change
    // the test outcome (predicate: first > second; sorting gives the opposite).
    let nodes = vec![int_node(0, 10, 5), int_node(0, 10, 3)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            let (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) = (&n[0].value, &n[1].value)
            else {
                return (false, n.to_vec());
            };
            (a > b, n.to_vec())
        }),
        nodes,
    );
    shrinker.sort_values();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 5);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 3);
}

// ── try_shortening_via_increment ────────────────────────────────────────────

#[test]
fn try_shortening_via_increment_breaks_inner_loop_when_consider_truncates_nodes() {
    // When a candidate triggers an `interesting` result whose actual_nodes
    // is shorter than the index we're iterating at, current_nodes shrinks
    // mid-loop. The next candidate iteration must observe `i >= len` and
    // break out, without indexing past the end.
    //
    // Setup: two integer nodes; the test fn returns interesting with a
    // single-node sequence whenever the second node's value is anything
    // other than its current 0. The first candidate processed for i=1 is
    // therefore interesting+shorter, which shrinks current_nodes to one
    // node. The remaining candidates must hit the i-bounds break at the
    // top of the inner loop.
    let nodes = vec![int_node(0, 1000, 0), int_node(0, 1000, 0)];
    let mut shrinker = Shrinker::new(
        Box::new(|n: &[ChoiceNode]| {
            if n.len() < 2 {
                return (false, n.to_vec());
            }
            let ChoiceValue::Integer(v1) = n[1].value else {
                return (false, n.to_vec());
            };
            if v1 != 0 {
                // Interesting with a single-node sequence — triggers
                // current_nodes truncation inside the inner candidates loop.
                return (true, vec![int_node(0, 1000, 0)]);
            }
            (false, n.to_vec())
        }),
        nodes,
    );
    shrinker.try_shortening_via_increment();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

// ── try_bump_ij defensive guards ────────────────────────────────────────────
//
// `try_bump_ij` is the helper that `lower_and_bump` calls inside its
// fallback exponential-probe loop. Between successive calls in that loop
// `current_nodes` may have been mutated by an earlier `consider`, which
// can leave `j` out of range or change the kind at position `j`. The two
// early-return guards exist to absorb both shapes of staleness — exercise
// them directly with a hand-crafted Shrinker, since reproducing the exact
// fallback-loop sequence through `lower_and_bump` requires very specific
// nested predicates.

use super::index_passes::try_bump_ij;

#[test]
fn try_bump_ij_returns_false_when_j_out_of_bounds() {
    // current_nodes has only one node; calling try_bump_ij with j=1 must
    // short-circuit at the j-bounds guard without touching the
    // out-of-range index (a panic would mean the guard regressed).
    let nodes = vec![int_node(0, 100, 5)];
    let mut shrinker = Shrinker::new(
        Box::new(|_n: &[ChoiceNode]| panic!("test_fn must not be called when j is out of bounds")),
        nodes,
    );
    let result = try_bump_ij(
        &mut shrinker,
        0,
        &ChoiceValue::Integer(0),
        1,
        &ChoiceValue::Integer(7),
    );
    assert!(!result);
    // Sanity: current_nodes wasn't touched.
    assert_eq!(shrinker.current_nodes.len(), 1);
    assert_eq!(int_at(&shrinker.current_nodes, 0), 5);
}

#[test]
fn try_bump_ij_returns_false_when_bump_val_does_not_validate() {
    // bump_val is a Boolean but the kind at j is Integer; validate must
    // reject it before any test-fn call.
    let nodes = vec![int_node(0, 100, 5), int_node(0, 100, 5)];
    let mut shrinker = Shrinker::new(
        Box::new(|_n: &[ChoiceNode]| {
            panic!("test_fn must not be called when bump_val fails validate")
        }),
        nodes,
    );
    let result = try_bump_ij(
        &mut shrinker,
        0,
        &ChoiceValue::Integer(0),
        1,
        &ChoiceValue::Boolean(true),
    );
    assert!(!result);
}
