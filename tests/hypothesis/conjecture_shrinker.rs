//! Ported from hypothesis-python/tests/conjecture/test_shrinker.py
//!
//! Exercises the native shrinker (`src/native/shrinker/`) on small hand-built
//! choice sequences. The Python original uses an `@shrinking_from(initial)`
//! fixture that runs a ConjectureRunner, caches the starting choice sequence,
//! and builds a `Shrinker` from it; here the local [`shrinking_from`] helper
//! skips the runner and goes directly to `NativeTestCase::for_choices` +
//! `Shrinker::new`, which is enough to exercise the shrink pipeline.
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

fn extract_integers(nodes: &[ChoiceNode]) -> Vec<i128> {
    nodes
        .iter()
        .map(|n| match n.value {
            ChoiceValue::Integer(v) => v,
            ref other => panic!("expected integer, got {other:?}"),
        })
        .collect()
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
