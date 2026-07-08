mod common;

use std::sync::OnceLock;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

#[test]
fn debug_verbosity_failing_run_exercises_shrink_eprintlns() {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|tc| {
            let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(20));
            assert!(n < 5);
        })
        .settings(
            Settings::new()
                .verbosity(Verbosity::Debug)
                .test_cases(100)
                .database(None),
        )
        .run();
    });
    assert!(result.is_err(), "expected the property to fail");
}

#[test]
fn verbose_verbosity_failing_run_exercises_trying_example_eprintln() {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|tc| {
            let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(20));
            assert!(n < 5);
        })
        .settings(
            Settings::new()
                .verbosity(Verbosity::Verbose)
                .test_cases(100)
                .database(None),
        )
        .run();
    });
    assert!(result.is_err(), "expected the property to fail");
}

#[test]
fn tree_exhausted_filter_too_much_fires_on_tiny_filtered_domain() {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|tc| {
            let _ = tc.draw(gs::booleans());
            tc.assume(false);
        })
        .settings(Settings::new().test_cases(50).database(None))
        .run();
    });
    let msg = result
        .expect_err("expected FailedHealthCheck panic")
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("FilterTooMuch") || msg.contains("filtered out"),
        "expected FilterTooMuch panic, got {:?}",
        msg
    );
}

#[test]
fn db_replay_drops_corrupted_stored_entry() {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};
    use tempfile::TempDir;
    let db_dir = TempDir::new().unwrap();
    let key = b"db_replay_drops_corrupted_stored_entry";
    let mut prefixed = b"native:".to_vec();
    prefixed.extend_from_slice(key);
    let key_dir = db_dir.path().join(fnv_hex(&prefixed));
    std::fs::create_dir_all(&key_dir).unwrap();
    let garbage = b"not-a-valid-choice-encoding";
    std::fs::write(key_dir.join(fnv_hex(garbage)), garbage).unwrap();

    Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
    })
    .__database_key("db_replay_drops_corrupted_stored_entry".to_string())
    .settings(
        Settings::new()
            .test_cases(5)
            .database(Some(db_dir.path().to_str().unwrap().to_string())),
    )
    .run();

    assert!(!key_dir.join(fnv_hex(garbage)).exists());
}

#[test]
fn debug_verbosity_replay_aligned_emits_skipping_shrink_message() {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};
    use tempfile::TempDir;
    let db_dir = TempDir::new().unwrap();
    let db_path = db_dir.path().to_str().unwrap().to_string();

    let run_once = |verbosity: Verbosity| {
        std::panic::catch_unwind(|| {
            Hegel::new(|tc| {
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(2));
                assert!(n < 1);
            })
            .__database_key("replay_aligned_skip_shrink_test".to_string())
            .settings(
                Settings::new()
                    .verbosity(verbosity)
                    .test_cases(20)
                    .database(Some(db_path.clone())),
            )
            .run();
        })
    };

    let first = run_once(Verbosity::Normal);
    assert!(first.is_err(), "first run should fail");

    let second = run_once(Verbosity::Debug);
    assert!(second.is_err(), "second run should also fail");
}

fn fnv_hex(s: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in s {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

const FAILING_TEST_CODE: &str = r#"
use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        let x = tc.draw(gs::integers::<i32>());
        panic!("intentional failure: {}", x);
    });
}
"#;

fn failing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| TempRustProject::new().main_file(FAILING_TEST_CODE))
}

#[test]
fn test_failing_test_output() {
    let output = failing_project()
        .invoke()
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    assert_matches_regex(
        &output.stderr,
        concat!(
            r"let draw_1 = -?\d+;\n",
            r"thread '.*' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
            r"(?:Property test failed: )?intentional failure: -?\d+",
        ),
    );
}

/// A backtrace frame that should symbolize as `name`, tolerating the
/// `-C instrument-coverage` artifact where the symbolizer resolves a frame
/// to its `__covrec_*` coverage-record symbol — or to no symbol at all
/// (`<unknown>`, seen on aarch64 coverage builds) — instead of the function
/// name. Which frames lose their name is binary-layout luck, so any of the
/// frames these tests assert on can be hit on a coverage build. The fallback
/// alternatives are anchored to the frame's `at <file>` line, so the
/// assertion still proves the right frame is present.
fn frame_named(name: &str, file: &str) -> String {
    format!(r"(?:{name}|(?:__covrec_[0-9A-Fa-f]+u?|<unknown>)\n\s+at [^\n]*{file}:\d+:\d+\n)")
}

#[test]
fn test_failing_test_output_with_backtrace() {
    let output = failing_project()
        .invoke()
        .env("RUST_BACKTRACE", "1")
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
                r"(?:Property test failed: )?intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"core::panicking::panic_fmt\n",
                r".*",
                r"{user_closure}",
                r".*",
                r"{hegel_internals}",
                r".*",
                r"{user_main}",
                r".*",
                r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
            ),
            user_closure = frame_named(
                &format!(r"temp_hegel_test_\d+_\d+::main::{closure_name}\n"),
                r"src[/\\]main\.rs",
            ),
            hegel_internals = frame_named(r"hegel::runner::", r"src[/\\]runner\.rs"),
            user_main = frame_named(r"temp_hegel_test_\d+_\d+::main\n", r"src[/\\]main\.rs"),
        ),
    );
}

#[test]
fn test_failing_test_output_with_full_backtrace() {
    let output = failing_project()
        .invoke()
        .env("RUST_BACKTRACE", "full")
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
                r"(?:Property test failed: )?intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"{user_closure}",
                r".*",
                r"{hegel_internals}",
                r".*",
                r"{user_main}",
                r".*$",
            ),
            user_closure = frame_named(
                &format!(r"temp_hegel_test_\d+_\d+::main::{closure_name}"),
                r"src[/\\]main\.rs",
            ),
            hegel_internals = frame_named(r"hegel::runner::", r"src[/\\]runner\.rs"),
            user_main = frame_named(r"temp_hegel_test_\d+_\d+::main\n", r"src[/\\]main\.rs"),
        ),
    );
    assert!(
        !output.stderr.contains("Some details are omitted"),
        "Actual: {}",
        output.stderr
    );
}

mod reporting {
    use std::sync::OnceLock;

    use super::common::project::TempRustProject;

    const FAILING_TEST_CODE: &str = r#"
use hegel::{Hegel, Settings};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let _x: i64 = tc.draw(gs::integers());
        panic!("intentional failure");
    })
    .settings(Settings::new().database(None))
    .run();
}
"#;

    fn failing_project() -> &'static TempRustProject {
        static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
        PROJECT.get_or_init(|| {
            TempRustProject::new()
                .main_file(FAILING_TEST_CODE)
                .expect_failure("intentional failure")
        })
    }

    #[test]
    fn test_prints_output_by_default() {
        let output = failing_project().cargo_run(&[]);
        assert!(
            output.stderr.contains("let draw_1 = "),
            "Expected 'let draw_1 = ' in stderr (default failing-example output):\n{}",
            output.stderr
        );
    }
}

mod verbosity {
    //! test_prints_initial_attempts_on_find is omitted: it uses hypothesis.find(),
    //! a public API with no hegel-rust counterpart.

    use std::sync::OnceLock;

    use super::common::project::TempRustProject;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};

    const VERBOSE_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let x: bool = tc.draw(gs::booleans());
        assert!(x, "x should be true");
    })
    .settings(Settings::new().verbosity(Verbosity::Verbose).database(None))
    .run();
}
"#;

    fn verbose_failing_project() -> &'static TempRustProject {
        static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
        PROJECT.get_or_init(|| {
            TempRustProject::new()
                .main_file(VERBOSE_FAILING_CODE)
                .expect_failure("")
        })
    }

    const QUIET_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let x: bool = tc.draw(gs::booleans());
        assert!(x, "x should be true");
    })
    .settings(Settings::new().verbosity(Verbosity::Quiet).database(None))
    .run();
}
"#;

    fn quiet_failing_project() -> &'static TempRustProject {
        static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
        PROJECT.get_or_init(|| {
            TempRustProject::new()
                .main_file(QUIET_FAILING_CODE)
                .expect_failure("")
        })
    }

    #[test]
    fn test_does_not_log_in_quiet_mode() {
        let output = quiet_failing_project().cargo_run(&[]);
        assert!(
            !output.stderr.contains("Running test case"),
            "Unexpected progress output in quiet mode:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_includes_progress_in_verbose_mode() {
        let output = verbose_failing_project().cargo_run(&[]);
        assert!(
            output.stderr.contains("Running test case"),
            "Expected per-test-case progress output in verbose mode:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_no_indexerror_in_quiet_mode() {
        Hegel::new(|tc| {
            let _x: i64 = tc.draw(gs::integers());
        })
        .settings(Settings::new().verbosity(Verbosity::Quiet))
        .run();
    }

    #[test]
    fn test_verbose_run_succeeds_in_process() {
        Hegel::new(|tc| {
            let _x: bool = tc.draw(gs::booleans());
        })
        .settings(Settings::new().verbosity(Verbosity::Verbose).database(None))
        .run();
    }

    #[test]
    fn test_no_indexerror_in_quiet_mode_report_multiple() {
        quiet_failing_project().cargo_run(&[]);
    }

    #[test]
    fn test_no_indexerror_in_quiet_mode_report_one() {
        quiet_failing_project().cargo_run(&[]);
    }
}

mod verbose_per_test_case_output {
    //! In-process tests for the per-test-case verbose output: notes printing
    //! in every test case, stop-reason lines for Invalid/Overrun, and the
    //! panic diagnostic emitted as soon as a non-final test case fails.
    //!
    //! These tests capture output via `hegel::with_output_override`, which
    //! also intercepts the new verbose output paths.
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex};

    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};

    fn collect_output<F>(verbosity: Verbosity, body: F) -> Vec<String>
    where
        F: FnMut(hegel::TestCase) + 'static,
    {
        collect_output_with_cases(verbosity, 20, body)
    }

    fn collect_output_with_cases<F>(verbosity: Verbosity, test_cases: u64, body: F) -> Vec<String>
    where
        F: FnMut(hegel::TestCase) + 'static,
    {
        let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let buf_writer = buf.clone();
        let sink: Arc<dyn Fn(&str) + Send + Sync> =
            Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

        let _ = catch_unwind(AssertUnwindSafe(|| {
            hegel::with_output_override(sink, || {
                Hegel::new(body)
                    .settings(
                        Settings::new()
                            .verbosity(verbosity)
                            .test_cases(test_cases)
                            .database(None)
                            .derandomize(true),
                    )
                    .run();
            });
        }));

        buf.lock().unwrap().clone()
    }

    #[test]
    fn verbose_notes_print_in_every_test_case() {
        let lines = collect_output(Verbosity::Verbose, |tc| {
            let _ = tc.draw(gs::booleans());
            tc.note("hello from a test case");
        });
        let n = lines
            .iter()
            .filter(|s| s.contains("hello from a test case"))
            .count();
        assert!(
            n >= 2,
            "expected the note to print in multiple test cases, got {} matches in {:?}",
            n,
            lines
        );
    }

    #[test]
    fn normal_mode_does_not_print_notes_for_non_final_test_cases() {
        let lines = collect_output(Verbosity::Normal, |tc| {
            let _ = tc.draw(gs::booleans());
            tc.note("should not appear");
        });
        assert!(
            !lines.iter().any(|s| s.contains("should not appear")),
            "expected no notes at Normal verbosity, got {:?}",
            lines
        );
    }

    #[test]
    fn verbose_prints_stop_reason_for_failed_assumption() {
        let lines = collect_output(Verbosity::Verbose, |tc| {
            tc.assume(false);
        });
        assert!(
            lines.iter().any(|s| s.contains("failed assumption")),
            "expected a stop-reason line about a failed assumption, got {:?}",
            lines
        );
    }

    #[test]
    fn verbose_prints_stop_reason_for_out_of_data() {
        let lines = collect_output(Verbosity::Verbose, |tc| {
            let xs: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).min_size(10));
            let sum: i128 = xs.iter().map(|&x| i128::from(x)).sum();
            assert!(sum < 1000);
        });
        assert!(
            lines.iter().any(|s| s.contains("out of data")),
            "expected a stop-reason line about out of data, got {:?}",
            lines
        );
    }

    #[test]
    fn verbose_prints_full_panic_message_when_a_test_case_fails() {
        let lines = collect_output(Verbosity::Verbose, |tc| {
            let _ = tc.draw(gs::booleans());
            panic!("the canary panic message");
        });
        assert!(
            lines.iter().any(|s| s.contains("the canary panic message")),
            "expected the panic message to appear during the verbose run, got {:?}",
            lines
        );
    }

    #[test]
    fn normal_mode_does_not_print_stop_reason_for_failed_assumption() {
        let lines = collect_output(Verbosity::Normal, |tc| {
            tc.assume(false);
        });
        assert!(
            !lines
                .iter()
                .any(|s| s.contains("failed assumption") || s.contains("out of data")),
            "did not expect any stop-reason lines at Normal verbosity, got {:?}",
            lines
        );
    }
}

mod debug_information {
    use super::common::project::TempRustProject;
    use std::sync::OnceLock;

    const DEBUG_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let i: i64 = tc.draw(gs::integers::<i64>());
        assert!(i < 10);
    })
    .settings(Settings::new()
        .verbosity(Verbosity::Debug)
        .test_cases(1000)
        .database(None))
    .run();
}
"#;

    fn debug_failing_project() -> &'static TempRustProject {
        static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
        PROJECT.get_or_init(|| {
            TempRustProject::new()
                .main_file(DEBUG_FAILING_CODE)
                .expect_failure("assertion failed")
        })
    }

    #[test]
    fn test_reports_passes() {
        let output = debug_failing_project().cargo_run(&[]);
        let stderr = &output.stderr;

        assert!(
            stderr.contains("Test done."),
            "Expected 'Test done.' in debug output:\n{}",
            stderr
        );
    }
}

mod snapshots_combinators {
    //! The upstream file uses syrupy's `.ambr` snapshots to pin the exact
    //! "Falsifying example: inner(...)" output text. The portable claim is
    //! about the shrunk values, not the format string; the port asserts on
    //! the shrunk values via `minimal()` instead of capturing stderr.

    use super::common::utils::minimal;
    use hegel::generators as gs;

    #[test]
    fn test_data_draw() {
        let (x, s) = minimal(
            hegel::compose!(|tc| {
                let x = tc.draw(gs::integers::<i64>());
                let s = tc.draw(gs::text().max_size(3));
                (x, s)
            }),
            |_: &(i64, String)| true,
        );
        assert_eq!(x, 0);
        assert_eq!(s, "");
    }
}

mod snapshots_shrinking {
    //! The upstream file uses syrupy's `.ambr` snapshots of Hypothesis's
    //! "Falsifying example: inner(...)" output to pin the shrunk
    //! counterexample for each test. The underlying claim is about the
    //! shrunk value, not the format string; these ports assert on the
    //! shrunk value directly via `minimal()` instead of capturing stderr.

    use super::common::utils::minimal;
    use hegel::generators as gs;

    #[test]
    fn test_shrunk_list() {
        let xs = minimal(
            gs::vecs(gs::integers::<i64>()).min_size(1),
            |xs: &Vec<i64>| xs.iter().map(|&x| i128::from(x)).sum::<i128>() > 1000,
        );
        assert_eq!(xs, vec![1001]);
    }

    #[test]
    fn test_shrunk_float() {
        let x = minimal(
            gs::floats::<f64>().min_value(0.0).max_value(1.0),
            |x: &f64| *x > 0.5,
        );
        assert_eq!(x, 1.0);
    }
}
