//! Antithesis integration.
//!
//! Always compiled in; activates at runtime when `ANTITHESIS_OUTPUT_DIR` is
//! set (the environment variable Antithesis injects into every workload).
//! Everything is emitted as single-line JSON appended to
//! `$ANTITHESIS_OUTPUT_DIR/sdk.jsonl`, the [fallback SDK] channel that the
//! Antithesis environment ingests as if SDK functions had been called inline.
//!
//! Three kinds of message are emitted:
//!
//! - `antithesis_assert` — the SDK's assertion message, reporting each test's
//!   verdict as an `always` assertion (a declaration line followed by an
//!   evaluation line).
//! - `hegel_strategy_state` — a Hegel-defined lifecycle event emitted
//!   immediately before each stateful-testing rule draw, so the moment at
//!   which the next rule is chosen is distinguishable to the Antithesis
//!   fuzzer as a strategy state (a point worth saving and exploring from).
//! - `hegel_soft_terminate` — a Hegel-defined lifecycle event emitted when a
//!   `Mode::SingleTestCase` run's test case is marked invalid, telling the
//!   environment that nothing further of interest will happen on this branch.
//!
//! The `hegel_*` event names follow the fallback SDK's named-event convention
//! (`{"<name>": <details>}`) and are the contract the Antithesis platform
//! matches on; change them only in coordination with the platform side.
//!
//! [fallback SDK]: https://antithesis.com/docs/using_antithesis/sdk/fallback/
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct TestLocation {
    pub function: String,
    pub file: String,
    pub class: String,
    pub begin_line: u32,
}

/// The Antithesis output directory when running inside Antithesis, `None`
/// otherwise. Panics if `ANTITHESIS_OUTPUT_DIR` names a missing directory —
/// that is a configuration error in how the process was launched.
fn antithesis_output_dir() -> Option<String> {
    #[cfg(not(windows))]
    if let Ok(output_dir) = std::env::var("ANTITHESIS_OUTPUT_DIR") {
        // nocov start
        check_antithesis_output_dir(&output_dir);
        return Some(output_dir);
        // nocov end
    }
    None
}

pub(crate) fn is_running_in_antithesis() -> bool {
    antithesis_output_dir().is_some()
}

/// Validate the Antithesis launch configuration. Called at run start so that
/// a bad `ANTITHESIS_OUTPUT_DIR` fails before any test case runs, rather
/// than surfacing mid-run from an emission.
pub(crate) fn validate_launch_configuration() {
    is_running_in_antithesis();
}

/// Validate the directory `ANTITHESIS_OUTPUT_DIR` points at. A missing
/// directory is a configuration error in how the process was launched —
/// reported as a plain panic, not an internal invariant. Split from the
/// env read so it can be unit-tested without mutating the environment.
fn check_antithesis_output_dir(output_dir: &str) {
    if !Path::new(output_dir).exists() {
        panic!(
            "Expected ANTITHESIS_OUTPUT_DIR={output_dir} to exist when running inside of Antithesis"
        );
    }
}

/// Report a test's verdict to Antithesis as an `always` assertion: a
/// declaration line (registering the assertion in the catalog) followed by
/// an evaluation line carrying `passed`. A no-op outside Antithesis.
pub(crate) fn emit_assertion(location: &TestLocation, passed: bool) {
    emit_lines_to(
        antithesis_output_dir(),
        &[
            assertion_line(location, false, false),
            assertion_line(location, true, passed),
        ],
    );
}

/// Mark the moment immediately before a stateful rule draw as an Antithesis
/// strategy state. `step` is the 1-based number of the rule application
/// about to be drawn within the current test case. A no-op outside
/// Antithesis.
pub(crate) fn emit_strategy_state(step: i64) {
    emit_lines_to(antithesis_output_dir(), &[strategy_state_line(step)]);
}

/// Tell Antithesis that nothing further of interest will happen on this
/// branch — a soft terminate, as opposed to `fuzz_exit`'s hard process
/// exit. A no-op outside Antithesis.
pub(crate) fn emit_soft_terminate(reason: &str) {
    emit_lines_to(antithesis_output_dir(), &[soft_terminate_line(reason)]);
}

/// Append `lines` to `output_dir`'s `sdk.jsonl` when `output_dir` is set;
/// do nothing otherwise. The emitters above always build their lines and
/// leave the are-we-in-Antithesis decision to this single seam.
fn emit_lines_to(output_dir: Option<String>, lines: &[String]) {
    if let Some(output_dir) = output_dir {
        append_sdk_jsonl(&output_dir, lines);
    }
}

fn append_sdk_jsonl(output_dir: &str, lines: &[String]) {
    let path = Path::new(output_dir).join("sdk.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|_| panic!("failed to open {}", path.display()));
    for line in lines {
        writeln!(file, "{line}").unwrap();
    }
}

/// One `antithesis_assert` message. The declaration uses
/// `(hit: false, condition: false)`; the evaluation uses `hit: true` and the
/// actual verdict.
fn assertion_line(location: &TestLocation, hit: bool, condition: bool) -> String {
    let id = json_string(&format!(
        "{}::{} passes properties",
        location.class, location.function
    ));
    let location = format!(
        "{{\"class\":{},\"function\":{},\"file\":{},\"begin_line\":{},\"begin_column\":0}}",
        json_string(&location.class),
        json_string(&location.function),
        json_string(&location.file),
        location.begin_line,
    );
    format!(
        "{{\"antithesis_assert\":{{\"hit\":{hit},\"must_hit\":true,\
         \"assert_type\":\"always\",\"display_type\":\"Always\",\
         \"condition\":{condition},\"id\":{id},\"message\":{id},\
         \"location\":{location}}}}}"
    )
}

fn strategy_state_line(step: i64) -> String {
    format!("{{\"hegel_strategy_state\":{{\"step\":{step}}}}}")
}

fn soft_terminate_line(reason: &str) -> String {
    format!(
        "{{\"hegel_soft_terminate\":{{\"reason\":{}}}}}",
        json_string(reason)
    )
}

/// Encode `s` as a JSON string literal (RFC 8259: quote, backslash, and
/// control characters escaped; everything else passed through as UTF-8).
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

#[cfg(test)]
#[path = "../tests/embedded/antithesis_tests.rs"]
mod tests;
