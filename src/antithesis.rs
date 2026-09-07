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

/// Hand-rolled so the always-compiled Antithesis path doesn't need the
/// optional serde_json dependency.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn assertion_json(location: &TestLocation, hit: bool, condition: bool) -> String {
    let id = json_string(&format!(
        "{}::{} passes properties",
        location.class, location.function
    ));
    format!(
        "{{\"antithesis_assert\":{{\"hit\":{hit},\"must_hit\":true,\
         \"assert_type\":\"always\",\"display_type\":\"Always\",\
         \"condition\":{condition},\"id\":{id},\"message\":{id},\
         \"location\":{{\"class\":{},\"function\":{},\"file\":{},\
         \"begin_line\":{},\"begin_column\":0}}}}}}",
        json_string(&location.class),
        json_string(&location.function),
        json_string(&location.file),
        location.begin_line,
    )
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

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|_| panic!("failed to open {}", path));
    writeln!(file, "{}", assertion_json(location, false, false)).unwrap();
    writeln!(file, "{}", assertion_json(location, true, passed)).unwrap();
}
// nocov end
