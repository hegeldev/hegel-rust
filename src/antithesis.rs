use std::path::Path;

// This file provides functionality for running Hegel inside of Antithesis. It requires the `antithesis` feature to be enabled.
//
// Antithesis will never be required to use Hegel. This functionality is only to provide a better user experience when
// Hegel happens to be run inside of Antithesis.

pub struct TestLocation {
    pub function: String,
    pub file: String,
    pub class: String,
    pub begin_line: u32,
}

pub(crate) fn is_running_in_antithesis() -> bool {
    match std::env::var("ANTITHESIS_OUTPUT_DIR") {
        Ok(output_dir) => {
            assert!(
                Path::new(&output_dir).exists(),
                "Expected ANTITHESIS_OUTPUT_DIR={output_dir} to exist when running inside of Antithesis"
            );
            true
        }
        Err(_) => false,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ENV_TEST_MUTEX;

    #[test]
    fn test_is_running_in_antithesis_with_valid_dir() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap().to_string();
        let original = std::env::var("ANTITHESIS_OUTPUT_DIR").ok();
        // SAFETY: serialized by ENV_LOCK
        unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", &path) };
        assert!(is_running_in_antithesis());
        match original {
            Some(v) => unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", v) },
            None => unsafe { std::env::remove_var("ANTITHESIS_OUTPUT_DIR") },
        }
    }

    #[test]
    fn test_is_running_in_antithesis_without_env() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var("ANTITHESIS_OUTPUT_DIR").ok();
        if original.is_some() {
            unsafe { std::env::remove_var("ANTITHESIS_OUTPUT_DIR") };
        }
        assert!(!is_running_in_antithesis());
        if let Some(v) = original {
            unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", v) };
        }
    }

    #[cfg(feature = "antithesis")]
    #[test]
    fn test_emit_assertion_writes_jsonl() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap().to_string();
        let original = std::env::var("ANTITHESIS_OUTPUT_DIR").ok();
        // SAFETY: serialized by ENV_LOCK
        unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", &path) };

        let loc = TestLocation {
            function: "test_func_42".into(),
            file: "test_file.rs".into(),
            class: "test_module".into(),
            begin_line: 99,
        };
        emit_assertion(&loc, true);

        match original {
            Some(v) => unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", v) },
            None => unsafe { std::env::remove_var("ANTITHESIS_OUTPUT_DIR") },
        }

        let jsonl_path = dir.path().join("sdk.jsonl");
        assert!(jsonl_path.exists());
        let contents = std::fs::read_to_string(&jsonl_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(contents.contains("test_func_42"));
        assert!(contents.contains("test_module"));
    }
}
