// This file provides functionality for running Hegel inside of Antithesis. It requires the `antithesis` feature to be enabled.
//
// Antithesis will never be required to use Hegel. This functionality is only to provide a better user experience when
// Hegel happens to be run inside of Antithesis.
//
// Antithesis only supports Linux, so the feature is not available on Windows.

#[cfg(not(windows))]
use std::path::Path;

#[cfg(all(feature = "antithesis", windows))]
compile_error!(
    "The `antithesis` feature is not supported on Windows. Antithesis only runs on Linux."
);

pub struct TestLocation {
    pub function: String,
    pub file: String,
    pub class: String,
    pub begin_line: u32,
}

pub(crate) fn is_running_in_antithesis() -> bool {
    // Antithesis only supports Linux; skip the check entirely on Windows.
    #[cfg(not(windows))]
    // nocov start
    if let Ok(output_dir) = std::env::var("ANTITHESIS_OUTPUT_DIR") {
        assert!(
            Path::new(&output_dir).exists(),
            "Expected ANTITHESIS_OUTPUT_DIR={output_dir} to exist when running inside of Antithesis"
        );
        return true;
    }
    // nocov end
    false
}

// nocov start
#[cfg(feature = "antithesis")]
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
