#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

fn shared_target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hegel-shared-target")
}

static PACKAGE_NAME_ID: AtomicU64 = AtomicU64::new(0);

static WARMUP: OnceLock<()> = OnceLock::new();

fn warmup_shared_target() {
    WARMUP.get_or_init(|| {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shared_target = shared_target_dir();
        run_warmup_build(&hegel_path, &shared_target);
    });
}

fn write_atomic(path: &Path, content: &[u8]) {
    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let tmp_id = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), tmp_id));
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn run_warmup_build(hegel_path: &Path, shared_target: &Path) {
    let warmup_dir = shared_target.join("warmup");
    std::fs::create_dir_all(warmup_dir.join("src")).unwrap();

    let hegel_path_str = hegel_path.display().to_string().replace('\\', "/");
    let cargo_toml = format!(
        r#"[workspace]

[package]
name = "hegel_warmup"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}" }}
"#,
        path = hegel_path_str,
    );
    write_atomic(&warmup_dir.join("Cargo.toml"), cargo_toml.as_bytes());
    write_atomic(&warmup_dir.join("src/lib.rs"), b"");

    let lock_src = hegel_path.join("Cargo.lock");
    if lock_src.exists() {
        let lock_bytes = std::fs::read(&lock_src).unwrap();
        write_atomic(&warmup_dir.join("Cargo.lock"), &lock_bytes);
    }

    let output = Command::new(env!("CARGO"))
        .args(["build", "--quiet"])
        .current_dir(&warmup_dir)
        .env("CARGO_TARGET_DIR", shared_target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "warmup cargo build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

pub struct TempRustProject {
    _temp_dir: TempDir,
    project_path: PathBuf,
    crate_name: String,
    features: Vec<String>,
    env_vars: Vec<(String, String)>,
    env_removes: Vec<String>,
    expect_failure: Option<String>,
}

pub struct RunOutput {
    pub status: ExitStatus,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
}

/// A single `cargo run` / `cargo test` invocation of a `TempRustProject`.
///
/// Enables reusing one built project across multiple `#[test]`s: build the
/// project once (e.g. behind a `OnceLock`), then call `.invoke()` per test
/// and configure env / expected failure / CLI args independently. Each
/// invocation still spawns its own cargo subprocess, but the wrapper crate
/// and its deps are only compiled once, and subsequent cargo calls reuse
/// the cached binary.
pub struct Invocation<'a> {
    project: &'a TempRustProject,
    env_vars: Vec<(String, String)>,
    env_removes: Vec<String>,
    expect_failure: Option<String>,
}

impl TempRustProject {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().to_path_buf();

        let id = PACKAGE_NAME_ID.fetch_add(1, Ordering::Relaxed);
        let crate_name = format!("temp_hegel_test_{}_{}", std::process::id(), id);

        let lock_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
        if lock_src.exists() {
            std::fs::copy(&lock_src, project_path.join("Cargo.lock")).unwrap();
        }

        let features: Vec<String> = Vec::new();

        Self {
            _temp_dir: temp_dir,
            project_path,
            crate_name,
            features,
            env_vars: Vec::new(),
            env_removes: Vec::new(),
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

    /// Begin a fresh invocation of this project. Use this when reusing one
    /// built project across several tests with different env / expected
    /// failure / CLI args.
    pub fn invoke(&self) -> Invocation<'_> {
        Invocation {
            project: self,
            env_vars: Vec::new(),
            env_removes: Vec::new(),
            expect_failure: None,
        }
    }

    fn default_invocation(&self) -> Invocation<'_> {
        Invocation {
            project: self,
            env_vars: self.env_vars.clone(),
            env_removes: self.env_removes.clone(),
            expect_failure: self.expect_failure.clone(),
        }
    }

    pub fn cargo_run(&self, args: &[&str]) -> RunOutput {
        self.default_invocation().cargo_run(args)
    }

    pub fn cargo_test(&self, args: &[&str]) -> RunOutput {
        self.default_invocation().cargo_test(args)
    }
}

impl<'a> Invocation<'a> {
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn env_remove(mut self, key: &str) -> Self {
        self.env_removes.push(key.to_string());
        self
    }

    pub fn expect_failure(mut self, pattern: &str) -> Self {
        self.expect_failure = Some(pattern.to_string());
        self
    }

    fn cargo(self, args: &[&str]) -> RunOutput {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let features = if self.project.features.is_empty() {
            String::new()
        } else {
            format!(
                ", features = [{}]",
                self.project
                    .features
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let hegel_path_str = hegel_path.display().to_string().replace('\\', "/");
        let cargo_toml = format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}"{features} }}
"#,
            crate_name = self.project.crate_name,
            path = hegel_path_str,
            features = features,
        );
        write_atomic(
            &self.project.project_path.join("Cargo.toml"),
            cargo_toml.as_bytes(),
        );

        let use_coverage = option_env!("__CARGO_LLVM_COV_RUSTC_WRAPPER").is_some();

        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(args)
            .current_dir(&self.project.project_path)
            .env("CARGO_TARGET_DIR", shared_target_dir());

        cmd.env("CARGO_PROFILE_DEV_DEBUG", "line-tables-only")
            .env("CARGO_PROFILE_TEST_DEBUG", "line-tables-only");

        if use_coverage {
            let existing =
                std::env::var("__CARGO_LLVM_COV_RUSTC_WRAPPER_CRATE_NAMES").unwrap_or_default();
            let new_names = if existing.is_empty() {
                format!("{},test", self.project.crate_name)
            } else {
                format!("{},{},test", existing, self.project.crate_name)
            };
            cmd.env("__CARGO_LLVM_COV_RUSTC_WRAPPER_CRATE_NAMES", new_names);
        }

        for key in &self.env_removes {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

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

    pub fn cargo_run(self, args: &[&str]) -> RunOutput {
        let mut all = vec!["run", "--quiet"];
        all.extend(args);
        self.cargo(&all)
    }

    pub fn cargo_test(self, args: &[&str]) -> RunOutput {
        let mut all = vec!["test", "--quiet"];
        all.extend(args);
        self.cargo(&all)
    }
}
