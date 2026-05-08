use crate::statements::{Statement, generate_statements};
use hegel::TestCase;

/// Generate a complete, valid hegel-rust program.
pub fn generate_program(tc: &TestCase) -> String {
    let stmts = generate_statements(tc);
    render_program(&stmts)
}

pub fn render_program(statements: &[Statement]) -> String {
    let mut out = String::new();

    // Imports — Generator and the std::collections imports are needed only by
    // some generated bodies, so allow unused on those.
    out.push_str("#![allow(unused_variables, unused_mut, unused_imports)]\n");
    out.push_str("use hegel::TestCase;\n");
    out.push_str("use hegel::generators;\n");
    out.push_str("use hegel::generators::Generator;\n");
    out.push_str("use hegel::{Hegel, Settings, HealthCheck};\n");
    out.push_str("use std::collections::{HashMap, HashSet};\n");
    out.push('\n');

    // Main function
    out.push_str("fn main() {\n");
    out.push_str("    Hegel::new(|tc: TestCase| {\n");

    for stmt in statements {
        for line in stmt.render().lines() {
            out.push_str(&format!("        {line}\n"));
        }
    }

    out.push_str("    })\n");
    out.push_str("    .settings(Settings::new()\n");
    out.push_str("        .test_cases(10)\n");
    out.push_str(
        "        .suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow]))\n",
    );
    out.push_str("    .run();\n");
    out.push_str("}\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen_expr::{GenExpr, IntType};

    #[test]
    fn test_render_empty_program() {
        let program = render_program(&[]);
        assert!(program.contains("use hegel::TestCase;"));
        assert!(program.contains("use hegel::generators;"));
        assert!(program.contains("fn main() {"));
        assert!(program.contains("Hegel::new(|tc: TestCase| {"));
        assert!(program.contains(".test_cases(10)"));
        assert!(program.contains(".run();"));
    }

    #[test]
    fn test_render_program_with_draw_and_assert() {
        let stmts = vec![
            Statement::Draw {
                var_name: "v0".into(),
                type_annotation: crate::types::RustType::I32,
                gen_expr: GenExpr::Integers {
                    int_type: IntType::I32,
                    min: None,
                    max: None,
                },
            },
            Statement::Assert {
                condition: "v0 == v0".into(),
            },
        ];
        let program = render_program(&stmts);
        assert!(program.contains("let v0: i32 = tc.draw(generators::integers::<i32>());"));
        assert!(program.contains("assert!(v0 == v0);"));
    }

    #[test]
    fn test_render_program_is_valid_structure() {
        let stmts = vec![Statement::Assert {
            condition: "true".into(),
        }];
        let program = render_program(&stmts);

        // Check the program has balanced braces
        let opens: usize = program.chars().filter(|c| *c == '{').count();
        let closes: usize = program.chars().filter(|c| *c == '}').count();
        assert_eq!(opens, closes);

        // Check it ends with a closing brace
        assert!(program.trim().ends_with('}'));
    }
}
