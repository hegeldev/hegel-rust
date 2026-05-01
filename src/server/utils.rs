/// Search PATH for a bare command name, returning the resolved path of the first match.
///
/// On Windows, also tries appending extensions from `PATHEXT` (e.g. `.EXE`, `.CMD`).
pub fn which(name: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
        #[cfg(windows)]
        for ext in executable_extensions() {
            let with_ext = dir.join(format!("{name}{ext}"));
            if with_ext.is_file() {
                return Some(with_ext.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(windows)]
pub(crate) fn executable_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .map(|s| s.to_string())
        .collect()
}

/// Panic if `path` exists but is not executable.
pub fn validate_executable(path: &str) {
    let _ = path;
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
#[path = "../../tests/embedded/server/utils_tests.rs"]
mod tests;
