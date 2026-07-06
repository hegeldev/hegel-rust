//! Embedded tests for the libhegel C-ABI boundary (`crate::ffi`).
//!
//! These drive whole runs through the safe wrappers — settings building, the
//! pull loop, every per-test-case primitive, result inspection, and blob
//! replay — so each wrapper is exercised end-to-end against the real engine in
//! `hegel-c`. They are the frontend's analogue of `hegel-c/tests/smoke.rs`,
//! but going through the Rust wrappers the rest of hegeltest will use.

use super::*;
use crate::runner::{Backend, Settings};

#[test]
fn ffi_settings_builds_with_each_explicit_backend() {
    for backend in [Backend::Default, Backend::Urandom] {
        let _sh = SettingsHandle::build(&test_settings(1).backend(backend), None);
    }
}

/// A deterministic, database-free settings for tests.
fn test_settings(seed: u64) -> Settings {
    Settings::new()
        .test_cases(25)
        .database(None)
        .derandomize(true)
        .seed(Some(seed))
}

const VALID: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_VALID;
const OVERRUN: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_OVERRUN;
const INTERESTING: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_INTERESTING;

/// Run `f` per test case, treating a `HEGEL_E_STOP_TEST` anywhere inside it
/// as an overrun (completing the case accordingly).
fn drive_run(run: &RunHandle, mut f: impl FnMut(&CTestCase) -> Result<(), hegel_result_t>) {
    while let Some(tc) = run.next_test_case() {
        match f(&tc) {
            Ok(()) => tc.mark_complete(VALID, None).unwrap(),
            Err(hegel_c::hegel_result_t::HEGEL_E_STOP_TEST) => {
                tc.mark_complete(OVERRUN, None).unwrap()
            }
            Err(rc) => panic!("unexpected rc={rc:?}"),
        }
    }
}

#[test]
fn ffi_drives_a_passing_run_exercising_every_primitive() {
    let settings = test_settings(1);
    let sh = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&sh).unwrap();
    let text = StringGenerator::text(0, 5, None, 0, None, None, None, None, None).unwrap();

    let mut cases = 0usize;
    drive_run(&run, |tc| {
        cases += 1;

        tc.start_span(hegel_c::hegel_label_t::HEGEL_LABEL_LIST as u64)
            .unwrap();
        let cid = tc.new_collection(0, Some(3)).unwrap();
        loop {
            if !tc.collection_more(cid)? {
                break;
            }
            if tc.generate_integer(0, 100)? == 0 {
                tc.collection_reject(cid, Some("zero")).unwrap();
            }
        }
        tc.stop_span(false).unwrap();

        let pool = tc.new_pool().unwrap();
        let added = tc.pool_add(pool).unwrap();
        let drawn = tc.pool_generate(pool, false)?;
        assert_eq!(drawn, added, "non-consuming draw returns the added id");

        tc.target(0.0, "score").unwrap();

        let n = tc.generate_integer(0, 100)?;
        assert!((0..=100).contains(&n));

        let min = 0u128.to_le_bytes();
        let max = u128::MAX.to_le_bytes();
        let mut min17 = [0u8; 17];
        let mut max17 = [0u8; 17];
        min17[..16].copy_from_slice(&min);
        max17[..16].copy_from_slice(&max);
        let big = tc.generate_integer_big(&min17, &max17)?;
        assert_eq!(big[16], 0, "value in [0, u128::MAX] is non-negative");

        let f = tc.generate_float(64, 0.0, 1.0, false, false, false, false, f64::from_bits(1))?;
        assert!((0.0..=1.0).contains(&f));

        tc.generate_boolean(0.5)?;

        let bytes = tc.generate_bytes(2, 4)?;
        assert!((2..=4).contains(&bytes.len()));

        let s = tc.generate_string(&text)?;
        assert!(s.chars().count() <= 5);

        let d = tc.generate_date()?;
        assert!((1..=9999).contains(&d.year));
        let t = tc.generate_time()?;
        assert!(t.hour <= 23);
        let dt = tc.generate_datetime()?;
        assert!((1..=12).contains(&dt.date.month));
        let uuid = tc.generate_uuid(Some(4))?;
        assert_eq!(uuid[6] >> 4, 4);
        tc.generate_ipv4()?;
        tc.generate_ipv6()?;

        Ok(())
    });
    assert!(cases >= 1);

    let result = run.result();
    assert!(result.status() == hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);
    assert_eq!(result.failure_count(), 0);
    assert!(result.error().is_none());
}

#[test]
fn ffi_string_generator_debug_is_opaque() {
    let g = StringGenerator::email().unwrap();
    assert_eq!(format!("{g:?}"), "StringGenerator { .. }");
}

#[test]
fn ffi_string_generator_constructors_cover_every_kind() {
    let alphabet = StringGenerator::text(
        0,
        1,
        Some("ascii"),
        0,
        Some(127),
        None,
        Some(&["Cc".to_string()]),
        Some("a"),
        Some("b"),
    )
    .unwrap();
    StringGenerator::regex("[a-z]{1,4}", true, Some(&alphabet)).unwrap();
    StringGenerator::email().unwrap();
    StringGenerator::url().unwrap();
    StringGenerator::domain(255).unwrap();

    let err =
        StringGenerator::text(0, 1, Some("ebcdic"), 0, None, None, None, None, None).unwrap_err();
    assert!(err.contains("invalid codec"), "{err}");
    let err = StringGenerator::regex("(unclosed", false, None).unwrap_err();
    assert!(err.contains("invalid regex pattern"), "{err}");
    let err = StringGenerator::domain(3).unwrap_err();
    assert!(err.contains("no eligible TLDs"), "{err}");
    let err = StringGenerator::text(
        0,
        1,
        None,
        0,
        None,
        Some(&["Nd".to_string()]),
        None,
        None,
        None,
    )
    .and_then(|nd| StringGenerator::regex("x", false, Some(&nd)).map(|_| ()))
    .map(|_| String::new());
    assert!(err.is_ok(), "a text alphabet is accepted for regex");
}

#[test]
fn ffi_reports_failure_with_blob_then_replays_it() {
    let settings = test_settings(7);
    let sh = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&sh).unwrap();

    let origin = "n != 0";
    while let Some(tc) = run.next_test_case() {
        match tc.generate_integer(0, 100) {
            Ok(n) => {
                if n != 0 {
                    tc.mark_complete(INTERESTING, Some(origin)).unwrap();
                } else {
                    tc.mark_complete(VALID, None).unwrap();
                }
            }
            Err(hegel_c::hegel_result_t::HEGEL_E_STOP_TEST) => {
                tc.mark_complete(OVERRUN, None).unwrap()
            }
            Err(rc) => panic!("generate_integer rc={rc:?}"),
        }
    }

    let result = run.result();
    assert!(result.status() == hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
    assert_eq!(result.failure_count(), 1);
    let blob = result
        .failure(0)
        .reproduce_blob
        .expect("a shrunk failure carries a blob");

    let sh2 = SettingsHandle::build(&settings, None);
    let replay = CTestCase::from_blob(&sh2, &blob).unwrap();
    assert_eq!(
        replay.generate_integer(0, 100).unwrap(),
        1,
        "the blob replays the minimal counterexample"
    );
    replay.mark_complete(INTERESTING, Some(origin)).unwrap();
}

/// `clone_handle` yields an independent handle onto the same test case: both
/// the root and the clone draw from one shared source, each holding its own
/// reference. Dropping the clone frees only its own handle (dropping its
/// reference); the shared test case stays alive for the root, which is still
/// usable.
#[test]
fn ffi_clone_handle_shares_the_test_case() {
    let settings = test_settings(1);
    let sh = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&sh).unwrap();

    let tc = run.next_test_case().unwrap();
    let clone = tc.clone_handle();
    clone.generate_integer(0, 100).unwrap();
    tc.generate_integer(0, 100).unwrap();
    drop(clone);
    tc.generate_integer(0, 100).unwrap();
    tc.mark_complete(VALID, None).unwrap();

    while let Some(t) = run.next_test_case() {
        let _ = t.generate_integer(0, 100);
        t.mark_complete(VALID, None).unwrap();
    }
}

#[test]
fn ffi_from_blob_rejects_undecodable_input() {
    let settings = test_settings(1);
    let sh = SettingsHandle::build(&settings, None);
    let err = match CTestCase::from_blob(&sh, "not a valid base64 hegel blob!!!") {
        Err(e) => e,
        Ok(_) => panic!("expected an undecodable blob to be rejected"),
    };
    assert!(!err.is_empty(), "an undecodable blob yields a diagnostic");
}
