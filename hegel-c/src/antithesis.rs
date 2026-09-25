//! The [Antithesis](https://antithesis.com/) integration: detecting that the
//! process runs inside Antithesis, and reporting each test's verdict to it.
//!
//! Antithesis sets `ANTITHESIS_OUTPUT_DIR` for every process it runs and
//! collects assertions from the `sdk.jsonl` file inside that directory, one
//! JSON object per line in the format its language SDKs write. libhegel
//! reports every run whose settings carry a test location (see
//! `hegel_settings_set_test_location`) as one `always` assertion named
//! after the test — declared once and evaluated once, with the run's verdict
//! as its condition — so Antithesis lists the property alongside the
//! assertions in the system under test and flags it when it fails.

use crate::backend::RunError;
use crate::settings::Output;
use alloc::format;
use alloc::string::String;

/// The environment variable Antithesis sets for every process it runs.
const OUTPUT_DIR_VAR: &str = "ANTITHESIS_OUTPUT_DIR";

/// The file inside the output directory that Antithesis reads assertions
/// from.
const SDK_FILE: &str = "sdk.jsonl";

/// Where a property test lives, as reported to Antithesis: the function, the
/// class, module or package enclosing it, and the source position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestLocation {
    pub(crate) function: String,
    pub(crate) class: String,
    pub(crate) file: String,
    pub(crate) begin_line: u32,
}

impl TestLocation {
    /// The assertion's identifier, which Antithesis groups evaluations by.
    fn assertion_id(&self) -> String {
        format!("{}::{} passes properties", self.class, self.function)
    }
}

/// Everything needed to report a verdict once it is known: the test's
/// location and the run's output, where a failure to write the report is
/// announced.
pub(crate) struct Reporter {
    location: TestLocation,
    output: Output,
}

impl Reporter {
    pub(crate) fn new(location: TestLocation, output: Output) -> Self {
        Reporter { location, output }
    }

    /// Report the test's verdict to Antithesis when running inside it;
    /// outside Antithesis this does nothing.
    pub(crate) fn report(&self, passed: bool) {
        report_with(crate::sys::env_var, &self.location, passed, &self.output);
    }
}

/// [`Reporter::report`] with the environment read injected, so the
/// inside-Antithesis path can be unit-tested without mutating the process
/// environment.
fn report_with(
    env: impl Fn(&str) -> Option<String>,
    location: &TestLocation,
    passed: bool,
    output: &Output,
) {
    let Some(output_dir) = antithesis_output_dir_from(env) else {
        return;
    };
    let path = format!("{output_dir}/{SDK_FILE}");
    if append(&path, &assertion_lines(location, passed)).is_err() {
        output.line(&format!(
            "Failed to write the Antithesis assertion for {} to {path}",
            location.assertion_id()
        ));
    }
}

#[cfg(not(target_family = "wasm"))]
fn append(path: &str, text: &str) -> Result<(), crate::sys::Error> {
    crate::sys::fs::append(path, text.as_bytes())
}

#[cfg(target_family = "wasm")]
fn append(_path: &str, _text: &str) -> Result<(), crate::sys::Error> {
    Err(crate::sys::Error)
}

/// The two lines reporting one verdict, each terminated by a newline: the
/// declaration of the assertion (`hit: false`), which registers it with
/// Antithesis whether or not it is ever evaluated, then its evaluation
/// (`hit: true`) with `passed` as the condition.
fn assertion_lines(location: &TestLocation, passed: bool) -> String {
    let id = json_string(&location.assertion_id());
    let location = format!(
        "{{\"class\":{},\"function\":{},\"file\":{},\"begin_line\":{},\"begin_column\":0}}",
        json_string(&location.class),
        json_string(&location.function),
        json_string(&location.file),
        location.begin_line,
    );
    let line = |hit: bool, condition: bool| {
        format!(
            "{{\"antithesis_assert\":{{\"hit\":{hit},\"must_hit\":true,\
             \"assert_type\":\"always\",\"display_type\":\"Always\",\
             \"condition\":{condition},\"id\":{id},\"message\":{id},\
             \"location\":{location}}}}}\n"
        )
    };
    let mut lines = line(false, false);
    lines.push_str(&line(true, passed));
    lines
}

/// `s` as a JSON string literal, quotes included.
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
#[path = "../tests/embedded/antithesis_tests.rs"]
mod tests;
