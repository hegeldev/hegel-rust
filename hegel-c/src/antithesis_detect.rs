use std::path::Path;

pub(crate) fn is_running_in_antithesis() -> bool {
    #[cfg(not(windows))]
    // nocov start
    if let Ok(output_dir) = std::env::var("ANTITHESIS_OUTPUT_DIR") {
        return check_antithesis_output_dir(&output_dir);
    }
    // nocov end
    false
}

/// Validate the directory `ANTITHESIS_OUTPUT_DIR` points at. A missing
/// directory is a configuration error in how the process was launched —
/// reported as a plain panic, not an internal invariant. Split from the
/// env read so it can be unit-tested without mutating the environment.
fn check_antithesis_output_dir(output_dir: &str) -> bool {
    if !Path::new(output_dir).exists() {
        panic!(
            "Expected ANTITHESIS_OUTPUT_DIR={output_dir} to exist when running inside of Antithesis"
        );
    }
    true
}

#[cfg(test)]
#[path = "../tests/embedded/antithesis_detect_tests.rs"]
mod tests;
