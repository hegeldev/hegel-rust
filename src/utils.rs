/// Search PATH for a bare command name, returning the resolved path of the first match.
pub fn which(name: &str) -> Option<String> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

const HEGEL_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub fn is_hegel_file(file_path: &str) -> bool {
    std::path::Path::new(file_path).starts_with(HEGEL_CRATE_DIR)
}

/// Panic if `path` exists but is not executable.
pub fn validate_executable(path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.permissions().mode() & 0o111 == 0 {
                panic!(
                    "Hegel server binary at '{}' is not executable. \
                     Check file permissions.",
                    path
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hegel_file() {
        // returns true
        assert!(is_hegel_file(&format!("{}/src/runner.rs", HEGEL_CRATE_DIR)));

        // returns false
        assert!(!is_hegel_file("/tmp/user_project/src/main.rs"));
        // doesn't return true on a dir that happens to share a prefix
        assert!(!is_hegel_file(&format!(
            "{}-extra/src/lib.rs",
            HEGEL_CRATE_DIR
        )));
    }
}
