use crate::backend::RunError;
#[cfg(not(target_family = "wasm"))]
use alloc::format;
use alloc::string::String;

/// The environment variable Antithesis sets for every process it runs.
const OUTPUT_DIR_VAR: &str = "ANTITHESIS_OUTPUT_DIR";

/// Whether the process appears to be running inside Antithesis, judged by
/// the presence of `ANTITHESIS_OUTPUT_DIR` alone. Used to pick settings
/// defaults; the directory itself is validated at run start by
/// [`check_environment`].
pub(crate) fn antithesis_env_var_set() -> bool {
    antithesis_env_var_set_from(crate::sys::env_var)
}

/// [`antithesis_env_var_set`] with the environment read injected, for use in
/// profile selection where the whole environment is injected together.
pub(crate) fn antithesis_env_var_set_from(env: impl Fn(&str) -> Option<String>) -> bool {
    antithesis_output_dir_from(env).is_some()
}

/// Fail if the process claims to be inside Antithesis but the output
/// directory it names does not exist. Outside Antithesis this is a no-op.
pub(crate) fn check_environment() -> Result<(), RunError> {
    #[cfg(target_family = "wasm")]
    return Ok(());

    #[cfg(not(target_family = "wasm"))]
    check_environment_from(crate::sys::env_var)
}

/// `ANTITHESIS_OUTPUT_DIR` as seen through `env`, or `None` outside
/// Antithesis. Always `None` on Windows, where Antithesis does not run, so
/// a stray variable there cannot switch on Antithesis behaviour.
fn antithesis_output_dir_from(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    env(OUTPUT_DIR_VAR).filter(|_| !cfg!(windows))
}

/// [`check_environment`] with the environment read injected, so the
/// inside-Antithesis path can be unit-tested without mutating the process
/// environment.
#[cfg(not(target_family = "wasm"))]
fn check_environment_from(env: impl Fn(&str) -> Option<String>) -> Result<(), RunError> {
    match antithesis_output_dir_from(env) {
        Some(output_dir) => check_antithesis_output_dir(&output_dir),
        None => Ok(()),
    }
}

/// Validate the directory `ANTITHESIS_OUTPUT_DIR` points at. A missing
/// directory is a configuration error in how the process was launched —
/// reported as a run-level [`RunError::UsageError`], not an internal
/// invariant. Split from the env read so it can be unit-tested without
/// mutating the process environment.
#[cfg(not(target_family = "wasm"))]
fn check_antithesis_output_dir(output_dir: &str) -> Result<(), RunError> {
    if !crate::sys::fs::exists(output_dir) {
        return Err(RunError::UsageError(format!(
            "Expected {OUTPUT_DIR_VAR}={output_dir} to exist when running inside of Antithesis"
        )));
    }
    Ok(())
}

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "../tests/embedded/antithesis_detect_tests.rs"]
mod tests;
