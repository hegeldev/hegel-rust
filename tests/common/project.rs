use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

pub struct TempRustProject {
    _temp_dir: TempDir,
    project_path: PathBuf,
    env_vars: Vec<(String, String)>,
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

        let src_dir = project_path.join("src");
        std::fs::create_dir(&src_dir).expect("Failed to create src directory");
        std::fs::write(src_dir.join("main.rs"), main_rs).expect("Failed to write main.rs");

        Self {
            _temp_dir: temp_dir,
            project_path,
            env_vars: Vec::new(),
        }
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn run(self) -> RunOutput {
        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(["run", "--quiet"]).current_dir(&self.project_path);

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
}
