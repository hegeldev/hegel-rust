//! Shrinker-level efficiency tests: drive the native `Shrinker` directly
//! with bounded `max_improvements` to assert that the shrinker reaches the
//! true minimum within an O(log) shrink budget.  Bypasses the engine and
//! generator layers so the bound measures shrinker work alone.

#![cfg(feature = "native")]

use hegel::__test_internals::{
    ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice, ShrinkRun, Shrinker,
};

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: min,
            max_value: max,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn int_val(node: &ChoiceNode) -> i128 {
    match node.value {
        ChoiceValue::Integer(v) => v,
        _ => panic!("expected Integer choice, got {:?}", node.value),
    }
}

fn run_shrinker(
    initial: Vec<ChoiceNode>,
    max_improvements: usize,
    mut interesting: impl FnMut(&[ChoiceNode]) -> bool + 'static,
) -> Shrinker<'static> {
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun| match run {
            ShrinkRun::Full(nodes) => (interesting(nodes), nodes.to_vec()),
            ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.max_improvements = Some(max_improvements);
    shrinker.shrink();
    shrinker
}

/// Variant for tests that need to model "the generator drew fewer nodes
/// than the candidate had" — the returned `actual_nodes` is the prefix
/// the test actually consumed.
fn run_shrinker_with_consumed(
    initial: Vec<ChoiceNode>,
    max_improvements: usize,
    mut probe: impl FnMut(&[ChoiceNode]) -> (bool, usize) + 'static,
) -> Shrinker<'static> {
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun| match run {
            ShrinkRun::Full(nodes) => {
                let (interesting, consumed) = probe(nodes);
                let consumed = consumed.min(nodes.len());
                (interesting, nodes[..consumed].to_vec())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.max_improvements = Some(max_improvements);
    shrinker.shrink();
    shrinker
}

// ── 2-integer zigzag: |m - n| == 1 ────────────────────────────────────────
//
// Both individual moves break the predicate.  Joint motion (m-k, n-k)
// preserves it.  `lower_integers_together` covers adjacent integer
// pairs (gap ≤ 3 in integer-index space), so this pair should reach
// (0, 1) in O(log(max)) successful shrinks.

#[test]
fn zigzag_two_ints_converges() {
    let max_v = 1_000_000i128;
    let initial = vec![int_node(0, max_v, 100_000), int_node(0, max_v, 100_001)];
    let shrinker = run_shrinker(initial, 100, |nodes| {
        nodes.len() == 2 && (int_val(&nodes[0]) - int_val(&nodes[1])).unsigned_abs() == 1
    });
    assert_eq!(
        (
            int_val(&shrinker.current_nodes[0]),
            int_val(&shrinker.current_nodes[1])
        ),
        (0, 1),
        "improvements = {}",
        shrinker.improvements,
    );
}

// ── 2-integer zigzag with lower bound on m ────────────────────────────────
//
// m floored at lb=500: the joint pair must hug the floor.

#[test]
fn zigzag_two_ints_lower_bound_converges() {
    let max_v = 1_000_000i128;
    let lb = 500i128;
    let initial = vec![int_node(lb, max_v, 100_000), int_node(0, max_v, 100_001)];
    let shrinker = run_shrinker(initial, 100, move |nodes| {
        if nodes.len() != 2 {
            return false;
        }
        let m = int_val(&nodes[0]);
        let n = int_val(&nodes[1]);
        m >= lb && (m - n).unsigned_abs() == 1
    });
    let m = int_val(&shrinker.current_nodes[0]);
    let n = int_val(&shrinker.current_nodes[1]);
    assert_eq!(m, lb);
    assert!(n == lb - 1 || n == lb + 1, "got m={m}, n={n}");
}

// ── 5-integer chained zigzag ──────────────────────────────────────────────
//
// Five integers (a, b, c, d, e) with |a-b|==1 ∧ |b-c|==1 ∧ |c-d|==1 ∧
// |d-e|==1.  All five are linked.  `lower_integers_together` walks
// integer-index pairs at gap ≤ 3, so it can pair (a, b), (b, c), …
// (d, e) but not the full quintuple at once.  This stresses whether the
// pairwise pass propagates joint motion through the chain in a
// reasonable number of shrinks.

#[test]
fn zigzag_five_ints_chained_converges() {
    let max_v = 100_000i128;
    let initial = vec![
        int_node(0, max_v, 10_000),
        int_node(0, max_v, 10_001),
        int_node(0, max_v, 10_002),
        int_node(0, max_v, 10_001),
        int_node(0, max_v, 10_002),
    ];
    let shrinker = run_shrinker(initial, 200, |nodes| {
        nodes.len() == 5
            && (int_val(&nodes[0]) - int_val(&nodes[1])).unsigned_abs() == 1
            && (int_val(&nodes[1]) - int_val(&nodes[2])).unsigned_abs() == 1
            && (int_val(&nodes[2]) - int_val(&nodes[3])).unsigned_abs() == 1
            && (int_val(&nodes[3]) - int_val(&nodes[4])).unsigned_abs() == 1
    });
    // Joint lowering should pull the whole chain down to a staircase
    // hugging 0; the per-element max is 1 (any chain with `|diff|==1`
    // and a 0 anywhere is two values, alternating).  Reaching the
    // canonical `(0, 1, 0, 1, 0)` representative additionally needs a
    // five-node parity flip the existing passes can't synthesise, so
    // we accept either parity here.
    let vs: Vec<i128> = shrinker.current_nodes.iter().map(int_val).collect();
    let max_v = vs.iter().copied().max().unwrap();
    assert!(
        max_v <= 1,
        "chain not lowered enough: {vs:?}, improvements = {}",
        shrinker.improvements,
    );
}

// ── shrink_duplicates with rich predicate ─────────────────────────────────
//
// Three identical integer nodes with a non-monotone predicate: only
// values with a specific bit pattern + size threshold pass.
// `shrink_duplicates` lowers them jointly via pure binary search; an
// `Integer.shrink`-style pass would additionally try shift_right and
// mask_high_bits.

#[test]
fn shrink_duplicates_three_copies_converges() {
    let max_v = 1_000_000i128;
    let initial = vec![
        int_node(0, max_v, 100_000),
        int_node(0, max_v, 100_000),
        int_node(0, max_v, 100_000),
    ];
    let shrinker = run_shrinker(initial, 50, |nodes| {
        if nodes.len() != 3 {
            return false;
        }
        let a = int_val(&nodes[0]);
        let b = int_val(&nodes[1]);
        let c = int_val(&nodes[2]);
        a == b && b == c && a > 0
    });
    assert_eq!(
        (
            int_val(&shrinker.current_nodes[0]),
            int_val(&shrinker.current_nodes[1]),
            int_val(&shrinker.current_nodes[2])
        ),
        (1, 1, 1),
        "improvements = {}",
        shrinker.improvements,
    );
}

// ── try_shortening_via_increment for a float-kind node ───────────────────
//
// When a float-kind node's index-space exponential probes
// (curr_idx + 1, +2, +4, +8, +16) and `max_index` haven't found a
// shorter-sequence path, the powers-of-2 raw-magnitude fallback tries
// ±1, ±2, …, ±1024.  On main those candidates are constructed as
// `ChoiceValue::Integer(±m)` and rejected by `kind.validate` against a
// Float kind, so the fallback contributes nothing for float-typed
// nodes.  This test sets up a node whose only "shorter sequence" reachable
// path requires reaching one of those powers-of-2 magnitudes as a float.

#[test]
fn try_shortening_via_increment_float() {
    // Single Float node, initial value -90.0 (satisfies predicate
    // f < -86.0, but in a regime where the additional draw at offset 1
    // would still be made).  We seed a second node representing that
    // "extra" draw; the test predicate counts the sequence length to
    // detect when shortening succeeded.
    //
    // Wrap-up after dispatching to try_shortening_via_increment: if the
    // pass replaces nodes[0] with a float of |f| >= 100 (e.g. -128.0
    // from the powers-of-2 magnitude probe), the test function returns
    // a shorter actual_nodes (just `[f_node]` without the extra), and
    // the shrinker's sort_key compares it favourably.
    let f_node = ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: -1e9,
            max_value: 1e9,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(-90.0),
        was_forced: false,
    };
    let extra_bool_node = ChoiceNode {
        kind: ChoiceKind::Boolean(hegel::__test_internals::BooleanChoice),
        value: ChoiceValue::Boolean(false),
        was_forced: false,
    };

    let initial = vec![f_node.clone(), extra_bool_node];
    let shrinker = run_shrinker_with_consumed(initial, 200, move |nodes| {
        let Some(f_choice) = nodes.first() else {
            return (false, 0);
        };
        let f = match f_choice.value {
            ChoiceValue::Float(v) => v,
            _ => return (false, 0),
        };
        if f >= -86.0 {
            return (false, 1);
        }
        // Mirror a real generator that draws a follow-up boolean only
        // when |f| < 100, then short-circuits otherwise.  Reaching a
        // |f| >= 100 magnitude (e.g. -128.0 from the powers-of-2
        // fallback in `try_shortening_via_increment`) lops off the
        // trailing boolean and produces a strictly shorter — therefore
        // simpler — choice sequence.  When |f| < 100 but the candidate
        // doesn't carry a second node, the replay would fail to draw
        // and the test is not interesting (mirroring an OverRun).
        if f.abs() < 100.0 {
            if nodes.len() >= 2 {
                (true, 2)
            } else {
                (false, 1)
            }
        } else {
            (true, 1)
        }
    });
    let f = match shrinker.current_nodes[0].value {
        ChoiceValue::Float(v) => v,
        _ => panic!("expected Float"),
    };
    // The shrunk sequence should reach the |f| >= 100 short-sequence
    // regime.
    assert!(
        f.abs() >= 100.0,
        "expected |f| >= 100 (short-sequence regime), got f={f}, improvements = {}",
        shrinker.improvements,
    );
}
