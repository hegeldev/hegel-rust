use std::path::Path;

pub struct TestLocation {
    pub function: String,
    pub file: String,
    pub class: String,
    pub begin_line: u32,
}

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
#[path = "../tests/embedded/antithesis_tests.rs"]
mod tests;

// nocov start
pub(crate) fn emit_assertion(location: &TestLocation, passed: bool) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let path = format!(
        "{}/sdk.jsonl",
        std::env::var("ANTITHESIS_OUTPUT_DIR").unwrap()
    );

    let id = format!(
        "{}::{} passes properties",
        location.class, location.function
    );

    let location_obj = serde_json::json!({
        "class": location.class,
        "function": location.function,
        "file": location.file,
        "begin_line": location.begin_line,
        "begin_column": 0,
    });

    let declaration = serde_json::json!({
        "antithesis_assert": {
            "hit": false,
            "must_hit": true,
            "assert_type": "always",
            "display_type": "Always",
            "condition": false,
            "id": id,
            "message": id,
            "location": location_obj,
        }
    });

    let evaluation = serde_json::json!({
        "antithesis_assert": {
            "hit": true,
            "must_hit": true,
            "assert_type": "always",
            "display_type": "Always",
            "condition": passed,
            "id": id,
            "message": id,
            "location": location_obj,
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|_| panic!("failed to open {}", path));
    writeln!(file, "{}", serde_json::to_string(&declaration).unwrap()).unwrap();
    writeln!(file, "{}", serde_json::to_string(&evaluation).unwrap()).unwrap();
}
// nocov end
