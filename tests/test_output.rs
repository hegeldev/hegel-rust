mod common;

use common::exec::fixture;
use common::utils::assert_matches_regex;

const OUTPUT_FAILING: &str = env!("CARGO_BIN_EXE_fixture_output_failing");

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

#[test]
fn test_failing_test_output() {
    let output = fixture(OUTPUT_FAILING)
        .expect_failure("intentional failure")
        .run();

    assert_matches_regex(
        &output.stderr,
        concat!(
            r"let draw_1 = -?\d+;\n",
            r"thread '.*' \(\d+\) panicked at tests[/\\]fixtures[/\\]output_failing\.rs:\d+:\d+:\n",
            r"(?:Property test failed: )?intentional failure: -?\d+",
        ),
    );
}

/// A backtrace frame attributed to `file`, whose symbol matches `name` —
/// tolerating the two ways the symbolizer can lose the pretty name:
///
/// - the fixture binary is built with the workspace's `[profile.dev]`
///   (`opt-level = 1`, `debug = "line-tables-only"`), where closure and
///   monomorphized frames symbolize with short names (`{closure#0}`, bare
///   `main`, `run<…>`) rather than full `crate::module::fn` paths;
/// - `-C instrument-coverage` builds, where a frame can resolve to its
///   `__covrec_*` coverage-record symbol or to no symbol at all
///   (`<unknown>`, seen on aarch64 coverage builds).
///
/// The `at <file>:line[:col]` anchor is therefore what proves the right
/// frame is present (Windows backtraces omit the column); `name` narrows the
/// symbol text where it survives.
///
/// A frame can also be inlined away entirely rather than merely losing its
/// name: on aarch64 dev builds the fixture's trivial `main` is folded into
/// the runtime's `fn()` call, leaving no `main` frame at all. Callers wrap
/// such optional frames in `(?:…)?` so the assertion still holds where the
/// frame survives (e.g. x86_64) without requiring it where it does not.
fn frame_at(name: &str, file: &str) -> String {
    format!(
        r"(?:[^\n]*(?:{name})[^\n]*|__covrec_[0-9A-Fa-f]+u?|<unknown>)\n\s+at [^\n]*{file}:\d+(?::\d+)?\n"
    )
}

#[test]
fn test_failing_test_output_with_backtrace() {
    let output = fixture(OUTPUT_FAILING)
        .env("RUST_BACKTRACE", "1")
        .expect_failure("intentional failure")
        .run();

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at tests[/\\]fixtures[/\\]output_failing\.rs:\d+:\d+:\n",
                r"(?:Property test failed: )?intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"core::panicking::panic_fmt\n",
                r".*",
                r"{user_closure}",
                r".*",
                r"{hegel_internals}",
                r".*",
                r"(?:{user_main})?",
                r".*",
                r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
            ),
            user_closure = frame_at(closure_name, r"tests[/\\]fixtures[/\\]output_failing\.rs"),
            hegel_internals = frame_at(r"run", r"src[/\\]runner\.rs"),
            user_main = frame_at(
                r"(?:fixture_output_failing::)?main",
                r"tests[/\\]fixtures[/\\]output_failing\.rs",
            ),
        ),
    );
}

#[test]
fn test_failing_test_output_with_full_backtrace() {
    let output = fixture(OUTPUT_FAILING)
        .env("RUST_BACKTRACE", "full")
        .expect_failure("intentional failure")
        .run();

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at tests[/\\]fixtures[/\\]output_failing\.rs:\d+:\d+:\n",
                r"(?:Property test failed: )?intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"{user_closure}",
                r".*",
                r"{hegel_internals}",
                r".*",
                r"(?:{user_main})?",
                r".*$",
            ),
            user_closure = frame_at(closure_name, r"tests[/\\]fixtures[/\\]output_failing\.rs"),
            hegel_internals = frame_at(r"run", r"src[/\\]runner\.rs"),
            user_main = frame_at(
                r"(?:fixture_output_failing::)?main",
                r"tests[/\\]fixtures[/\\]output_failing\.rs",
            ),
        ),
    );
    assert!(
        !output.stderr.contains("Some details are omitted"),
        "Actual: {}",
        output.stderr
    );
}

mod reporting {
    use super::common::exec::fixture;

    #[test]
    fn test_prints_output_by_default() {
        let output = fixture(env!("CARGO_BIN_EXE_fixture_report_failing"))
            .expect_failure("assertion failed")
            .run();
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

    use super::common::exec::{Cmd, fixture};
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};

    fn report_failing(verbosity: &str) -> Cmd {
        // Quiet mode suppresses the report AND the closing re-raise skips
        // the panic hook, so a quiet failing run legitimately prints
        // nothing: only the nonzero exit status is expected (the empty
        // pattern matches any output, as with the old spawned projects).
        fixture(env!("CARGO_BIN_EXE_fixture_report_failing"))
            .env("HEGEL_FIXTURE_VERBOSITY", verbosity)
            .expect_failure(if verbosity == "quiet" {
                ""
            } else {
                "assertion failed"
            })
    }

    #[test]
    fn test_does_not_log_in_quiet_mode() {
        let output = report_failing("quiet").run();
        assert!(
            !output.stderr.contains("Running test case"),
            "Unexpected progress output in quiet mode:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_includes_progress_in_verbose_mode() {
        let output = report_failing("verbose").run();
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
        report_failing("quiet").run();
    }

    #[test]
    fn test_no_indexerror_in_quiet_mode_report_one() {
        report_failing("quiet").run();
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
    use super::common::exec::fixture;

    #[test]
    fn test_reports_passes() {
        let output = fixture(env!("CARGO_BIN_EXE_fixture_report_failing"))
            .env("HEGEL_FIXTURE_VERBOSITY", "debug")
            .expect_failure("assertion failed")
            .run();
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

mod override_captures_run_output {
    //! `with_output_override` captures *all* of a run's output — the engine's
    //! own progress lines, the final failure diagnostics with the
    //! reproducer line, the multi-failure headline, and output produced by
    //! clones driven on other threads. The sink is resolved once when the
    //! run starts, so it travels with the run rather than being looked up
    //! thread-locally at each emit site.
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex};

    use hegel::generators as gs;
    use hegel::{Hegel, Settings, Verbosity};

    fn collect_with<F>(settings: Settings, body: F) -> Vec<String>
    where
        F: FnMut(hegel::TestCase) + 'static,
    {
        let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let buf_writer = buf.clone();
        let sink: Arc<dyn Fn(&str) + Send + Sync> =
            Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

        let _ = catch_unwind(AssertUnwindSafe(|| {
            hegel::with_output_override(sink, || {
                Hegel::new(body).settings(settings).run();
            });
        }));

        buf.lock().unwrap().clone()
    }

    fn settings(verbosity: Verbosity) -> Settings {
        Settings::new()
            .verbosity(verbosity)
            .test_cases(5)
            .database(None)
            .derandomize(true)
    }

    #[test]
    fn engine_progress_lines_reach_the_sink() {
        let lines = collect_with(settings(Verbosity::Debug), |tc| {
            let _ = tc.draw(gs::booleans());
        });
        assert!(
            lines.iter().any(|l| l == "Starting phase: Generate"),
            "expected the engine's phase line in the sink, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("test case #")),
            "expected the engine's per-case debug lines in the sink, got {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "Test done. interesting_test_cases=0"),
            "expected the engine's summary line in the sink, got {lines:?}"
        );
    }

    #[test]
    fn engine_shrink_and_blob_replay_lines_reach_the_sink() {
        let lines = collect_with(settings(Verbosity::Debug), |tc| {
            let _ = tc.draw(gs::integers::<i64>());
            panic!("always fails");
        });
        assert!(
            lines.iter().any(|l| l.starts_with("Shrinking:")),
            "expected the engine's shrink progress in the sink, got {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("replaying failure blob:")),
            "expected the final replay's blob trace in the sink, got {lines:?}"
        );
    }

    #[test]
    fn failure_diagnostic_and_reproducer_reach_the_sink() {
        let lines = collect_with(settings(Verbosity::Normal).print_blob(true), |tc| {
            let _ = tc.draw(gs::booleans());
            panic!("canary for the sink");
        });
        assert!(
            lines.iter().any(|l| l.contains("panicked at")),
            "expected the diagnostic header in the sink, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("canary for the sink")),
            "expected the panic message in the sink, got {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("To reproduce this failure")),
            "expected the reproducer line in the sink, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("hegel::reproduce_failure")),
            "expected the reproduce_failure attribute in the sink, got {lines:?}"
        );
    }

    #[test]
    fn multi_failure_headline_reaches_the_sink() {
        let lines = collect_with(
            settings(Verbosity::Normal)
                .test_cases(20)
                .report_multiple_failures(true),
            |tc| {
                if tc.draw(gs::booleans()) {
                    panic!("first distinct bug");
                } else {
                    panic!("second distinct bug");
                }
            },
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "Property-based test failed with 2 distinct failures."),
            "expected the multi-failure headline in the sink, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("first distinct bug")),
            "expected the first bug's diagnostic in the sink, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("second distinct bug")),
            "expected the second bug's diagnostic in the sink, got {lines:?}"
        );
    }

    #[test]
    fn notes_from_a_clone_on_another_thread_reach_the_sink() {
        let lines = collect_with(settings(Verbosity::Verbose), |tc| {
            let clone = tc.clone();
            std::thread::spawn(move || {
                let _ = clone.draw(gs::booleans());
                clone.note("note from another thread");
            })
            .join()
            .unwrap();
        });
        assert!(
            lines.iter().any(|l| l.contains("note from another thread")),
            "expected the spawned thread's note in the sink, got {lines:?}"
        );
    }
}
