//! The fuzzer client: running the one test case an external fuzzer asks
//! for, and reporting what it did.
//!
//! An external coverage-guided fuzzer drives a test program one execution
//! at a time through the environment. When `HEGEL_FUZZ_OUTPUT` names a
//! file, the run executes exactly one test case — skipping database
//! replay, the retry of `assume`-rejected cases, nondeterminism replays and
//! shrinking — and writes a JSON record of that case there: its status and
//! failure origin, every choice it made with the constraint it was drawn
//! under, its spans, events and targets, and the choice sequence encoded
//! for feeding back as a prefix. A failing case is still reported as the
//! run's failure, so the process exits as any failing run does.
//!
//! `HEGEL_FUZZ_PREFIX` names a file holding a choice sequence in the
//! failure database's entry format. The case replays it as a prefix and
//! draws randomly past its end. A stored value that does not fit its draw
//! is replaced by a fresh random one, or by the simplest fitting value when
//! `HEGEL_FUZZ_MISFIT=simplest`. With `HEGEL_FUZZ_TAIL=none` the case
//! draws nothing past the prefix's end: a case that asks for more
//! overruns, and a misfit is punned as the shrinker's own replays pun it,
//! so a fuzzer that shrinks by replaying candidates sees exactly what the
//! engine's shrinker would see. The record then also carries the case's
//! realized form, nodes with constraints and spans, for seeding a shrink.
//!
//! `HEGEL_FUZZ_REPRODUCE` names a prefix file to run the ordinary way
//! instead: the sequence is replayed like a database entry, and a failure
//! is shrunk, reported and saved to the database, so the test's normal runs
//! replay it from then on.
//!
//! `HEGEL_FUZZ_TEST` names the database key of the test the variables are
//! for; any other test in the process runs no test case at all, so a test
//! binary holding several tests can be driven one test at a time.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::backend::{Failure, RunError, TestRunResult};
use crate::exchange::CaseExchange;
use crate::native::base64::base64_encode;
use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue, Span, Status};
use crate::native::database::{deserialize_choices, serialize_nodes};
use crate::native::realized::serialize_realized;
use crate::native::test_runner::{self, RunResult};
use crate::settings::Settings;

const OUTPUT_VAR: &str = "HEGEL_FUZZ_OUTPUT";
const PREFIX_VAR: &str = "HEGEL_FUZZ_PREFIX";
const REPRODUCE_VAR: &str = "HEGEL_FUZZ_REPRODUCE";
const TEST_VAR: &str = "HEGEL_FUZZ_TEST";
const MISFIT_VAR: &str = "HEGEL_FUZZ_MISFIT";
const TRACE_VAR: &str = "HEGEL_FUZZ_TRACE";
const TAIL_VAR: &str = "HEGEL_FUZZ_TAIL";

/// What the environment asks of this run.
#[derive(Debug)]
pub(crate) enum FuzzMode {
    /// Run one test case from `prefix` and write its record to `output`.
    Case {
        prefix: Vec<ChoiceValue>,
        output: String,
        random_misfits: bool,
        exact: bool,
        trace: Option<String>,
    },
    /// Replay `entry` like a database entry: shrink and persist a failure.
    Reproduce { entry: Vec<u8> },
    /// The variables are for another test: run nothing.
    Skip,
}

/// The fuzz mode the process environment selects, or `None` outside a
/// fuzzer.
pub(crate) fn from_env(database_key: Option<&str>) -> Result<Option<FuzzMode>, RunError> {
    from_env_with(crate::sys::env_var, database_key)
}

/// [`from_env`] with the environment read injected, so each mode can be
/// unit-tested without mutating the process environment.
fn from_env_with(
    env: impl Fn(&str) -> Option<String>,
    database_key: Option<&str>,
) -> Result<Option<FuzzMode>, RunError> {
    let var = |name: &str| env(name).filter(|value| !value.is_empty());
    let output = var(OUTPUT_VAR);
    let reproduce = var(REPRODUCE_VAR);
    if output.is_none() && reproduce.is_none() {
        return Ok(None);
    }
    if let Some(test) = var(TEST_VAR) {
        if database_key != Some(test.as_str()) {
            return Ok(Some(FuzzMode::Skip));
        }
    }
    if let Some(path) = reproduce {
        if database_key.is_none() {
            return Err(usage(format!(
                "{REPRODUCE_VAR} needs a test with a database key to replay the entry under"
            )));
        }
        let (entry, _) = read_entry(REPRODUCE_VAR, &path)?;
        return Ok(Some(FuzzMode::Reproduce { entry }));
    }
    let prefix = match var(PREFIX_VAR) {
        Some(path) => read_entry(PREFIX_VAR, &path)?.1,
        None => Vec::new(),
    };
    let random_misfits = match var(MISFIT_VAR).as_deref() {
        None | Some("random") => true,
        Some("simplest") => false,
        Some(other) => {
            return Err(usage(format!(
                "{MISFIT_VAR} must be random or simplest, got {other:?}"
            )));
        }
    };
    let exact = match var(TAIL_VAR).as_deref() {
        None | Some("random") => false,
        Some("none") => true,
        Some(other) => {
            return Err(usage(format!(
                "{TAIL_VAR} must be random or none, got {other:?}"
            )));
        }
    };
    Ok(output.map(|output| FuzzMode::Case {
        prefix,
        output,
        random_misfits,
        exact,
        trace: var(TRACE_VAR),
    }))
}

/// The choice sequence stored at `path`, both as the entry bytes and
/// decoded, or the usage error naming the variable that pointed there.
fn read_entry(var: &str, path: &str) -> Result<(Vec<u8>, Vec<ChoiceValue>), RunError> {
    let Ok(entry) = crate::sys::fs::read(path) else {
        return Err(usage(format!("{var}={path} could not be read")));
    };
    let Some(choices) = deserialize_choices(&entry) else {
        return Err(usage(format!(
            "{var}={path} is not a choice sequence this version of Hegel can read"
        )));
    };
    Ok((entry, choices))
}

fn usage(message: String) -> RunError {
    RunError::UsageError(message)
}

/// Run the mode the environment selected, in place of the ordinary
/// exploration.
pub(crate) async fn run(
    mode: FuzzMode,
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
) -> Result<TestRunResult, RunError> {
    match mode {
        FuzzMode::Skip => Ok(TestRunResult {
            failures: Vec::new(),
        }),
        FuzzMode::Reproduce { entry } => {
            test_runner::reproduce_entry(settings, database_key, entry, exchange).await
        }
        FuzzMode::Case {
            prefix,
            output,
            random_misfits,
            exact,
            trace,
        } => {
            if let Some(path) = &trace {
                if crate::sys::fs::write(path, &[]).is_err() {
                    return Err(usage(format!("{TRACE_VAR}={path} could not be written")));
                }
            }
            let started = crate::sys::Instant::now();
            let run = test_runner::fuzz_case(
                settings,
                database_key,
                &prefix,
                random_misfits,
                exact,
                trace.as_deref(),
                exchange,
            )
            .await?;
            let elapsed = started.map(|started| started.elapsed());
            let record = record_json(database_key, &prefix, &run, elapsed);
            if crate::sys::fs::write(&output, record.as_bytes()).is_err() {
                return Err(usage(format!("{OUTPUT_VAR}={output} could not be written")));
            }
            let failures = match (run.status, run.origin) {
                (Status::Interesting, Some(origin)) => {
                    let values: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value()).collect();
                    alloc::vec![Failure {
                        origin,
                        reproduce_blob: crate::native::blob::encode_failure(&values),
                        caveat: None,
                    }]
                }
                _ => Vec::new(),
            };
            Ok(TestRunResult { failures })
        }
    }
}

/// The JSON record of one fuzz-mode execution.
fn record_json(
    database_key: Option<&str>,
    prefix: &[ChoiceValue],
    run: &RunResult,
    elapsed: Option<core::time::Duration>,
) -> String {
    let status = match run.status {
        Status::EarlyStop => "overrun",
        Status::Invalid => "invalid",
        Status::Valid => "valid",
        Status::Interesting => "interesting",
    };
    let misaligned_at = prefix
        .iter()
        .zip(run.nodes.iter())
        .position(|(stored, node)| *stored != node.value());
    let mut targets: Vec<(&String, &f64)> = run.target_observations.iter().collect();
    targets.sort_by(|a, b| a.0.cmp(b.0));
    object(&[
        ("engine_version", quoted(env!("CARGO_PKG_VERSION"))),
        ("test", optional(database_key.map(quoted))),
        ("status", quoted(status)),
        ("origin", optional(run.origin.as_deref().map(quoted))),
        ("prefix_length", prefix.len().to_string()),
        (
            "prefix_consumed",
            prefix.len().min(run.nodes.len()).to_string(),
        ),
        (
            "misaligned_at",
            optional(misaligned_at.map(|i| i.to_string())),
        ),
        ("choices", array(run.nodes.iter().map(node_json))),
        (
            "choices_base64",
            optional(serialize_nodes(&run.nodes).map(|bytes| quoted(&base64_encode(&bytes)))),
        ),
        ("spans", array(run.spans.iter().map(span_json))),
        (
            "realized_base64",
            optional(
                serialize_realized(&run.nodes, &run.spans)
                    .map(|bytes| quoted(&base64_encode(&bytes))),
            ),
        ),
        (
            "events",
            array(run.events.iter().map(|(name, value)| {
                object(&[
                    ("name", quoted(name)),
                    ("value", optional(value.map(number))),
                ])
            })),
        ),
        (
            "targets",
            object(
                &targets
                    .iter()
                    .map(|(label, score)| (label.as_str(), number(**score)))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "elapsed_ms",
            optional(elapsed.map(|elapsed| number(elapsed.as_secs_f64() * 1000.0))),
        ),
    ])
}

fn node_json(node: &ChoiceNode) -> String {
    let forced = ("forced", boolean(node.was_forced));
    match &node.data {
        ChoiceData::Integer(constraint, value) => object(&[
            ("kind", quoted("integer")),
            ("value", quoted(&value.to_string())),
            ("min", quoted(&constraint.min_value.to_string())),
            ("max", quoted(&constraint.max_value.to_string())),
            (
                "shrink_towards",
                quoted(&constraint.shrink_towards.to_string()),
            ),
            forced,
        ]),
        ChoiceData::Boolean(constraint, value) => object(&[
            ("kind", quoted("boolean")),
            ("value", boolean(*value)),
            ("p", number(constraint.p)),
            forced,
        ]),
        ChoiceData::Float(constraint, value) => object(&[
            ("kind", quoted("float")),
            ("value", number(*value)),
            ("min", number(constraint.min_value)),
            ("max", number(constraint.max_value)),
            ("allow_nan", boolean(constraint.allow_nan)),
            ("allow_infinity", boolean(constraint.allow_infinity)),
            (
                "smallest_nonzero_magnitude",
                number(constraint.smallest_nonzero_magnitude),
            ),
            forced,
        ]),
        ChoiceData::Bytes(constraint, value) => object(&[
            ("kind", quoted("bytes")),
            ("value", quoted(&hex(value))),
            ("min_size", constraint.min_size.to_string()),
            ("max_size", constraint.max_size.to_string()),
            forced,
        ]),
        ChoiceData::String(constraint, value) => object(&[
            ("kind", quoted("string")),
            ("value", string_of_codepoints(value.iter().copied())),
            ("min_size", constraint.min_size.to_string()),
            ("max_size", constraint.max_size.to_string()),
            forced,
        ]),
        ChoiceData::Clone(stream) => object(&[
            ("kind", quoted("clone")),
            ("children", array(stream.nodes().iter().map(node_json))),
            ("spans", array(stream.spans().iter().map(span_json))),
            forced,
        ]),
    }
}

fn span_json(span: &Span) -> String {
    object(&[
        ("label", quoted(&span.label.to_string())),
        ("start", span.start.to_string()),
        ("end", span.end.to_string()),
        ("depth", span.depth.to_string()),
        ("parent", optional(span.parent.map(|p| p.to_string()))),
        ("discarded", boolean(span.discarded)),
    ])
}

/// A JSON object from its fields, each value already rendered as JSON.
fn object(fields: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&quoted(key));
        out.push(':');
        out.push_str(value);
    }
    out.push('}');
    out
}

fn array(items: impl Iterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

fn optional(value: Option<String>) -> String {
    value.unwrap_or_else(|| "null".to_string())
}

fn boolean(value: bool) -> String {
    value.to_string()
}

/// A finite float as a JSON number; NaN and the infinities, which JSON
/// cannot represent, as the strings `NaN`, `inf` and `-inf`.
fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:?}")
    } else {
        quoted(&format!("{value}"))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn quoted(s: &str) -> String {
    string_of_codepoints(s.chars().map(|c| c as u32))
}

/// A JSON string literal of a sequence of code points, quotes included.
/// A lone surrogate, which no `char` can hold and which JSON parsers
/// commonly reject even escaped, prints as U+FFFD; the encoded choice
/// sequence keeps the exact value.
fn string_of_codepoints(codepoints: impl Iterator<Item = u32>) -> String {
    let mut out = String::from("\"");
    for cp in codepoints {
        match char::from_u32(cp) {
            Some('"') => out.push_str("\\\""),
            Some('\\') => out.push_str("\\\\"),
            Some('\n') => out.push_str("\\n"),
            Some('\r') => out.push_str("\\r"),
            Some('\t') => out.push_str("\\t"),
            Some(c) if cp >= 0x20 => out.push(c),
            Some(_) => out.push_str(&format!("\\u{cp:04x}")),
            None => out.push('\u{FFFD}'),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "../tests/embedded/fuzz_client_tests.rs"]
mod tests;
