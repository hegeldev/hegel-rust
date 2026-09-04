use crate::backend::RunError;
use alloc::format;

/// The `ANTITHESIS_OUTPUT_DIR` variable Antithesis sets for every process it
/// runs, or `None` outside Antithesis (and always on Windows, where Antithesis
/// does not run).
fn antithesis_output_dir() -> Option<alloc::string::String> {
    #[cfg(not(windows))]
    // nocov start
    if let Some(output_dir) = crate::sys::env_var("ANTITHESIS_OUTPUT_DIR") {
        return Some(output_dir);
    }
    // nocov end
    None
}

/// Whether the process appears to be running inside Antithesis, judged by
/// the presence of `ANTITHESIS_OUTPUT_DIR` alone. Used to pick settings
/// defaults; the directory itself is validated later by
/// [`is_running_in_antithesis`], which is what the run consults.
pub(crate) fn antithesis_env_var_set() -> bool {
    antithesis_output_dir().is_some()
}

pub(crate) fn is_running_in_antithesis() -> Result<bool, RunError> {
    match antithesis_output_dir() {
        Some(output_dir) => check_antithesis_output_dir(&output_dir), // nocov
        None => Ok(false),
    }
}

/// Validate the directory `ANTITHESIS_OUTPUT_DIR` points at. A missing
/// directory is a configuration error in how the process was launched —
/// reported as a run-level [`RunError::UsageError`], not an internal
/// invariant. Split from the env read so it can be unit-tested without
/// mutating the environment.
fn check_antithesis_output_dir(output_dir: &str) -> Result<bool, RunError> {
    if !crate::sys::fs::exists(output_dir) {
        return Err(RunError::UsageError(format!(
            "Expected ANTITHESIS_OUTPUT_DIR={output_dir} to exist when running inside of Antithesis"
        )));
    }
    Ok(true)
}

#[cfg(test)]
#[path = "../tests/embedded/antithesis_detect_tests.rs"]
mod tests;
