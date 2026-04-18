// Compile-time error behaviour of #[hegel::test] (duplicate #[test], zero or
// two parameters) lives in tests/compile/fail/hegel_test_*.rs, driven by
// `trybuild`.

mod common;

use common::utils::expect_panic;
use hegel::TestCase;
use hegel::generators as gs;

#[test]
fn test_text_invalid_codec_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.draw(gs::text().codec("not-a-real-codec"));
            })
            .run();
        },
        "not-a-real-codec",
    );
}

#[hegel::test]
fn test_basic_usage(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test]
fn test_characters(tc: TestCase) {
    let c: char = tc.draw(gs::characters());
    assert!(c.len_utf8() > 0);
}

#[hegel::test(test_cases = 10)]
fn test_with_named_arg(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(hegel::Settings::new().test_cases(10))]
fn test_with_positional_settings(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(hegel::Settings::new(), test_cases = 10)]
fn test_with_positional_and_named(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(test_cases = 10, derandomize = true)]
fn test_with_multiple_named_args(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(seed = Some(42))]
fn test_with_seed(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[test]
fn test_database_persists_failing_examples() {
    let db_path = tempfile::tempdir().unwrap();
    let db_str = db_path.path().to_str().unwrap().to_string();

    assert!(std::fs::read_dir(db_path.path()).unwrap().next().is_none());

    expect_panic(
        || {
            hegel::Hegel::new(|_tc: hegel::TestCase| {
                panic!("");
            })
            .settings(hegel::Settings::new().database(Some(db_str)))
            .__database_key("test_database_persists".to_string())
            .run();
        },
        "Property test failed",
    );

    let entries: Vec<_> = std::fs::read_dir(db_path.path()).unwrap().collect();
    assert!(!entries.is_empty());
}
