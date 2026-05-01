//! Ported from hypothesis-python/tests/conjecture/test_shrinker.py
//!
//! Exercises the native shrinker (`src/native/shrinker/`) on small hand-built
//! choice sequences. The Python original uses an `@shrinking_from(initial)`
//! fixture that runs a ConjectureRunner, caches the starting choice sequence,
//! and builds a `Shrinker` from it; here the local [`shrinking_from`] helper
//! skips the runner and goes directly to `NativeTestCase::for_choices` +
//! `Shrinker::new`, which is enough to exercise the shrink pipeline.
//!
//! Python `data.start_span(label)` / `data.stop_span()` brackets are
//! reproduced via the local [`with_span`] helper, which captures the pre- and
//! post-draw positions on `NativeTestCase.nodes` and calls `record_span` after
//! the body runs. The native shrinker's passes don't consume span metadata
//! (unlike Hypothesis's `pass_to_descendant` / `reorder_spans`), so these
//! recorded spans are faithfulness-only; the tests still exercise the shrink
//! pipeline end-to-end.
//!
//! Individually-skipped tests:
//!
//! - `test_can_pass_to_an_indirect_descendant`,
//!   `test_can_reorder_spans`,
//!   `test_dependent_block_pairs_is_up_to_shrinking_integers`,
//!   `test_zig_zags_quickly_with_shrink_towards` (all 4 parametrize rows),
//!   `test_can_simultaneously_lower_non_duplicated_nearby_integers` (3
//!   parametrize rows), `test_redistribute_with_forced_node_integer`,
//!   `test_redistribute_numeric_pairs`,
//!   `test_lower_duplicated_characters_across_choices` (8 parametrize rows),
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_integer`,
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_float`,
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_combined`,
//!   `test_redistribute_numeric_pairs_shrink_towards_integer` — each
//!   depends on a Python-only feature or engine internal not yet in the
//!   native backend: a `shrink_towards` constraint on `draw_integer`,
//!   `forced=` on `draw_integer`, `stop_span(discard=True)` semantics
//!   that the native shrinker would have to consult for descendant-
//!   passing / reorder-spans, or `Sampler` for block-distribution.
//!   (The other fixate-on-named-pass tests in this file, like
//!   `test_can_shrink_variable_draws_with_just_deletion`, port cleanly
//!   by running `Shrinker::shrink()` end-to-end; the full pipeline
//!   converges on the same minimum as the single pass the Python
//!   original fixates on.)
//!
//! - `test_deletion_and_lowering_fails_to_shrink`,
//!   `test_permits_but_ignores_raising_order` — monkey-patch
//!   `ConjectureRunner.generate_new_examples` / `Shrinker.shrink` to control
//!   the engine's first example and shrink path. No monkey-patching entry
//!   point in the native engine.
//!
//! - `test_node_programs_are_adaptive`,
//!   `test_will_let_fixate_shrink_passes_do_a_full_run_through` — use
//!   `shrinker.node_program("X" * i)` (adaptive deletion pass) or the
//!   `StopShrinking` / `max_stall` control surface. Neither the adaptive
//!   node-program pass nor the `max_stall`/`StopShrinking` API exists in
//!   the native shrinker.
//!
//! - `test_will_terminate_stalled_shrinks` — asserts
//!   `shrinker.calls <= 1 + 2 * shrinker.max_stall`; native `Shrinker` has
//!   no `calls` counter or `max_stall` knob. (The termination behaviour is
//!   covered by `MAX_SHRINK_ITERATIONS` which has no equivalent assertion
//!   hook.)
//!
//! - `test_alternative_shrinking_will_lower_to_alternate_value` — calls
//!   `shrinker.initial_coarse_reduction()`, a Python-specific
//!   coarse-grained pre-pass. The asserted final state depends on the
//!   pre-pass discovering an alternate interesting origin via stateful
//!   scratch, which the full `Shrinker::shrink()` pipeline doesn't
//!   trigger from the initial input.
//!
//! - `test_silly_shrinker_subclass` — subclasses the generic base-class
//!   `hypothesis.internal.conjecture.shrinking.common.Shrinker` with a
//!   no-op `run_step`. Hegel's value-shrinker ports (`IntegerShrinker`,
//!   `OrderingShrinker`) are concrete structs with fixed `run_step`
//!   implementations and no subclass-pluggable base class.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{ChoiceNode, ChoiceValue, NativeTestCase, Shrinker};

const SOME_LABEL: &str = "label";

/// Build a Shrinker that replays `initial` through `user_test_fn`, then
/// shrinks. `user_test_fn` returns `true` to signal "mark_interesting".
fn shrinking_from<F>(initial: Vec<ChoiceValue>, user_test_fn: F) -> Shrinker<'static>
where
    F: FnMut(&mut NativeTestCase) -> bool + 'static,
{
    let mut user_test_fn = user_test_fn;

    let mut ntc = NativeTestCase::for_choices(&initial, None, None);
    let is_interesting = user_test_fn(&mut ntc);
    assert!(
        is_interesting,
        "initial choices did not trigger mark_interesting"
    );
    let initial_nodes = ntc.nodes.clone();

    let test_fn = Box::new(move |candidate: &[ChoiceNode]| {
        let values: Vec<ChoiceValue> = candidate.iter().map(|n| n.value.clone()).collect();
        let mut ntc = NativeTestCase::for_choices(&values, Some(candidate), None);
        let is_interesting = user_test_fn(&mut ntc);
        (is_interesting, ntc.nodes)
    });

    Shrinker::new(test_fn, initial_nodes)
}

fn integer_choices(values: &[i128]) -> Vec<ChoiceValue> {
    values.iter().map(|&v| ChoiceValue::Integer(v)).collect()
}

fn boolean_choices(values: &[bool]) -> Vec<ChoiceValue> {
    values.iter().map(|&v| ChoiceValue::Boolean(v)).collect()
}

fn extract_integers(nodes: &[ChoiceNode]) -> Vec<i128> {
    nodes
        .iter()
        .map(|n| match n.value {
            ChoiceValue::Integer(v) => v,
            ref other => panic!("expected integer, got {other:?}"),
        })
        .collect()
}

fn extract_booleans(nodes: &[ChoiceNode]) -> Vec<bool> {
    nodes
        .iter()
        .map(|n| match n.value {
            ChoiceValue::Boolean(v) => v,
            ref other => panic!("expected boolean, got {other:?}"),
        })
        .collect()
}

/// Record a span covering the draws made inside `body` (mirrors the Python
/// `data.start_span(label)` / `data.stop_span()` bracket). The native shrinker
/// doesn't consume span metadata, but recording spans keeps the test body
/// faithful to the upstream shape.
fn with_span<F, R>(tc: &mut NativeTestCase, label: &str, body: F) -> R
where
    F: FnOnce(&mut NativeTestCase) -> R,
{
    let start = tc.nodes.len();
    let result = body(tc);
    let end = tc.nodes.len();
    tc.record_span(start, end, label.to_string());
    result
}

#[test]
fn test_retain_end_of_buffer() {
    let mut shrinker = shrinking_from(integer_choices(&[1, 2, 3, 4, 5, 6, 0]), |tc| {
        let mut interesting = false;
        loop {
            let n = match tc.draw_integer(0, 255) {
                Ok(v) => v,
                Err(_) => return false,
            };
            if n == 6 {
                interesting = true;
            }
            if n == 0 {
                break;
            }
        }
        interesting
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![6, 0]);
}

#[test]
fn test_can_expand_zeroed_region() {
    let mut shrinker = shrinking_from(integer_choices(&[255; 5]), |tc| {
        let mut seen_non_zero = false;
        for _ in 0..5 {
            let v = match tc.draw_integer(0, 255) {
                Ok(v) => v,
                Err(_) => return false,
            };
            if v == 0 {
                if seen_non_zero {
                    return false;
                }
            } else {
                seen_non_zero = true;
            }
        }
        true
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![0; 5]);
}

#[test]
fn test_can_zero_subintervals() {
    let initial: Vec<i128> = (0..10).flat_map(|_| [3, 0, 0, 0, 1]).collect();
    let mut shrinker = shrinking_from(integer_choices(&initial), |tc| {
        for _ in 0..10 {
            let early_return = with_span(tc, SOME_LABEL, |tc| {
                let n = match tc.draw_integer(0, 255) {
                    Ok(v) => v,
                    Err(_) => return true,
                };
                for _ in 0..n {
                    if tc.draw_integer(0, 255).is_err() {
                        return true;
                    }
                }
                false
            });
            if early_return {
                return false;
            }
            match tc.draw_integer(0, 255) {
                Ok(v) if v != 1 => return false,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
        true
    });
    shrinker.shrink();
    let expected: Vec<i128> = (0..10).flat_map(|_| [0, 1]).collect();
    assert_eq!(extract_integers(&shrinker.current_nodes), expected);
}

#[test]
fn test_zero_examples_with_variable_min_size() {
    let mut shrinker = shrinking_from(integer_choices(&[255; 100]), |tc| {
        let mut any_nonzero = false;
        for i in 1..10 {
            let v = match tc.draw_integer(0, (1i128 << i) - 1) {
                Ok(v) => v,
                Err(_) => return false,
            };
            if v > 0 {
                any_nonzero = true;
            }
        }
        any_nonzero
    });
    shrinker.shrink();
    let mut expected = vec![0i128; 8];
    expected.push(1);
    assert_eq!(extract_integers(&shrinker.current_nodes), expected);
}

#[test]
fn test_zero_contained_examples() {
    let mut shrinker = shrinking_from(integer_choices(&[1; 8]), |tc| {
        for _ in 0..4 {
            let outer_invalid = with_span(tc, SOME_LABEL, |tc| {
                let v = match tc.draw_integer(0, 255) {
                    Ok(v) => v,
                    Err(_) => return true,
                };
                if v == 0 {
                    return true;
                }
                with_span(tc, SOME_LABEL, |tc| tc.draw_integer(0, 255).is_err())
            });
            if outer_invalid {
                return false;
            }
        }
        true
    });
    shrinker.shrink();
    let expected: Vec<i128> = (0..4).flat_map(|_| [1, 0]).collect();
    assert_eq!(extract_integers(&shrinker.current_nodes), expected);
}

#[test]
fn test_zero_irregular_examples() {
    let mut shrinker = shrinking_from(integer_choices(&[255; 6]), |tc| {
        let first_err = with_span(tc, SOME_LABEL, |tc| {
            tc.draw_integer(0, 255).is_err() || tc.draw_integer(0, 65535).is_err()
        });
        if first_err {
            return false;
        }
        let (a, b) = with_span(tc, SOME_LABEL, |tc| {
            let a = match tc.draw_integer(0, 255) {
                Ok(v) => v,
                Err(_) => return (None, None),
            };
            let b = match tc.draw_integer(0, 65535) {
                Ok(v) => v,
                Err(_) => return (Some(a), None),
            };
            (Some(a), Some(b))
        });
        match (a, b) {
            (Some(a), Some(b)) => a > 0 && b > 0,
            _ => false,
        }
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![0, 0, 1, 1]);
}

#[test]
fn test_can_expand_deleted_region() {
    let mut shrinker = shrinking_from(integer_choices(&[1, 2, 3, 4, 0, 0]), |tc| {
        fn t(tc: &mut NativeTestCase) -> Option<(i128, i128)> {
            with_span(tc, SOME_LABEL, |tc| {
                let m = with_span(tc, SOME_LABEL, |tc| tc.draw_integer(0, 255)).ok()?;
                let n = with_span(tc, SOME_LABEL, |tc| tc.draw_integer(0, 255)).ok()?;
                Some((m, n))
            })
        }
        let v1 = match t(tc) {
            Some(v) => v,
            None => return false,
        };
        if v1 == (1, 2) {
            match t(tc) {
                Some(v) if v != (3, 4) => return false,
                None => return false,
                _ => {}
            }
        }
        if v1 == (0, 0) {
            return true;
        }
        matches!(t(tc), Some((0, 0)))
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![0, 0]);
}

#[test]
fn test_finding_a_minimal_balanced_binary_tree() {
    fn tree(tc: &mut NativeTestCase) -> Option<(u32, bool)> {
        with_span(tc, SOME_LABEL, |tc| {
            let branch = tc.weighted(0.5, None).ok()?;
            if !branch {
                Some((1, true))
            } else {
                let (h1, b1) = tree(tc)?;
                let (h2, b2) = tree(tc)?;
                Some((1 + h1.max(h2), b1 && b2 && h1.abs_diff(h2) <= 1))
            }
        })
    }
    // Starting from an unbalanced tree of depth six: five True then six False.
    let mut initial = vec![true; 5];
    initial.extend(std::iter::repeat_n(false, 6));
    let mut shrinker = shrinking_from(boolean_choices(&initial), |tc| match tree(tc) {
        Some((_, balanced)) => !balanced,
        None => false,
    });
    shrinker.shrink();
    assert_eq!(
        extract_booleans(&shrinker.current_nodes),
        vec![true, false, true, false, true, false, false]
    );
}

#[test]
fn test_can_shrink_variable_draws_with_just_deletion() {
    // Python parametrizes n in [1, 5, 8, 15]; the Python original fixates on
    // `minimize_individual_choices`. The full native pipeline includes
    // deletion + integer shrinking, which converges on the same minimum.
    for n in [1usize, 5, 8, 15] {
        let mut initial = vec![n as i128];
        initial.extend(std::iter::repeat_n(0i128, n - 1));
        initial.push(1);
        let mut shrinker = shrinking_from(integer_choices(&initial), |tc| {
            let k = match tc.draw_integer(0, (1i128 << 4) - 1) {
                Ok(v) => v as usize,
                Err(_) => return false,
            };
            let mut bs = Vec::with_capacity(k);
            for _ in 0..k {
                match tc.draw_integer(0, 255) {
                    Ok(v) => bs.push(v),
                    Err(_) => return false,
                }
            }
            bs.iter().any(|&v| v != 0)
        });
        shrinker.shrink();
        assert_eq!(
            extract_integers(&shrinker.current_nodes),
            vec![1i128, 1],
            "n = {n}"
        );
    }
}

#[test]
fn test_handle_empty_draws() {
    // Python uses `run_to_nodes` to let ConjectureRunner find the initial
    // interesting case; we seed `(1, 0)` which exercises the same body
    // (discarded first iteration, then n=0 break).
    let mut shrinker = shrinking_from(integer_choices(&[1, 0]), |tc| {
        loop {
            let n = match tc.draw_integer(0, 1) {
                Ok(v) => v,
                Err(_) => return false,
            };
            if n > 0 {
                tc.has_discards = true;
            }
            if n == 0 {
                return true;
            }
        }
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![0]);
}

#[test]
fn test_zig_zags_quickly() {
    // Python fixates on `minimize_individual_choices` and additionally
    // asserts `shrinker.engine.valid_examples <= 100`; our native
    // `Shrinker` has no valid_examples counter, so we drop that clause
    // and keep the minimum-choice assertion.
    let mut shrinker = shrinking_from(integer_choices(&[255; 4]), |tc| {
        let m = match tc.draw_integer(0, 65535) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let n = match tc.draw_integer(0, 65535) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if m == 0 || n == 0 {
            return false;
        }
        (m - n).abs() <= 1 || (m - n).abs() <= 10
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![1, 1]);
}

#[test]
fn test_can_quickly_shrink_to_trivial_collection() {
    // Python parametrizes n in [10, 50, 100, 200] and fixates on
    // `minimize_individual_choices`; we run the full pipeline and drop the
    // incidental `shrinker.calls < 10` assertion. Python's
    // `data.draw_bytes()` with no size uses a default max; hegel-rust
    // requires explicit bounds so we cap at 200 (the largest n in the
    // parametrize).
    for n in [10usize, 50, 100, 200] {
        let initial = vec![ChoiceValue::Bytes(vec![1u8; n])];
        let mut shrinker = shrinking_from(initial, move |tc| match tc.draw_bytes(0, 200) {
            Ok(b) => b.len() >= n,
            Err(_) => false,
        });
        shrinker.shrink();
        let actual: Vec<ChoiceValue> = shrinker
            .current_nodes
            .iter()
            .map(|node| node.value.clone())
            .collect();
        assert_eq!(actual, vec![ChoiceValue::Bytes(vec![0u8; n])], "n = {n}");
    }
}

#[test]
fn test_shrinking_blocks_from_common_offset() {
    // Python calls `shrinker.mark_changed(i)` / `shrinker.lower_common_node_offset()`
    // directly; the native `Shrinker` doesn't expose those mutator methods,
    // but the full pipeline's alternating `binary_search_integer_towards_zero`
    // walks `(11, 10)` → `(9, 10)` → `(9, 8)` → … → `(1, 0)` (or `(0, 1)`).
    let mut shrinker = shrinking_from(integer_choices(&[11, 10]), |tc| {
        let m = match tc.draw_integer(0, 255) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let n = match tc.draw_integer(0, 255) {
            Ok(v) => v,
            Err(_) => return false,
        };
        (m - n).abs() <= 1 && m.max(n) > 0
    });
    shrinker.shrink();
    let result = extract_integers(&shrinker.current_nodes);
    assert!(
        result == vec![0i128, 1] || result == vec![1i128, 0],
        "unexpected result: {result:?}"
    );
}

#[test]
fn test_node_deletion_can_delete_short_ranges() {
    // Python fixates on `[node_program("X" * i) for i in range(1, 5)]`
    // (contiguous-chunk deletion of sizes 1..=4); the native pipeline's
    // `delete_chunks` pass achieves the same contiguous deletion and the
    // full pipeline converges on the `(4,) * 5` block.
    let mut initial: Vec<i128> = Vec::new();
    for i in 0..5i128 {
        for _ in 0..=i {
            initial.push(i);
        }
    }
    let mut shrinker = shrinking_from(integer_choices(&initial), |tc| {
        loop {
            let n = match tc.draw_integer(0, 65535) {
                Ok(v) => v,
                Err(_) => return false,
            };
            for _ in 0..n {
                match tc.draw_integer(0, 65535) {
                    Ok(v) if v != n => return false,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
            if n == 4 {
                return true;
            }
        }
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![4i128; 5]);
}

#[test]
fn test_shrinking_one_of_with_same_shape() {
    // Python fixates on `shrinker.initial_coarse_reduction()`; the asserted
    // final sequence `(1, 0)` is the initial value (already minimal, since
    // `n` must be 1 for interesting and the second draw is already 0), so
    // the full native pipeline is a strict superset that still preserves
    // it. Python's second `data.draw_integer()` is unbounded; hegel-rust
    // requires a concrete max, so we use a wide i32 range.
    let initial = integer_choices(&[1, 0]);
    let mut shrinker = shrinking_from(initial, |tc| {
        let n = match tc.draw_integer(0, 1) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if tc.draw_integer(i32::MIN as i128, i32::MAX as i128).is_err() {
            return false;
        }
        n == 1
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![1, 0]);
}

#[test]
fn test_duplicate_nodes_that_go_away() {
    // Python uses `draw_integer(min_value=0)` with no upper bound; hegel-rust
    // requires a concrete max, so we cap at 2^24 which comfortably holds the
    // initial 1234567. Python fixates on `minimize_duplicated_choices`; the
    // full native pipeline's `shrink_duplicates` pass drives x=y=0 together,
    // at which point the trailing 135-byte prefix goes unread.
    let mut initial = vec![ChoiceValue::Integer(1234567), ChoiceValue::Integer(1234567)];
    initial.extend(std::iter::repeat_n(
        ChoiceValue::Bytes(vec![1]),
        (1234567 & 255) as usize,
    ));
    let mut shrinker = shrinking_from(initial, |tc| {
        let x = match tc.draw_integer(0, (1i128 << 24) - 1) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let y = match tc.draw_integer(0, (1i128 << 24) - 1) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if x != y {
            return false;
        }
        let mut bs = Vec::new();
        for _ in 0..(x & 255) {
            match tc.draw_bytes(1, 1) {
                Ok(v) => bs.push(v),
                Err(_) => return false,
            }
        }
        bs.iter().collect::<std::collections::HashSet<_>>().len() <= 1
    });
    shrinker.shrink();
    assert_eq!(extract_integers(&shrinker.current_nodes), vec![0, 0]);
}

#[test]
fn test_accidental_duplication() {
    let mut initial = vec![ChoiceValue::Integer(12), ChoiceValue::Integer(12)];
    initial.extend(std::iter::repeat_n(ChoiceValue::Bytes(vec![2]), 12));
    let mut shrinker = shrinking_from(initial, |tc| {
        let x = match tc.draw_integer(0, 255) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let y = match tc.draw_integer(0, 255) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if x != y || x < 5 {
            return false;
        }
        let mut bs = Vec::with_capacity(x as usize);
        for _ in 0..x {
            match tc.draw_bytes(1, 1) {
                Ok(v) => bs.push(v),
                Err(_) => return false,
            }
        }
        bs.iter().collect::<std::collections::HashSet<_>>().len() == 1
    });
    shrinker.shrink();
    let mut expected = vec![ChoiceValue::Integer(5), ChoiceValue::Integer(5)];
    expected.extend(std::iter::repeat_n(ChoiceValue::Bytes(vec![0]), 5));
    let actual: Vec<ChoiceValue> = shrinker
        .current_nodes
        .iter()
        .map(|n| n.value.clone())
        .collect();
    assert_eq!(actual, expected);
}
