//! Ported from resources/pbtkit/tests/test_text.py.
//!
//! Tests that exercise `StringChoice` / index-helper internals require
//! `--features native`; they are `#[cfg(feature = "native")]`-gated here.
//! They are also present as embedded tests in
//! `tests/embedded/native/choices_tests.rs` — redundancy is fine.
//!
//! `test_string_sort_key_type_mismatch` is listed in `SKIPPED.md`: Rust's typed
//! `sort_key(&str)` makes the "non-string argument" case unrepresentable.

use crate::common::utils::{assert_all_examples, expect_panic, minimal};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_text_basic() {
    assert_all_examples(gs::text().min_size(1).max_size(5), |s: &String| {
        let len = s.chars().count();
        (1..=5).contains(&len)
    });
}

#[test]
fn test_text_ascii() {
    assert_all_examples(
        gs::text().min_codepoint(32).max_codepoint(126),
        |s: &String| s.chars().all(|c| (32..=126).contains(&(c as u32))),
    );
}

#[test]
fn test_text_shrinks_to_short() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32),
        |s: &String| !s.is_empty(),
    );
    assert_eq!(s, "a");
}

#[test]
fn test_text_shrinks_characters() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .min_size(1)
            .max_size(5),
        |s: &String| s.contains('z'),
    );
    assert_eq!(s, "z");
}

#[test]
fn test_text_no_surrogates() {
    assert_all_examples(
        gs::text().min_codepoint(0xD700).max_codepoint(0xE000),
        |s: &String| s.chars().all(|c| !(0xD800..=0xDFFF).contains(&(c as u32))),
    );
}

#[test]
fn test_text_unicode_shrinks() {
    let s = minimal(
        gs::text()
            .min_codepoint(128)
            .max_codepoint(256)
            .min_size(1)
            .max_size(3),
        |s: &String| s.chars().any(|c| (c as u32) >= 200),
    );
    // Shrinks to a single high-codepoint character at the boundary.
    assert_eq!(s.chars().count(), 1);
    assert!(s.chars().all(|c| (c as u32) >= 200));
}

#[test]
fn test_text_shrinks_to_simplest() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .max_size(5),
        |_: &String| true,
    );
    assert_eq!(s, "");
}

#[test]
fn test_text_sorts_characters() {
    let s = minimal(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'z' as u32)
            .min_size(3)
            .max_size(5),
        |s: &String| {
            let chars: Vec<char> = s.chars().collect();
            chars.len() >= 3 && chars.windows(2).all(|w| w[0] > w[1])
        },
    );
    let chars: Vec<char> = s.chars().collect();
    assert!(chars.len() >= 3);
    assert!(chars.windows(2).all(|w| w[0] > w[1]));
}

#[test]
fn test_text_redistributes_to_empty() {
    let (s1, s2) = minimal(
        gs::tuples!(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .max_size(10),
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .max_size(10),
        ),
        |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 3,
    );
    assert!(s1.is_empty() || s2.is_empty());
}

#[test]
fn test_text_redistributes_pair() {
    let (s1, s2) = minimal(
        gs::tuples!(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(1)
                .max_size(10),
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(1)
                .max_size(10),
        ),
        |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 5,
    );
    assert!(!s1.is_empty());
    assert!(!s2.is_empty());
}

#[test]
fn test_draw_string_invalid_range() {
    // Python: `tc.draw_string(min_codepoint=200, max_codepoint=100)` raises
    // ValueError. In hegel-rust drawing from such a generator panics: the
    // server returns an InvalidArgument error, and the native backend panics
    // with a similar message from `schema::text::interpret_string`.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: String = tc.draw(gs::text().min_codepoint(200).max_codepoint(100));
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        "InvalidArgument",
    );
}

// ── Tests against native `StringChoice` internals ─────────────────────────
//
// These exercise engine internals that are only reachable under
// `--features native`. They are also covered as embedded tests in
// `tests/embedded/native/choices_tests.rs`; redundancy is fine.

#[cfg(feature = "native")]
mod string_choice_internals {
    //! These exercise engine internals that are only reachable under
    //! `--features native`. Values are codepoint sequences (`Vec<u32>`) —
    //! the engine's internal text model — so assertions use codepoint
    //! vectors rather than `&str` literals.
    use hegel::__native_test_internals::{BigUint, StringChoice};

    fn cps(s: &str) -> Vec<u32> {
        s.chars().map(|c| c as u32).collect()
    }

    #[test]
    fn test_string_single_codepoint_unit_variable_length() {
        // Single codepoint '0', variable length: unit lengthens by one codepoint.
        let kind = StringChoice {
            min_codepoint: 48,
            max_codepoint: 48,
            min_size: 0,
            max_size: 5,
        };
        assert_eq!(kind.unit(), cps("0"));
        assert_eq!(kind.simplest(), Vec::<u32>::new());
    }

    #[test]
    fn test_string_single_codepoint_unit_fixed_length() {
        // Single codepoint '0', fixed length: unit degenerates to simplest.
        let kind = StringChoice {
            min_codepoint: 48,
            max_codepoint: 48,
            min_size: 2,
            max_size: 2,
        };
        assert_eq!(kind.unit(), kind.simplest());
    }

    #[test]
    fn test_string_single_codepoint_unit_non_zero() {
        // Single codepoint 'A': 'A' itself (not '0') is the unit.
        let kind = StringChoice {
            min_codepoint: 65,
            max_codepoint: 65,
            min_size: 0,
            max_size: 5,
        };
        assert_eq!(kind.unit(), cps("A"));
    }

    #[test]
    fn test_string_validate() {
        let kind = StringChoice {
            min_codepoint: 32,
            max_codepoint: 126,
            min_size: 1,
            max_size: 5,
        };
        assert!(kind.validate(&cps("abc")));
        assert!(!kind.validate(&cps(""))); // too short
        assert!(!kind.validate(&cps("abcdef"))); // too long
    }

    #[test]
    fn test_string_from_index_out_of_range() {
        // from_index past max_index returns None.
        let sc = StringChoice {
            min_codepoint: 32,
            max_codepoint: 126,
            min_size: 0,
            max_size: 2,
        };
        assert!(
            sc.from_index(sc.max_index() + BigUint::from(1u32))
                .is_none()
        );
    }

    #[test]
    fn test_string_from_index_past_end() {
        // alpha_size = 95; max_index = 95^0 + 95^1 + 95^2 - 1 = 9120;
        // index 9121 exhausts all length buckets.
        let sc = StringChoice {
            min_codepoint: 32,
            max_codepoint: 126,
            min_size: 0,
            max_size: 2,
        };
        assert_eq!(sc.alpha_size(), 95);
        assert_eq!(sc.max_index(), BigUint::from(9120u32));
        assert!(sc.from_index(BigUint::from(9121u32)).is_none());
    }

    #[test]
    fn test_string_max_index_exceeds_u128() {
        // Regression: with the full Unicode range and max_size=16, max_index
        // is ~10^97, far above u128::MAX. The bignum-backed index arithmetic
        // must handle it without overflowing.
        let sc = StringChoice {
            min_codepoint: 0,
            max_codepoint: 0x10FFFF,
            min_size: 0,
            max_size: 16,
        };
        let idx = sc.max_index();
        assert!(idx > BigUint::from(u128::MAX));
        let v = vec![0x10FFFDu32; 16];
        let v_idx = sc.to_index(&v);
        assert!(v_idx > BigUint::from(u128::MAX));
        assert!(v_idx <= idx);
        assert_eq!(sc.from_index(v_idx), Some(v));
    }

    #[test]
    fn test_string_codepoint_rank_with_surrogates() {
        // Range spanning the surrogate block (0xD800..=0xDFFF).
        let sc = StringChoice {
            min_codepoint: 0xD700,
            max_codepoint: 0xE000,
            min_size: 0,
            max_size: 1,
        };
        // A codepoint above the surrogate block has correct rank.
        let rank = sc.codepoint_rank(0xE000);
        assert!(rank > 0);
        // Round-trip through to_index/from_index.
        let v = vec![0xE000u32];
        let idx = sc.to_index(&v);
        assert_eq!(sc.from_index(idx), Some(v));
    }
}

// ── Database-round-trip and corrupt-entry tests ────────────────────────────
//
// Ported from `test_text_database_round_trip` and
// `test_truncated_string_database_entry`. Both pbtkit tests exercise the
// engine's persistence layer; hegel-rust's native database lives at
// `src/native/database.rs` and uses a different binary layout, so we match
// the semantics (write then replay, and corrupt entries are ignored
// gracefully) rather than the exact on-disk bytes.

#[cfg(feature = "native")]
#[test]
fn test_text_database_round_trip() {
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
fn test_text_db(tc: hegel::TestCase) {{
    let s: String = tc.draw(gs::text().min_size(1).max_size(5));
    record_test_case("test_text_db", &s);
    assert!(s.chars().count() < 1);
}}
"#
    );

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();
    let project = TempRustProject::new()
        .test_file("text_db.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        .expect_failure("FAILED");

    // First run: database starts empty, failure gets shrunk and saved.
    project.cargo_test(&["test_text_db"]);
    let first_run: Vec<String> = std::fs::read_to_string(values_path.join("test_text_db"))
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let shrunk_first = first_run.last().unwrap().clone();

    // Second run: the saved failing case should replay immediately as the
    // first value.
    std::fs::remove_file(values_path.join("test_text_db")).unwrap();
    project.cargo_test(&["test_text_db"]);
    let second_run: Vec<String> = std::fs::read_to_string(values_path.join("test_text_db"))
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

#[cfg(feature = "native")]
#[test]
fn test_truncated_string_database_entry() {
    // Write a corrupt string entry into the on-disk database and verify the
    // test still runs instead of crashing. Mirrors the Python test that
    // seeds a database dict with a truncated `SerializationTag.STRING`
    // record; the hegel-rust native serialization uses type-tag 4 for
    // strings followed by a 4-byte little-endian codepoint count and N*4
    // bytes of little-endian u32 codepoints (see
    // `src/native/database.rs::serialize_choices`).
    use crate::common::project::TempRustProject;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();

    // Key directory uses FNV-1a hex of the database key; we don't know the
    // key ahead of time, so write the corrupt entry into every dir that
    // ends up under db_root by creating a catch-all key directory. Since
    // the key is derived from the test fn path, we instead shell out to a
    // test that creates the file itself before the second run.
    let db_str = db_path.to_str().unwrap();

    let test_code = format!(
        r#"
use hegel::generators as gs;

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_text_corrupt(tc: hegel::TestCase) {{
    let _: String = tc.draw(gs::text().min_size(0).max_size(5));
}}
"#
    );

    let project = TempRustProject::new().test_file("text_corrupt.rs", &test_code);

    // First run populates the database with a real entry so we know which
    // hashed directory to target.
    project.cargo_test(&["test_text_corrupt"]);

    // Now corrupt every entry on disk: truncated-length headers and
    // length-past-payload records. Either should be ignored and the test
    // should still run (and pass).
    for entry in std::fs::read_dir(&db_path).unwrap() {
        let entry = entry.unwrap();
        let best = entry.path().join("best");
        if best.exists() {
            // count=1, type=4 (String), then a truncated length prefix.
            let mut bytes = vec![1u8, 0, 0, 0, 4u8];
            bytes.extend_from_slice(&[0u8, 0]); // 2 of 4 length bytes
            std::fs::write(&best, &bytes).unwrap();
        }
    }
    project.cargo_test(&["test_text_corrupt"]);

    // Second corruption: length-overruns-payload.
    for entry in std::fs::read_dir(&db_path).unwrap() {
        let entry = entry.unwrap();
        let best = entry.path().join("best");
        if best.exists() {
            let mut bytes = vec![1u8, 0, 0, 0, 4u8];
            bytes.extend_from_slice(&5u32.to_le_bytes());
            bytes.push(b'a'); // 1 byte of payload, length claims 5
            std::fs::write(&best, &bytes).unwrap();
        }
    }
    project.cargo_test(&["test_text_corrupt"]);
}
