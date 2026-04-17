// internal helper code
#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

// Sibling cache dirs older than this are considered abandoned by some
// previously-running test binary and are safe to remove. Well above the
// time any single test binary spends between spawn and first build, so
// we never sweep a dir that's in live use.
const STALE_TARGET_DIR_AGE: Duration = Duration::from_secs(60 * 60);

// Cache build output from TempRustProject across tests within the same binary.
// Compilation time is substantial (10+ seconds) and this lets us only incur that
// cost on the first test in each binary.
//
// Each test binary gets its own directory (keyed by executable path hash) so
// concurrent binaries don't destroy each other's build artifacts. Before
// creating our own bucket we sweep old sibling buckets — binaries that ran
// once and never ran again would otherwise leak ~300-800MB each and fill
// the disk. The directory is also cleaned on startup to remove stale
// artifacts from previous runs of the same binary.
static SHARED_TARGET_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let exe = std::env::current_exe().unwrap_or_default();
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        exe.hash(&mut h);
        h.finish()
    };
    let tmp_root = std::env::temp_dir();
    let path = tmp_root.join(format!("hegel-test-cargo-target-{:016x}", hash));
    sweep_stale_cargo_target_dirs(&tmp_root, &path, STALE_TARGET_DIR_AGE);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
});

/// Remove sibling `hegel-test-cargo-target-*` directories under `root`
/// whose mtime is older than `max_age`. `keep` is skipped regardless of
/// mtime (it's the bucket the calling binary is about to populate).
///
/// Silent on any I/O error: this is a best-effort cleanup that runs at
/// test startup, and a failure here must not prevent the test binary
/// from proceeding.
pub fn sweep_stale_cargo_target_dirs(root: &Path, keep: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("hegel-test-cargo-target-") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(mtime)
            .map(|d| d > max_age)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

// use a unique package name in our Cargo.toml to avoid lock contention of parallel
// cargo builds within the shared target dir.
static PACKAGE_NAME_ID: AtomicU64 = AtomicU64::new(0);

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
        let cached_target = &*SHARED_TARGET_DIR;

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
        cmd.args(args)
            .current_dir(&self.project_path)
            .env("CARGO_TARGET_DIR", cached_target);

        for key in &self.env_removes {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

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
