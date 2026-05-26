//! Embedded tests for `src/native/test_runner.rs` helpers.  Cover
//! the defensive branches (flaky-final-replay panic and TooSlow
//! health-check) that were previously masked by `// nocov` annotations.

use super::*;
use std::time::Duration;

#[test]
fn too_slow_check_panics_when_under_threshold_and_unsuppressed() {
    let result = std::panic::catch_unwind(|| {
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        );
    });
    assert!(result.is_err(), "expected too_slow_check to panic");
}

#[test]
fn too_slow_check_quiet_when_suppressed() {
    too_slow_check(
        /* valid_test_cases */ 1,
        /* total_test_time */ Duration::from_secs(60),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ true,
    );
}

#[test]
fn too_slow_check_quiet_when_under_threshold() {
    too_slow_check(
        /* valid_test_cases */ 1,
        /* total_test_time */ Duration::from_secs(1),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ false,
    );
}

#[test]
fn too_slow_check_quiet_when_enough_valid_cases() {
    // Once enough valid cases have run, the health check is no longer
    // applied even if total_test_time exceeds the threshold.
    too_slow_check(
        /* valid_test_cases */ 10_000,
        /* total_test_time */ Duration::from_secs(60),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ false,
    );
}

#[test]
#[should_panic(expected = "Flaky test detected")]
fn flaky_final_replay_panic_panics_with_diagnostic() {
    flaky_final_replay_panic();
}
