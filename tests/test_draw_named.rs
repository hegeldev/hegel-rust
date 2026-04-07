mod common;

use common::project::TempRustProject;
use common::utils::expect_panic;
use hegel::generators as gs;

/// Run a test body via `#[hegel::test]` and extract the draw output lines.
///
/// Wraps `body` in a `#[hegel::test(test_cases = 1)]` function, appends a panic
/// at the end to trigger output, and returns the full draw output lines
/// (e.g. `["let x = false;", "let y = false;"]`).
fn draw_lines(body: &str) -> Vec<String> {
    let code = format!(
        r#"
use hegel::generators as gs;

#[allow(unused_variables)]
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

// ============================================================
// Macro rewriting: output tests
// ============================================================

#[test]
fn test_macro_unique_names_at_top_level() {
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        let y = tc.draw(gs::booleans());
    ",
    );
    assert_eq!(lines, vec!["let x = false;", "let y = false;"]);
}

#[test]
fn test_macro_for_loop_is_repeatable() {
    let lines = draw_lines(
        "
        for _ in 0..3 {
            let val = tc.draw(gs::booleans());
        }
    ",
    );
    assert_eq!(
        lines,
        vec![
            "let val_1 = false;",
            "let val_2 = false;",
            "let val_3 = false;"
        ]
    );
}

#[test]
fn test_macro_while_loop_is_repeatable() {
    let lines = draw_lines(
        "
        let mut i = 0;
        while i < 3 {
            let val = tc.draw(gs::booleans());
            i += 1;
        }
    ",
    );
    assert_eq!(
        lines,
        vec![
            "let val_1 = false;",
            "let val_2 = false;",
            "let val_3 = false;"
        ]
    );
}

#[test]
fn test_macro_loop_is_repeatable() {
    let lines = draw_lines(
        "
        let mut i = 0;
        loop {
            let val = tc.draw(gs::booleans());
            i += 1;
            if i >= 3 {
                break;
            }
        }
    ",
    );
    assert_eq!(
        lines,
        vec![
            "let val_1 = false;",
            "let val_2 = false;",
            "let val_3 = false;"
        ]
    );
}

#[test]
fn test_macro_closure_is_repeatable() {
    let lines = draw_lines(
        "
        #[allow(clippy::let_and_return)]
        let f = || {
            let val = tc.draw(gs::booleans());
            val
        };
        let a = f();
        let b = f();
    ",
    );
    assert_eq!(lines, vec!["let val_1 = false;", "let val_2 = false;"]);
}

#[test]
fn test_macro_non_assignment_draw_not_rewritten() {
    // draw calls not in `let x = tc.draw(...)` form stay as draw(),
    // which delegates to __draw_named("draw", true) — repeatable, so no panic.
    let lines = draw_lines(
        "
        let _ = vec![
            tc.draw(gs::booleans()),
            tc.draw(gs::booleans()),
        ];
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = false;", "let draw_2 = false;"]);
}

#[test]
fn test_macro_type_annotated_draw() {
    let lines = draw_lines(
        "
        let x: bool = tc.draw(gs::booleans());
        let y: bool = tc.draw(gs::booleans());
    ",
    );
    assert_eq!(lines, vec!["let x = false;", "let y = false;"]);
}

#[test]
fn test_macro_draw_in_if_is_repeatable() {
    let lines = draw_lines(
        "
        if true {
            let a = tc.draw(gs::booleans());
        }
        let b = tc.draw(gs::booleans());
    ",
    );
    assert_eq!(lines, vec!["let a_1 = false;", "let b = false;"]);
}

#[test]
fn test_macro_variable_shadowing_in_block() {
    // Same variable name at top level and inside a block should work,
    // because the block-nested draw is repeatable (shadowing is expected).
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        {
            let x = tc.draw(gs::booleans());
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = false;", "let x_2 = false;"]);
}

#[test]
fn test_macro_shadowing_in_if_block() {
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        if true {
            let x = tc.draw(gs::booleans());
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = false;", "let x_2 = false;"]);
}

#[test]
fn test_macro_shadowing_across_block_types() {
    // Same name at top level, in a for loop, and in a closure.
    // All uses become repeatable because the loop/closure occurrences force it.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
        let f = || {
            let x = tc.draw(gs::booleans());
        };
        f();
    ",
    );
    assert_eq!(
        lines,
        vec![
            "let x_1 = false;",
            "let x_2 = false;",
            "let x_3 = false;",
            "let x_4 = false;"
        ]
    );
}

#[test]
fn test_macro_shadowing_with_different_generator_types() {
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        {
            let x: i32 = tc.draw(gs::integers());
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = false;", "let x_2 = 0;"]);
}

#[test]
fn test_macro_shadowing_nested_blocks() {
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        {
            let x = tc.draw(gs::booleans());
            {
                let x = tc.draw(gs::booleans());
            }
        }
    ",
    );
    assert_eq!(
        lines,
        vec!["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
    );
}

#[test]
fn test_macro_shadowing_only_in_nested_contexts() {
    // Name never appears at top level — only in nested blocks.
    let lines = draw_lines(
        "
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
        {
            let x = tc.draw(gs::booleans());
        }
    ",
    );
    assert_eq!(
        lines,
        vec!["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
    );
}

#[test]
fn test_macro_repeatable_skips_taken_name() {
    // _x_1 at top level is non-repeatable, _x in loop is repeatable.
    // The repeatable "_x" draws must skip "_x_1" which is already taken.
    let lines = draw_lines(
        "
        let x_1 = tc.draw(gs::booleans());
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
    ",
    );
    assert_eq!(
        lines,
        vec!["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
    );
}

#[test]
fn test_macro_if_block_same_name_ok() {
    // Draw inside if block is repeatable due to potential shadowing,
    // so reusing the same name across the if body and outside is fine.
    let lines = draw_lines(
        "
        if true {
            let x = tc.draw(gs::booleans());
        }
        let x = tc.draw(gs::booleans());
    ",
    );
    assert_eq!(lines, vec!["let x_1 = false;", "let x_2 = false;"]);
}

#[test]
fn test_macro_output_uses_variable_name() {
    let lines = draw_lines(
        "
        let my_number: i32 = tc.draw(gs::integers());
    ",
    );
    assert_eq!(lines, vec!["let my_number = 0;"]);
}

#[test]
fn test_macro_loop_output_has_counter() {
    let lines = draw_lines(
        "
        for _ in 0..2 {
            let val: i32 = tc.draw(gs::integers());
        }
    ",
    );
    assert_eq!(lines, vec!["let val_1 = 0;", "let val_2 = 0;"]);
}

#[test]
fn test_macro_bare_block_output_has_suffix() {
    // A unique name inside a bare {} gets the _1 suffix because the macro
    // treats all nested blocks as repeatable.
    let lines = draw_lines(
        "
        {
            let x: i32 = tc.draw(gs::integers());
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;"]);
}

// ============================================================
// Known limitations of syntactic rewriting
//
// The macro rewrites `let x = tc.draw(gen)` -> `tc.__draw_named(gen, "x", ...)`
// by matching syntax, not semantics. The tests below document cases where
// the macro cannot determine the variable name and falls back to the
// generic "draw_N" output. These are accepted limitations, not bugs.
// ============================================================

#[test]
fn test_limitation_aliased_tc_not_rewritten() {
    // The macro only matches `tc.draw(...)`, not draws on an alias like `t`.
    // The draw on `t` stays as draw() -> __draw_named("draw", true).
    let lines = draw_lines(
        "
        let t = tc.clone();
        let my_var = t.draw(gs::integers::<i32>());
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}

#[test]
fn test_limitation_draw_not_in_let_binding() {
    // Draws inside vec![] are not in `let x = tc.draw(...)` form,
    // so the macro does not rewrite them.
    let lines = draw_lines(
        "
        let _ = vec![
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
        ];
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;", "let draw_2 = 0;"]);
}

#[test]
fn test_limitation_destructuring_pattern() {
    // extract_ident_from_pat returns None for tuple patterns, so neither
    // draw is rewritten.
    let lines = draw_lines(
        "
        let (a, b) = (
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
        );
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;", "let draw_2 = 0;"]);
}

#[test]
fn test_limitation_chained_method_on_draw() {
    // `let _x = tc.draw(gen).abs()` — the init expression is `.abs()`, not
    // `.draw()`, so is_test_case_draw_binding doesn't match.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::integers::<i32>()).abs();
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}

#[test]
fn test_macro_top_level_shadowing_is_repeatable() {
    // Top-level variable shadowing is valid Rust. The macro detects that
    // the same name is used for multiple draws and marks it repeatable.
    let lines = draw_lines(
        "
        let x = tc.draw(gs::booleans());
        let x = tc.draw(gs::booleans());
    ",
    );
    assert_eq!(lines, vec!["let x_1 = false;", "let x_2 = false;"]);
}

#[test]
fn test_draw_named_mixed_repeatable_panics() {
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
fn test_draw_named_mixed_repeatable_reverse_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.__draw_named(gs::booleans(), "x", true);
                tc.__draw_named(gs::booleans(), "x", false);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"__draw_named.*inconsistent.*repeatable"#,
    );
}

// Covering tests. This logic is already covered by our TempRustProject tests, but those
// don't contribute to coverage.
#[test]
fn test_draw_named_non_repeatable_reuse_panics() {
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
fn test_draw_named_repeatable_skips_taken_name() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        tc.__draw_named(gs::booleans(), "x_1", false);
        tc.__draw_named(gs::booleans(), "x", true);
    })
    .settings(hegel::Settings::new().test_cases(1))
    .run();
}
