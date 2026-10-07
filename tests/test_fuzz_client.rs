//! End-to-end tests for the fuzzer client — the `HEGEL_FUZZ_*` environment
//! variables an external fuzzer drives a test program with — against the
//! prebuilt `#[hegel::main]` fixture binaries.

mod common;

use common::exec::fixture;

const BASIC_MAIN: &str = env!("CARGO_BIN_EXE_fixture_basic_main");
const MAIN_FAILING: &str = env!("CARGO_BIN_EXE_fixture_main_failing");

/// A one-integer choice sequence in the failure database's entry format:
/// the choice count, the integer tag, the big-integer sub-tag, the byte
/// length and the integer's two's-complement little-endian bytes.
fn integer_entry(value: &[u8]) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&1u32.to_le_bytes());
    entry.push(0);
    entry.push(10);
    entry.extend_from_slice(&(value.len() as u32).to_le_bytes());
    entry.extend_from_slice(value);
    entry
}

fn read_record(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn fuzz_output_runs_one_case_and_records_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
    let record = read_record(&out);
    assert_eq!(record["status"], "valid");
    assert_eq!(record["test"], "fixture_basic_main::main");
    assert_eq!(record["origin"], serde_json::Value::Null);
    assert_eq!(record["prefix_length"], 0);
    assert_eq!(record["choices"].as_array().unwrap().len(), 1);
    assert_eq!(record["choices"][0]["kind"], "integer");
    assert!(!record["spans"].as_array().unwrap().is_empty());
    assert!(record["choices_base64"].is_string());
}

#[test]
fn fuzz_trace_streams_the_choices_without_the_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let trace = dir.path().join("trace");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TRACE", trace.to_str().unwrap())
        .run();
    let record = read_record(&out);
    let entry = base64_decode(record["choices_base64"].as_str().unwrap());
    assert_eq!(std::fs::read(&trace).unwrap(), entry[4..]);
}

#[test]
fn fuzz_output_records_a_failure_and_the_run_fails_as_usual() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(MAIN_FAILING)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .expect_failure("got nonneg")
        .run();
    assert_eq!(
        output.stderr.matches("got nonneg").count(),
        1,
        "fuzz mode runs the body exactly once, with no replays or shrinking: {}",
        output.stderr
    );
    assert!(output.stderr.contains("reproduce_failure"));
    let record = read_record(&out);
    assert_eq!(record["status"], "interesting");
    assert!(
        record["origin"].as_str().unwrap().starts_with("Panic at "),
        "{record}"
    );
}

#[test]
fn fuzz_prefix_replays_the_stored_choices() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&(-1234i16).to_le_bytes())).unwrap();
    let out = dir.path().join("record.json");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_PREFIX", prefix.to_str().unwrap())
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .run();
    let record = read_record(&out);
    assert_eq!(record["choices"][0]["value"], "-1234");
    assert_eq!(record["prefix_length"], 1);
    assert_eq!(record["prefix_consumed"], 1);
    assert_eq!(record["misaligned_at"], serde_json::Value::Null);
    let entry = base64_decode(record["choices_base64"].as_str().unwrap());
    assert_eq!(entry, integer_entry(&(-1234i16).to_le_bytes()));
}

#[test]
fn fuzz_test_selects_the_test_the_variables_are_for() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TEST", "some_other_crate::main")
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 0);
    assert!(!out.exists());

    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TEST", "fixture_basic_main::main")
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
    assert!(out.exists());
}

#[test]
fn fuzz_reproduce_shrinks_and_persists_the_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&[37])).unwrap();
    let db = dir.path().join("db");
    let output = fixture(MAIN_FAILING)
        .env("HEGEL_FUZZ_REPRODUCE", prefix.to_str().unwrap())
        .args(&["--database", db.to_str().unwrap()])
        .expect_failure("got nonneg 0")
        .run();
    assert!(db.is_dir(), "{}", output.stderr);

    let output = fixture(MAIN_FAILING)
        .args(&["--database", db.to_str().unwrap(), "--verbosity", "debug"])
        .expect_failure("got nonneg 0")
        .run();
    assert!(
        output.stderr.contains("Starting phase: Reuse"),
        "{}",
        output.stderr
    );
}

#[test]
fn fuzz_reproduce_of_a_passing_entry_runs_it_once_and_generates_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&[5])).unwrap();
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_REPRODUCE", prefix.to_str().unwrap())
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
}

#[test]
fn a_malformed_fuzz_variable_is_a_run_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env(
            "HEGEL_FUZZ_PREFIX",
            dir.path().join("missing").to_str().unwrap(),
        )
        .expect_failure("HEGEL_FUZZ_PREFIX=.* could not be read")
        .run();
    assert!(!out.exists());
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in text.bytes().filter(|&b| b != b'=') {
        let value = ALPHABET.iter().position(|&a| a == byte).unwrap() as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    out
}
