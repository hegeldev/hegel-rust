//! Ported from hypothesis-python/tests/conjecture/test_data_tree.py.
//!
//! Tests Hypothesis's `DataTree` engine internal — the tree of explored
//! choice sequences used for novel-prefix generation and exhaustion
//! detection.  The native counterpart is the internal `DataTreeNode`
//! tree inside `NativeConjectureRunner`, exposed via
//! `NativeDataTreeView` (returned by `runner.tree()`).
//!
//! ## Individually-skipped tests:
//!
//! - `test_stores_the_tree_flat_until_needed` — accesses
//!   `root.constraints / root.values / root.transition.status`: Python's
//!   DataTree uses a flat-list optimisation for pre-branch runs that
//!   `DataTreeNode` does not replicate.
//! - `test_split_in_the_middle` — accesses
//!   `root.transition.children[i].values`: same flat-list structure.
//! - `test_stores_forced_nodes` — accesses `root.forced` (a set of
//!   forced positions): `DataTreeNode` does not track forced indices.
//! - `test_correctly_relocates_forced_nodes` — same.
//! - `test_can_go_from_interesting_to_valid` — uses standalone
//!   `DataTree()` + `ConjectureData.for_choices(observer=tree.new_observer())`:
//!   the observer API is not exposed in native mode.
//! - `test_going_from_interesting_to_invalid_is_flaky` — Flaky detection
//!   for status changes is not implemented in native mode (only kind
//!   mismatches raise in `record_tree`).
//! - `test_concluding_at_prefix_is_flaky` — standalone DataTree + Flaky.
//! - `test_concluding_with_overrun_at_prefix_is_not_flaky` — standalone
//!   DataTree observer API not exposed.
//! - `test_changing_n_bits_is_flaky_in_prefix` — standalone DataTree + Flaky.
//! - `test_changing_n_bits_is_flaky_in_branch` — standalone DataTree + Flaky.
//! - `test_extending_past_conclusion_is_flaky` — standalone DataTree + Flaky.
//! - `test_changing_to_forced_is_flaky` — standalone DataTree + Flaky.
//! - `test_changing_value_of_forced_is_flaky` — standalone DataTree + Flaky.
//! - `test_child_becomes_exhausted_after_split` — accesses
//!   `tree.root.transition.children[b"\0"].is_exhausted`: internal tree
//!   children not exposed on `NativeDataTreeView`.
//! - `test_will_mark_changes_in_discard_as_flaky` — standalone DataTree +
//!   Flaky on `stop_span(discard=True)`.
//! - `test_is_not_flaky_on_positive_zero_and_negative_zero` — accesses
//!   `tree.root.transition.children[float_to_int(...)]`.
//! - `test_observed_choice_type_draw` (×5) — accesses
//!   `tree.root.choice_types / tree.root.transition`: not on
//!   `DataTreeNode`.
//! - `test_non_observed_choice_type_draw` (×5) — same.
//! - `test_datatree_repr` — tests `pretty.pretty(tree)`: Python repr.
//! - `test_can_generate_hard_floats` — requires forced float draws via
//!   `run_to_nodes` + `draw_float(..., forced=f)` with bit-precise float
//!   constraints; the native draw_float path for the runner's
//!   `NativeConjectureData` does not expose a `forced` parameter.
//! - `test_simulate_forced_floats` — uses standalone DataTree +
//!   `tree.simulate_test_function(data)` where `data` is a
//!   `ConjectureData` object; the native `simulate_test_function` takes
//!   `&[ChoiceValue]`, not a data object, and the `nodes()` strategy
//!   from `tests/conjecture/common.py` has no native equivalent.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ChoiceValue, NativeConjectureData, NativeConjectureRunner, NativeRunnerSettings, Status,
};
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// Port of the Python `runner_for(*examples)` helper.
///
/// Creates a runner, runs `cached_test_function` on each set of input
/// choices, then asserts that `tree.rewrite(actual_choices)` returns the
/// recorded status for every run.  Uses the actual drawn choices (`d.choices`)
/// rather than the input choices because the native tree records what the
/// test function *drew*, not what was offered — forced draws write the
/// forced value to the tree, not the prefix value.
fn runner_for(
    examples: Vec<Vec<ChoiceValue>>,
    test_fn: impl FnMut(&mut NativeConjectureData) + 'static,
) -> NativeConjectureRunner {
    let mut runner = NativeConjectureRunner::new(
        test_fn,
        NativeRunnerSettings::new()
            .max_examples(100)
            .suppress_health_check(vec![]),
        SmallRng::seed_from_u64(0),
    );
    let mut ran_results = Vec::new();
    for choices in &examples {
        let d = runner.cached_test_function(choices);
        ran_results.push(d);
    }
    for d in &ran_results {
        let (rewritten, status) = runner.tree().rewrite(&d.choices);
        assert_eq!(status, Some(d.status), "rewrite status mismatch for choices {:?}", d.choices);
        assert_eq!(rewritten, d.choices, "rewritten choices mismatch");
    }
    runner
}

#[test]
fn test_can_lookup_cached_examples() {
    runner_for(
        vec![
            vec![ChoiceValue::Integer(0), ChoiceValue::Integer(0)],
            vec![ChoiceValue::Integer(0), ChoiceValue::Integer(1)],
        ],
        |data| {
            data.draw_integer(0, 1000);
            data.draw_integer(0, 1000);
        },
    );
}

#[test]
fn test_can_lookup_cached_examples_with_forced() {
    // Python uses `draw_integer(forced=1)` — simulated here by
    // constraining the first draw to the single-value range [1, 1].
    runner_for(
        vec![
            vec![ChoiceValue::Integer(1), ChoiceValue::Integer(0)],
            vec![ChoiceValue::Integer(1), ChoiceValue::Integer(1)],
        ],
        |data| {
            data.draw_integer(1, 1);
            data.draw_integer(0, 1000);
        },
    );
}

#[test]
fn test_can_detect_when_tree_is_exhausted() {
    let runner = runner_for(
        vec![
            vec![ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(true)],
        ],
        |data| {
            data.draw_boolean(0.5);
        },
    );
    assert!(runner.tree().is_exhausted());
}

#[test]
fn test_can_detect_when_tree_is_exhausted_variable_size() {
    let runner = runner_for(
        vec![
            vec![ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
        ],
        |data| {
            if data.draw_boolean(0.5) {
                data.draw_boolean(0.5);
            }
        },
    );
    assert!(runner.tree().is_exhausted());
}

#[test]
fn test_one_dead_branch() {
    let mut examples: Vec<Vec<ChoiceValue>> = (0..16_i128)
        .map(|i| vec![ChoiceValue::Integer(0), ChoiceValue::Integer(i)])
        .collect();
    examples.extend((1..16_i128).map(|i| vec![ChoiceValue::Integer(i)]));

    let runner = runner_for(examples, |data| {
        let i = data.draw_integer(0, 15);
        if i > 0 {
            data.mark_invalid(None);
        }
        data.draw_integer(0, 15);
    });
    assert!(runner.tree().is_exhausted());
}

#[test]
fn test_non_dead_root() {
    runner_for(
        vec![
            vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
        ],
        |data| {
            data.draw_boolean(0.5);
            data.draw_boolean(0.5);
        },
    );
}

#[test]
fn test_can_reexecute_dead_examples() {
    runner_for(
        vec![
            vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)],
            vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)],
            vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)],
        ],
        |data| {
            data.draw_boolean(0.5);
            data.draw_boolean(0.5);
        },
    );
}

#[test]
fn test_novel_prefixes_are_novel() {
    let mut runner = NativeConjectureRunner::new(
        |data| {
            for _ in 0..4 {
                data.draw_bytes(1, 1);
                data.draw_integer(0, 3);
            }
        },
        NativeRunnerSettings::new()
            .max_examples(1000)
            .suppress_health_check(vec![]),
        SmallRng::seed_from_u64(0),
    );
    for _ in 0..100 {
        let prefix = runner.generate_novel_prefix();
        let result = runner.cached_test_function_full(&prefix);
        // After running, the actual choice sequence is recorded in the tree.
        let (rewritten, status) = runner.tree().rewrite(&result.choices);
        assert_eq!(status, Some(result.status));
        assert_eq!(rewritten, result.choices);
    }
}

#[test]
fn test_overruns_if_prefix() {
    let mut runner = NativeConjectureRunner::new(
        |data| {
            data.draw_boolean(0.5);
            data.draw_boolean(0.5);
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );
    runner.cached_test_function(&[
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(false),
    ]);
    let (_, status) = runner.tree().rewrite(&[ChoiceValue::Boolean(false)]);
    assert_eq!(status, Some(Status::EarlyStop)); // OVERRUN analog
}

#[test]
fn test_does_not_truncate_if_unseen() {
    // An empty tree returns (choices, None) for any input.
    let runner = NativeConjectureRunner::new(
        |_data| {},
        NativeRunnerSettings::new().max_examples(0),
        SmallRng::seed_from_u64(0),
    );
    let choices = vec![
        ChoiceValue::Integer(1),
        ChoiceValue::Integer(2),
        ChoiceValue::Integer(3),
        ChoiceValue::Integer(4),
    ];
    let (rewritten, status) = runner.tree().rewrite(&choices);
    assert_eq!(status, None);
    assert_eq!(rewritten, choices);
}

#[test]
fn test_truncates_if_seen() {
    let choices = vec![
        ChoiceValue::Integer(1),
        ChoiceValue::Integer(2),
        ChoiceValue::Integer(3),
        ChoiceValue::Integer(4),
    ];
    let mut runner = NativeConjectureRunner::new(
        |data| {
            data.draw_integer(0, 1000);
            data.draw_integer(0, 1000);
            // Only draws 2 integers; the remaining prefix choices are unused.
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );
    runner.cached_test_function(&choices);

    // Tree recorded only the 2 drawn choices; rewrite truncates to them.
    let (rewritten, status) = runner.tree().rewrite(&choices);
    assert_eq!(status, Some(Status::Valid));
    assert_eq!(rewritten, &choices[..2]);
}

#[test]
fn test_will_generate_novel_prefix_to_avoid_exhausted_branches() {
    // Record Integer(1) → exhausted, Integer(0) + Bytes([1]) → exhausted.
    // generate_novel_prefix must return a prefix starting with Integer(0)
    // followed by some bytes value other than [1].
    let mut runner = NativeConjectureRunner::new(
        |data| {
            let i = data.draw_integer(0, 1);
            if i == 0 {
                data.draw_bytes(1, 1);
            }
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );

    runner.cached_test_function(&[ChoiceValue::Integer(1)]);
    runner.cached_test_function(&[ChoiceValue::Integer(0), ChoiceValue::Bytes(vec![1])]);

    let prefix = runner.generate_novel_prefix();
    assert_eq!(prefix.len(), 2, "prefix should be 2 elements, got {:?}", prefix);
    assert_eq!(prefix[0], ChoiceValue::Integer(0));
}

#[test]
fn test_low_probabilities_are_still_explored() {
    // Even after recording the False branch, generate_novel_prefix must
    // return a prefix starting with True (the unexplored branch).
    let mut runner = NativeConjectureRunner::new(
        |data| {
            data.draw_boolean(1e-10);
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );
    runner.cached_test_function(&[ChoiceValue::Boolean(false)]);
    let prefix = runner.generate_novel_prefix();
    assert!(!prefix.is_empty());
    assert_eq!(prefix[0], ChoiceValue::Boolean(true));
}

#[test]
fn test_can_generate_hard_values() {
    let min_value: i128 = 0;
    let max_value: i128 = 1000;
    let mut runner = NativeConjectureRunner::new(
        move |data| {
            data.draw_integer(min_value, max_value);
        },
        NativeRunnerSettings::new().max_examples(100),
        SmallRng::seed_from_u64(0),
    );
    // Record all values 0..999, leaving only 1000 unexplored.
    for i in 0..max_value {
        runner.cached_test_function(&[ChoiceValue::Integer(i)]);
    }

    // generate_novel_prefix must return the one remaining value.
    for _ in 0..20 {
        let prefix = runner.generate_novel_prefix();
        assert_eq!(
            prefix,
            vec![ChoiceValue::Integer(max_value)],
            "expected novel prefix [1000], got {:?}",
            prefix
        );
    }
}
