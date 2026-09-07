use crate::backend::RunError;
use alloc::format;
use alloc::string::String;

/// The environment variable Antithesis sets for every process it runs.
const OUTPUT_DIR_VAR: &str = "ANTITHESIS_OUTPUT_DIR";

/// Whether the process appears to be running inside Antithesis, judged by
/// the presence of `ANTITHESIS_OUTPUT_DIR` alone. Used to pick settings
/// defaults; the directory itself is validated later by
/// [`is_running_in_antithesis`], which is what the run consults.
pub(crate) fn antithesis_env_var_set() -> bool {
    antithesis_output_dir_from(crate::sys::env_var).is_some()
}

pub(crate) fn is_running_in_antithesis() -> Result<bool, RunError> {
    is_running_in_antithesis_from(crate::sys::env_var)
}

/// `ANTITHESIS_OUTPUT_DIR` as seen through `env`, or `None` outside
/// Antithesis. Always `None` on Windows, where Antithesis does not run, so
/// a stray variable there cannot switch on Antithesis behaviour.
fn antithesis_output_dir_from(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    env(OUTPUT_DIR_VAR).filter(|_| !cfg!(windows))
}

/// [`is_running_in_antithesis`] with the environment read injected, so the
/// inside-Antithesis path can be unit-tested without mutating the process
/// environment.
fn is_running_in_antithesis_from(env: impl Fn(&str) -> Option<String>) -> Result<bool, RunError> {
    match antithesis_output_dir_from(env) {
        Some(output_dir) => check_antithesis_output_dir(&output_dir),
        None => Ok(false),
    }
}

/// Validate the directory `ANTITHESIS_OUTPUT_DIR` points at. A missing
/// directory is a configuration error in how the process was launched —
/// reported as a run-level [`RunError::UsageError`], not an internal
/// invariant.
fn check_antithesis_output_dir(output_dir: &str) -> Result<bool, RunError> {
    if !crate::sys::fs::exists(output_dir) {
        return Err(RunError::UsageError(format!(
            "Expected {OUTPUT_DIR_VAR}={output_dir} to exist when running inside of Antithesis"
        )));
    }
    Ok(true)
}

#[cfg(test)]
#[path = "../tests/embedded/antithesis_detect_tests.rs"]
mod tests;
