//! Ported from hypothesis-python/tests/conjecture/test_engine.py.
//!
//! The upstream file exercises Hypothesis's `ConjectureRunner` engine
//! directly via `runner.interesting_examples`, `runner.exit_reason`,
//! `runner.shrinks`, `runner.call_count`, `runner.cached_test_function`,
//! `runner.reuse_existing_examples`, `runner.save_choices`, `runner.tree`,
//! `runner.new_shrinker(data, predicate)`, plus the `run_to_nodes(f)`
//! fixture, `buffer_size_limit(n)` context manager,
//! `InMemoryExampleDatabase` introspection (`db.data`, `db.save`,
//! `db.fetch`), and `monkeypatch.setattr(ConjectureRunner, ...)` for
//! overriding `generate_new_examples` / `MAX_SHRINKS`. None of these
//! surfaces are available on hegel-rust's native engine; the existing
//! `hegel::__native_test_internals::TargetedRunner` exposes only the
//! `cached_test_function` / `optimise_targets` slice needed by
//! `conjecture/test_optimiser.py`, and does not model interesting-example
//! tracking, exit reasons, shrink counters, the data tree, the shrinker
//! entry point, or database replay.
//!
//! This commit ports only the shrink-quality tests that are expressible at
//! the public `minimal(strategy, condition)` surface. The remaining tests
//! are listed in the individually-skipped section below, each tagged with
//! the specific engine-internals surface that would need to be exposed
//! through `__native_test_internals` before they can be ported. These are
//! parked under the missing-native-feature individual-skip path
//! (`.claude/skills/porting-tests/SKILL.md`); the concrete missing feature
//! is a `NativeConjectureRunner` wrapper, tracked in `TODO.yaml` under
//! "Expose a NativeConjectureRunner wrapper in `__native_test_internals`".
//! That entry's acceptance criteria include removing every individually-
//! skipped test named below.
//!
//! Individually-skipped tests (rest of the file is ported):
//!
//! - `run_to_nodes(f)` fixture — runs a `ConjectureRunner` to completion
//!   on `f` and returns the shrunk `data.nodes` of the sole interesting
//!   example. Requires a native runner that exposes the post-shrink node
//!   sequence. Blocks:
//!   `test_non_cloneable_intervals`, `test_deletable_draws`,
//!   `test_variadic_draw`, `test_draw_to_overrun`, `test_erratic_draws`,
//!   `test_no_read_no_shrink`, `test_one_dead_branch`, `test_returns_forced`,
//!   `test_run_nothing`, `test_interleaving_engines`.
//!
//! - `ConjectureRunner(f, settings=, random=, database_key=).run()` +
//!   `runner.interesting_examples`, `runner.exit_reason`, `runner.shrinks`,
//!   `runner.call_count`. Blocks:
//!   `test_can_load_data_from_a_corpus`, `test_detects_flakiness`,
//!   `test_recursion_error_is_not_flaky`, `test_can_navigate_to_a_valid_example`,
//!   `test_stops_after_max_examples_when_reading`,
//!   `test_stops_after_max_examples_when_generating`,
//!   `test_stops_after_max_examples_when_generating_more_bugs`,
//!   `test_phases_can_disable_shrinking`,
//!   `test_reuse_phase_runs_for_max_examples_if_generation_is_disabled`,
//!   `test_does_not_save_on_interrupt`,
//!   `test_saves_on_skip_exceptions_to_reraise`,
//!   `test_exit_because_max_iterations`, `test_max_iterations_with_all_invalid`,
//!   `test_max_iterations_with_some_valid`,
//!   `test_exit_because_shrink_phase_timeout`,
//!   `test_does_not_shrink_multiple_bugs_when_told_not_to`,
//!   `test_does_not_keep_generating_when_multiple_bugs`,
//!   `test_shrink_after_max_examples`, `test_shrink_after_max_iterations`,
//!   `test_runs_full_set_of_examples`,
//!   `test_does_not_shrink_if_replaying_from_database`,
//!   `test_does_shrink_if_replaying_inexact_from_database`,
//!   `test_stops_if_hits_interesting_early_and_only_want_one_bug`,
//!   `test_skips_secondary_if_interesting_is_found`,
//!   `test_discards_invalid_db_entries`,
//!   `test_discards_invalid_db_entries_pareto`.
//!
//! - `monkeypatch.setattr(ConjectureRunner, "generate_new_examples", ...)` /
//!   `monkeypatch.setattr(engine_module, "MAX_SHRINKS", n)` — Python
//!   attribute injection to seed a specific initial buffer or cap the
//!   shrink loop. No analog in the native engine. Blocks:
//!   `test_terminates_shrinks`, `test_shrinks_both_interesting_examples`,
//!   `test_discarding`, `test_shrinking_from_mostly_zero`,
//!   `test_handles_nesting_of_discard_correctly`,
//!   `test_prefix_cannot_exceed_buffer_size`,
//!   `test_will_evict_entries_from_the_cache`,
//!   `test_simulate_to_evicted_data`.
//!
//! - `FailedHealthCheck` introspection via `runner.run()` — asserts the
//!   raised exception carries a specific `HealthCheck` label. hegel-rust's
//!   native engine panics with a free-form string; the
//!   `fails_health_check(label)` helper compares against the label's
//!   `str(HealthCheck.xxx)` which maps to distinct panic messages.
//!   Blocks:
//!   `test_fails_health_check_for_all_invalid`,
//!   `test_fails_health_check_for_large_base`,
//!   `test_fails_health_check_for_large_non_base`,
//!   `test_fails_health_check_for_slow_draws`,
//!   `test_health_check_too_slow_with_invalid_examples`,
//!   `test_health_check_too_slow_with_overrun_examples`,
//!   `test_too_slow_report`.
//!
//! - `InMemoryExampleDatabase` with `.data` / `.save(key, choices)` /
//!   `.fetch(key)` introspection, plus `choices_to_bytes` /
//!   `choices_from_bytes`. Native has `NativeDatabase` (path-backed) but
//!   no in-memory variant exposed for tests and no
//!   `choices_to_bytes` / `choices_from_bytes` helpers to round-trip.
//!   Blocks (in addition to tests listed above):
//!   `test_clears_out_its_database_on_shrinking`,
//!   `test_database_clears_secondary_key`,
//!   `test_database_uses_values_from_secondary_key`.
//!
//! - `runner.cached_test_function(start)` + `new_shrinker(last_data,
//!   predicate)` / `shrinker.shrink()` engine-internal shrink harness.
//!   `TargetedRunner` has a bare `cached_test_function` but no
//!   `new_shrinker`. Blocks:
//!   `test_can_remove_discarded_data`,
//!   `test_discarding_iterates_to_fixed_point`,
//!   `test_discarding_is_not_fooled_by_empty_discards`,
//!   `test_discarding_can_fail`,
//!   `test_can_write_bytes_towards_the_end`,
//!   `test_uniqueness_is_preserved_when_writing_at_beginning`,
//!   `test_dependent_block_pairs_can_lower_to_zero`,
//!   `test_handle_size_too_large_during_dependent_lowering`,
//!   `test_block_may_grow_during_lexical_shrinking`,
//!   `test_lower_common_node_offset_does_nothing_when_changed_blocks_are_zero`,
//!   `test_lower_common_node_offset_ignores_zeros`,
//!   `test_cached_test_function_returns_right_value`,
//!   `test_cached_test_function_does_not_reinvoke_on_prefix`,
//!   `test_branch_ending_in_write`, `test_exhaust_space`,
//!   `test_discards_kill_branches`,
//!   `test_number_of_examples_in_integer_range_is_bounded`,
//!   `test_does_not_cache_extended_prefix`,
//!   `test_does_cache_if_extend_is_not_used`,
//!   `test_does_result_for_reuse`,
//!   `test_does_not_use_cached_overrun_if_extending`,
//!   `test_uses_cached_overrun_if_not_extending`,
//!   `test_can_be_set_to_ignore_limits`,
//!   `test_overruns_with_extend_are_not_cached`.
//!
//! - `runner.pareto_front` / `ParetoFront` / `dominance` — Pareto-front
//!   bookkeeping for targeting; no native equivalent. Blocks:
//!   `test_populates_the_pareto_front`,
//!   `test_pareto_front_contains_smallest_valid`,
//!   `test_replaces_all_dominated`, `test_does_not_duplicate_elements`,
//!   `test_includes_right_hand_side_targets_in_dominance`,
//!   `test_smaller_interesting_dominates_larger_valid`,
//!   `test_runs_optimisation_even_if_not_generating`,
//!   `test_runs_optimisation_once_when_generating`,
//!   `test_does_not_run_optimisation_when_max_examples_is_small`.
//!
//! - `capsys`-style stdout capture for `Verbosity::Debug` diagnostic
//!   lines. `test_debug_data` — covered in spirit by
//!   `tests/hypothesis/verbosity.rs`.

#![cfg(feature = "native")]

use crate::common::utils::minimal;
use hegel::TestCase;
use hegel::generators as gs;

// Port of the `@st.composite strategy(draw)` defined inline in
// `test_can_shrink_variable_draws`. Draws a variable `n ∈ [0, 15]` and
// then `n` integer choices. The outer `n` is a separate choice node,
// which is what makes the shrinker's "shrink the length, then each
// element" pattern non-trivial.
#[hegel::composite]
fn variable_int_list(tc: TestCase) -> Vec<i64> {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(15));
    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        result.push(tc.draw(gs::integers::<i64>().min_value(0).max_value(255)));
    }
    result
}

fn check_can_shrink_variable_draws(n_large: i64) {
    let target = 128 * n_large;
    let ints = minimal(variable_int_list(), move |v: &Vec<i64>| {
        v.iter().copied().map(i128::from).sum::<i128>() >= i128::from(target)
    });
    // should look like [target % 255] + [255] * (len - 1)
    let expected_first = target % 255;
    assert_eq!(ints[0], expected_first, "ints = {ints:?}");
    for x in &ints[1..] {
        assert_eq!(*x, 255, "ints = {ints:?}");
    }
}

#[test]
fn test_can_shrink_variable_draws_1() {
    check_can_shrink_variable_draws(1);
}

#[test]
fn test_can_shrink_variable_draws_5() {
    check_can_shrink_variable_draws(5);
}

#[test]
fn test_can_shrink_variable_draws_8() {
    check_can_shrink_variable_draws(8);
}

#[test]
fn test_can_shrink_variable_draws_15() {
    check_can_shrink_variable_draws(15);
}

// Port of the `@st.composite` that draws `n` first, then a string of
// exactly `n` ASCII characters.
#[hegel::composite]
fn variable_ascii_string(tc: TestCase) -> String {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(20));
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_can_shrink_variable_string_draws() {
    let s = minimal(variable_ascii_string(), |s: &String| {
        s.len() >= 10 && s.contains('a')
    });
    // Upstream TODO_BETTER_SHRINK: ideally `"0" * 9 + "a"`. In practice
    // the shrinker settles on a string matching `0+a`. We mirror that
    // weaker assertion so the test stays faithful to the upstream
    // expectation.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Port of the `@st.composite` that inverts `n` so the *strategy's*
// `n` axis shrinks towards 10 rather than 0.
#[hegel::composite]
fn inverted_ascii_string(tc: TestCase) -> String {
    let n_drawn: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(10));
    let n = 10 - n_drawn;
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_variable_size_string_increasing() {
    let s = minimal(inverted_ascii_string(), |s: &String| {
        s.len() >= 5 && s.contains('a')
    });
    // Same TODO_BETTER_SHRINK caveat as
    // `test_can_shrink_variable_string_draws`.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Coverage tests for engine.py / shrinker.py code paths that are
// exercised by shrinking any mildly-complicated strategy. Upstream is
// a single parametrised test with three rows; the third
// (`st.sampled_from(enum.Flag(...))` → `bit_count(f.value) > 1`) uses
// Python's `enum.Flag` factory which has no direct Rust analogue — it
// builds a 64-bit flag type at runtime. The two `st.lists(...)` rows
// port directly.
#[test]
fn test_mildly_complicated_strategies_integers_list() {
    minimal(
        gs::vecs(gs::integers::<i64>()).min_size(5),
        |_: &Vec<i64>| true,
    );
}

#[test]
fn test_mildly_complicated_strategies_unique_text_list() {
    minimal(
        gs::vecs(gs::text()).min_size(2).unique(true),
        |_: &Vec<String>| true,
    );
}
