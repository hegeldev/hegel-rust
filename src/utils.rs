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

const HEGEL_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Source directories within the hegel crate that count as "internal".
/// Only panics from these directories are treated as hegel errors; panics
/// from tests/ or other paths are treated as user test failures.
const HEGEL_SRC_DIRS: &[&str] = &["src", "hegel-macros/src"];

pub fn is_hegel_file(file_path: &str) -> bool {
    let path = std::path::Path::new(file_path);

    // Get the path relative to the hegel crate root.
    let relative = if path.is_absolute() {
        match path.strip_prefix(HEGEL_CRATE_DIR) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => return false,
        }
    } else {
        // When running inside hegel's own test binary, panic locations use
        // paths relative to the crate root. Verify the file exists there.
        if !std::path::Path::new(HEGEL_CRATE_DIR).join(path).is_file() {
            return false;
        }
        path.to_path_buf()
    };

    // Normalize the relative path (resolve ".." components) and check it
    // lives under a hegel source directory, not under tests/ or elsewhere.
    let mut normalized = std::path::PathBuf::new();
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            c => normalized.push(c),
        }
    }

    HEGEL_SRC_DIRS.iter().any(|dir| normalized.starts_with(dir))
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
mod tests {
    use super::*;

    #[test]
    fn test_is_hegel_file() {
        // absolute path within hegel crate src/ returns true
        assert!(is_hegel_file(&format!("{}/src/runner.rs", HEGEL_CRATE_DIR)));

        // relative path under src/ that exists returns true
        assert!(is_hegel_file("src/runner.rs"));
        assert!(is_hegel_file("src/utils.rs"));

        // absolute path outside hegel crate returns false
        assert!(!is_hegel_file("/tmp/user_project/src/main.rs"));
        // doesn't return true on a dir that happens to share a prefix
        assert!(!is_hegel_file(&format!(
            "{}-extra/src/lib.rs",
            HEGEL_CRATE_DIR
        )));

        // relative path that doesn't exist under the crate root returns false
        assert!(!is_hegel_file("src/nonexistent_file.rs"));
        assert!(!is_hegel_file("user_code/main.rs"));

        // test files are NOT hegel-internal, even though they exist in the crate
        assert!(!is_hegel_file("tests/common/utils.rs"));
        assert!(!is_hegel_file(&format!(
            "{}/tests/common/utils.rs",
            HEGEL_CRATE_DIR
        )));

        // paths with ".." that resolve to tests/ are NOT hegel-internal
        assert!(!is_hegel_file("tests/test_find_quality/../common/utils.rs"));

        // paths with "./" current-dir component are normalized correctly
        assert!(is_hegel_file("./src/runner.rs"));
    }
}
