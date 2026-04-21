//! Ported from resources/pbtkit/tests/test_bytes.py.
//!
//! Tests that exercise `BytesChoice` internals require `--features native`;
//! they are `#[cfg(feature = "native")]`-gated here. They are also present
//! as embedded tests in `tests/embedded/native/choices_tests.rs` —
//! redundancy is fine.
//!
//! `test_targeting_with_bytes` and `test_bytes_sort_key_type_mismatch` are
//! listed in `SKIPPED.md`: the former depends on the `tc.target(score)`
//! public API (no hegel-rust analog, same reason as the whole-file skip of
//! `test_targeting.py`); the latter exercises Python-dynamic-typing
//! `sort_key(non-bytes)`, which Rust's typed `sort_key(&[u8])` makes
//! unrepresentable.

use crate::common::utils::{Minimal, assert_all_examples, minimal};
use hegel::generators as gs;

#[test]
fn test_finds_short_binary() {
    let b = minimal(gs::binary().max_size(10), |b: &Vec<u8>| !b.is_empty());
    assert_eq!(b, vec![0u8]);
}

#[test]
fn test_shrinks_bytes_to_minimal() {
    let b = Minimal::new(gs::binary().min_size(1).max_size(5), |b: &Vec<u8>| {
        b.contains(&0xFFu8)
    })
    .test_cases(1000)
    .run();
    assert_eq!(b, vec![0xFFu8]);
}

#[test]
fn test_binary_respects_size_bounds() {
    assert_all_examples(gs::binary().min_size(2).max_size(4), |b: &Vec<u8>| {
        (2..=4).contains(&b.len())
    });
}

#[test]
fn test_shrinks_bytes_with_constraints() {
    // When the simplest bytes value (all zeros at min_size) doesn't trigger
    // the failure, the shrinker falls back to shortening and shrinking
    // individual byte values. The exact byte distribution varies because
    // the shrinker can't redistribute value between bytes, so we only pin
    // length and total.
    let b = Minimal::new(gs::binary().min_size(2).max_size(10), |b: &Vec<u8>| {
        b.iter().map(|&x| x as u32).sum::<u32>() > 10
    })
    .test_cases(1000)
    .run();
    assert_eq!(b.len(), 2);
    assert_eq!(b.iter().map(|&x| x as u32).sum::<u32>(), 11);
}

#[test]
fn test_shrinks_bytes_to_simplest() {
    // When the simplest bytes value itself triggers the failure, the
    // shrinker finds it immediately.
    let b = minimal(gs::binary().max_size(10), |b: &Vec<u8>| {
        b.iter().map(|&x| x as u32).sum::<u32>() == 0
    });
    assert_eq!(b, Vec::<u8>::new());
}

// ── Tests against native `BytesChoice` internals ──────────────────────────
//
// These exercise engine internals that are only reachable under
// `--features native`. They are also covered as embedded tests in
// `tests/embedded/native/choices_tests.rs`; redundancy is fine.

#[cfg(feature = "native")]
mod bytes_choice_internals {
    use hegel::__native_test_internals::{BigUint, BytesChoice};

    #[test]
    fn test_bytes_choice_unit() {
        // Second-simplest in sort_key order: next byte value, not next length.
        assert_eq!(
            BytesChoice {
                min_size: 0,
                max_size: 10,
            }
            .unit(),
            vec![0x01u8]
        );
        assert_eq!(
            BytesChoice {
                min_size: 3,
                max_size: 10,
            }
            .unit(),
            vec![0x00u8, 0x00u8, 0x01u8]
        );
    }

    #[test]
    fn test_bytes_from_index_out_of_range() {
        // from_index past max_index returns None.
        let bc = BytesChoice {
            min_size: 0,
            max_size: 2,
        };
        assert!(
            bc.from_index(bc.max_index() + BigUint::from(1u32))
                .is_none()
        );
    }

    #[test]
    fn test_bytes_from_index_past_end() {
        // BytesChoice(0, 2).max_index == 65792 (to_index(b"\xff\xff")), so
        // from_index(65793) exhausts all length buckets and returns None.
        let bc = BytesChoice {
            min_size: 0,
            max_size: 2,
        };
        assert!(bc.from_index(BigUint::from(65793u32)).is_none());
    }
}

// ── Database-round-trip test ──────────────────────────────────────────────
//
// Port of `test_mixed_types_database_round_trip`. The hegel-rust equivalent
// lives under `src/native/database.rs` and uses a different binary layout;
// we match the semantics (write then replay the shrunk value) rather than
// the exact on-disk bytes. Draws integer + boolean + binary in the same
// test to exercise the round-trip for all three choice types.

#[cfg(feature = "native")]
#[test]
fn test_mixed_types_database_round_trip() {
    use crate::common::project::TempRustProject;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    let db_str = db_path.to_str().unwrap();

    let test_code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

fn record_test_case(label: &str, s: &str) {{
    let path = format!("{{}}/{{}}", std::env::var("VALUES_DIR").unwrap(), label);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{{}}", s).unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_bytes_db(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    let flag: bool = tc.draw(gs::booleans());
    let b: Vec<u8> = tc.draw(gs::binary().max_size(10));
    let hex: String = b.iter().map(|x| format!("{{:02x}}", x)).collect();
    record_test_case("test_bytes_db", &format!("{{}}|{{}}|{{}}", n, flag, hex));
    assert!(b.len() < 1);
}}
"#
    );

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();
    let project = TempRustProject::new()
        .test_file("bytes_db.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        .expect_failure("FAILED");

    // First run: database starts empty, failure gets shrunk and saved.
    project.cargo_test(&["test_bytes_db"]);
    let first_run: Vec<String> = std::fs::read_to_string(values_path.join("test_bytes_db"))
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let shrunk_first = first_run.last().unwrap().clone();

    // Second run: the saved failing case should replay immediately as the
    // first value.
    std::fs::remove_file(values_path.join("test_bytes_db")).unwrap();
    project.cargo_test(&["test_bytes_db"]);
    let second_run: Vec<String> = std::fs::read_to_string(values_path.join("test_bytes_db"))
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        second_run[0], shrunk_first,
        "Expected to replay shrunk value {shrunk_first:?} first, got {:?}",
        second_run[0]
    );
}
