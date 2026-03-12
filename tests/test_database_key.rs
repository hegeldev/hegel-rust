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
    let test_code = r#"
use hegel::generators;
use std::io::Write;

fn record_test_case(label: &str, n: i64) {
    let path = format!("{}/{}", std::env::var("VALUES_DIR").unwrap(), label);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{}", n).unwrap();
}

#[hegel::test]
fn test_1() {
    let n: i64 = hegel::draw(&generators::integers());
    record_test_case("test_1", n);
    assert!(n < 1_000_000);
}

#[hegel::test]
fn test_2() {
    let n: i64 = hegel::draw(&generators::integers());
    record_test_case("test_2", n);
    assert!(n < 1_000_000);
}
"#;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let values_path = temp_dir.path().to_str().unwrap();

    let project = TempRustProject::new("fn main() {}")
        .test_file("integration.rs", test_code)
        .env("VALUES_DIR", values_path);

    // run test_1. Database now has a failing entry for test_1
    let output = project.cargo_test(&["test_1"]);
    assert!(!output.status.success());

    let shrunk_value = *read_values(temp_dir.path(), "test_1").last().unwrap();
    assert_eq!(shrunk_value, 1_000_000);

    // clear the log file
    std::fs::remove_file(temp_dir.path().join("test_1")).unwrap();

    // run test_1 again. It should replay the shrunk test case immediately
    let output = project.cargo_test(&["test_1"]);
    assert!(!output.status.success());

    let values = read_values(temp_dir.path(), "test_1");
    assert_eq!(
        values[0], shrunk_value,
        "Expected to replay shrunk test case {shrunk_value} first, got {}",
        values[0]
    );

    // run test_2. It should not replay the test_1 shrunk test case.
    let output = project.cargo_test(&["test_2"]);
    assert!(!output.status.success());

    let values = read_values(temp_dir.path(), "test_2");
    assert_ne!(values[0], shrunk_value);
}
