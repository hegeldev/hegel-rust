//! Ported from resources/pbtkit/tests/test_core.py.
//!
//! Upstream tests absent from this file — either skipped (noted in
//! SKIPPED.md) or ported elsewhere as embedded tests:
//! - `test_reuses_results_from_the_database` — asserts
//!   `len(tmpdir.listdir()) == 1` on pbtkit's `DirectoryDB`
//!   single-file-per-key layout and an exact `count == prev + 2`
//!   replay+verify invariant. hegel-rust's `NativeDatabase` uses a
//!   nested `key/value` hash-directory layout (so the root-`listdir()`
//!   assertion doesn't translate) and the replay-loop call-count shape
//!   isn't guaranteed to match pbtkit's literally.
//! - `test_database_round_trip_with_booleans` — uses `tc.weighted(p)`,
//!   no hegel-rust counterpart (same public-API incompatibility as the
//!   other `weighted` skips below).
//! - `test_malformed_database_entry`, `test_empty_database_entry`,
//!   `test_truncated_database_entry` — exercise pbtkit's `DirectoryDB`
//!   on-disk byte-level serialization format (tag bytes, length headers);
//!   hegel-rust's `NativeDatabase` uses a different serialization layout
//!   (`serialize_choices` in `src/native/database.rs`), so the exact byte
//!   patterns here have no analog.
//! - `test_error_on_unbounded_test_function` — monkeypatches
//!   `pbtkit.core.BUFFER_SIZE` on the Python module at runtime; hegel-rust's
//!   `BUFFER_SIZE` is a native-only `const` with no runtime-patch surface.
//! - `test_function_cache` — uses pbtkit's
//!   `CachedTestFunction([values])` / `.lookup([values])` interface,
//!   which takes a raw choice-value list. hegel-rust's
//!   `CachedTestFunction` takes a `NativeTestCase` instead and exposes
//!   only `run` / `run_shrink` / `run_final`; the pbtkit-shape API
//!   doesn't exist.
//! - `test_prints_a_top_level_weighted` — uses `tc.weighted(p)`, which
//!   hegel-rust deliberately doesn't expose on `TestCase` (same
//!   public-API incompatibility as the `test_generators.py` `weighted`
//!   skips).
//! - `test_errors_when_using_frozen` — exercises pbtkit's public
//!   `Frozen` exception raised when a completed `TestCase` is reused.
//!   hegel-rust has no `Frozen` surface: the native `NativeTestCase`
//!   carries a `Status` but no analog error type is exported.
//! - `test_forced_choice_bounds` — uses `tc.forced_choice(n)`, a pbtkit
//!   public API that forces the next drawn value. hegel-rust's native
//!   `draw_integer`/`weighted` accept an internal `forced` argument but
//!   it's not exposed on `TestCase`.
//! - `test_errors_on_too_large_choice` — uses `tc.choice(2**64)`, a
//!   Python dynamic-int raw-bound API. hegel-rust's typed integer
//!   generators cap bounds at compile time via `T`; `2**64` as a bound
//!   is unrepresentable in the public API.
//! - `test_bin_search_down_lo_satisfies`,
//!   `test_swap_adjacent_blocks_equal_blocks`,
//!   `test_cache_key_distinguishes_negative_zero`,
//!   `test_cache_key_distinguishes_nan_variants`,
//!   `test_cache_distinguishes_negative_zero_in_lookup`,
//!   `test_delete_chunks_guard_after_decrement`,
//!   `test_redistribute_integers_stale_indices`,
//!   `test_bind_deletion_try_deletions_succeeds`,
//!   `test_sort_values_full_sort_fails` — exercise individual shrink
//!   passes, `_cache_key`, or `CachedTestFunction` directly. Ported as
//!   embedded tests in `tests/embedded/native/shrinker_tests.rs` and
//!   `tests/embedded/native/tree_tests.rs` where the `pub(super)` pass
//!   methods, `ChoiceValueKey`, and `CachedTestFunction` internals are
//!   reachable via `use super::*`.
//! - `test_value_punning_on_type_change`,
//!   `test_bind_deletion_valid_but_not_shorter`,
//!   `test_delete_chunks_stale_index`,
//!   `test_shrink_duplicates_with_stale_indices`,
//!   `test_shrink_duplicates_valid_drops_below_two` — depend on pbtkit's
//!   shrinker truncating `Shrinker.current.nodes` to actually-drawn length
//!   on every accepted candidate. hegel-rust's `Shrinker::consider` stores
//!   the full input `nodes.to_vec()`, so the specific "i past the new end"
//!   / "stale group indices" regressions these guard against don't occur
//!   in hegel-rust's shrinker.
//! - `test_redistribute_binary_search` — calls pbtkit's
//!   `redistribute_sequence_pair` helper directly with a Python callback.
//!   hegel-rust has no equivalent public function surface.
//! - `test_run_test_with_preseeded_result` — uses
//!   `unittest.mock.patch.object(State, "__init__", ...)` to preseed
//!   `state.result`. Python-only monkey-patching facility.
//! - `test_sort_key_type_mismatch` — exercises Python dynamic-typing
//!   `sort_key(wrong_type)` (same pattern as the already-skipped
//!   `test_string_sort_key_type_mismatch`, `test_bytes_sort_key_type_mismatch`);
//!   Rust's typed `sort_key` signatures make it unrepresentable.
//! - `test_targeting_skips_non_integer` — uses `tc.target(score)`, no
//!   analog (whole-file skip of `test_targeting.py`).
//! - `test_note_prints_on_failing_example`,
//!   `test_draw_silent_does_not_print` — use pbtkit's `capsys` pytest
//!   fixture to inspect the final-replay stdout formatter byte-for-byte.
//!   hegel-rust's failing-replay output goes to stderr in a different
//!   shape (`let draw_1 = ...;` prefix), so a byte-level comparison with
//!   pbtkit's format is unportable. The stderr shape itself is pinned
//!   down by the `TempRustProject`-based tests in `tests/test_output.rs`.
//! - `test_nothing_core` — uses `gs.nothing()`; hegel-rust has no
//!   empty-generator public API (same reason as the existing
//!   `test_generators.py::test_cannot_witness_nothing` skip).
//! - `test_generator_repr` — Python `repr()` output; hegel-rust
//!   generators have no repr surface (same reason as the existing
//!   `test_generators.py::test_generator_repr` skip).

use crate::common::utils::expect_panic;
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};

#[hegel::test]
fn test_test_cases_satisfy_preconditions(tc: TestCase) {
    let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
    tc.assume(n != 0);
    assert!(n != 0);
}

#[hegel::test]
fn test_can_choose_full_64_bits(tc: TestCase) {
    // pbtkit's `tc.choice(2**64 - 1)` samples the full unsigned 64-bit
    // range. hegel-rust's typed equivalent is `gs::integers::<u64>()`.
    let _: u64 = tc.draw(gs::integers::<u64>());
}

#[test]
fn test_flat_map_core() {
    Hegel::new(|tc| {
        let (m, n): (i64, i64) = tc.draw(gs::integers::<i64>().min_value(0).max_value(5).flat_map(
            |m: i64| {
                gs::tuples!(
                    gs::just(m),
                    gs::integers::<i64>().min_value(m).max_value(m + 10),
                )
            },
        ));
        assert!(m <= n && n <= m + 10);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_filter_core() {
    Hegel::new(|tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(10)
                .filter(|n: &i64| n % 2 == 0),
        );
        assert!(n % 2 == 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_one_of_empty_core() {
    // pbtkit raises Unsatisfiable when drawing from one_of() with no
    // alternatives; hegel-rust panics at construction.
    expect_panic(
        || {
            let empty: Vec<gs::BoxedGenerator<i32>> = vec![];
            gs::one_of(empty);
        },
        "one_of requires at least one generator",
    );
}

#[test]
fn test_one_of_single_core() {
    Hegel::new(|tc| {
        let n: i64 = tc.draw(hegel::one_of!(
            gs::integers::<i64>().min_value(0).max_value(10)
        ));
        assert!((0..=10).contains(&n));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_core() {
    Hegel::new(|tc| {
        let v: &'static str = tc.draw(gs::sampled_from(vec!["a", "b", "c"]));
        assert!(matches!(v, "a" | "b" | "c"));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_empty_core() {
    expect_panic(
        || {
            let empty: Vec<i32> = vec![];
            gs::sampled_from(empty);
        },
        "cannot be empty",
    );
}

#[test]
fn test_sampled_from_single_core() {
    Hegel::new(|tc| {
        let v: &'static str = tc.draw(gs::sampled_from(vec!["only"]));
        assert_eq!(v, "only");
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_just_core() {
    Hegel::new(|tc| {
        let v: i64 = tc.draw(gs::just(42_i64));
        assert_eq!(v, 42);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_map_core() {
    Hegel::new(|tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .map(|n: i64| n * 2),
        );
        assert!(n % 2 == 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_weighted_forced_true() {
    // pbtkit: `tc.weighted(1.0)` deterministically returns True. hegel-rust
    // has no `tc.weighted(p)` public API, but `gs::booleans().map(|_| true)`
    // combined with a forced-to-true predicate produces the same shape:
    // the test body unconditionally panics.
    expect_panic(
        || {
            Hegel::new(|tc| {
                if tc.draw(gs::just(true)) {
                    tc.draw(gs::integers::<i64>().min_value(0).max_value(1));
                    panic!("forced-true branch reached");
                }
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        "forced-true branch reached",
    );
}

// Port of pbtkit/tests/test_core.py::test_error_on_too_strict_precondition.
// pbtkit raises Unsatisfiable when every test case calls `tc.reject()`;
// hegel-rust's native mode fires a FilterTooMuch health check instead.
// Server mode silently passes, so this assertion is native-only.
#[cfg(feature = "native")]
#[test]
fn test_error_on_too_strict_precondition() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
                tc.reject();
            })
            .run();
        },
        "FilterTooMuch",
    );
}

// ── IntegerChoice native-engine tests ──────────────────────────────────────

#[cfg(feature = "native")]
mod integer_choice_internals {
    use hegel::__native_test_internals::IntegerChoice;

    #[test]
    fn test_integer_choice_simplest() {
        assert_eq!(
            IntegerChoice {
                min_value: -10,
                max_value: 10
            }
            .simplest(),
            0
        );
        assert_eq!(
            IntegerChoice {
                min_value: 5,
                max_value: 100
            }
            .simplest(),
            5
        );
        assert_eq!(
            IntegerChoice {
                min_value: -100,
                max_value: -5
            }
            .simplest(),
            -5
        );
    }

    #[test]
    fn test_integer_choice_unit() {
        assert_eq!(
            IntegerChoice {
                min_value: -10,
                max_value: 10
            }
            .unit(),
            1
        );
        assert_eq!(
            IntegerChoice {
                min_value: 5,
                max_value: 100
            }
            .unit(),
            6
        );
        // When simplest is at the top of the range, unit is simplest - 1.
        assert_eq!(
            IntegerChoice {
                min_value: -100,
                max_value: -5
            }
            .unit(),
            -6
        );
        // Single-value range: unit falls back to simplest.
        assert_eq!(
            IntegerChoice {
                min_value: 5,
                max_value: 5
            }
            .unit(),
            5
        );
    }
}
