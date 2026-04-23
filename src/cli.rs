//! Command line argument parsing for standalone Hegel binaries produced by
//! `#[hegel::main]`.
//!
//! The parser starts from a caller-provided [`Settings`] value (so that
//! `#[hegel::main(test_cases = 500)]` produces a binary whose `--test-cases`
//! flag defaults to 500) and applies CLI overrides on top of it.

use crate::settings::{HealthCheck, Mode, Settings, Verbosity};

/// Result of applying CLI overrides. The macro wrapper in `#[hegel::main]`
/// dispatches on this to print messages and exit the process; keeping the
/// I/O and process-exit out of this function makes it directly testable.
#[derive(Debug)]
pub enum CliOutcome {
    /// Settings parsed successfully.
    Success(Settings),
    /// `--help` / `-h` was passed. Caller should print the message to stdout
    /// and exit with code 0.
    Help(String),
    /// Unknown argument or malformed value. Caller should print the message
    /// to stderr and exit with a nonzero code.
    ParseError(String),
}

/// Apply CLI overrides to `settings`.
///
/// `args` should include the program name at index 0 (i.e., pass
/// `std::env::args()` directly).
///
/// This is called from the entry point produced by `#[hegel::main]`; it is
/// exported here so that other main wrappers can construct Settings from
/// the same CLI surface.
pub fn apply_cli_args<I>(settings: Settings, args: I) -> CliOutcome
where
    I: IntoIterator<Item = String>,
{
    match try_apply_cli_args(settings, args) {
        Ok(s) => CliOutcome::Success(s),
        Err(CliError::Help(msg)) => CliOutcome::Help(msg),
        Err(CliError::Parse(msg)) => CliOutcome::ParseError(format!("{}\n\n{}", msg, usage())),
    }
}

#[derive(Debug)]
enum CliError {
    Help(String),
    Parse(String),
}

fn try_apply_cli_args<I>(mut settings: Settings, args: I) -> Result<Settings, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();
    let args: Vec<String> = iter.collect();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--help" | "-h" => {
                return Err(CliError::Help(usage()));
            }
            "--test-cases" => {
                let value = next_value(&args, &mut i, "--test-cases")?;
                let n: u64 = value.parse().map_err(|_| {
                    CliError::Parse(format!(
                        "--test-cases expects a non-negative integer, got {value:?}"
                    ))
                })?;
                settings = settings.test_cases(n);
            }
            "--seed" => {
                let value = next_value(&args, &mut i, "--seed")?;
                if value == "none" {
                    settings = settings.seed(None);
                } else {
                    let n: u64 = value.parse().map_err(|_| {
                        CliError::Parse(format!(
                            "--seed expects an integer or 'none', got {value:?}"
                        ))
                    })?;
                    settings = settings.seed(Some(n));
                }
            }
            "--verbosity" => {
                let value = next_value(&args, &mut i, "--verbosity")?;
                let v = parse_verbosity(&value)?;
                settings = settings.verbosity(v);
            }
            "--derandomize" => {
                let value = next_value(&args, &mut i, "--derandomize")?;
                let b = parse_bool(&value, "--derandomize")?;
                settings = settings.derandomize(b);
            }
            "--database" => {
                let value = next_value(&args, &mut i, "--database")?;
                if value == "disabled" || value == "none" {
                    settings = settings.database(None);
                } else {
                    settings = settings.database(Some(value));
                }
            }
            "--suppress-health-check" => {
                let value = next_value(&args, &mut i, "--suppress-health-check")?;
                let checks = parse_health_check(&value)?;
                settings = settings.suppress_health_check(checks);
            }
            "--single-test-case" => {
                settings = settings.mode(Mode::SingleTestCase);
            }
            _ => {
                return Err(CliError::Parse(format!("Unknown argument: {arg}")));
            }
        }
        i += 1;
    }
    Ok(settings)
}

fn next_value(args: &[String], i: &mut usize, name: &str) -> Result<String, CliError> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| CliError::Parse(format!("{name} requires a value")))
}

fn parse_verbosity(s: &str) -> Result<Verbosity, CliError> {
    match s {
        "quiet" => Ok(Verbosity::Quiet),
        "normal" => Ok(Verbosity::Normal),
        "verbose" => Ok(Verbosity::Verbose),
        "debug" => Ok(Verbosity::Debug),
        other => Err(CliError::Parse(format!(
            "--verbosity expects one of quiet|normal|verbose|debug, got {other:?}"
        ))),
    }
}

fn parse_bool(s: &str, name: &str) -> Result<bool, CliError> {
    match s {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(CliError::Parse(format!(
            "{name} expects true|false, got {other:?}"
        ))),
    }
}

fn parse_health_check(s: &str) -> Result<Vec<HealthCheck>, CliError> {
    if s == "all" {
        return Ok(HealthCheck::all().to_vec());
    }
    let mut checks = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let hc = match part {
            "filter_too_much" => HealthCheck::FilterTooMuch,
            "too_slow" => HealthCheck::TooSlow,
            "test_cases_too_large" => HealthCheck::TestCasesTooLarge,
            "large_initial_test_case" => HealthCheck::LargeInitialTestCase,
            other => {
                return Err(CliError::Parse(format!(
                    "--suppress-health-check does not recognise {other:?}. \
                     Known names: all, filter_too_much, too_slow, test_cases_too_large, large_initial_test_case"
                )));
            }
        };
        checks.push(hc);
    }
    Ok(checks)
}

fn usage() -> String {
    let mut s = String::new();
    s.push_str("Usage: <program> [options]\n");
    s.push('\n');
    s.push_str("Hegel property-based testing binary.\n");
    s.push('\n');
    s.push_str("Options:\n");
    s.push_str("  --test-cases <N>                     Number of test cases to run\n");
    s.push_str(
        "  --seed <N|none>                      Seed for randomisation ('none' for unset)\n",
    );
    s.push_str("  --verbosity <LEVEL>                  quiet | normal | verbose | debug\n");
    s.push_str("  --derandomize <true|false>           Use a deterministic derived seed\n");
    s.push_str(
        "  --database <PATH|disabled>           Database path for failing-example storage\n",
    );
    s.push_str(
        "  --suppress-health-check <NAMES>      Comma-separated health check names, or 'all'\n",
    );
    s.push_str(
        "  --single-test-case                   Run one test case, no shrinking or replay\n",
    );
    s.push_str("  -h, --help                           Show this help and exit\n");
    s
}

#[cfg(test)]
#[path = "../tests/embedded/cli_tests.rs"]
mod tests;
