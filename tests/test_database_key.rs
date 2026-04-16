mod common;

use common::project::TempRustProject;

fn read_values(dir: &std::path::Path, label: &str) -> Vec<i64> {
    let path = dir.join(label);
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect()
}

#[test]
fn test_database_key_replays_failure() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    // Use forward slashes to avoid invalid escape sequences in generated Rust string literals
    let db_str = db_path.to_str().unwrap().replace('\\', "/");

    let test_code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

fn record_test_case(label: &str, n: i64) {{
    let path = format!("{{}}/{{}}", std::env::var("VALUES_DIR").unwrap(), label);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{{}}", n).unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_1(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    record_test_case("test_1", n);
    assert!(n < 1_000_000);
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_2(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    record_test_case("test_2", n);
    assert!(n < 1_000_000);
}}
"#
    );

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();
    let project = TempRustProject::new()
        .test_file("integration.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        // "FAILED" appears in cargo test output for both native and server backends.
        .expect_failure("FAILED");

    // run test_1. Database now has a failing entry for test_1
    project.cargo_test(&["test_1"]);

    let shrunk_value = *read_values(&values_path, "test_1").last().unwrap();
    assert_eq!(shrunk_value, 1_000_000);

    // clear the log file
    std::fs::remove_file(values_path.join("test_1")).unwrap();

    // run test_1 again. It should replay the shrunk test case immediately
    project.cargo_test(&["test_1"]);

    let values = read_values(&values_path, "test_1");
    assert_eq!(
        values[0], shrunk_value,
        "Expected to replay shrunk test case {shrunk_value} first, got {}",
        values[0]
    );

    // run test_2. It should not replay the test_1 shrunk test case.
    project.cargo_test(&["test_2"]);

    let values = read_values(&values_path, "test_2");
    assert_ne!(values[0], shrunk_value);
}
