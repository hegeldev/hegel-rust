#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

pub fn shared_target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hegel-shared-target")
}

static PACKAGE_NAME_ID: AtomicU64 = AtomicU64::new(0);

static WARMUP: OnceLock<()> = OnceLock::new();

fn warmup_shared_target() {
    WARMUP.get_or_init(|| {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shared_target = shared_target_dir();
        sweep_stale_temp_artifacts(&shared_target, &pid_is_live);
        run_warmup_build(&hegel_path, &shared_target);
    });
}

/// The PID embedded in a `temp_hegel_test_{pid}_{counter}` crate's artifact
/// name (with or without a trailing `-hash` / extension), or `None` for
/// anything that is not a temp-crate artifact.
pub fn temp_crate_pid(artifact_name: &str) -> Option<u32> {
    let rest = artifact_name.strip_prefix("temp_hegel_test_")?;
    let (pid, _) = rest.split_once('_')?;
    pid.parse().ok()
}

/// Whether a process with this PID is currently running. Only Linux gives a
/// cheap dependency-free answer (`/proc/<pid>`); elsewhere every PID is
/// conservatively reported live, making the sweep a no-op.
pub fn pid_is_live(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        true
    }
}

/// Remove artifacts left in the shared target directory by temp crates whose
/// owning process is gone.
///
/// Temp crate names are unique per (process, counter), so cargo can never
/// reuse a dead run's artifacts — without this sweep they accumulate at
/// roughly 15MB per `TempRustProject` per suite run until the disk fills.
/// The current process's artifacts are always kept, whatever `is_live` says;
/// removal errors are ignored (another test binary may be sweeping the same
/// entries concurrently).
pub fn sweep_stale_temp_artifacts(shared_target: &Path, is_live: &dyn Fn(u32) -> bool) {
    for dir in artifact_dirs(shared_target) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(temp_crate_pid) else {
                continue;
            };
            if pid == std::process::id() || is_live(pid) {
                continue;
            }
            remove_entry(&entry.path());
        }
    }
}

/// The directories of a cargo target dir in which a crate leaves artifacts.
fn artifact_dirs(shared_target: &Path) -> [PathBuf; 4] {
    let debug = shared_target.join("debug");
    [
        debug.clone(),
        debug.join("deps"),
        debug.join("incremental"),
        debug.join(".fingerprint"),
    ]
}

fn remove_entry(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Remove every artifact this crate left in the shared target directory.
///
/// A temp crate's name is unique per (process, counter), so its artifacts
/// (`{crate}`, `{crate}.d`, `deps/{crate}-{hash}*`, fingerprint dirs, …) can
/// never be reused by a later build — dropping them as soon as the project is
/// done keeps a long test-binary run from accumulating one 10-25MB binary per
/// `TempRustProject`. Matching requires a `-` or `.` right after the crate
/// name so one temp crate can never sweep a longer-named sibling; removal
/// errors are ignored (another sweep may run concurrently).
pub fn remove_crate_artifacts(shared_target: &Path, crate_name: &str) {
    for dir in artifact_dirs(shared_target) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let owned_by_crate = name == crate_name
                || name
                    .strip_prefix(crate_name)
                    .is_some_and(|rest| rest.starts_with('-') || rest.starts_with('.'));
            if owned_by_crate {
                remove_entry(&entry.path());
            }
        }
    }
}

/// A temp directory in the system temp dir whose name embeds the owning PID
/// (`hegel_rust_tmp_{pid}_{random}`), so that directories orphaned by killed
/// runs — `TempDir`'s `Drop` never runs on SIGKILL — can be attributed to a
/// dead process and swept by a later run instead of accumulating forever.
pub fn scratch_tempdir() -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("hegel_rust_tmp_{}_", std::process::id()))
        .tempdir()
        .unwrap()
}

/// The PID embedded in a `hegel_rust_tmp_{pid}_{random}` scratch directory
/// name, or `None` for anything that is not a scratch directory.
pub fn scratch_dir_pid(dir_name: &str) -> Option<u32> {
    let rest = dir_name.strip_prefix("hegel_rust_tmp_")?;
    let (pid, _) = rest.split_once('_')?;
    pid.parse().ok()
}

/// Remove scratch directories under `parent` whose owning process is gone.
///
/// The current process's directories are always kept, whatever `is_live`
/// says; entries that are not directories are never touched (our scratch
/// entries are always directories); removal errors are ignored (another test
/// binary may be sweeping the same entries concurrently).
pub fn sweep_stale_scratch_dirs(parent: &Path, is_live: &dyn Fn(u32) -> bool) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(scratch_dir_pid) else {
            continue;
        };
        if pid == std::process::id() || is_live(pid) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
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
        .env("CARGO_INCREMENTAL", "0")
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

impl Drop for TempRustProject {
    fn drop(&mut self) {
        remove_crate_artifacts(&shared_target_dir(), &self.crate_name);
    }
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
        let temp_dir = scratch_tempdir();
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

    /// The root directory of the temp project.
    pub fn path(&self) -> &Path {
        &self.project_path
    }

    /// The unique (per process, per counter) name of the temp crate.
    pub fn crate_name(&self) -> &str {
        &self.crate_name
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

        // A temp crate's name is unique per run, so its incremental cache can
        // never be reused — it would only bloat the shared target dir.
        cmd.env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "line-tables-only")
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
