//! Draw-name rewriting by the `#[hegel::test]` macro, tested fully
//! in-process: each case is a real `#[hegel::test(test_cases = 1)]` function
//! (marked `#[ignore]` so libtest doesn't run its deliberately-failing body)
//! that its sibling driver test calls directly, capturing the final replay's
//! draw-output lines through `hegel::with_output_override`.

mod common;

use common::utils::{capture_draw_lines, expect_panic};
use hegel::generators as gs;

/// Call a fixture and return its replayed draw-output lines.
fn draw_lines(fixture: fn()) -> Vec<String> {
    capture_draw_lines(fixture, "__draw_lines_fail")
}

/// Define an `#[ignore]`d `#[hegel::test]` fixture whose body ends in a
/// panic (so the final replay prints its draws), plus a driver test
/// asserting the exact draw-output lines.
macro_rules! draw_lines_case {
    ($driver:ident, $fixture:ident, $tc:ident, { $($body:tt)* }, [$($expected:expr),* $(,)?]) => {
        #[allow(unused_variables, unused_mut, clippy::let_unit_value, clippy::let_and_return)]
        #[hegel::test(test_cases = 1)]
        #[ignore = "fixture: driven in-process by the sibling driver test"]
        fn $fixture($tc: hegel::TestCase) {
            $($body)*
            panic!("__draw_lines_fail");
        }

        #[test]
        fn $driver() {
            let lines = crate::draw_lines($fixture);
            let expected: Vec<&str> = vec![$($expected),*];
            assert_eq!(lines, expected, "draw lines mismatch");
        }
    };
}

draw_lines_case!(
    test_macro_unique_names_at_top_level,
    unique_names_at_top_level_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        let y = tc.draw(gs::booleans());
    },
    ["let x = false;", "let y = false;"]
);

draw_lines_case!(
    test_macro_for_loop_is_repeatable,
    for_loop_is_repeatable_fixture,
    tc,
    {
        for _ in 0..3 {
            let val = tc.draw(gs::booleans());
        }
    },
    [
        "let val_1 = false;",
        "let val_2 = false;",
        "let val_3 = false;"
    ]
);

draw_lines_case!(
    test_macro_while_loop_is_repeatable,
    while_loop_is_repeatable_fixture,
    tc,
    {
        let mut i = 0;
        while i < 3 {
            let val = tc.draw(gs::booleans());
            i += 1;
        }
    },
    [
        "let val_1 = false;",
        "let val_2 = false;",
        "let val_3 = false;"
    ]
);

draw_lines_case!(
    test_macro_loop_is_repeatable,
    loop_is_repeatable_fixture,
    tc,
    {
        let mut i = 0;
        loop {
            let val = tc.draw(gs::booleans());
            i += 1;
            if i >= 3 {
                break;
            }
        }
    },
    [
        "let val_1 = false;",
        "let val_2 = false;",
        "let val_3 = false;"
    ]
);

draw_lines_case!(
    test_macro_closure_is_repeatable,
    closure_is_repeatable_fixture,
    tc,
    {
        let f = || {
            let val = tc.draw(gs::booleans());
            val
        };
        let a = f();
        let b = f();
    },
    ["let val_1 = false;", "let val_2 = false;"]
);

draw_lines_case!(
    test_macro_non_assignment_draw_not_rewritten,
    non_assignment_draw_not_rewritten_fixture,
    tc,
    {
        let _ = vec![tc.draw(gs::booleans()), tc.draw(gs::booleans())];
    },
    ["let draw_1 = false;", "let draw_2 = false;"]
);

draw_lines_case!(
    test_macro_type_annotated_draw,
    type_annotated_draw_fixture,
    tc,
    {
        let x: bool = tc.draw(gs::booleans());
        let y: bool = tc.draw(gs::booleans());
    },
    ["let x = false;", "let y = false;"]
);

draw_lines_case!(
    test_macro_draw_in_if_is_repeatable,
    draw_in_if_is_repeatable_fixture,
    tc,
    {
        if true {
            let a = tc.draw(gs::booleans());
        }
        let b = tc.draw(gs::booleans());
    },
    ["let a_1 = false;", "let b = false;"]
);

draw_lines_case!(
    test_macro_variable_shadowing_in_block,
    variable_shadowing_in_block_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        {
            let x = tc.draw(gs::booleans());
        }
    },
    ["let x_1 = false;", "let x_2 = false;"]
);

draw_lines_case!(
    test_macro_shadowing_in_if_block,
    shadowing_in_if_block_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        if true {
            let x = tc.draw(gs::booleans());
        }
    },
    ["let x_1 = false;", "let x_2 = false;"]
);

draw_lines_case!(
    test_macro_shadowing_across_block_types,
    shadowing_across_block_types_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
        let f = || {
            let x = tc.draw(gs::booleans());
        };
        f();
    },
    [
        "let x_1 = false;",
        "let x_2 = false;",
        "let x_3 = false;",
        "let x_4 = false;"
    ]
);

draw_lines_case!(
    test_macro_shadowing_with_different_generator_types,
    shadowing_with_different_generator_types_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        {
            let x: i32 = tc.draw(gs::integers());
        }
    },
    ["let x_1 = false;", "let x_2 = 0;"]
);

draw_lines_case!(
    test_macro_shadowing_nested_blocks,
    shadowing_nested_blocks_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        {
            let x = tc.draw(gs::booleans());
            {
                let x = tc.draw(gs::booleans());
            }
        }
    },
    ["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
);

draw_lines_case!(
    test_macro_shadowing_only_in_nested_contexts,
    shadowing_only_in_nested_contexts_fixture,
    tc,
    {
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
        {
            let x = tc.draw(gs::booleans());
        }
    },
    ["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
);

draw_lines_case!(
    test_macro_repeatable_skips_taken_name,
    repeatable_skips_taken_name_fixture,
    tc,
    {
        let x_1 = tc.draw(gs::booleans());
        for _ in 0..2 {
            let x = tc.draw(gs::booleans());
        }
    },
    ["let x_1 = false;", "let x_2 = false;", "let x_3 = false;"]
);

draw_lines_case!(
    test_macro_if_block_same_name_ok,
    if_block_same_name_ok_fixture,
    tc,
    {
        if true {
            let x = tc.draw(gs::booleans());
        }
        let x = tc.draw(gs::booleans());
    },
    ["let x_1 = false;", "let x_2 = false;"]
);

draw_lines_case!(
    test_macro_output_uses_variable_name,
    output_uses_variable_name_fixture,
    tc,
    {
        let my_number: i32 = tc.draw(gs::integers());
    },
    ["let my_number = 0;"]
);

draw_lines_case!(
    test_macro_loop_output_has_counter,
    loop_output_has_counter_fixture,
    tc,
    {
        for _ in 0..2 {
            let val: i32 = tc.draw(gs::integers());
        }
    },
    ["let val_1 = 0;", "let val_2 = 0;"]
);

draw_lines_case!(
    test_macro_bare_block_output_has_suffix,
    bare_block_output_has_suffix_fixture,
    tc,
    {
        {
            let x: i32 = tc.draw(gs::integers());
        }
    },
    ["let x_1 = 0;"]
);

draw_lines_case!(
    test_limitation_aliased_tc_not_rewritten,
    aliased_tc_not_rewritten_fixture,
    tc,
    {
        let t = tc.clone();
        let my_var = t.draw(gs::integers::<i32>());
    },
    ["let draw_1 = 0;"]
);

draw_lines_case!(
    test_limitation_draw_not_in_let_binding,
    draw_not_in_let_binding_fixture,
    tc,
    {
        let _ = vec![
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
        ];
    },
    ["let draw_1 = 0;", "let draw_2 = 0;"]
);

draw_lines_case!(
    test_limitation_destructuring_pattern,
    destructuring_pattern_fixture,
    tc,
    {
        let (a, b) = (
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
        );
    },
    ["let draw_1 = 0;", "let draw_2 = 0;"]
);

draw_lines_case!(
    test_limitation_chained_method_on_draw,
    chained_method_on_draw_fixture,
    tc,
    {
        let x = tc.draw(gs::integers::<i32>()).wrapping_abs();
    },
    ["let draw_1 = 0;"]
);

draw_lines_case!(
    test_macro_top_level_shadowing_is_repeatable,
    top_level_shadowing_is_repeatable_fixture,
    tc,
    {
        let x = tc.draw(gs::booleans());
        let x = tc.draw(gs::booleans());
    },
    ["let x_1 = false;", "let x_2 = false;"]
);

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

    draw_lines_case!(
        test_draw_counter_increments,
        draw_counter_increments_fixture,
        tc,
        {
            let _ = (
                tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
                tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
                tc.draw(gs::integers::<i32>().min_value(0).max_value(0)),
            );
        },
        ["let draw_1 = 0;", "let draw_2 = 0;", "let draw_3 = 0;"]
    );

    draw_lines_case!(
        test_draw_uses_debug_format,
        draw_uses_debug_format_fixture,
        tc,
        {
            let _ = tc.draw(gs::just("hello"));
        },
        [r#"let draw_1 = "hello";"#]
    );

    draw_lines_case!(
        test_draw_silent_does_not_print,
        draw_silent_does_not_print_fixture,
        tc,
        {
            tc.draw_silent(gs::just(5i32));
        },
        []
    );

    draw_lines_case!(
        test_draw_named_non_repeatable_single_use,
        draw_named_non_repeatable_single_use_fixture,
        tc,
        {
            tc.__draw_named(gs::just(3i32), "x", false);
        },
        ["let x = 3;"]
    );

    draw_lines_case!(
        test_draw_named_repeatable_single_use,
        draw_named_repeatable_single_use_fixture,
        tc,
        {
            tc.__draw_named(gs::just(3i32), "x", true);
        },
        ["let x_1 = 3;"]
    );

    draw_lines_case!(
        test_draw_named_repeatable_skips_taken_suffixes,
        draw_named_repeatable_skips_taken_suffixes_fixture,
        tc,
        {
            tc.__draw_named(gs::just(0i32), "x_1", false);
            tc.__draw_named(gs::just(5i32), "x", true);
        },
        ["let x_1 = 0;", "let x_2 = 5;"]
    );

    draw_lines_case!(
        test_draw_named_repeatable_multiple_uses,
        draw_named_repeatable_multiple_uses_fixture,
        tc,
        {
            tc.__draw_named(gs::just(1i32), "x", true);
            tc.__draw_named(gs::just(2i32), "x", true);
            tc.__draw_named(gs::just(3i32), "x", true);
        },
        ["let x_1 = 1;", "let x_2 = 2;", "let x_3 = 3;"]
    );

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

    draw_lines_case!(
        test_draw_named_different_names_ok,
        draw_named_different_names_ok_fixture,
        tc,
        {
            tc.__draw_named(gs::just(1i32), "x", false);
            tc.__draw_named(gs::just(2i32), "y", false);
        },
        ["let x = 1;", "let y = 2;"]
    );

    draw_lines_case!(
        test_rewriter_top_level_assignment,
        rewriter_top_level_assignment_fixture,
        tc,
        {
            let x = tc.draw(gs::just(5i32));
        },
        ["let x = 5;"]
    );

    draw_lines_case!(
        test_rewriter_for_loop_body_is_repeatable,
        rewriter_for_loop_body_is_repeatable_fixture,
        tc,
        {
            for _ in 0..2 {
                let x = tc.draw(gs::just(0i32));
            }
        },
        ["let x_1 = 0;", "let x_2 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_while_loop_body_is_repeatable,
        rewriter_while_loop_body_is_repeatable_fixture,
        tc,
        {
            let mut i = 0;
            while i < 1 {
                let x = tc.draw(gs::just(0i32));
                i += 1;
            }
        },
        ["let x_1 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_if_body_is_repeatable,
        rewriter_if_body_is_repeatable_fixture,
        tc,
        {
            if true {
                let x = tc.draw(gs::just(0i32));
            }
        },
        ["let x_1 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_nested_block_is_repeatable,
        rewriter_nested_block_is_repeatable_fixture,
        tc,
        {
            {
                let x = tc.draw(gs::just(0i32));
            }
        },
        ["let x_1 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_name_seen_at_top_and_loop_all_repeatable,
        rewriter_name_seen_at_top_and_loop_all_repeatable_fixture,
        tc,
        {
            let x = tc.draw(gs::just(0i32));
            for _ in 0..1 {
                let x = tc.draw(gs::just(0i32));
            }
        },
        ["let x_1 = 0;", "let x_2 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_no_draws_is_noop,
        rewriter_no_draws_is_noop_fixture,
        tc,
        {},
        []
    );

    draw_lines_case!(
        test_rewriter_expression_context_not_rewritten,
        rewriter_expression_context_not_rewritten_fixture,
        tc,
        {
            assert!(tc.draw(gs::just(0i32)) >= 0);
        },
        ["let draw_1 = 0;"]
    );

    draw_lines_case!(
        test_rewriter_tuple_target_not_rewritten,
        rewriter_tuple_target_not_rewritten_fixture,
        tc,
        {
            let (_a, _b) = tc.draw(gs::tuples!(gs::just(0i32), gs::just(0i32)));
        },
        ["let draw_1 = (0, 0);"]
    );

    draw_lines_case!(
        test_rewrite_draws_output_is_named,
        rewrite_draws_output_is_named_fixture,
        tc,
        {
            let value = tc.draw(gs::just(0i32));
        },
        ["let value = 0;"]
    );

    draw_lines_case!(
        test_rewrite_draws_two_draws,
        rewrite_draws_two_draws_fixture,
        tc,
        {
            let first = tc.draw(gs::just(0i32));
            let second = tc.draw(gs::just(0i32));
        },
        ["let first = 0;", "let second = 0;"]
    );

    draw_lines_case!(
        test_rewrite_draws_final_replay_uses_rewritten_function,
        rewrite_draws_final_replay_uses_rewritten_function_fixture,
        tc,
        {
            let answer = tc.draw(gs::just(0i32));
        },
        ["let answer = 0;"]
    );

    draw_lines_case!(
        test_rewrite_draws_loop_output_numbered,
        rewrite_draws_loop_output_numbered_fixture,
        tc,
        {
            for _ in 0..2 {
                let item = tc.draw(gs::just(0i32));
            }
        },
        ["let item_1 = 0;", "let item_2 = 0;"]
    );

    draw_lines_case!(
        test_rewrite_draws_no_error_for_no_draw_function,
        rewrite_draws_no_error_for_no_draw_function_fixture,
        tc,
        {},
        []
    );

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

    draw_lines_case!(
        test_rewriter_tuple_target_mixed_with_simple,
        rewriter_tuple_target_mixed_with_simple_fixture,
        tc,
        {
            let x = tc.draw(gs::just(0i32));
            let (_a, _b) = tc.draw(gs::tuples!(gs::just(0i32), gs::just(0i32)));
        },
        ["let x = 0;", "let draw_1 = (0, 0);"]
    );

    draw_lines_case!(
        test_rewriter_nested_fn_item_does_not_break_outer_rewrite,
        rewriter_nested_fn_item_does_not_break_outer_rewrite_fixture,
        tc,
        {
            let x = tc.draw(gs::just(0i32));
            fn inner() {}
            inner();
        },
        ["let x = 0;"]
    );
}
