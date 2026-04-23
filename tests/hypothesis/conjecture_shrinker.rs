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
//! - `test_can_shrink_variable_draws_with_just_deletion`,
//!   `test_duplicate_nodes_that_go_away`, `test_accidental_duplication`,
//!   `test_can_pass_to_an_indirect_descendant`,
//!   `test_shrinking_blocks_from_common_offset`, `test_can_reorder_spans`,
//!   `test_dependent_block_pairs_is_up_to_shrinking_integers`,
//!   `test_zig_zags_quickly`,
//!   `test_zig_zags_quickly_with_shrink_towards` (all 4 parametrize rows),
//!   `test_can_simultaneously_lower_non_duplicated_nearby_integers` (3
//!   parametrize rows), `test_redistribute_with_forced_node_integer`,
//!   `test_can_quickly_shrink_to_trivial_collection` (4 parametrize rows),
//!   `test_redistribute_numeric_pairs`,
//!   `test_lower_duplicated_characters_across_choices` (8 parametrize rows),
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_integer`,
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_float`,
//!   `test_redistribute_numeric_pairs_shrink_towards_explicit_combined`,
//!   `test_redistribute_numeric_pairs_shrink_towards_integer` —
//!   all fixate on a single named Python shrink pass
//!   (`minimize_individual_choices`, `minimize_duplicated_choices`,
//!   `pass_to_descendant`, `lower_common_node_offset`, `reorder_spans`,
//!   `redistribute_numeric_pairs`, `lower_integers_together`,
//!   `lower_duplicated_characters`) via `fixate_shrink_passes` or a direct
//!   method call. Hegel's native shrinker only exposes `Shrinker::shrink()`
//!   (the full pipeline); its individual passes are `pub(super)` and the pass
//!   names differ (`binary_search_integer_towards_zero`,
//!   `redistribute_integers`, `shrink_duplicates`, `redistribute_string_pairs`,
//!   `swap_adjacent_blocks`, …) so asserting "exactly this pass did exactly
//!   that much" is not portable without exposing the pass API.
//!
//! - `test_deletion_and_lowering_fails_to_shrink`,
//!   `test_permits_but_ignores_raising_order` — monkey-patch
//!   `ConjectureRunner.generate_new_examples` / `Shrinker.shrink` to control
//!   the engine's first example and shrink path. No monkey-patching entry
//!   point in the native engine.
//!
//! - `test_handle_empty_draws`, `test_node_deletion_can_delete_short_ranges`,
//!   `test_node_programs_are_adaptive`,
//!   `test_will_let_fixate_shrink_passes_do_a_full_run_through` — use
//!   `node_program("X" * i)` / `run_to_nodes`, or the `StopShrinking`
//!   / `max_stall` control surface. Neither the adaptive node-program pass
//!   nor the `max_stall`/`StopShrinking` API exists in the native shrinker.
//!
//! - `test_will_terminate_stalled_shrinks` — asserts
//!   `shrinker.calls <= 1 + 2 * shrinker.max_stall`; native `Shrinker` has
//!   no `calls` counter or `max_stall` knob. (The termination behaviour is
//!   covered by `MAX_SHRINK_ITERATIONS` which has no equivalent assertion
//!   hook.)
//!
//! - `test_alternative_shrinking_will_lower_to_alternate_value`,
//!   `test_shrinking_one_of_with_same_shape` — call
//!   `shrinker.initial_coarse_reduction()`, a Python-specific coarse-grained
//!   pre-pass with no native counterpart.
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

    let mut ntc = NativeTestCase::for_choices(&initial, None);
    let is_interesting = user_test_fn(&mut ntc);
    assert!(
        is_interesting,
        "initial choices did not trigger mark_interesting"
    );
    let initial_nodes = ntc.nodes.clone();

    let test_fn = Box::new(move |candidate: &[ChoiceNode]| {
        let values: Vec<ChoiceValue> = candidate.iter().map(|n| n.value.clone()).collect();
        let mut ntc = NativeTestCase::for_choices(&values, Some(candidate));
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
