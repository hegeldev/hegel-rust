mod common;

use common::project::TempRustProject;

#[test]
fn test_reproduce_failure_on_bare_function() {
    let code = r#"
#[hegel::reproduce_failure("AAEC")]
fn my_func(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("can only be used together with.*hegel::test")
        .cargo_run(&[]);
}

#[test]
fn test_reproduce_failure_wrong_order() {
    let code = r#"
#[hegel::reproduce_failure("AAEC")]
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("must appear below.*hegel::test.*not above")
        .cargo_run(&[]);
}

/// A correct-usage attribute compiles (exercising the `#[hegel::test]`
/// wiring that injects `.reproduce_failure(...)`); at runtime an undecodable
/// blob panics with a clear message rather than passing silently.
#[test]
fn test_reproduce_failure_undecodable_blob_panics() {
    let code = r#"
#[hegel::test]
#[hegel::reproduce_failure("!!! not a blob !!!")]
fn my_test(tc: hegel::TestCase) {
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    let _ = x;
}
"#;
    TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", code)
        .expect_failure("could not be decoded")
        .cargo_test(&["--test", "repro"]);
}

/// The blob argument may be any expression, not just a string literal — e.g.
/// a `const`. Here a `const` with a bogus blob compiles and reaches the
/// runtime decode (which then panics: it can't be decoded).
#[test]
fn test_reproduce_failure_accepts_a_const_blob() {
    let code = r#"
const BLOB: &str = "!!! not a blob !!!";

#[hegel::test]
#[hegel::reproduce_failure(BLOB)]
fn my_test(tc: hegel::TestCase) {
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    let _ = x;
}
"#;
    TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", code)
        .expect_failure("could not be decoded")
        .cargo_test(&["--test", "repro"]);
}

/// End-to-end: a failing test prints a reproducer blob; pasting that blob
/// into `#[hegel::reproduce_failure(...)]` deterministically reproduces the
/// same failure.
#[test]
fn test_reproduce_failure_replays_real_counterexample() {
    let failing = r#"
#[hegel::test(print_blob = true)]
fn my_test(tc: hegel::TestCase) {
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}
"#;
    let project = TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", failing);
    let out = project
        .invoke()
        .expect_failure("x was")
        .cargo_test(&["--test", "repro"]);

    let combined = format!("{}\n{}", out.stdout, out.stderr);
    let re = regex::Regex::new(r#"reproduce_failure\("([^"]+)"\)"#).unwrap();
    let blob = re
        .captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| panic!("no reproduce_failure line in output:\n{combined}"));

    let reproducing = format!(
        r#"
#[hegel::test]
#[hegel::reproduce_failure("{blob}")]
fn my_test(tc: hegel::TestCase) {{
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {{x}}");
}}
"#
    );
    TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", &reproducing)
        .expect_failure("x was")
        .cargo_test(&["--test", "repro"]);

    let stacked = format!(
        r#"
#[hegel::test]
#[hegel::reproduce_failure("{blob}")]
#[hegel::reproduce_failure("!!! stale bookkeeping blob !!!")]
fn my_test(tc: hegel::TestCase) {{
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {{x}}");
}}
"#
    );
    TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", &stacked)
        .expect_failure("x was")
        .cargo_test(&["--test", "repro"]);
}
