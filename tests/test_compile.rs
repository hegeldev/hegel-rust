//! Compile-time tests for hegel's proc macros.
//!
//! **Pass tests** are included as modules: if they compile as part of this
//! binary they pass; no runtime work needed.
//!
//! **Fail tests** each spawn a `cargo check` against a temporary crate that
//! depends on `hegeltest`. They reuse the workspace's own target directory
//! (where `cargo test`'s build phase already compiled every dependency), so
//! each invocation is just a type-check of the tiny fixture file (~0.2 s).
//!
//! This replaced a trybuild-based approach that maintained its own target
//! directory and recompiled the full dependency graph on cold cache,
//! routinely exceeding 60 s on CI.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Pass tests ──────────────────────────────────────────────────────────
// Including the fixture as a module verifies it compiles under exactly the
// same conditions as a standalone crate (module scope has no extra imports).

#[allow(dead_code, unused_imports)]
#[path = "compile/pass/derive_compiles_without_generator_trait_import.rs"]
mod pass_derive_without_trait_import;

#[allow(dead_code, unused_imports)]
#[path = "compile/pass/composite_successful_expansion.rs"]
mod pass_composite_expansion;

#[allow(dead_code, unused_imports, unexpected_cfgs)]
#[path = "compile/pass/stateful_cfg_attributes_are_copied_to_rules.rs"]
mod pass_stateful_cfg_attrs;

// ── Fail-test infrastructure ────────────────────────────────────────────

static CHECK_ID: AtomicU64 = AtomicU64::new(0);

fn target_dir() -> PathBuf {
    match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"),
    }
}

fn assert_compile_fails(code: &str, expected_error: &str) {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir = temp_dir.path();
    let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), code).unwrap();

    let id = CHECK_ID.fetch_add(1, Ordering::Relaxed);
    let cargo_toml = format!(
        "[package]\n\
         name = \"compile-fail-{pid}-{id}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         \n\
         [dependencies]\n\
         hegeltest = {{ path = \"{path}\" }}\n",
        pid = std::process::id(),
        id = id,
        path = hegel_path.display(),
    );
    std::fs::write(dir.join("Cargo.toml"), &cargo_toml).unwrap();

    let lock_src = hegel_path.join("Cargo.lock");
    if lock_src.exists() {
        std::fs::copy(&lock_src, dir.join("Cargo.lock")).unwrap();
    }

    let output = Command::new(env!("CARGO"))
        .args(["check", "--quiet", "--offline"])
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target_dir())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "Expected compilation to fail but it succeeded",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected_error),
        "Expected error containing: {expected_error}\nActual stderr:\n{stderr}",
    );
}

// ── Fail tests ──────────────────────────────────────────────────────────

#[test]
fn fail_explicit_test_case_bad_syntax() {
    assert_compile_fails(
        include_str!("compile/fail/explicit_test_case_bad_syntax.rs"),
        "expected `,`",
    );
}

#[test]
fn fail_explicit_test_case_no_parens() {
    assert_compile_fails(
        include_str!("compile/fail/explicit_test_case_no_parens.rs"),
        "#[hegel::explicit_test_case] requires arguments.",
    );
}

#[test]
fn fail_explicit_test_case_on_bare_function() {
    assert_compile_fails(
        include_str!("compile/fail/explicit_test_case_on_bare_function.rs"),
        "#[hegel::explicit_test_case] can only be used together with #[hegel::test].",
    );
}

#[test]
fn fail_explicit_test_case_wrong_order() {
    assert_compile_fails(
        include_str!("compile/fail/explicit_test_case_wrong_order.rs"),
        "#[hegel::explicit_test_case] must appear below #[hegel::test], not above it.",
    );
}

#[test]
fn fail_explicit_test_case_empty_args() {
    assert_compile_fails(
        include_str!("compile/fail/explicit_test_case_empty_args.rs"),
        "#[hegel::explicit_test_case] requires at least one name = value pair.",
    );
}

#[test]
fn fail_composite_nullary() {
    assert_compile_fails(
        include_str!("compile/fail/composite_nullary.rs"),
        "must define a first parameter of type TestCase",
    );
}

#[test]
fn fail_composite_missing_return_type() {
    assert_compile_fails(
        include_str!("compile/fail/composite_missing_return_type.rs"),
        "#[composite] generators must explicitly declare a return type.",
    );
}

#[test]
fn fail_composite_missing_test_case_parameter() {
    assert_compile_fails(
        include_str!("compile/fail/composite_missing_test_case_parameter.rs"),
        "first parameter in a #[composite] generator must have type TestCase",
    );
}

#[test]
fn fail_hegel_test_zero_params() {
    assert_compile_fails(
        include_str!("compile/fail/hegel_test_zero_params.rs"),
        "functions must take exactly one parameter of type hegel::TestCase",
    );
}

#[test]
fn fail_hegel_test_two_params() {
    assert_compile_fails(
        include_str!("compile/fail/hegel_test_two_params.rs"),
        "functions must take exactly one parameter of type hegel::TestCase",
    );
}

#[test]
fn fail_hegel_test_duplicate_test_attribute() {
    assert_compile_fails(
        include_str!("compile/fail/hegel_test_duplicate_test_attribute.rs"),
        "#[hegel::test] used on a function with #[test].",
    );
}

#[test]
fn fail_vec_unique_requires_partial_eq() {
    assert_compile_fails(
        include_str!("compile/fail/vec_unique_requires_partial_eq.rs"),
        "trait bounds were not satisfied",
    );
}
