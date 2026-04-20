use super::*;
use crate::native::core::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice,
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
            let ok = b == &[2u8, 3, 1]
                || b == &[2u8, 1, 3]
                || b == &[1u8, 2, 3];
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
    let nodes = vec![
        bool_node(true),
        float_node(unbounded_float(), 4.5),
    ];
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
            (f >= 2.0 && f <= 5.0 && f.fract() != 0.0, 1)
        }),
        nodes,
    );
    shrinker.shrink_floats();
    let v = float_at(&shrinker.current_nodes, 0);
    assert!(v >= 2.0 && v <= 5.0);
    assert!(v.fract() != 0.0);
}
