use std::path::Path;

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
pub(crate) fn emit_assertion(
    location: &TestLocation,
    passed: bool,
    failing_inputs: &[String],
    seed: &str,
) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let id = format!(
        "{}::{} passes properties",
        location.class, location.function
    );

    let path = format!(
        "{}/sdk.jsonl",
        std::env::var("ANTITHESIS_OUTPUT_DIR").unwrap()
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
            "details": null,
        }
    });

    let details = if !passed && !failing_inputs.is_empty() {
        serde_json::json!({
            "failing_inputs": failing_inputs,
            "seed": seed
        })
    } else {
        serde_json::Value::Null
    };

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
            "details": details,
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
