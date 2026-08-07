use crate::backend::RunError;
use alloc::format;

pub(crate) fn is_running_in_antithesis() -> Result<bool, RunError> {
    #[cfg(not(windows))]
    // nocov start
    if let Some(output_dir) = crate::sys::env_var("ANTITHESIS_OUTPUT_DIR") {
        return check_antithesis_output_dir(&output_dir);
    }
    // nocov end
    Ok(false)
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
