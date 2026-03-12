// internal helper code
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

pub struct TempRustProject {
    _temp_dir: TempDir,
    project_path: PathBuf,
    env_vars: Vec<(String, String)>,
    test_files: Vec<(String, String)>,
}

pub struct RunOutput {
    pub status: ExitStatus,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
}

impl TempRustProject {
    pub fn new(main_rs: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let project_path = temp_dir.path().to_path_buf();

        let hegel_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cargo_toml = format!(
            r#"[package]
name = "temp_hegel_test"
version = "0.1.0"
edition = "2021"

[dependencies]
hegel = {{ path = "{}" }}
"#,
            hegel_path.display()
        );
        std::fs::write(project_path.join("Cargo.toml"), cargo_toml)
            .expect("Failed to write Cargo.toml");

        // Copy the main project's Cargo.lock so the temp project uses the same
        // pinned dependency versions. Without this, cargo resolves fresh and may
        // pull in crates (e.g. getrandom 0.4+) that require a newer Rust edition
        // than our MSRV supports.
        let lock_src = hegel_path.join("Cargo.lock");
        if lock_src.exists() {
            std::fs::copy(&lock_src, project_path.join("Cargo.lock"))
                .expect("Failed to copy Cargo.lock");
        }

        let src_dir = project_path.join("src");
        std::fs::create_dir(&src_dir).expect("Failed to create src directory");
        std::fs::write(src_dir.join("main.rs"), main_rs).expect("Failed to write main.rs");

        Self {
            _temp_dir: temp_dir,
            project_path,
            env_vars: Vec::new(),
            test_files: Vec::new(),
        }
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn test_file(mut self, name: &str, content: &str) -> Self {
        self.test_files
            .push((name.to_string(), content.to_string()));
        self
    }

    fn write_test_files(&self) {
        if self.test_files.is_empty() {
            return;
        }
        let tests_dir = self.project_path.join("tests");
        std::fs::create_dir_all(&tests_dir).expect("Failed to create tests directory");
        for (name, content) in &self.test_files {
            std::fs::write(tests_dir.join(name), content).expect("Failed to write test file");
        }
    }

    fn cargo(&self, args: &[&str]) -> RunOutput {
        self.write_test_files();
        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(args).current_dir(&self.project_path);

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("Failed to run cargo");

        RunOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
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
