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
fn write_to_sdk_log(value: &serde_json::Value, create: bool) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let path = format!(
        "{}/sdk.jsonl",
        std::env::var("ANTITHESIS_OUTPUT_DIR").unwrap()
    );

    let mut opts = OpenOptions::new();
    opts.append(true);
    if create {
        opts.create(true);
    }
    let mut file = opts
        .open(&path)
        .unwrap_or_else(|_| panic!("failed to open {}", path));
    writeln!(file, "{}", serde_json::to_string(value).unwrap()).unwrap();
}

#[cfg(feature = "antithesis")]
pub(crate) fn emit_assertion(location: &TestLocation, passed: bool, failing_inputs: &[String], seed: &str) {
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

    write_to_sdk_log(&declaration, false);
    write_to_sdk_log(&evaluation, false);
}

#[cfg(feature = "antithesis")]
pub(crate) fn emit_setup_complete() {
    let setup_msg = serde_json::json!({
        "antithesis_setup": {
            "status": "complete",
            "details": {
                "message" : "Set up complete - ready for testing!"
            }
        }
    });

    write_to_sdk_log(&setup_msg, true);
}
