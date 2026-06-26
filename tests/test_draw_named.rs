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
    let lines = draw_lines(
        "
        {
            let x: i32 = tc.draw(gs::integers());
        }
    ",
    );
    assert_eq!(lines, vec!["let x_1 = 0;"]);
}

#[test]
fn test_limitation_aliased_tc_not_rewritten() {
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
    let lines = draw_lines(
        "
        let x = tc.draw(gs::integers::<i32>()).wrapping_abs();
    ",
    );
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}

#[test]
fn test_macro_top_level_shadowing_is_repeatable() {
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

mod draw_names {
    //! pbtkit's `draw_names` module is a Python-source rewriter (libcst + runtime
    //! `inspect.getsource` + import-time monkey-patching of `TestCase`) that turns
    //! `x = tc.draw(gen)` into `tc.draw_named(gen, "x", repeatable)`. Hegel-rust's
    //! equivalent is the `#[hegel::test]` proc macro, which does the same rewrite
    //! at compile time.

    use super::common::utils::expect_panic;
    use hegel::generators as gs;

    #[test]
    fn test_draw_counter_increments() {
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
        let lines = draw_lines(
            r#"
            let _ = tc.draw(gs::just("hello"));
        "#,
        );
        assert_eq!(lines, vec![r#"let draw_1 = "hello";"#]);
    }

    #[test]
    fn test_draw_silent_does_not_print() {
        let lines = draw_lines(
            "
            tc.draw_silent(gs::just(5i32));
        ",
        );
        assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
    }

    #[test]
    fn test_draw_named_non_repeatable_single_use() {
        let lines = draw_lines(
            r#"
            tc.__draw_named(gs::just(3i32), "x", false);
        "#,
        );
        assert_eq!(lines, vec!["let x = 3;"]);
    }

    #[test]
    fn test_draw_named_repeatable_single_use() {
        let lines = draw_lines(
            r#"
            tc.__draw_named(gs::just(3i32), "x", true);
        "#,
        );
        assert_eq!(lines, vec!["let x_1 = 3;"]);
    }

    #[test]
    fn test_draw_named_repeatable_skips_taken_suffixes() {
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

    #[test]
    fn test_rewriter_top_level_assignment() {
        let lines = draw_lines(
            "
            let x = tc.draw(gs::just(5i32));
        ",
        );
        assert_eq!(lines, vec!["let x = 5;"]);
    }

    #[test]
    fn test_rewriter_for_loop_body_is_repeatable() {
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
        let lines = draw_lines("");
        assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
    }

    #[test]
    fn test_rewriter_expression_context_not_rewritten() {
        let lines = draw_lines(
            "
            assert!(tc.draw(gs::just(0i32)) >= 0);
        ",
        );
        assert_eq!(lines, vec!["let draw_1 = 0;"]);
    }

    #[test]
    fn test_rewriter_tuple_target_not_rewritten() {
        let lines = draw_lines(
            "
            let (_a, _b) = tc.draw(gs::tuples!(gs::just(0i32), gs::just(0i32)));
        ",
        );
        assert_eq!(lines, vec!["let draw_1 = (0, 0);"]);
    }

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
        let lines = draw_lines("");
        assert!(lines.is_empty(), "expected no draw lines, got {lines:?}");
    }

    #[test]
    fn test_draw_named_validation_runs_outside_composite() {
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

    #[test]
    fn test_rewriter_tuple_target_mixed_with_simple() {
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
        let lines = draw_lines(
            "
            let x = tc.draw(gs::just(0i32));
            fn inner() {}
            inner();
        ",
        );
        assert_eq!(lines, vec!["let x = 0;"]);
    }

    /// Run a test body via `#[hegel::test]` and return the replayed draw-output
    /// lines. Mirrors the helper in `tests/test_draw_named.rs` so this file can
    /// live under `tests/pbtkit/` without adding a shared helper.
    fn draw_lines(body: &str) -> Vec<String> {
        use super::common::project::TempRustProject;

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
}
