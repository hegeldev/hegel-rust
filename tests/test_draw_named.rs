mod common;

use common::project::TempRustProject;
use common::utils::{assert_matches_regex, expect_panic};
use hegel::TestCase;
use hegel::generators;

// ============================================================
// draw_named runtime behavior
// ============================================================

#[test]
fn test_draw_named_non_repeatable_reuse_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                let _a = tc.draw_named(generators::booleans(), "x", false);
                let _b = tc.draw_named(generators::booleans(), "x", false);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"draw_named.*"x".*more than once"#,
    );
}

#[hegel::test(test_cases = 1)]
fn test_draw_named_repeatable_reuse_ok(tc: TestCase) {
    let _a = tc.draw_named(generators::booleans(), "x", true);
    let _b = tc.draw_named(generators::booleans(), "x", true);
}

#[hegel::test(test_cases = 1)]
fn test_draw_named_different_names_ok(tc: TestCase) {
    let _a = tc.draw_named(generators::booleans(), "x", false);
    let _b = tc.draw_named(generators::booleans(), "y", false);
}

#[test]
fn test_draw_named_mixed_repeatable_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                let _a = tc.draw_named(generators::booleans(), "x", false);
                let _b = tc.draw_named(generators::booleans(), "x", true);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"draw_named.*inconsistent.*repeatable"#,
    );
}

#[test]
fn test_draw_named_mixed_repeatable_reverse_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                let _a = tc.draw_named(generators::booleans(), "x", true);
                let _b = tc.draw_named(generators::booleans(), "x", false);
            })
            .settings(hegel::Settings::new().test_cases(1))
            .run();
        },
        r#"draw_named.*inconsistent.*repeatable"#,
    );
}

// ============================================================
// draw_named output format (via TempRustProject)
// ============================================================

#[test]
fn test_draw_named_non_repeatable_output_format() {
    let code = r#"
fn main() {
    hegel::hegel(|tc| {
        let _x = tc.draw_named(hegel::generators::integers::<i32>(), "my_var", false);
        panic!("intentional");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("intentional")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, r"let my_var = -?\d+;");
    assert!(
        !output.stderr.contains("my_var_"),
        "Non-repeatable should not have suffix. Actual: {}",
        output.stderr
    );
}

#[test]
fn test_draw_named_repeatable_output_format() {
    let code = r#"
fn main() {
    hegel::hegel(|tc| {
        let _a = tc.draw_named(hegel::generators::integers::<i32>(), "val", true);
        let _b = tc.draw_named(hegel::generators::integers::<i32>(), "val", true);
        panic!("intentional");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("intentional")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, r"let val_1 = -?\d+;");
    assert_matches_regex(&output.stderr, r"let val_2 = -?\d+;");
}

// ============================================================
// Macro rewriting: succeeding tests (inline)
// ============================================================

#[hegel::test(test_cases = 1)]
fn test_macro_unique_names_at_top_level(tc: TestCase) {
    let x = tc.draw(generators::booleans());
    let y = tc.draw(generators::booleans());
    let _ = (x, y);
}

#[hegel::test(test_cases = 1)]
fn test_macro_for_loop_is_repeatable(tc: TestCase) {
    for _ in 0..3 {
        let val = tc.draw(generators::booleans());
        let _ = val;
    }
}

#[hegel::test(test_cases = 1)]
fn test_macro_while_loop_is_repeatable(tc: TestCase) {
    let mut i = 0;
    while i < 3 {
        let val = tc.draw(generators::booleans());
        let _ = val;
        i += 1;
    }
}

#[hegel::test(test_cases = 1)]
fn test_macro_loop_is_repeatable(tc: TestCase) {
    let mut i = 0;
    loop {
        let val = tc.draw(generators::booleans());
        let _ = val;
        i += 1;
        if i >= 3 {
            break;
        }
    }
}

#[hegel::test(test_cases = 1)]
fn test_macro_closure_is_repeatable(tc: TestCase) {
    #[allow(clippy::let_and_return)]
    let f = || {
        let val = tc.draw(generators::booleans());
        val
    };
    let _a = f();
    let _b = f();
}

#[hegel::test(test_cases = 1)]
fn test_macro_non_assignment_draw_not_rewritten(tc: TestCase) {
    // draw calls not in `let x = tc.draw(...)` form stay as draw(),
    // which delegates to draw_named("draw", true) — repeatable, so no panic.
    let _ = vec![
        tc.draw(generators::booleans()),
        tc.draw(generators::booleans()),
    ];
}

#[hegel::test(test_cases = 1)]
fn test_macro_type_annotated_draw(tc: TestCase) {
    let x: bool = tc.draw(generators::booleans());
    let y: bool = tc.draw(generators::booleans());
    let _ = (x, y);
}

#[hegel::test(test_cases = 1)]
fn test_macro_draw_in_if_is_repeatable(tc: TestCase) {
    // Draw inside an if block is repeatable (block scope allows variable shadowing).
    // Using unique names here, so this trivially succeeds.
    if true {
        let a = tc.draw(generators::booleans());
        let _ = a;
    }
    let b = tc.draw(generators::booleans());
    let _ = b;
}

#[hegel::test(test_cases = 1)]
fn test_macro_variable_shadowing_in_block(tc: TestCase) {
    // Same variable name at top level and inside a block should work,
    // because the block-nested draw is repeatable (shadowing is expected).
    let x = tc.draw(generators::booleans());
    let _ = x;
    {
        let x = tc.draw(generators::booleans());
        let _ = x;
    }
}

#[hegel::test(test_cases = 1)]
fn test_macro_shadowing_in_if_block(tc: TestCase) {
    let x = tc.draw(generators::booleans());
    let _ = x;
    if true {
        let x = tc.draw(generators::booleans());
        let _ = x;
    }
}

// ============================================================
// Macro rewriting: failing tests (via TempRustProject)
// ============================================================

#[test]
fn test_macro_top_level_same_name_panics() {
    let code = r#"
#[hegel::test(test_cases = 1)]
fn test_dup(tc: hegel::TestCase) {
    let x = tc.draw(hegel::generators::booleans());
    let x = tc.draw(hegel::generators::booleans());
    let _ = x;
}
"#;
    TempRustProject::new()
        .test_file("test_dup.rs", code)
        .expect_failure(r#"draw_named.*"x".*more than once"#)
        .cargo_test(&["--test", "test_dup"]);
}

#[test]
fn test_macro_if_block_same_name_ok() {
    // Draw inside if block is repeatable due to potential shadowing,
    // so reusing the same name across the if body and outside is fine.
    let code = r#"
#[hegel::test(test_cases = 1)]
fn test_if_dup(tc: hegel::TestCase) {
    if true {
        let x = tc.draw(hegel::generators::booleans());
        let _ = x;
    }
    let x = tc.draw(hegel::generators::booleans());
    let _ = x;
}
"#;
    TempRustProject::new()
        .test_file("test_if_dup.rs", code)
        .cargo_test(&["--test", "test_if_dup"]);
}

// ============================================================
// Macro rewriting: output format (via TempRustProject)
// ============================================================

#[test]
fn test_macro_output_uses_variable_name() {
    let code = r#"
fn main() {
    hegel::hegel(|tc| {
        // Simulate what #[hegel::test] would produce for:
        //   let my_number: i32 = tc.draw(generators::integers());
        let my_number: i32 = tc.draw_named(hegel::generators::integers(), "my_number", false);
        panic!("fail: {}", my_number);
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("fail")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, r"let my_number = -?\d+;");
}

#[test]
fn test_macro_loop_output_has_counter() {
    let code = r#"
fn main() {
    hegel::hegel(|tc| {
        // Simulate what #[hegel::test] would produce for a loop:
        //   for _ in 0..2 { let val: i32 = tc.draw(generators::integers()); }
        for _ in 0..2 {
            let val: i32 = tc.draw_named(hegel::generators::integers(), "val", true);
            let _ = val;
        }
        panic!("fail");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("fail")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, r"let val_1 = -?\d+;");
    assert_matches_regex(&output.stderr, r"let val_2 = -?\d+;");
}
