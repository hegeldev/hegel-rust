use super::*;
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::vec;

fn parse_err(text: &str) -> ParseError {
    parse(text).unwrap_err()
}

fn parse_one(text: &str) -> ProfileDelta {
    let config = parse(text).unwrap();
    assert_eq!(config.profiles.len(), 1);
    config.profiles.into_iter().next().unwrap().1
}

#[test]
fn parses_an_empty_file() {
    assert_eq!(parse(""), Ok(ConfigFile::default()));
    assert_eq!(parse("\n\n# just a comment\n"), Ok(ConfigFile::default()));
}

#[test]
fn parses_every_key() {
    let delta = parse_one(
        r#"
        [profiles.nightly]
        extends = "ci"
        test_cases = 10000
        seed = 42
        derandomize = true
        report_multiple_failures = true
        show_statistics = true
        print_blob = false
        verbosity = "verbose"
        backend = "default"
        database = "my/db"
        suppress_health_check = ["too_slow", "filter_too_much"]
        phases = ["reuse", "generate", "shrink"]
        "#,
    );
    assert_eq!(
        delta,
        ProfileDelta {
            extends: Some("ci".to_owned()),
            test_cases: Some(10000),
            verbosity: Some(Verbosity::Verbose),
            seed: Some(42),
            derandomize: Some(true),
            database: Some(Database::Path("my/db".to_owned())),
            suppress_health_check: Some(vec![HealthCheck::TooSlow, HealthCheck::FilterTooMuch]),
            phases: Some(vec![Phase::Reuse, Phase::Generate, Phase::Shrink]),
            report_multiple_failures: Some(true),
            show_statistics: Some(true),
            print_blob: Some(false),
            backend: Some(Some(Backend::Default)),
        }
    );
}

#[test]
fn parses_multiple_sections_in_order() {
    let config = parse("[profiles.a]\ntest_cases = 1\n[profiles.b]\ntest_cases = 2\n").unwrap();
    assert_eq!(config.profiles[0].0, "a");
    assert_eq!(config.profiles[0].1.test_cases, Some(1));
    assert_eq!(config.profiles[1].0, "b");
    assert_eq!(config.profiles[1].1.test_cases, Some(2));
}

#[test]
fn parses_verbosity_and_backend_vocabularies() {
    for (name, expected) in [
        ("quiet", Verbosity::Quiet),
        ("normal", Verbosity::Normal),
        ("verbose", Verbosity::Verbose),
        ("debug", Verbosity::Debug),
    ] {
        let text = format!("[profiles.x]\nverbosity = \"{name}\"\n");
        assert_eq!(parse_one(&text).verbosity, Some(expected));
    }
    for (name, expected) in [
        ("auto", None),
        ("default", Some(Backend::Default)),
        ("urandom", Some(Backend::Urandom)),
    ] {
        let text = format!("[profiles.x]\nbackend = \"{name}\"\n");
        assert_eq!(parse_one(&text).backend, Some(expected));
    }
}

#[test]
fn parses_database_disabled_keyword() {
    let delta = parse_one("[profiles.x]\ndatabase = \"disabled\"\n");
    assert_eq!(delta.database, Some(Database::Disabled));
}

#[test]
fn parses_all_health_checks_keyword() {
    let delta = parse_one("[profiles.x]\nsuppress_health_check = [\"all\"]\n");
    assert_eq!(delta.suppress_health_check.unwrap().len(), 4);
}

#[test]
fn parses_every_phase_name() {
    let delta = parse_one(
        "[profiles.x]\nphases = [\"explicit\", \"reuse\", \"generate\", \"target\", \"shrink\"]\n",
    );
    assert_eq!(delta.phases.unwrap().len(), 5);
}

#[test]
fn parses_remaining_health_check_names() {
    let delta = parse_one(
        "[profiles.x]\nsuppress_health_check = [\"test_cases_too_large\", \"large_initial_test_case\"]\n",
    );
    assert_eq!(
        delta.suppress_health_check,
        Some(vec![
            HealthCheck::TestCasesTooLarge,
            HealthCheck::LargeInitialTestCase
        ])
    );
}

#[test]
fn parses_empty_arrays() {
    let delta = parse_one("[profiles.x]\nphases = []\nsuppress_health_check = []\n");
    assert_eq!(delta.phases, Some(vec![]));
    assert_eq!(delta.suppress_health_check, Some(vec![]));
}

#[test]
fn parses_comments_everywhere() {
    let delta = parse_one(
        "# leading\n[profiles.x] # header comment\ntest_cases = 5 # value comment\n# trailing\n",
    );
    assert_eq!(delta.test_cases, Some(5));
}

#[test]
fn parses_string_escapes() {
    let delta = parse_one("[profiles.x]\ndatabase = \"a\\\\b\\\"c\\nd\\te\"\n");
    assert_eq!(
        delta.database,
        Some(Database::Path("a\\b\"c\nd\te".to_owned()))
    );
}

#[test]
fn rejects_non_profile_tables() {
    assert_eq!(
        parse_err("[foo]\n").message,
        "only [profiles.<name>] tables are allowed"
    );
    assert_eq!(
        parse_err("[profiles]\n").message,
        "only [profiles.<name>] tables are allowed"
    );
}

#[test]
fn rejects_unterminated_table_header() {
    assert_eq!(
        parse_err("[profiles.x\n").message,
        "unterminated table header"
    );
}

#[test]
fn rejects_text_after_table_header() {
    assert_eq!(
        parse_err("[profiles.x] junk\n").message,
        "unexpected text after table header"
    );
}

#[test]
fn rejects_invalid_profile_names() {
    for text in ["[profiles.]\n", "[profiles.bad name]\n", "[profiles.a.b]\n"] {
        assert!(parse_err(text).message.starts_with("invalid profile name"));
    }
}

#[test]
fn rejects_duplicate_sections() {
    let e = parse_err("[profiles.x]\n[profiles.x]\n");
    assert_eq!(e.line, 2);
    assert_eq!(e.message, "duplicate section [profiles.x]");
}

#[test]
fn rejects_lines_that_are_neither_headers_nor_entries() {
    assert_eq!(
        parse_err("[profiles.x]\nwhat is this\n").message,
        "expected `key = value` or a [profiles.<name>] header"
    );
}

#[test]
fn rejects_entries_before_any_section() {
    assert_eq!(
        parse_err("test_cases = 5\n").message,
        "entry before any [profiles.<name>] header"
    );
}

#[test]
fn rejects_duplicate_keys_within_a_section() {
    let e = parse_err("[profiles.x]\ntest_cases = 1\ntest_cases = 2\n");
    assert_eq!(e.line, 3);
    assert_eq!(e.message, "duplicate key `test_cases`");
}

#[test]
fn allows_the_same_key_in_different_sections() {
    let config = parse("[profiles.a]\ntest_cases = 1\n[profiles.b]\ntest_cases = 2\n").unwrap();
    assert_eq!(config.profiles.len(), 2);
}

#[test]
fn rejects_trailing_text_after_values() {
    assert_eq!(
        parse_err("[profiles.x]\ntest_cases = 5 junk\n").message,
        "unexpected trailing text `junk`"
    );
}

#[test]
fn rejects_unrecognised_bare_tokens() {
    assert_eq!(
        parse_err("[profiles.x]\nderandomize = yes\n").message,
        "expected a string, integer, boolean, or array, got `yes`"
    );
    assert_eq!(
        parse_err("[profiles.x]\ntest_cases =\n").message,
        "expected a string, integer, boolean, or array, got ``"
    );
}

#[test]
fn rejects_unsupported_escapes() {
    assert_eq!(
        parse_err("[profiles.x]\ndatabase = \"a\\qb\"\n").message,
        "unsupported escape `\\q`"
    );
}

#[test]
fn rejects_unterminated_strings() {
    assert_eq!(
        parse_err("[profiles.x]\ndatabase = \"abc\n").message,
        "unterminated string"
    );
    assert_eq!(
        parse_err("[profiles.x]\ndatabase = \"abc\\\n").message,
        "unterminated string"
    );
}

#[test]
fn rejects_non_string_array_elements() {
    assert_eq!(
        parse_err("[profiles.x]\nphases = [1]\n").message,
        "arrays may contain only strings"
    );
}

#[test]
fn rejects_malformed_arrays() {
    assert_eq!(
        parse_err("[profiles.x]\nphases = [\"a\" \"b\"]\n").message,
        "expected `,` or `]` in array"
    );
    assert_eq!(
        parse_err("[profiles.x]\nphases = [\"a\"\n").message,
        "expected `,` or `]` in array"
    );
}

#[test]
fn rejects_wrong_value_types() {
    assert_eq!(
        parse_err("[profiles.x]\nextends = 5\n").message,
        "`extends` expects a string, got an integer"
    );
    assert_eq!(
        parse_err("[profiles.x]\nderandomize = \"true\"\n").message,
        "`derandomize` expects a boolean, got a string"
    );
    assert_eq!(
        parse_err("[profiles.x]\ntest_cases = true\n").message,
        "`test_cases` expects an integer, got a boolean"
    );
    assert_eq!(
        parse_err("[profiles.x]\nphases = \"generate\"\n").message,
        "`phases` expects an array of strings, got a string"
    );
    assert_eq!(
        parse_err("[profiles.x]\ntest_cases = [\"a\"]\n").message,
        "`test_cases` expects an integer, got an array"
    );
}

#[test]
fn rejects_out_of_range_integers() {
    assert!(
        parse_err("[profiles.x]\ntest_cases = 0\n")
            .message
            .starts_with("`test_cases` must be between 1 and")
    );
    assert!(
        parse_err("[profiles.x]\nseed = -1\n")
            .message
            .starts_with("`seed` must be between 0 and")
    );
    assert!(
        parse_err("[profiles.x]\ntest_cases = 18446744073709551616\n")
            .message
            .starts_with("`test_cases` must be between 1 and")
    );
}

#[test]
fn rejects_unknown_enum_names() {
    assert_eq!(
        parse_err("[profiles.x]\nverbosity = \"loud\"\n").message,
        "`verbosity` expects one of quiet|normal|verbose|debug, got \"loud\""
    );
    assert_eq!(
        parse_err("[profiles.x]\nbackend = \"dice\"\n").message,
        "`backend` expects one of auto|default|urandom, got \"dice\""
    );
    assert!(
        parse_err("[profiles.x]\nphases = [\"explode\"]\n")
            .message
            .starts_with("`phases` does not recognise \"explode\"")
    );
    assert!(
        parse_err("[profiles.x]\nsuppress_health_check = [\"nope\"]\n")
            .message
            .starts_with("`suppress_health_check` does not recognise \"nope\"")
    );
}

#[test]
fn rejects_all_mixed_with_other_health_checks() {
    assert_eq!(
        parse_err("[profiles.x]\nsuppress_health_check = [\"all\", \"too_slow\"]\n").message,
        "\"all\" must be the only element of `suppress_health_check`"
    );
}

#[test]
fn rejects_empty_database_strings() {
    assert_eq!(
        parse_err("[profiles.x]\ndatabase = \"\"\n").message,
        "`database` expects a path or \"disabled\", got \"\""
    );
}

#[test]
fn rejects_invalid_extends_names() {
    assert!(
        parse_err("[profiles.x]\nextends = \"bad name\"\n")
            .message
            .starts_with("invalid profile name")
    );
}

#[test]
fn rejects_unknown_keys() {
    let e = parse_err("[profiles.x]\nmax_examples = 5\n");
    assert_eq!(e.line, 2);
    assert_eq!(e.message, "unknown key `max_examples`");
}

#[test]
fn parent_walks_toward_the_root() {
    assert_eq!(parent("/a/b"), Some("/a"));
    assert_eq!(parent("/a/b/"), Some("/a"));
    assert_eq!(parent("/a"), Some("/"));
    assert_eq!(parent("/"), None);
    assert_eq!(parent("relative"), None);
    assert_eq!(parent_on("C:/foo", true), Some("C:/"));
    assert_eq!(parent_on("C:/foo", false), Some("C:"));
}

#[test]
fn join_handles_trailing_separators() {
    assert_eq!(join("/a", FILE_NAME), "/a/hegel.toml");
    assert_eq!(join("/", FILE_NAME), "/hegel.toml");
}

#[test]
fn discover_finds_the_nearest_file() {
    let fs = |paths: &'static [&'static str]| {
        (
            move |path: &str| paths.contains(&path),
            move |path: &str| paths.contains(&path).then(|| path.as_bytes().to_vec()),
        )
    };
    let (exists, read) = fs(&["/a/hegel.toml", "/a/b/hegel.toml"]);
    assert_eq!(
        discover(Some("/a/b/c".to_owned()), exists, read),
        Some(("/a/b/hegel.toml".to_owned(), b"/a/b/hegel.toml".to_vec()))
    );
    let (exists, read) = fs(&["/a/hegel.toml"]);
    assert_eq!(
        discover(Some("/a/b/c".to_owned()), exists, read),
        Some(("/a/hegel.toml".to_owned(), b"/a/hegel.toml".to_vec()))
    );
    let (exists, read) = fs(&[]);
    assert_eq!(discover(Some("/a/b/c".to_owned()), exists, read), None);
}

#[test]
fn discover_without_a_cwd_checks_only_the_relative_path() {
    let found = discover(None, |p| p == "hegel.toml", |_| Some(vec![1]));
    assert_eq!(found, Some(("hegel.toml".to_owned(), vec![1])));
    assert_eq!(discover(None, |_| false, |_| Some(vec![1])), None);
}

#[test]
fn discover_skips_files_that_exist_but_cannot_be_read() {
    let found = discover(
        Some("/a/b".to_owned()),
        |_| true,
        |path| (path == "/a/hegel.toml").then(|| vec![2]),
    );
    assert_eq!(found, Some(("/a/hegel.toml".to_owned(), vec![2])));
}

#[test]
fn load_from_maps_parse_errors_onto_the_path() {
    let e = load_from(
        None,
        Some("/a".to_owned()),
        |p| p == "/a/hegel.toml",
        |_| Some(b"[profiles.x]\nboom = 1\n".to_vec()),
    )
    .unwrap_err();
    assert_eq!(
        e,
        ProfileError::Config {
            path: "/a/hegel.toml".to_owned(),
            line: 2,
            message: "unknown key `boom`".to_owned(),
        }
    );
}

#[test]
fn load_from_rejects_invalid_utf8() {
    let e = load_from(
        None,
        Some("/a".to_owned()),
        |p| p == "/a/hegel.toml",
        |_| Some(vec![0xff, 0xfe]),
    )
    .unwrap_err();
    assert_eq!(
        e,
        ProfileError::Config {
            path: "/a/hegel.toml".to_owned(),
            line: 0,
            message: "file is not valid UTF-8".to_owned(),
        }
    );
}

#[test]
fn load_from_without_a_file_is_an_empty_config() {
    assert_eq!(
        load_from(None, Some("/a".to_owned()), |_| false, |_| None),
        Ok(ConfigFile::default())
    );
}

#[test]
fn load_from_records_the_loaded_path() {
    let config = load_from(
        None,
        Some("/a".to_owned()),
        |p| p == "/a/hegel.toml",
        |_| Some(b"[profiles.x]\ntest_cases = 5\n".to_vec()),
    )
    .unwrap();
    assert_eq!(config.path.as_deref(), Some("/a/hegel.toml"));
}

#[test]
fn config_var_names_the_file_directly() {
    let config = load_from(
        Some("/elsewhere/alt.toml".to_owned()),
        Some("/a".to_owned()),
        |_| panic!("discovery must be skipped"),
        |path| {
            assert_eq!(path, "/elsewhere/alt.toml");
            Some(b"[profiles.x]\ntest_cases = 5\n".to_vec())
        },
    )
    .unwrap();
    assert_eq!(config.path.as_deref(), Some("/elsewhere/alt.toml"));
    assert_eq!(config.profiles.len(), 1);
}

#[test]
fn config_var_naming_an_unreadable_file_is_an_error() {
    let e = load_from(
        Some("/elsewhere/alt.toml".to_owned()),
        Some("/a".to_owned()),
        |_| false,
        |_| None,
    )
    .unwrap_err();
    assert_eq!(
        e,
        ProfileError::Config {
            path: "/elsewhere/alt.toml".to_owned(),
            line: 0,
            message: "cannot read the file named by HEGEL_CONFIG".to_owned(),
        }
    );
}

#[test]
fn an_empty_config_var_falls_back_to_discovery() {
    assert_eq!(
        load_from(
            Some(String::new()),
            Some("/a".to_owned()),
            |_| false,
            |_| { None }
        ),
        Ok(ConfigFile::default())
    );
}

/// The real, sys-backed `load`. It must succeed: this repository does not
/// keep a `hegel.toml` anywhere above the test's working directory, and
/// must never gain a malformed one.
#[test]
fn load_reads_the_real_filesystem() {
    assert!(load().is_ok());
}
