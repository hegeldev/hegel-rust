// internal helper code
#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::ExitStatus;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
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

// First-build gate. A cold `cargo build` of `hegeltest` and its
// transitive dep graph peaks at hundreds of MBs of rustc memory; with a
// previous-generation unconditional `Mutex` around every cargo
// invocation, we avoided letting parallel test threads race several of
// those cold builds at once (which OOM-kills rustc on ≤8 GiB machines).
// The mutex also serialised every *warm* rebuild, which is wasted
// parallelism once the shared target dir is populated.
//
// This `OnceLock` is the narrower replacement: the first thread to call
// `warmup_shared_target` runs a dedicated warmup build (a synthetic
// project that depends on hegeltest with the feature set this test
// binary uses) and all other threads block on `get_or_init` until it
// completes. After that the shared target dir is hot, every subsequent
// per-test cargo call is mostly linking the temp wrapper crate, and
// they can all run concurrently.
//
// Cross-process (cargo-test-launched sibling binaries) coordination is
// delegated to cargo's own `.cargo-lock` inside the shared target dir,
// same as before; we've never tried to serialise those and the workload
// hasn't been a problem in practice.
static WARMUP: OnceLock<()> = OnceLock::new();

fn shared_target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hegel-shared-target")
}

fn warmup_shared_target() {
    WARMUP.get_or_init(|| {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shared_target = shared_target_dir();
        let features: &[&str] = if cfg!(feature = "native") {
            &["native"]
        } else {
            &[]
        };
        run_warmup_build(&hegel_path, &shared_target, features);
    });
}

fn run_warmup_build(hegel_path: &Path, shared_target: &Path, features: &[&str]) {
    let suffix = if features.is_empty() {
        "base".to_string()
    } else {
        features.join("_")
    };
    let warmup_dir = shared_target.join(format!("warmup_{}", suffix));
    std::fs::create_dir_all(warmup_dir.join("src")).unwrap();

    let features_str = if features.is_empty() {
        String::new()
    } else {
        format!(
            ", features = [{}]",
            features
                .iter()
                .map(|f| format!("\"{}\"", f))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // `[workspace]` marks this as its own workspace root. The warmup dir
    // lives inside `target/`, which is inside the outer hegeltest
    // workspace, so cargo would otherwise complain that the warmup
    // project looks like it should be a workspace member.
    std::fs::write(
        warmup_dir.join("Cargo.toml"),
        format!(
            r#"[workspace]

[package]
name = "hegel_warmup_{suffix}"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}"{features_str} }}
"#,
            suffix = suffix,
            path = hegel_path.display(),
            features_str = features_str,
        ),
    )
    .unwrap();
    std::fs::write(warmup_dir.join("src/lib.rs"), "").unwrap();

    let lock_src = hegel_path.join("Cargo.lock");
    if lock_src.exists() {
        std::fs::copy(&lock_src, warmup_dir.join("Cargo.lock")).unwrap();
    }

    let output = Command::new(env!("CARGO"))
        .args(["build", "--quiet"])
        .current_dir(&warmup_dir)
        .env("CARGO_TARGET_DIR", shared_target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "warmup cargo build failed for features {:?}\nstdout:\n{}\nstderr:\n{}",
        features,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
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
        let shared_target = shared_target_dir();
        cmd.env("CARGO_TARGET_DIR", &shared_target);

        for key in &self.env_removes {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        // Ensure the shared target dir has been warmed up once in this
        // process before any test thread spawns its own cargo. See the
        // comment on `WARMUP` above for why a one-shot gate is enough.
        warmup_shared_target();
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
