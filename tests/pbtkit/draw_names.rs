//! Ported from resources/pbtkit/tests/test_draw_names.py
//!
//! pbtkit's `draw_names` module is a Python-source rewriter (libcst + runtime
//! `inspect.getsource` + import-time monkey-patching of `TestCase`) that turns
//! `x = tc.draw(gen)` into `tc.draw_named(gen, "x", repeatable)`. Hegel-rust's
//! equivalent is the `#[hegel::test]` proc macro, which does the same rewrite
//! at compile time.
//!
//! Individually-skipped tests (rest of the file is ported):
//!
//! - Section A `test_draw_counter_resets_per_test_case`,
//!   `test_draw_counter_only_fires_when_print_results`: access
//!   `tc._draw_counter` on pbtkit's `TestCase` — a Python-internal attribute
//!   with no hegel-rust counterpart.
//! - Section A `test_choice_output_unchanged`: pbtkit's `tc.choice(n)` prints
//!   `choice(5): …`. Hegel-rust models the same via
//!   `tc.draw(gs::integers().min_value(0).max_value(n-1))` whose output shape
//!   is the generic `let draw_N = …;` format — the pbtkit-specific prefix is
//!   unrepresentable.
//! - Section A `test_weighted_output_unchanged`: uses `tc.weighted(p)`; no
//!   hegel-rust counterpart on `TestCase` (same policy as the other
//!   `weighted` skips in SKIPPED.md).
//! - Section B `test_draw_named_no_print_when_print_results_false`: pbtkit's
//!   `print_results=False` flag has no equivalent on hegel-rust's `TestCase`
//!   — replay-output gating is run-level (last-run flag), not per-testcase.
//! - Section C `test_rewriter_try_block_is_repeatable`: Python `try`/`except`
//!   has no stable Rust syntactic analog (no `try` blocks, no bare-block
//!   `except`), so "draw inside a try block becomes repeatable" has no
//!   direct Rust equivalent.
//! - Section C `test_rewriter_nested_function_is_repeatable`: the upstream
//!   comment notes the inner `tc.draw(...)` is a `return` expression, not an
//!   assignment, so the test drains output but asserts nothing — no
//!   observable behaviour to pin.
//! - Section D `test_auto_rewriting_without_decorator`: pbtkit's import-time
//!   `TestCase` monkey-patching is replaced in hegel-rust by the always-on
//!   `#[hegel::test]` macro — no "importing a module flips a switch" surface.
//! - Section D `test_rewrite_draws_with_closure`: tests that pbtkit's libcst
//!   rewriter preserves Python `__closure__` cell references. Rust's
//!   proc-macro rewrite operates on tokens, so closure-variable preservation
//!   is not a meaningful rewriter concern.
//! - Section E `test_importing_draw_names_enables_auto_rewriting`: same
//!   import-time monkey-patching as the Section D entry above.
//! - Section E `test_draw_named_stub_raises_before_import`: tests pbtkit's
//!   stub-before-import behaviour (`NotImplementedError` if `draw_names`
//!   isn't imported). Hegel-rust has no such stub; `__draw_named` is always
//!   available on `TestCase`.
//! - Section F `test_collector_trystar_marks_repeatable`,
//!   `test_collector_classdef_marks_repeatable`,
//!   `test_collector_chained_assignment_skipped`: direct uses of
//!   `cst.parse_module(...)` + `_DrawNameCollector` — external Python
//!   library (libcst) integration with no Rust surface.
//! - Section F `test_rewriter_multiple_targets_in_same_fn`: exercises Python
//!   chained assignment (`a = b = tc.draw(...)`), a Python-syntax construct
//!   that doesn't exist in Rust.
//! - Section F `test_rewriter_kwdefaults_preserved`: asserts
//!   `rewritten.__kwdefaults__ == {...}` — Python-specific
//!   keyword-only-default machinery.
//! - Section F `test_rewriter_draw_with_no_args`: pbtkit's `tc.draw()` takes
//!   no argument; hegel-rust's `tc.draw(g)` requires a generator, so the
//!   zero-arg case is unrepresentable in the Rust type system.
//! - Section F `test_rewrite_fallback_on_bad_source`: tests pbtkit's
//!   `inspect.getsource` fallback (runtime Python source reflection); the
//!   proc macro has no equivalent failure mode.
//! - Section F `test_hook_noop_when_original_test_is_none`: exercises
//!   pbtkit's internal `_draw_names_hook` against a `PbtkitState` with
//!   `_original_test is None` — an internal hook with no Rust counterpart.

use crate::common::utils::expect_panic;
use hegel::generators as gs;

// ---------------------------------------------------------------------------
// Section A: Basic draw counter
// ---------------------------------------------------------------------------

#[test]
fn test_draw_counter_increments() {
    // Draws not in `let x = tc.draw(...)` form stay as `draw()`, which
    // delegates to `__draw_named("draw", true)` — so three draws yield
    // `draw_1`, `draw_2`, `draw_3`.
    let lines = draw_lines(
        "
        let _ = (
            tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
            tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
            tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
        );
    ",
    );
    assert_eq!(
        lines,
        vec!["let draw_1 = 0;", "let draw_2 = 0;", "let draw_3 = 0;"]
    );
}

#[test]
fn test_draw_uses_debug_format() {
    // Rust analog of pbtkit's `test_draw_uses_repr_format`: `draw()`
    // renders values via `Debug`, so `&str` values print with quotes
    // (the Rust equivalent of Python `repr()` quoting — just `"hello"`
    // instead of `'hello'`).
    let lines = draw_lines(
        r#"
        let _ = tc.draw(gs::just("hello"));
    "#,
    );
    assert_eq!(lines, vec![r#"let draw_1 = "hello";"#]);
}

#[test]
fn test_draw_silent_does_not_print() {
    // draw_silent bypasses the named-draw machinery: no `let draw_N = …;`
    // line in the replay output.
    let lines = draw_lines(
        "
        tc.draw_silent(gs::just(5i32));
    ",
    );
    assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
}

// ---------------------------------------------------------------------------
// Section B: draw_named semantics
// ---------------------------------------------------------------------------

#[test]
fn test_draw_named_non_repeatable_single_use() {
    // Single non-repeatable use: label is bare `x` (no suffix).
    let lines = draw_lines(
        r#"
        tc.__draw_named(gs::just(3i32), "x", false);
    "#,
    );
    assert_eq!(lines, vec!["let x = 3;"]);
}

#[test]
fn test_draw_named_repeatable_single_use() {
    // Single repeatable use: label is suffixed (`x_1`).
    let lines = draw_lines(
        r#"
        tc.__draw_named(gs::just(3i32), "x", true);
    "#,
    );
    assert_eq!(lines, vec!["let x_1 = 3;"]);
}

#[test]
fn test_draw_named_repeatable_skips_taken_suffixes() {
    // Repeatable numbering skips names already consumed by a prior
    // non-repeatable draw. Here `x_1` is taken non-repeatably first, so
    // the subsequent repeatable `x` must start at `x_2`.
    // (Upstream Python mutates `tc._named_draw_used` directly; the same
    // state is reachable through the public `__draw_named` API.)
    let lines = draw_lines(
        r#"
        tc.__draw_named(gs::just(0i32), "x_1", false);
        tc.__draw_named(gs::just(5i32), "x", true);
    "#,
    );
    assert_eq!(lines, vec!["let x_1 = 0;", "let x_2 = 5;"]);
}

#[test]
fn test_draw_named_repeatable_multiple_uses() {
    let lines = draw_lines(
        r#"
        tc.__draw_named(gs::just(1i32), "x", true);
        tc.__draw_named(gs::just(2i32), "x", true);
        tc.__draw_named(gs::just(3i32), "x", true);
    "#,
    );
    assert_eq!(lines, vec!["let x_1 = 1;", "let x_2 = 2;", "let x_3 = 3;"]);
}

#[test]
fn test_draw_named_non_repeatable_reuse_raises() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.__draw_named(gs::booleans(), "x", false);
                tc.__draw_named(gs::booleans(), "x", false);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"__draw_named.*"x".*more than once"#,
    );
}

#[test]
fn test_draw_named_inconsistent_flags_raises() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.__draw_named(gs::booleans(), "x", false);
                tc.__draw_named(gs::booleans(), "x", true);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"__draw_named.*inconsistent.*repeatable"#,
    );
}

#[test]
fn test_draw_named_different_names_ok() {
    let lines = draw_lines(
        r#"
        tc.__draw_named(gs::just(1i32), "x", false);
        tc.__draw_named(gs::just(2i32), "y", false);
    "#,
    );
    assert_eq!(lines, vec!["let x = 1;", "let y = 2;"]);
}

// ---------------------------------------------------------------------------
// Section C: Rewriter unit tests (rewritten as macro-output tests)
//
// Upstream tests pbtkit's libcst-based `rewrite_test_function`; here we
// exercise the `#[hegel::test]` proc-macro equivalent through its
// observable draw-output surface.
// ---------------------------------------------------------------------------

#[test]
fn test_rewriter_top_level_assignment() {
    // Top-level `let x = tc.draw(gen)` rewrites to non-repeatable
    // `__draw_named(..., "x", false)`, printing `let x = …;`.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::just(5i32));
    ",
    );
    assert_eq!(lines, vec!["let x = 5;"]);
}

#[test]
fn test_rewriter_for_loop_body_is_repeatable() {
    // A draw inside a `for` loop is repeatable — suffixed `x_1`, `x_2`.
    let lines = draw_lines(
        "
        for _ in 0..2 {
            let x = tc.draw(gs::just(0i32));
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;", "let x_2 = 0;"]);
}

#[test]
fn test_rewriter_while_loop_body_is_repeatable() {
    let lines = draw_lines(
        "
        let mut i = 0;
        while i < 1 {
            let x = tc.draw(gs::just(0i32));
            i += 1;
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;"]);
}

#[test]
fn test_rewriter_if_body_is_repeatable() {
    let lines = draw_lines(
        "
        if true {
            let x = tc.draw(gs::just(0i32));
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;"]);
}

#[test]
fn test_rewriter_nested_block_is_repeatable() {
    // Rust analog of Python's `with` block: any nested `{}` block marks
    // the draw repeatable. (Python: `with contextlib.nullcontext(): …`.)
    let lines = draw_lines(
        "
        {
            let x = tc.draw(gs::just(0i32));
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;"]);
}

#[test]
fn test_rewriter_name_seen_at_top_and_loop_all_repeatable() {
    // Same name used top-level AND in a loop → all uses become repeatable.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::just(0i32));
        for _ in 0..1 {
            let x = tc.draw(gs::just(0i32));
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;", "let x_2 = 0;"]);
}

#[test]
fn test_rewriter_no_draws_is_noop() {
    // A body with no `tc.draw(...)` calls produces no draw lines.
    let lines = draw_lines("");
    assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
}

#[test]
fn test_rewriter_expression_context_not_rewritten() {
    // `tc.draw(...)` in expression context (not a `let x = …` binding)
    // isn't rewritten — falls back to the `draw_N` counter format.
    let lines = draw_lines(
        "
        assert!(tc.draw(gs::just(0i32)) >= 0);
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}

#[test]
fn test_rewriter_tuple_target_not_rewritten() {
    // Tuple destructuring target isn't rewritten (only simple name targets
    // are) — falls back to `draw_N` format.
    let lines = draw_lines(
        "
        let (_a, _b) = tc.draw(gs::tuples!(gs::just(0i32), gs::just(0i32)));
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = (0, 0);"]);
}

// ---------------------------------------------------------------------------
// Section D: Integration tests (@hegel::test end-to-end)
// ---------------------------------------------------------------------------

#[test]
fn test_rewrite_draws_output_is_named() {
    let lines = draw_lines(
        "
        let value = tc.draw(gs::just(0i32));
    ",
    );
    assert_eq!(lines, vec!["let value = 0;"]);
}

#[test]
fn test_rewrite_draws_two_draws() {
    let lines = draw_lines(
        "
        let first = tc.draw(gs::just(0i32));
        let second = tc.draw(gs::just(0i32));
    ",
    );
    assert_eq!(lines, vec!["let first = 0;", "let second = 0;"]);
}

#[test]
fn test_rewrite_draws_final_replay_uses_rewritten_function() {
    // The final failing-example replay uses the rewritten function —
    // output carries the named binding, not the generic `draw_N`.
    let lines = draw_lines(
        "
        let answer = tc.draw(gs::just(0i32));
    ",
    );
    assert_eq!(lines, vec!["let answer = 0;"]);
}

#[test]
fn test_rewrite_draws_loop_output_numbered() {
    let lines = draw_lines(
        "
        for _ in 0..2 {
            let item = tc.draw(gs::just(0i32));
        }
    ",
    );
    assert_eq!(lines, vec!["let item_1 = 0;", "let item_2 = 0;"]);
}

#[test]
fn test_rewrite_draws_no_error_for_no_draw_function() {
    // A `#[hegel::test]` body with no draws still works — no draw output.
    let lines = draw_lines("");
    assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
}

// ---------------------------------------------------------------------------
// Section E: Full pbtkit integration
// ---------------------------------------------------------------------------

#[test]
fn test_draw_named_validation_runs_outside_composite() {
    // `__draw_named` validation (non-repeatable reuse) fires at the
    // top-level `TestCase`, independent of replay/output gating.
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.__draw_named(gs::booleans(), "x", false);
                tc.__draw_named(gs::booleans(), "x", false);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"__draw_named.*"x".*more than once"#,
    );
}

#[test]
fn test_draw_named_no_validation_inside_composite() {
    // Inside a `#[hegel::composite]` generator the nested `TestCase` runs at
    // `span_depth > 0`, so `__draw_named` skips the name-tracking validation.
    // The same non-repeatable name can therefore be used twice across two
    // `tc.draw(gen())` calls without panicking.
    hegel::Hegel::new(|tc: hegel::TestCase| {
        tc.draw(composite_reuses_inner_name());
        tc.draw(composite_reuses_inner_name());
    })
    .settings(hegel::Settings::new().test_cases(1))
    .run();
}

#[hegel::composite]
fn composite_reuses_inner_name(tc: hegel::TestCase) -> i32 {
    tc.__draw_named(gs::just(3i32), "inner", false)
}

// ---------------------------------------------------------------------------
// Section F: Rewriter edge cases (mixed bodies, nested items)
// ---------------------------------------------------------------------------

#[test]
fn test_rewriter_tuple_target_mixed_with_simple() {
    // A simple-target draw and a tuple-target draw in the same body: the
    // simple one is rewritten (`let x = …;`), the tuple one falls back to
    // the `draw_N` counter. Upstream `test_rewriter_tuple_target_when_regular_draw_present`.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::just(0i32));
        let (_a, _b) = tc.draw(gs::tuples!(gs::just(0i32), gs::just(0i32)));
    ",
    );
    assert_eq!(lines, vec!["let x = 0;", "let draw_1 = (0, 0);"]);
}

#[test]
fn test_rewriter_nested_fn_item_does_not_break_outer_rewrite() {
    // A nested `fn inner()` item alongside a draw doesn't interfere with
    // the outer rewrite — `let x = tc.draw(...)` still becomes `let x = …;`.
    // Upstream `test_rewriter_nested_funcdef_line_268`.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::just(0i32));
        fn inner() {}
        inner();
    ",
    );
    assert_eq!(lines, vec!["let x = 0;"]);
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Run a test body via `#[hegel::test]` and return the replayed draw-output
/// lines. Mirrors the helper in `tests/test_draw_named.rs` so this file can
/// live under `tests/pbtkit/` without adding a shared helper.
fn draw_lines(body: &str) -> Vec<String> {
    use crate::common::project::TempRustProject;

    let code = format!(
        r#"
use hegel::generators as gs;

#[allow(unused_variables, clippy::let_unit_value, clippy::let_and_return)]
#[hegel::test(test_cases = 1)]
fn test_body(tc: hegel::TestCase) {{
    {body}
    panic!("__draw_lines_fail");
}}
"#
    );

    let output = TempRustProject::new()
        .test_file("test_body.rs", &code)
        .expect_failure("__draw_lines_fail")
        .cargo_test(&["--test", "test_body", "--", "--nocapture"]);

    let re = regex::Regex::new(r"let \w+ = .+;").unwrap();
    re.find_iter(&output.stderr)
        .map(|m| m.as_str().to_string())
        .collect()
}
