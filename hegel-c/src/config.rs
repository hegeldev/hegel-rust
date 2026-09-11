//! `hegel.toml` discovery and parsing.
//!
//! The config file defines and modifies settings profiles
//! ([`crate::profiles`]). It is a TOML document with an optional top-level
//! `default = "<profile>"` entry naming the default profile, and
//! `[profiles.<name>]` tables whose entries are the settings keys. The
//! vocabulary is strict: an unknown key, a value of the wrong type, a
//! misplaced `default`, or any other top-level table is a hard error
//! carrying a line number, because silent misconfiguration in a file that
//! changes test behaviour is worse than strictness.
//!
//! The file is discovered by checking the current directory and then each
//! ancestor up to the filesystem root, first hit wins — the same shape as
//! cargo's config discovery, so a `hegel.toml` at either the package or the
//! workspace root is found from wherever the test process runs. Setting
//! `HEGEL_CONFIG` to a path bypasses discovery entirely, for environments
//! that relocate the test process outside the source tree.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use toml::Spanned;
use toml::de::{DeString, DeTable, DeValue};

use crate::profiles::{BASE, DEFAULT, ProfileDelta, ProfileError, is_valid_name};
use crate::settings::{Backend, Database, HealthCheck, Phase, Verbosity};

/// The config file's name, looked for in the current directory and every
/// ancestor.
pub(crate) const FILE_NAME: &str = "hegel.toml";

/// When set and non-empty, the path of the config file to load, replacing
/// discovery.
pub(crate) const CONFIG_VAR: &str = "HEGEL_CONFIG";

/// Parsed contents of a `hegel.toml`: the default-profile entry, the
/// profile deltas it defines in file order, and the path it was loaded from
/// (`None` for a config that was parsed rather than loaded, or the empty
/// default).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ConfigFile {
    pub(crate) default: Option<String>,
    pub(crate) profiles: Vec<(String, ProfileDelta)>,
    pub(crate) path: Option<String>,
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

/// The parsed document, for turning the byte offsets of the spans the TOML
/// parser records into line numbers.
struct Source<'t> {
    text: &'t str,
}

impl Source<'_> {
    fn line_at(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .matches('\n')
            .count()
            + 1
    }

    fn line_of<T>(&self, spanned: &Spanned<T>) -> usize {
        self.line_at(spanned.span().start)
    }

    fn err_at<T>(&self, spanned: &Spanned<T>, message: impl Into<String>) -> ParseError {
        err(self.line_of(spanned), message)
    }
}

/// Parse the text of a `hegel.toml`.
pub(crate) fn parse(text: &str) -> Result<ConfigFile, ParseError> {
    let src = Source { text };
    let table = DeTable::parse(text).map_err(|e| {
        err(
            src.line_at(e.span().map_or(0, |span| span.start)),
            e.message(),
        )
    })?;
    let mut out = ConfigFile::default();
    for (key, value) in in_file_order(table.get_ref()) {
        match key.get_ref().as_ref() {
            "default" => {
                let name = expect_string(&src, value, "default")?;
                if name == DEFAULT {
                    return Err(src.err_at(
                        value,
                        format!("`default` cannot name the {DEFAULT:?} alias it resolves"),
                    ));
                }
                if !is_valid_name(name) {
                    return Err(invalid_name_err(name, src.line_of(value)));
                }
                out.default = Some(name.to_string());
            }
            "profiles" => {
                let DeValue::Table(profiles) = value.get_ref() else {
                    return Err(type_error(
                        &src,
                        value,
                        "profiles",
                        "[profiles.<name>] tables",
                    ));
                };
                for (name, delta) in in_file_order(profiles) {
                    out.profiles.push(parse_profile(&src, name, delta)?);
                }
            }
            other => {
                return Err(src.err_at(
                    key,
                    format!(
                        "unknown top-level key `{other}`: only `default` and \
                         [profiles.<name>] tables are allowed"
                    ),
                ));
            }
        }
    }
    Ok(out)
}

/// A table's entries sorted by where they appear in the file, since the
/// parsed table orders them by key.
fn in_file_order<'a, 'i>(
    table: &'a DeTable<'i>,
) -> Vec<(&'a Spanned<DeString<'i>>, &'a Spanned<DeValue<'i>>)> {
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_by_key(|(key, _)| key.span().start);
    entries
}

/// Parse one `[profiles.<name>]` table into a named delta, validating the
/// name.
fn parse_profile(
    src: &Source<'_>,
    name: &Spanned<DeString<'_>>,
    table: &Spanned<DeValue<'_>>,
) -> Result<(String, ProfileDelta), ParseError> {
    let line = src.line_of(name);
    let name = name.get_ref().as_ref();
    if name == BASE {
        return Err(err(
            line,
            format!(
                "{BASE:?} is the reserved base profile and cannot be modified; \
                 customize [profiles.development] instead"
            ),
        ));
    }
    if name == DEFAULT {
        return Err(err(
            line,
            format!(
                "{DEFAULT:?} is an alias for the default profile and cannot be defined; \
                 choose it with `default = \"<profile>\"` instead"
            ),
        ));
    }
    if !is_valid_name(name) {
        return Err(invalid_name_err(name, line));
    }
    let DeValue::Table(entries) = table.get_ref() else {
        return Err(type_error(
            src,
            table,
            &format!("profiles.{name}"),
            "a table of settings",
        ));
    };
    let mut delta = ProfileDelta::default();
    for (key, value) in in_file_order(entries) {
        assign(src, &mut delta, key, value)?;
    }
    Ok((name.to_string(), delta))
}

fn invalid_name_err(name: &str, line_no: usize) -> ParseError {
    err(
        line_no,
        format!("invalid profile name {name:?}: use only ASCII letters, digits, '-' and '_'"),
    )
}

fn kind(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "a string",
        DeValue::Integer(_) => "an integer",
        DeValue::Float(_) => "a float",
        DeValue::Boolean(_) => "a boolean",
        DeValue::Datetime(_) => "a datetime",
        DeValue::Array(_) => "an array",
        DeValue::Table(_) => "a table",
    }
}

fn type_error(
    src: &Source<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
    expected: &str,
) -> ParseError {
    src.err_at(
        value,
        format!("`{key}` expects {expected}, got {}", kind(value.get_ref())),
    )
}

fn expect_string<'a>(
    src: &Source<'_>,
    value: &'a Spanned<DeValue<'_>>,
    key: &str,
) -> Result<&'a str, ParseError> {
    match value.get_ref() {
        DeValue::String(s) => Ok(s.as_ref()),
        _ => Err(type_error(src, value, key, "a string")),
    }
}

fn expect_bool(
    src: &Source<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
) -> Result<bool, ParseError> {
    match value.get_ref() {
        DeValue::Boolean(b) => Ok(*b),
        _ => Err(type_error(src, value, key, "a boolean")),
    }
}

fn expect_array<'a>(
    src: &Source<'_>,
    value: &'a Spanned<DeValue<'_>>,
    key: &str,
) -> Result<Vec<&'a str>, ParseError> {
    let DeValue::Array(items) = value.get_ref() else {
        return Err(type_error(src, value, key, "an array of strings"));
    };
    items
        .iter()
        .map(|item| match item.get_ref() {
            DeValue::String(s) => Ok(s.as_ref()),
            other => Err(src.err_at(
                item,
                format!("elements of `{key}` must be strings, got {}", kind(other)),
            )),
        })
        .collect()
}

fn expect_int(
    src: &Source<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
    min: i128,
    max: i128,
) -> Result<i128, ParseError> {
    let DeValue::Integer(n) = value.get_ref() else {
        return Err(type_error(src, value, key, "an integer"));
    };
    match i128::from_str_radix(n.as_str(), n.radix()) {
        Ok(v) if (min..=max).contains(&v) => Ok(v),
        _ => Err(src.err_at(
            value,
            format!("`{key}` must be between {min} and {max}, got {n}"),
        )),
    }
}

/// Set the field `key` names on `delta`, validating the value's type and
/// vocabulary.
fn assign(
    src: &Source<'_>,
    delta: &mut ProfileDelta,
    key: &Spanned<DeString<'_>>,
    value: &Spanned<DeValue<'_>>,
) -> Result<(), ParseError> {
    let line_no = src.line_of(value);
    let key = key.get_ref().as_ref();
    match key {
        "default" => {
            return Err(err(
                line_no,
                "`default` is a top-level key, not a profile setting: \
                 move it above the first [profiles.<name>] header",
            ));
        }
        "extends" => {
            let name = expect_string(src, value, key)?;
            if !is_valid_name(name) {
                return Err(invalid_name_err(name, line_no));
            }
            delta.extends = Some(name.to_string());
        }
        "test_cases" => {
            delta.test_cases = Some(expect_int(src, value, key, 1, u64::MAX as i128)? as u64);
        }
        "seed" => {
            delta.seed = Some(match value.get_ref() {
                DeValue::String(s) if s == "none" => None,
                DeValue::String(s) => {
                    return Err(err(
                        line_no,
                        format!("`seed` expects an integer or \"none\", got {s:?}"),
                    ));
                }
                _ => Some(expect_int(src, value, key, 0, u64::MAX as i128)? as u64),
            });
        }
        "derandomize" => delta.derandomize = Some(expect_bool(src, value, key)?),
        "report_multiple_failures" => {
            delta.report_multiple_failures = Some(expect_bool(src, value, key)?);
        }
        "show_statistics" => delta.show_statistics = Some(expect_bool(src, value, key)?),
        "print_blob" => delta.print_blob = Some(expect_bool(src, value, key)?),
        "verbosity" => {
            let s = expect_string(src, value, key)?;
            delta.verbosity = Some(match s {
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
            let s = expect_string(src, value, key)?;
            delta.backend = Some(match s {
                "default" => Backend::Default,
                "urandom" => Backend::Urandom,
                other => {
                    return Err(err(
                        line_no,
                        format!("`backend` expects one of default|urandom, got {other:?}"),
                    ));
                }
            });
        }
        "database" => {
            let s = expect_string(src, value, key)?;
            if s.is_empty() {
                return Err(err(
                    line_no,
                    "`database` expects a path, \"disabled\", or \"default\", got \"\"",
                ));
            }
            delta.database = Some(match s {
                "disabled" => Database::Disabled,
                "default" => Database::Unset,
                _ => Database::Path(s.to_string()),
            });
        }
        "suppress_health_check" => {
            let items = expect_array(src, value, key)?;
            delta.suppress_health_check = Some(parse_health_checks(&items, line_no)?);
        }
        "phases" => {
            let items = expect_array(src, value, key)?;
            let mut phases = Vec::with_capacity(items.len());
            for item in items {
                phases.push(match item {
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
fn parse_health_checks(items: &[&str], line_no: usize) -> Result<Vec<HealthCheck>, ParseError> {
    if items.contains(&"all") {
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
        checks.push(match *item {
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
/// config. A set, non-empty `HEGEL_CONFIG` names the file directly instead
/// of discovering one, and a file it names that cannot be read is a hard
/// error: the variable exists to guarantee a config is loaded, so failing
/// to load it must be loud.
///
/// The result is loaded once and cached for the life of the process, so
/// every settings resolution sees the same config even if the file changes
/// mid-run.
///
/// Under Miri the result is always an empty config: Miri has no shim for
/// the `stat` call the discovery makes.
pub(crate) fn load() -> Result<ConfigFile, ProfileError> {
    #[cfg(miri)]
    return Ok(ConfigFile::default());
    #[cfg(not(miri))]
    {
        static LOADED: crate::sys::sync::Lazy<Result<ConfigFile, ProfileError>> =
            crate::sys::sync::Lazy::new(|| {
                load_from(
                    crate::sys::env_var(CONFIG_VAR),
                    crate::sys::cwd(),
                    crate::sys::fs::exists,
                    |path| crate::sys::fs::read(path).ok(),
                )
            });
        LOADED.clone()
    }
}

/// [`load`] with the environment, directory, and filesystem reads injected.
pub(crate) fn load_from(
    config_var: Option<String>,
    cwd: Option<String>,
    exists: impl Fn(&str) -> bool,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<ConfigFile, ProfileError> {
    if let Some(path) = config_var.filter(|p| !p.is_empty()) {
        let Some(bytes) = read(&path) else {
            return Err(ProfileError::Config {
                path,
                line: 0,
                message: format!("cannot read the file named by {CONFIG_VAR}"),
            });
        };
        return parse_bytes(path, bytes);
    }
    let Some((path, bytes)) = discover(cwd, exists, read) else {
        return Ok(ConfigFile::default());
    };
    parse_bytes(path, bytes)
}

fn parse_bytes(path: String, bytes: Vec<u8>) -> Result<ConfigFile, ProfileError> {
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(ProfileError::Config {
            path,
            line: 0,
            message: "file is not valid UTF-8".to_string(),
        });
    };
    match parse(&text) {
        Ok(mut config) => {
            config.path = Some(path);
            Ok(config)
        }
        Err(e) => Err(ProfileError::Config {
            path,
            line: e.line,
            message: e.message,
        }),
    }
}

#[cfg(test)]
#[path = "../tests/embedded/config_tests.rs"]
mod tests;
