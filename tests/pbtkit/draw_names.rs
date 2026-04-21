//! Ported from resources/pbtkit/tests/test_draw_names.py
//!
//! pbtkit's `draw_names` module is a Python-source rewriter (libcst + runtime
//! `inspect.getsource` + import-time monkey-patching of `TestCase`) that turns
//! `x = tc.draw(gen)` into `tc.draw_named(gen, "x", repeatable)`. Hegel-rust's
//! equivalent is the `#[hegel::test]` proc macro, which does the same rewrite
//! at compile time. The behavioural output coverage for the Rust macro lives
//! in `tests/test_draw_named.rs`; this port covers the pbtkit tests that
//! exercise the `__draw_named` method contract and composite-depth semantics
//! independently of the rewriter.
//!
//! Individually-skipped tests (rest of the file is ported):
//!
//! - Section A `test_draw_counter_resets_per_test_case`,
//!   `test_draw_counter_only_fires_when_print_results`: access
//!   `tc._draw_counter` on pbtkit's `TestCase` — a Python-internal attribute
//!   with no hegel-rust counterpart.
//! - Section A `test_draw_silent_does_not_print`: pbtkit's `tc.draw_silent`
//!   has no exposed-output surface in hegel-rust (`draw_silent` exists but
//!   bypasses the named-draw machinery entirely, so there's nothing to assert
//!   beyond "no panic").
//! - Section A `test_choice_output_unchanged`: pbtkit's `tc.choice(n)` prints
//!   `choice(5): …`. Hegel-rust models the same via
//!   `tc.draw(gs::integers().min_value(0).max_value(n-1))` whose output shape
//!   is the generic `let draw_N = …;` format — the pbtkit-specific prefix is
//!   unrepresentable.
//! - Section A `test_weighted_output_unchanged`: uses `tc.weighted(p)`; no
//!   hegel-rust counterpart on `TestCase` (same policy as the other
//!   `weighted` skips in SKIPPED.md).
//! - Section A `test_draw_uses_repr_format`: Python `repr()` quoting
//!   (`'hello'`); Rust's `Debug` uses `"hello"` — a format mismatch with no
//!   one-to-one mapping.
//! - Section B `test_draw_named_repeatable_skips_taken_suffixes`: mutates
//!   `tc._named_draw_used` directly — a Python-internal attribute.
//!   (The same skip-taken-suffix behaviour is covered by
//!   `tests/test_draw_named.rs::test_draw_named_repeatable_skips_taken_name`.)
//! - Section B `test_draw_named_no_print_when_print_results_false`: pbtkit's
//!   `print_results=False` flag has no equivalent on hegel-rust's `TestCase`
//!   — replay-output gating is run-level (last-run flag), not per-testcase.
//! - Section C (all 12 rewriter unit tests): test pbtkit's
//!   `rewrite_test_function` / `_DrawNameCollector`, a libcst CST visitor
//!   that rewrites Python source at runtime. Hegel-rust's equivalent is the
//!   `#[hegel::test]` proc macro — a compile-time syn-based rewrite — whose
//!   behavioural coverage lives in `tests/test_draw_named.rs`.
//! - Section D (all 7 integration tests): already covered by
//!   `tests/test_draw_named.rs` tests (`test_macro_output_uses_variable_name`,
//!   `test_macro_loop_output_has_counter`, `test_macro_closure_is_repeatable`,
//!   `test_macro_unique_names_at_top_level`, etc.), which drive the same
//!   observable output end-to-end via the proc macro.
//! - Section E `test_importing_draw_names_enables_auto_rewriting`: pbtkit's
//!   import-time monkey-patching is replaced in hegel-rust by the always-on
//!   `#[hegel::test]` macro — no "importing a module flips a switch"
//!   surface.
//! - Section E `test_draw_named_stub_raises_before_import`: tests pbtkit's
//!   stub-before-import behaviour (`NotImplementedError` if draw_names
//!   isn't imported). Hegel-rust has no such stub; `__draw_named` is always
//!   available on `TestCase`.
//! - Section E `test_draw_named_validation_runs_outside_composite`: redundant
//!   with the non-repeatable-reuse and mixed-flags tests below (the
//!   `print_results=False` angle is the Python-internal flag described
//!   above; the validation-fires-anyway assertion is the same as the panics
//!   below).
//! - Section F (all 7 CST visitor coverage tests): same libcst-specific
//!   rationale as Section C.

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
// Section E: Full pbtkit integration
// ---------------------------------------------------------------------------

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
