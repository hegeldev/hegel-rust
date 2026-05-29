//! Embedded tests for `src/native/test_runner.rs` helpers.  Cover the
//! health-check diagnostics (TooSlow and the flaky-replay message) that the
//! runner folds into a failing `TestRunResult` instead of panicking, so no
//! panic crosses the FFI boundary into libhegel.

use super::*;
use std::time::Duration;

#[test]
fn too_slow_check_reports_when_under_threshold_and_unsuppressed() {
    let msg = too_slow_check(
        /* valid_test_cases */ 1,
        /* total_test_time */ Duration::from_secs(60),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ false,
    );
    assert!(msg.is_some(), "expected too_slow_check to report a failure");
    assert!(msg.unwrap().contains("TooSlow"));
}

#[test]
fn too_slow_check_quiet_when_suppressed() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ true,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_under_threshold() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(1),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_enough_valid_cases() {
    // Once enough valid cases have run, the health check is no longer
    // applied even if total_test_time exceeds the threshold.
    assert!(
        too_slow_check(
            /* valid_test_cases */ 10_000,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn flaky_diagnostic_mentions_flaky() {
    assert!(flaky_diagnostic().contains("Flaky test detected"));
}
