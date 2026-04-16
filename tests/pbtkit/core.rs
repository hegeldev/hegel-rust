//! Ported from pbtkit/tests/test_core.py

use crate::common::project::TempRustProject;
use crate::common::utils::{check_can_generate_examples, expect_panic};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[cfg(feature = "native")]
#[test]
fn test_reuses_results_from_the_database() {
    // Port of pbtkit's database round-trip: a failing test should save its
    // failing case so a subsequent run replays it instead of re-discovering it.
    // Database round-trip is only wired up on the native backend.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    let db_str = db_path.to_str().unwrap();

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();

    let test_code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

fn record() {{
    let path = format!("{{}}/log", std::env::var("VALUES_DIR").unwrap());
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "x").unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_count(tc: hegel::TestCase) {{
    record();
    let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(9999));
    assert!(n < 10);
}}
"#
    );

    let project = TempRustProject::new()
        .test_file("integration.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        .expect_failure("FAILED");

    project.cargo_test(&["test_count"]);
    let log_path = values_path.join("log");
    let first_count = std::fs::read_to_string(&log_path).unwrap().lines().count();

    // Database now has the failing case; replay should be quick.
    std::fs::remove_file(&log_path).unwrap();
    project.cargo_test(&["test_count"]);
    let second_count = std::fs::read_to_string(&log_path).unwrap().lines().count();

    // Replay should be much shorter than the original search.
    assert!(
        second_count < first_count,
        "Expected replay to run fewer cases than original ({second_count} >= {first_count})"
    );
}

#[test]
fn test_test_cases_satisfy_preconditions() {
    // Port of: a test where each draw is restricted to a small range and
    // tc.assume() rejects a specific value. The Python uses tc.choice(10);
    // we use an equivalent bounded integer draw.
    Hegel::new(|tc| {
        let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(9));
        tc.assume(n != 0);
        assert_ne!(n, 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_error_on_too_strict_precondition() {
    // TODO: hegel-rust has no public `tc.reject()` API (only `tc.assume(false)`,
    // which is counted as an assumption failure rather than an explicit reject),
    // and no stable public way to observe the Unsatisfiable error distinctly
    // from other panics from a single run.
    todo!()
}

#[test]
fn test_error_on_unbounded_test_function() {
    // TODO: depends on pbtkit's BUFFER_SIZE core constant (monkeypatched in the
    // Python original) and on observing an Unsatisfiable error; neither is
    // exposed publicly in hegel-rust.
    todo!()
}

#[test]
fn test_function_cache() {
    // TODO: hegel-rust's CachedTestFunction stores only (is_interesting,
    // node_count) and has no equivalent of pbtkit's EARLY_STOP / VALID /
    // INTERESTING tri-state cache result. Basic cache hit/miss behavior is
    // covered by the existing embedded tests in tree_tests.rs
    // (cache_miss_returns_none / cache_hit_returns_stored_result).
}

#[test]
fn test_cache_key_distinguishes_negative_zero() {
    // Ported as embedded test
    // tests/embedded/native/tree_tests.rs::cache_key_distinguishes_negative_zero
    // (the cache key is internal so this is exercised through the
    // crate-private CachedTestFunction surface).
}

#[test]
fn test_cache_key_distinguishes_nan_variants() {
    // Ported as embedded test
    // tests/embedded/native/tree_tests.rs::cache_key_distinguishes_nan_variants.
}

#[test]
fn test_cache_distinguishes_negative_zero_in_lookup() {
    // Ported as embedded test
    // tests/embedded/native/tree_tests.rs::cache_distinguishes_negative_zero_in_lookup.
}

#[test]
fn test_prints_a_top_level_weighted() {
    // TODO: hegel-rust has no public `tc.weighted(p)` API.
    todo!()
}

#[test]
fn test_errors_when_using_frozen() {
    // TODO: hegel-rust's TestCase has no public `mark_status`, `choice`, or
    // `forced_choice` API. The native NativeTestCase enforces a "Frozen"
    // panic via its private `pre_choice` (src/native/core/state.rs), but it
    // is `pub(crate)` and only meaningfully driven by the runner — there is
    // no public surface to set status before issuing a choice.
    todo!()
}

#[test]
fn test_errors_on_too_large_choice() {
    // TODO: hegel-rust's `gs::integers()` accepts the full i64 range without
    // raising; pbtkit's `tc.choice(n)` rejects n > 2**64. The closest
    // equivalent (an oversize integer min/max combination) is enforced by
    // `assert!(min_value <= max_value)` in NativeTestCase::draw_integer, not
    // by an explicit "too large" error, so there is no equivalent surface to
    // assert on.
    todo!()
}

#[test]
fn test_can_choose_full_64_bits() {
    // Port: exercise that an integer generator can produce values across the
    // full u64 range without panicking.
    check_can_generate_examples(gs::integers::<u64>());
}

#[test]
fn test_integer_choice_simplest() {
    // Ported as embedded tests on the native engine in
    // tests/embedded/native/choices_tests.rs (integer_choice_simplest_*).
    // IntegerChoice is `pub(crate)` so it can only be exercised from inside
    // the crate; this stub exists so the Python test surface is still
    // visible in this file.
}

#[test]
fn test_integer_choice_unit() {
    // Ported as embedded tests on the native engine in
    // tests/embedded/native/choices_tests.rs (integer_choice_unit_*).
    // See comment on test_integer_choice_simplest.
}

#[test]
fn test_value_punning_on_type_change() {
    // TODO: requires State, tc.draw_integer, tc.weighted, tc.mark_status, and
    // direct inspection of state.result — all pbtkit engine internals.
    todo!()
}

#[test]
fn test_forced_choice_bounds() {
    // TODO: tc.forced_choice is a pbtkit engine internal with no public
    // hegel-rust equivalent.
    todo!()
}

#[test]
fn test_malformed_database_entry() {
    // TODO: exercises pbtkit's specific serialized DB format; not applicable
    // to hegel-rust's database.
    todo!()
}

#[test]
fn test_empty_database_entry() {
    // TODO: exercises pbtkit's specific serialized DB format; not applicable
    // to hegel-rust's database.
    todo!()
}

#[test]
fn test_truncated_database_entry() {
    // TODO: exercises pbtkit's specific serialized DB format; not applicable
    // to hegel-rust's database.
    todo!()
}

#[test]
fn test_bind_deletion_valid_but_not_shorter() {
    // TODO: requires direct access to State and state.result — pbtkit engine
    // internals.
    todo!()
}

#[test]
fn test_flat_map_core() {
    // flat_map works with core types only.
    Hegel::new(|tc| {
        let (m, n) = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .flat_map(|m| {
                    hegel::tuples!(
                        gs::just(m),
                        gs::integers::<i64>().min_value(m).max_value(m + 10)
                    )
                }),
        );
        assert!(m <= n && n <= m + 10);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_filter_core() {
    // filter works with core types only.
    Hegel::new(|tc| {
        let n = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(10)
                .filter(|n: &i64| n % 2 == 0),
        );
        assert_eq!(n % 2, 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_nothing_core() {
    // TODO: hegel-rust has no `gs::nothing()` generator.
    todo!()
}

#[test]
fn test_one_of_empty_core() {
    // one_of() with no args panics (Python pbtkit raises Unsatisfiable).
    expect_panic(
        || {
            let _ = gs::one_of::<i64>(vec![]);
        },
        "one_of requires at least one generator",
    );
}

#[test]
fn test_one_of_single_core() {
    // one_of with a single generator passes through.
    Hegel::new(|tc| {
        let n = tc.draw(gs::one_of(vec![
            gs::integers::<i64>().min_value(0).max_value(10).boxed(),
        ]));
        assert!((0..=10).contains(&n));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_core() {
    // sampled_from with basic values.
    Hegel::new(|tc| {
        let v = tc.draw(gs::sampled_from(vec!["a", "b", "c"]));
        assert!(["a", "b", "c"].contains(&v));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_empty_core() {
    // sampled_from([]) panics in hegel-rust (Python pbtkit raises Unsatisfiable).
    expect_panic(
        || {
            let _ = gs::sampled_from::<i64>(vec![]);
        },
        "Collection passed to sampled_from cannot be empty",
    );
}

#[test]
fn test_sampled_from_single_core() {
    // sampled_from with one element returns just that.
    Hegel::new(|tc| {
        let v = tc.draw(gs::sampled_from(vec!["only"]));
        assert_eq!(v, "only");
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_just_core() {
    // just(v) always returns v.
    Hegel::new(|tc| {
        let v = tc.draw(gs::just(42i64));
        assert_eq!(v, 42);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_weighted_forced_true() {
    // TODO: hegel-rust has no public `tc.weighted(p)` API.
    todo!()
}

#[test]
fn test_draw_silent_does_not_print() {
    // draw_silent must not record the drawn value, so the failing-replay
    // output must contain no `let ... = ...;` line for it.
    const CODE: &str = r#"
use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        let n: i64 = tc.draw_silent(gs::integers());
        assert!(n != 0 || true);
        panic!("intentional failure");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(CODE)
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    // No "let <name> = <value>;" record should appear, since the only draw
    // was silent. (The other tests in this suite confirm that visible draws
    // do produce these lines on the failing replay.)
    let re = regex::Regex::new(r"let \w+ = .+;").unwrap();
    assert!(
        !re.is_match(&output.stderr),
        "draw_silent leaked draw output:\n{}",
        output.stderr
    );
}

#[test]
fn test_note_prints_on_failing_example() {
    // note() must appear on the final failing replay.
    const CODE: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        tc.note("hello from note");
        panic!("intentional failure");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(CODE)
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    assert!(
        output.stderr.contains("hello from note"),
        "note message not found in failing-replay output:\n{}",
        output.stderr
    );
}

#[cfg(feature = "native")]
#[test]
fn test_database_round_trip_with_booleans() {
    // Database round-trip with a boolean-valued failing case. Verifies that
    // a Boolean choice survives serialization and is correctly replayed.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    let db_str = db_path.to_str().unwrap();

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();

    let test_code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

fn record() {{
    let path = format!("{{}}/log", std::env::var("VALUES_DIR").unwrap());
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "x").unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_bool(tc: hegel::TestCase) {{
    record();
    let b: bool = tc.draw(gs::booleans());
    assert!(!b);
}}
"#
    );

    let project = TempRustProject::new()
        .test_file("integration.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        .expect_failure("FAILED");

    project.cargo_test(&["test_bool"]);
    let log_path = values_path.join("log");
    let first_count = std::fs::read_to_string(&log_path).unwrap().lines().count();

    std::fs::remove_file(&log_path).unwrap();
    project.cargo_test(&["test_bool"]);
    let second_count = std::fs::read_to_string(&log_path).unwrap().lines().count();

    // Replay should run fewer cases than the original search.
    assert!(
        second_count <= first_count,
        "Expected replay to be no longer than original ({second_count} > {first_count})"
    );
}

#[test]
fn test_map_core() {
    // Generator.map works with core types.
    Hegel::new(|tc| {
        let n = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .map(|n| n * 2),
        );
        assert_eq!(n % 2, 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_delete_chunks_stale_index() {
    // TODO: hegel-rust's Shrinker is public but its individual passes
    // (`delete_chunks`, etc.) are `pub(super)` on src/native/shrinker/, and
    // there is no public surface that returns the shrunk choice sequence
    // from a test run for asserting on length.
    todo!()
}

#[test]
fn test_delete_chunks_guard_after_decrement() {
    // TODO: same blocker as test_delete_chunks_stale_index — the Shrinker's
    // `delete_chunks` pass is `pub(super)` and pbtkit's `SHRINK_PASSES`
    // global registry has no equivalent in hegel-rust.
    todo!()
}

#[test]
fn test_run_test_with_preseeded_result() {
    // TODO: pbtkit overrides `State.__init__` to pre-seed `state.result`;
    // hegel-rust's NativeRunner does not expose any equivalent injection
    // surface and `state.result` is internal.
    todo!()
}

#[test]
fn test_targeting_skips_non_integer() {
    // TODO: hegel-rust has no public `tc.target(score)` API.
    todo!()
}

#[test]
fn test_bin_search_down_lo_satisfies() {
    // Ported as embedded test
    // tests/embedded/native/shrinker_tests.rs::bin_search_down_returns_lo_when_lo_satisfies
    // (the helper is `pub(super)` inside the shrinker module).
}

#[test]
fn test_sort_key_type_mismatch() {
    // TODO: hegel-rust has FloatChoice::sort_index but no StringChoice or
    // BytesChoice (pbtkit's text/bytes shrinking lives in different
    // shrinker passes). And FloatChoice::sort_index requires an f64
    // argument — there is no equivalent of pbtkit's "wrong-type value"
    // graceful sort_key behavior.
    todo!()
}

#[test]
fn test_shrink_duplicates_with_stale_indices() {
    // TODO: reproduction of a specific pbtsmith-triggered shrinker crash.
    // The Rust shrinker internals differ and the Python test uses
    // @gs.composite recursion plus tree_leaves/tree_nodes/tree_size helpers
    // that don't have a clean direct translation; a faithful port is out of
    // scope for the mechanical translation.
    todo!()
}

#[test]
fn test_generator_repr() {
    // TODO: Python's __repr__ returns "integers(min_value=0, max_value=10)";
    // hegel-rust generators have no comparable stable Debug/Display format,
    // and there is no public API for stringifying a generator's parameters.
    todo!()
}

#[test]
fn test_redistribute_binary_search() {
    // TODO: redistribute_sequence_pair is a pbtkit shrinking internal with no
    // public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_shrink_duplicates_valid_drops_below_two() {
    // TODO: SHRINK_PASSES / Shrinker / TC.for_choices are pbtkit engine
    // internals with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_redistribute_integers_stale_indices() {
    // TODO: SHRINK_PASSES / Shrinker / ChoiceNode are pbtkit engine internals
    // with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_bind_deletion_try_deletions_succeeds() {
    // TODO: SHRINK_PASSES / Shrinker / ChoiceNode are pbtkit engine internals
    // with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_sort_values_full_sort_fails() {
    // TODO: SHRINK_PASSES / Shrinker / ChoiceNode are pbtkit engine internals
    // with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_swap_adjacent_blocks_equal_blocks() {
    // TODO: SHRINK_PASSES / Shrinker / ChoiceNode are pbtkit engine internals
    // with no public hegel-rust equivalent.
    todo!()
}
