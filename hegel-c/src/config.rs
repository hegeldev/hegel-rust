//! `hegel.toml` discovery and parsing.
//!
//! The config file defines and modifies settings profiles
//! ([`crate::profiles`]). The accepted format is a strict subset of TOML:
//! `[profiles.<name>]` tables whose entries are basic strings, decimal
//! integers, booleans, or single-line arrays of basic strings, plus `#`
//! comments. Everything else — unknown keys, wrong value types, other
//! tables, multi-line values — is a hard error carrying a line number:
//! silent misconfiguration in a file that changes test behaviour is worse
//! than strictness.
//!
//! The file is discovered by checking the current directory and then each
//! ancestor up to the filesystem root, first hit wins — the same shape as
//! cargo's config discovery, so a `hegel.toml` at either the package or the
//! workspace root is found from wherever the test process runs.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::profiles::{ProfileDelta, ProfileError, is_valid_name};
use crate::settings::{Backend, Database, HealthCheck, Phase, Verbosity};

/// The config file's name, looked for in the current directory and every
/// ancestor.
pub(crate) const FILE_NAME: &str = "hegel.toml";

/// Parsed contents of a `hegel.toml`: the profile deltas it defines, in
/// file order.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ConfigFile {
    pub(crate) profiles: Vec<(String, ProfileDelta)>,
}

/// A parse failure at a 1-based line of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub(crate) line: usize,
    pub(crate) message: String,
}

fn err(line: usize, message: impl Into<String>) -> ParseError {
    ParseError {
        line,
        message: message.into(),
    }
}

/// One parsed value: the only shapes the format accepts.
enum Value {
    Str(String),
    Int(i128),
    Bool(bool),
    Array(Vec<String>),
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Str(_) => "a string",
            Value::Int(_) => "an integer",
            Value::Bool(_) => "a boolean",
            Value::Array(_) => "an array",
        }
    }
}

/// Parse the text of a `hegel.toml`.
pub(crate) fn parse(text: &str) -> Result<ConfigFile, ParseError> {
    let mut out = ConfigFile::default();
    let mut keys: Vec<String> = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = parse_header(rest, line_no)?;
            if out.profiles.iter().any(|(n, _)| n == name) {
                return Err(err(line_no, format!("duplicate section [profiles.{name}]")));
            }
            out.profiles
                .push((name.to_string(), ProfileDelta::default()));
            keys.clear();
            continue;
        }
        let Some(eq) = line.find('=') else {
            return Err(err(
                line_no,
                "expected `key = value` or a [profiles.<name>] header",
            ));
        };
        let Some((_, delta)) = out.profiles.last_mut() else {
            return Err(err(line_no, "entry before any [profiles.<name>] header"));
        };
        let key = line[..eq].trim();
        if keys.iter().any(|k| k == key) {
            return Err(err(line_no, format!("duplicate key `{key}`")));
        }
        let value = parse_value(line[eq + 1..].trim(), line_no)?;
        assign(delta, key, value, line_no)?;
        keys.push(key.to_string());
    }
    Ok(out)
}

/// Parse a table header after its opening `[`, returning the profile name.
fn parse_header(rest: &str, line_no: usize) -> Result<&str, ParseError> {
    let Some(rest) = rest.strip_prefix("profiles.") else {
        return Err(err(line_no, "only [profiles.<name>] tables are allowed"));
    };
    let Some(end) = rest.find(']') else {
        return Err(err(line_no, "unterminated table header"));
    };
    let name = &rest[..end];
    let after = rest[end + 1..].trim();
    if !after.is_empty() && !after.starts_with('#') {
        return Err(err(line_no, "unexpected text after table header"));
    }
    if !is_valid_name(name) {
        return Err(err(
            line_no,
            format!("invalid profile name {name:?}: use only ASCII letters, digits, '-' and '_'"),
        ));
    }
    Ok(name)
}

/// Parse the value part of a `key = value` line, allowing a trailing
/// comment.
fn parse_value(s: &str, line_no: usize) -> Result<Value, ParseError> {
    let (value, rest) = scan_value(s, line_no)?;
    let rest = rest.trim_start();
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err(err(line_no, format!("unexpected trailing text `{rest}`")));
    }
    Ok(value)
}

fn scan_value(s: &str, line_no: usize) -> Result<(Value, &str), ParseError> {
    if let Some(rest) = s.strip_prefix('"') {
        let (string, rest) = scan_string(rest, line_no)?;
        return Ok((Value::Str(string), rest));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (items, rest) = scan_array(rest, line_no)?;
        return Ok((Value::Array(items), rest));
    }
    let end = s
        .find(|c: char| c.is_whitespace() || c == '#')
        .unwrap_or(s.len());
    let (token, rest) = s.split_at(end);
    match token {
        "true" => Ok((Value::Bool(true), rest)),
        "false" => Ok((Value::Bool(false), rest)),
        _ => match token.parse::<i128>() {
            Ok(n) => Ok((Value::Int(n), rest)),
            Err(_) => Err(err(
                line_no,
                format!("expected a string, integer, boolean, or array, got `{token}`"),
            )),
        },
    }
}

/// Scan a basic string after its opening quote. Supports the escapes
/// `\"`, `\\`, `\n`, and `\t` only.
fn scan_string(s: &str, line_no: usize) -> Result<(String, &str), ParseError> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, other)) => {
                    return Err(err(line_no, format!("unsupported escape `\\{other}`")));
                }
                None => break,
            },
            other => out.push(other),
        }
    }
    Err(err(line_no, "unterminated string"))
}

/// Scan an array after its opening bracket. Elements must be basic strings.
fn scan_array(s: &str, line_no: usize) -> Result<(Vec<String>, &str), ParseError> {
    let mut items = Vec::new();
    let mut rest = s.trim_start();
    if let Some(r) = rest.strip_prefix(']') {
        return Ok((items, r));
    }
    loop {
        let Some(r) = rest.strip_prefix('"') else {
            return Err(err(line_no, "arrays may contain only strings"));
        };
        let (item, r) = scan_string(r, line_no)?;
        items.push(item);
        rest = r.trim_start();
        if let Some(r) = rest.strip_prefix(',') {
            rest = r.trim_start();
            continue;
        }
        if let Some(r) = rest.strip_prefix(']') {
            return Ok((items, r));
        }
        return Err(err(line_no, "expected `,` or `]` in array"));
    }
}

fn expect_string(value: Value, key: &str, line_no: usize) -> Result<String, ParseError> {
    match value {
        Value::Str(s) => Ok(s),
        other => Err(type_error(key, "a string", &other, line_no)),
    }
}

fn expect_bool(value: Value, key: &str, line_no: usize) -> Result<bool, ParseError> {
    match value {
        Value::Bool(b) => Ok(b),
        other => Err(type_error(key, "a boolean", &other, line_no)),
    }
}

fn expect_array(value: Value, key: &str, line_no: usize) -> Result<Vec<String>, ParseError> {
    match value {
        Value::Array(items) => Ok(items),
        other => Err(type_error(key, "an array of strings", &other, line_no)),
    }
}

fn expect_int(
    value: Value,
    key: &str,
    line_no: usize,
    min: i128,
    max: i128,
) -> Result<i128, ParseError> {
    let n = match value {
        Value::Int(n) => n,
        other => return Err(type_error(key, "an integer", &other, line_no)),
    };
    if n < min || n > max {
        return Err(err(
            line_no,
            format!("`{key}` must be between {min} and {max}, got {n}"),
        ));
    }
    Ok(n)
}

fn type_error(key: &str, expected: &str, got: &Value, line_no: usize) -> ParseError {
    err(
        line_no,
        format!("`{key}` expects {expected}, got {}", got.kind()),
    )
}

/// Set the field `key` names on `delta`, validating the value's type and
/// vocabulary.
fn assign(
    delta: &mut ProfileDelta,
    key: &str,
    value: Value,
    line_no: usize,
) -> Result<(), ParseError> {
    match key {
        "extends" => {
            let name = expect_string(value, key, line_no)?;
            if !is_valid_name(&name) {
                return Err(err(
                    line_no,
                    format!(
                        "invalid profile name {name:?}: use only ASCII letters, digits, '-' and '_'"
                    ),
                ));
            }
            delta.extends = Some(name);
        }
        "test_cases" => {
            delta.test_cases = Some(expect_int(value, key, line_no, 1, u64::MAX as i128)? as u64);
        }
        "seed" => {
            delta.seed = Some(expect_int(value, key, line_no, 0, u64::MAX as i128)? as u64);
        }
        "derandomize" => delta.derandomize = Some(expect_bool(value, key, line_no)?),
        "report_multiple_failures" => {
            delta.report_multiple_failures = Some(expect_bool(value, key, line_no)?);
        }
        "show_statistics" => delta.show_statistics = Some(expect_bool(value, key, line_no)?),
        "print_blob" => delta.print_blob = Some(expect_bool(value, key, line_no)?),
        "verbosity" => {
            let s = expect_string(value, key, line_no)?;
            delta.verbosity = Some(match s.as_str() {
                "quiet" => Verbosity::Quiet,
                "normal" => Verbosity::Normal,
                "verbose" => Verbosity::Verbose,
                "debug" => Verbosity::Debug,
                other => {
                    return Err(err(
                        line_no,
                        format!(
                            "`verbosity` expects one of quiet|normal|verbose|debug, got {other:?}"
                        ),
                    ));
                }
            });
        }
        "backend" => {
            let s = expect_string(value, key, line_no)?;
            delta.backend = Some(match s.as_str() {
                "auto" => None,
                "default" => Some(Backend::Default),
                "urandom" => Some(Backend::Urandom),
                other => {
                    return Err(err(
                        line_no,
                        format!("`backend` expects one of auto|default|urandom, got {other:?}"),
                    ));
                }
            });
        }
        "database" => {
            let s = expect_string(value, key, line_no)?;
            if s.is_empty() {
                return Err(err(
                    line_no,
                    "`database` expects a path or \"disabled\", got \"\"",
                ));
            }
            delta.database = Some(if s == "disabled" {
                Database::Disabled
            } else {
                Database::Path(s)
            });
        }
        "suppress_health_check" => {
            let items = expect_array(value, key, line_no)?;
            delta.suppress_health_check = Some(parse_health_checks(&items, line_no)?);
        }
        "phases" => {
            let items = expect_array(value, key, line_no)?;
            let mut phases = Vec::with_capacity(items.len());
            for item in &items {
                phases.push(match item.as_str() {
                    "explicit" => Phase::Explicit,
                    "reuse" => Phase::Reuse,
                    "generate" => Phase::Generate,
                    "target" => Phase::Target,
                    "shrink" => Phase::Shrink,
                    other => {
                        return Err(err(
                            line_no,
                            format!(
                                "`phases` does not recognise {other:?}. \
                                 Known names: explicit, reuse, generate, target, shrink"
                            ),
                        ));
                    }
                });
            }
            delta.phases = Some(phases);
        }
        other => return Err(err(line_no, format!("unknown key `{other}`"))),
    }
    Ok(())
}

/// The health-check name vocabulary, shared with the frontend's
/// `--suppress-health-check` flag: the four snake_case check names, or
/// `"all"` as the only element.
fn parse_health_checks(items: &[String], line_no: usize) -> Result<Vec<HealthCheck>, ParseError> {
    if items.iter().any(|i| i == "all") {
        if items.len() != 1 {
            return Err(err(
                line_no,
                "\"all\" must be the only element of `suppress_health_check`",
            ));
        }
        return Ok(alloc::vec![
            HealthCheck::FilterTooMuch,
            HealthCheck::TooSlow,
            HealthCheck::TestCasesTooLarge,
            HealthCheck::LargeInitialTestCase,
        ]);
    }
    let mut checks = Vec::with_capacity(items.len());
    for item in items {
        checks.push(match item.as_str() {
            "filter_too_much" => HealthCheck::FilterTooMuch,
            "too_slow" => HealthCheck::TooSlow,
            "test_cases_too_large" => HealthCheck::TestCasesTooLarge,
            "large_initial_test_case" => HealthCheck::LargeInitialTestCase,
            other => {
                return Err(err(
                    line_no,
                    format!(
                        "`suppress_health_check` does not recognise {other:?}. \
                         Known names: all, filter_too_much, too_slow, \
                         test_cases_too_large, large_initial_test_case"
                    ),
                ));
            }
        });
    }
    Ok(checks)
}

fn is_separator(byte: u8) -> bool {
    byte == b'/' || (cfg!(windows) && byte == b'\\')
}

/// The parent directory of `dir`, or `None` at a filesystem root or for a
/// single relative component. Windows drive roots keep their separator
/// (`C:\foo` → `C:\`), since a bare `C:` is drive-relative rather than the
/// root.
fn parent(dir: &str) -> Option<&str> {
    parent_on(dir, cfg!(windows))
}

fn parent_on(dir: &str, windows: bool) -> Option<&str> {
    let bytes = dir.as_bytes();
    let mut end = bytes.len();
    while end > 0 && is_separator(bytes[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let last_sep = dir[..end].bytes().rposition(is_separator)?;
    if last_sep == 0 {
        return Some(&dir[..1]);
    }
    let candidate = &dir[..last_sep];
    if windows && candidate.ends_with(':') {
        return Some(&dir[..last_sep + 1]);
    }
    Some(candidate)
}

fn join(dir: &str, name: &str) -> String {
    let mut path = String::with_capacity(dir.len() + name.len() + 1);
    path.push_str(dir);
    if !dir.as_bytes().last().copied().is_some_and(is_separator) {
        path.push('/');
    }
    path.push_str(name);
    path
}

/// Locate `hegel.toml`: the current directory first, then each ancestor up
/// to the filesystem root, first hit wins. Returns the path and raw bytes.
/// With no current directory, only the relative `hegel.toml` is checked.
/// A file that exists but cannot be read is skipped, per the sys philosophy
/// of degrading silently on OS failure.
pub(crate) fn discover(
    cwd: Option<String>,
    exists: impl Fn(&str) -> bool,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Option<(String, Vec<u8>)> {
    let check = |path: String| {
        if exists(&path) {
            read(&path).map(|bytes| (path, bytes))
        } else {
            None
        }
    };
    let Some(mut dir) = cwd else {
        return check(FILE_NAME.to_string());
    };
    loop {
        if let Some(found) = check(join(&dir, FILE_NAME)) {
            return Some(found);
        }
        match parent(&dir) {
            Some(p) if p != dir => dir = p.to_string(),
            _ => return None,
        }
    }
}

/// Discover and parse `hegel.toml`. No file found is `Ok` with an empty
/// config.
pub(crate) fn load() -> Result<ConfigFile, ProfileError> {
    load_from(crate::sys::cwd(), crate::sys::fs::exists, |path| {
        crate::sys::fs::read(path).ok()
    })
}

/// [`load`] with the directory and filesystem reads injected.
pub(crate) fn load_from(
    cwd: Option<String>,
    exists: impl Fn(&str) -> bool,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<ConfigFile, ProfileError> {
    let Some((path, bytes)) = discover(cwd, exists, read) else {
        return Ok(ConfigFile::default());
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(ProfileError::Config {
            path,
            line: 0,
            message: "file is not valid UTF-8".to_string(),
        });
    };
    parse(&text).map_err(|e| ProfileError::Config {
        path,
        line: e.line,
        message: e.message,
    })
}

#[cfg(test)]
#[path = "../tests/embedded/config_tests.rs"]
mod tests;
