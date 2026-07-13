#![allow(dead_code)] // each test binary uses its own subset of these helpers

//! Spawn helpers for end-to-end tests that need a real subprocess: either a
//! prebuilt fixture binary (a `[[bin]]` target under `tests/fixtures/`,
//! located via `env!("CARGO_BIN_EXE_…")`, so cargo owns every artifact) or
//! this very test binary re-executed to run one `#[ignore]`d fixture test
//! under the libtest harness (for scenarios that need `#[hegel::test]`
//! semantics plus a captured stderr/stdout or a controlled environment).

use super::utils::assert_matches_regex;
use std::process::Command;

pub struct RunOutput {
    pub status: std::process::ExitStatus,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
}

pub struct Cmd {
    command: Command,
    expect_failure: Option<String>,
    /// A fresh scratch cwd for the spawned process, so runs that write a
    /// default `.hegel/` database can't interfere with each other. Owned by
    /// `tempfile`, removed on drop.
    scratch_cwd: Option<tempfile::TempDir>,
}

/// Run a prebuilt fixture binary in a fresh scratch cwd. Pass the path via
/// `env!("CARGO_BIN_EXE_<name>")` so cargo guarantees the binary is built
/// and current.
pub fn fixture(exe: &str) -> Cmd {
    let scratch_cwd = tempfile::TempDir::new().unwrap();
    let mut command = Command::new(exe);
    command.current_dir(scratch_cwd.path());
    Cmd {
        command,
        expect_failure: None,
        scratch_cwd: Some(scratch_cwd),
    }
}

/// Re-execute the current test binary to run exactly one `#[ignore]`d
/// fixture test under the libtest harness. The child inherits this process's
/// (already isolated) cwd instead of chdir'ing into a tempdir of its own —
/// see the ctor in `tests/common/mod.rs`.
pub fn self_test(test_name: &str) -> Cmd {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", test_name, "--ignored", "--nocapture"]);
    command.env("HEGEL_TEST_INHERIT_CWD", "1");
    Cmd {
        command,
        expect_failure: None,
        scratch_cwd: None,
    }
}

impl Cmd {
    pub fn arg(mut self, arg: &str) -> Self {
        self.command.arg(arg);
        self
    }

    pub fn args(mut self, args: &[&str]) -> Self {
        self.command.args(args);
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.command.env(key, value);
        self
    }

    pub fn env_remove(mut self, key: &str) -> Self {
        self.command.env_remove(key);
        self
    }

    /// Expect the process to exit unsuccessfully, with combined output
    /// matching `pattern` (a regex). Without this, the process must succeed.
    pub fn expect_failure(mut self, pattern: &str) -> Self {
        self.expect_failure = Some(pattern.to_string());
        self
    }

    pub fn run(mut self) -> RunOutput {
        let output = self.command.output().unwrap();
        drop(self.scratch_cwd.take());
        let run_output = RunOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        };

        match &self.expect_failure {
            None => {
                assert!(
                    run_output.status.success(),
                    "Expected command to succeed.\nstdout:\n{}\nstderr:\n{}",
                    run_output.stdout,
                    run_output.stderr
                );
            }
            Some(pattern) => {
                assert!(
                    !run_output.status.success(),
                    "Expected command to fail.\nstdout:\n{}\nstderr:\n{}",
                    run_output.stdout,
                    run_output.stderr
                );
                let combined = format!("{}\n{}", run_output.stdout, run_output.stderr);
                assert_matches_regex(&combined, pattern);
            }
        }

        run_output
    }
}
