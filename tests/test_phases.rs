//! Ported from hypothesis-python/tests/cover/test_phases.py

mod common;

use common::project::TempRustProject;
use hegel::generators as gs;
use hegel::{Phase, TestCase};

// With phases=[Explicit], only the explicit test case runs.
// The body asserts i == 11; only the explicit case (i=11) satisfies this.
// Random generation would find values != 11 and fail, but it never runs.
#[hegel::test(phases = [Phase::Explicit])]
#[hegel::explicit_test_case(i = 11i32)]
fn test_only_runs_explicit_examples(tc: TestCase) {
    let i: i32 = tc.draw(gs::integers());
    assert_eq!(i, 11);
}

// With phases not including Explicit, explicit cases are skipped.
// The explicit case would fail at runtime (name mismatch: "hello_world" vs "b"),
// but it is never run because Phase::Explicit is not in the phase list.
#[hegel::test(test_cases = 5, phases = [Phase::Reuse, Phase::Generate])]
#[hegel::explicit_test_case(hello_world = "hello world".to_string())]
fn test_does_not_use_explicit_examples(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
}

// With phases=[Reuse, Shrink] (no Generate) and no database, no test cases are
// generated. The body would always panic, but it is never called.
#[hegel::test(database = None, phases = [Phase::Reuse, Phase::Shrink])]
fn test_this_would_fail_if_you_ran_it(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
    panic!("should not run");
}

// Disabling Phase::Shrink limits the number of interesting (failing) test case
// calls to exactly 2: one when the failure is first found, one for the final replay.
// With shrinking enabled, many more calls occur as Hypothesis minimises the example.
#[test]
fn test_phase_no_shrink_limits_interesting() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let log_path = temp_dir.path().join("interesting.log");
    let log_str = log_path.to_str().unwrap().replace('\\', "/");

    let code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

#[hegel::test(phases = [hegel::Phase::Generate])]
fn test_no_shrink(tc: hegel::TestCase) {{
    let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(200));
    if n > 100 {{
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("{log_str}")
            .unwrap();
        writeln!(f, "{{}}", n).unwrap();
        panic!("too large: {{}}", n);
    }}
}}
"#
    );

    TempRustProject::new()
        .test_file("test_no_shrink.rs", &code)
        .expect_failure("too large")
        .cargo_test(&["test_no_shrink"]);

    let interesting_count = std::fs::read_to_string(&log_path)
        .map(|s| s.lines().count())
        .unwrap_or(0);
    assert_eq!(
        interesting_count, 2,
        "Expected exactly 2 interesting calls without shrinking, got {interesting_count}"
    );
}

// Disabling Phase::Reuse means the database is not consulted, so a previously
// saved failing example is not replayed as the first test case.
#[test]
fn test_phase_no_reuse_skips_db_replay() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    let db_str = db_path.to_str().unwrap().replace('\\', "/");

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();
    let values_str = values_path.to_str().unwrap().replace('\\', "/");

    let make_code = |phases_attr: &str| {
        format!(
            r#"
use hegel::generators as gs;
use std::io::Write;

fn record(n: i64) {{
    let path = format!("{values_str}/log");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(f, "{{}}", n).unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()){phases_attr})]
fn test_reuse(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    record(n);
    assert!(n < 1_000_000);
}}
"#
        )
    };

    // First run: populate the database with a saved failing example.
    TempRustProject::new()
        .test_file("test_reuse.rs", &make_code(""))
        .expect_failure("Property test failed")
        .cargo_test(&["test_reuse"]);

    let shrunk_value: i64 = std::fs::read_to_string(values_path.join("log"))
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(shrunk_value, 1_000_000);

    std::fs::remove_file(values_path.join("log")).unwrap();

    // Second run: no Reuse phase — saved example should NOT be the first test case.
    TempRustProject::new()
        .test_file(
            "test_reuse.rs",
            &make_code(", phases = [hegel::Phase::Generate, hegel::Phase::Shrink]"),
        )
        .expect_failure("Property test failed")
        .cargo_test(&["test_reuse"]);

    let first_value: i64 = std::fs::read_to_string(values_path.join("log"))
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(
        first_value, shrunk_value,
        "Without Phase::Reuse, the saved example should not be the first test case"
    );
}
