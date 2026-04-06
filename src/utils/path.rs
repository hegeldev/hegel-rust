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
