// internal helper code
#![allow(dead_code)]

use super::utils::assert_matches_regex;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

// Shared cargo target dir across TempRustProject instances. Each project's
// `TempDir` still owns the per-test source tree (Cargo.toml, src/, tests/),
// but every `cargo` invocation is pointed at a single `CARGO_TARGET_DIR`
// under this test binary's `CARGO_TARGET_TMPDIR`. That way, `hegeltest`
// and its transitive deps compile once and are reused across all
// TempRustProjects in the suite instead of each one doing a cold build.
//
// Previously this dir lived at `/tmp/hegel-test-cargo-target`, which caused
// two problems: every parallel checkout of the repo fought over the same
// path (and each test run wiped it clean, trashing the other checkouts'
// caches), and cold builds were paid once per `cargo test` invocation
// rather than being cached across runs. Keeping it under the workspace's
// own `target/` (via `CARGO_TARGET_TMPDIR`) isolates checkouts and lets
// the cache persist across runs. Cleanup is now `cargo clean`'s job.
fn shared_target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hegel-shared-target")
}

// use a unique package name in our Cargo.toml to avoid lock contention of parallel
// cargo builds within the shared target dir. The PID prefix disambiguates across
// sibling test-binary processes that all share `CARGO_TARGET_DIR`: without it,
// each process starts the counter at 0 and they'd collide on
// `target/debug/temp_hegel_test_0` (the "shallow" output copy cargo produces for
// binary crates), which can fail with ETXTBSY if another process's copy is still
// running.
static PACKAGE_NAME_ID: AtomicU64 = AtomicU64::new(0);

// First-build gate. A cold `cargo build` of `hegeltest` and its transitive
// dep graph peaks at hundreds of MBs of rustc memory; without this, the
// first wave of parallel test threads would each kick off their own cold
// build and risk OOM-killing rustc on smaller machines. Cargo's own
// `.cargo-lock` inside the shared target dir serialises the concurrent
// builds, but we still pay multiple cold-build startup costs.
//
// This `OnceLock` is the narrower fix: the first thread to call
// `warmup_shared_target` runs a dedicated warmup build (a synthetic
// project that depends on hegeltest) and all other threads block on
// `get_or_init` until it completes. After that the shared target dir is
// hot, every subsequent per-test cargo call is mostly linking the temp
// wrapper crate, and those can all run concurrently.
//
// Cross-process coordination (sibling test binaries spawned by
// `cargo test`) is still delegated to cargo's `.cargo-lock` — each
// process has its own `WARMUP`, but once any process has populated the
// shared target dir, every other process's warmup call finishes fast.
static WARMUP: OnceLock<()> = OnceLock::new();

fn warmup_shared_target() {
    WARMUP.get_or_init(|| {
        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shared_target = shared_target_dir();
        run_warmup_build(&hegel_path, &shared_target);
    });
}

fn write_atomic(path: &Path, content: &[u8]) {
    // PID + counter keeps the temp name unique across concurrent processes
    // and across concurrent calls in the same process. `rename(2)` is atomic
    // on POSIX: a concurrent reader of `path` never sees an in-progress write,
    // only the old or new full content.
    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let tmp_id = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), tmp_id));
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn run_warmup_build(hegel_path: &Path, shared_target: &Path) {
    let warmup_dir = shared_target.join("warmup");
    std::fs::create_dir_all(warmup_dir.join("src")).unwrap();

    // `[workspace]` marks this as its own workspace root. The warmup dir
    // lives inside `target/`, which is inside the outer hegeltest
    // workspace, so cargo would otherwise complain that the warmup
    // project looks like it should be a workspace member.
    let cargo_toml = format!(
        r#"[workspace]

[package]
name = "hegel_warmup"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}" }}
"#,
        path = hegel_path.display(),
    );
    // Write Cargo.toml and src/lib.rs atomically. Sibling test-binary processes
    // may each run their own warmup concurrently, and a plain `fs::write`
    // O_TRUNCs the target first — if a peer's cargo is mid-read of the same
    // file, it would see a truncated byte stream. Writing to a PID-suffixed
    // temp path and renaming is atomic from the reader's perspective.
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
        let crate_name = format!("temp_hegel_test_{}_{}", std::process::id(), id);

        // Copy the main project's Cargo.lock so the temp project uses the same
        // pinned dependency versions. Without this, cargo resolves fresh and may
        // pull in crates (e.g. getrandom 0.4+) that require a newer Rust edition
        // than our MSRV supports.
        let lock_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
        if lock_src.exists() {
            std::fs::copy(&lock_src, project_path.join("Cargo.lock")).unwrap();
        }

        Self {
            _temp_dir: temp_dir,
            project_path,
            crate_name,
            env_vars: Vec::new(),
            env_removes: Vec::new(),
            features: Vec::new(),
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
        // Use forward slashes in the path to avoid TOML escape issues on Windows
        let hegel_path_str = hegel_path.display().to_string().replace('\\', "/");
        let cargo_toml = format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
hegeltest = {{ path = "{path}"{features} }}
"#,
            crate_name = self.crate_name,
            path = hegel_path_str,
            features = features,
        );
        std::fs::write(self.project_path.join("Cargo.toml"), cargo_toml).unwrap();

        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(args)
            .current_dir(&self.project_path)
            .env("CARGO_TARGET_DIR", shared_target_dir());

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
