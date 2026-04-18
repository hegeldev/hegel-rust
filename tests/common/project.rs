// internal helper code
#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

// Shared cargo target dir across TempRustProject instances. Each
// project's `TempDir` still owns the per-test source tree (Cargo.toml,
// src/, tests/), but every `cargo` invocation is pointed at a single
// `CARGO_TARGET_DIR` under `target/` (via `CARGO_TARGET_TMPDIR`). That
// way, `hegeltest` and its transitive deps compile once per feature
// configuration and then get reused across all 40+ TempRustProjects in
// the test suite, instead of each one doing a cold build.
//
// Cleanup is now `cargo clean`'s job, not RAII. The shared target dir
// sits inside the workspace's `target/` (where `CARGO_TARGET_TMPDIR`
// already lives), so it gets swept whenever the workspace is cleaned.
// A previous version of this file kept per-project `target/` dirs
// under `TempDir` precisely so RAII could clean them up — the
// motivation was a ~300-800MB leak into `/tmp` per stopped test binary
// when a shared target dir lived there. That motivation is gone now
// that the shared dir is inside `target/`.

static PACKAGE_NAME_ID: AtomicU64 = AtomicU64::new(0);

// Serialize cargo invocations across tests. Even with a shared
// `CARGO_TARGET_DIR`, a cold build of the shared artefacts kicked off
// by several tests in parallel can OOM-kill rustc on modest machines
// (confusing "signal: 9, SIGKILL" failures). Holding this lock around
// the cargo call means a single build runs at a time, but that build
// gets all cores — net wall time is similar and we avoid the OOM. Once
// the shared dir is warm, the per-test cost is mostly linking the test
// crate, which is cheap anyway.
static CARGO_LOCK: Mutex<()> = Mutex::new(());

fn lock_cargo() -> MutexGuard<'static, ()> {
    match CARGO_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub struct TempRustProject {
    _temp_dir: TempDir,
    project_path: PathBuf,
    crate_name: String,
    env_vars: Vec<(String, String)>,
    env_removes: Vec<String>,
    features: Vec<String>,
    expect_failure: Option<String>,
}

pub struct RunOutput {
    pub status: ExitStatus,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
}

impl TempRustProject {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().to_path_buf();

        let id = PACKAGE_NAME_ID.fetch_add(1, Ordering::Relaxed);
        let crate_name = format!("temp_hegel_test_{}", id);

        // Copy the main project's Cargo.lock so the temp project uses the same
        // pinned dependency versions. Without this, cargo resolves fresh and may
        // pull in crates (e.g. getrandom 0.4+) that require a newer Rust edition
        // than our MSRV supports.
        let lock_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
        if lock_src.exists() {
            std::fs::copy(&lock_src, project_path.join("Cargo.lock")).unwrap();
        }

        // When the outer test suite is compiled with --features native, automatically
        // enable the native feature in all TempRustProject subprocesses so they exercise
        // the same backend rather than silently falling back to the server path.
        let features = if cfg!(feature = "native") {
            vec!["native".to_string()]
        } else {
            Vec::new()
        };

        Self {
            _temp_dir: temp_dir,
            project_path,
            crate_name,
            env_vars: Vec::new(),
            env_removes: Vec::new(),
            features,
            expect_failure: None,
        }
    }

    pub fn main_file(self, code: &str) -> Self {
        let src_dir = self.project_path.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), code).unwrap();
        self
    }

    pub fn test_file(self, name: &str, content: &str) -> Self {
        let tests_dir = self.project_path.join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        std::fs::write(tests_dir.join(name), content).unwrap();
        self
    }

    pub fn feature(mut self, feature: &str) -> Self {
        self.features.push(feature.to_string());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn expect_failure(mut self, pattern: &str) -> Self {
        self.expect_failure = Some(pattern.to_string());
        self
    }

    pub fn env_remove(mut self, key: &str) -> Self {
        self.env_removes.push(key.to_string());
        self
    }

    fn cargo(&self, args: &[&str]) -> RunOutput {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let features = if self.features.is_empty() {
            String::new()
        } else {
            format!(
                ", features = [{}]",
                self.features
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let cargo_toml = format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}"{features} }}
"#,
            crate_name = self.crate_name,
            path = hegel_path.display(),
            features = features,
        );
        std::fs::write(self.project_path.join("Cargo.toml"), cargo_toml).unwrap();

        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(args).current_dir(&self.project_path);

        // Point every TempRustProject build at a single shared target
        // directory under `target/`. `CARGO_TARGET_TMPDIR` is provided
        // by cargo for integration tests and lives inside the outer
        // workspace's `target/`, so `cargo clean` still sweeps it.
        let shared_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hegel-shared-target");
        cmd.env("CARGO_TARGET_DIR", &shared_target);

        for key in &self.env_removes {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        let _guard = lock_cargo();
        let output = cmd.output().unwrap();

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

    pub fn cargo_run(&self, args: &[&str]) -> RunOutput {
        let mut all = vec!["run", "--quiet"];
        all.extend(args);
        self.cargo(&all)
    }

    pub fn cargo_test(&self, args: &[&str]) -> RunOutput {
        let mut all = vec!["test", "--quiet"];
        all.extend(args);
        self.cargo(&all)
    }
}
