use super::*;
use crate::runner::Phase;

#[test]
fn test_parse_version_valid() {
    assert!(parse_version("0.1") < parse_version("0.2"));
    assert!(parse_version("0.2") > parse_version("0.1"));
    assert_eq!(parse_version("1.0"), parse_version("1.0"));
    assert!(parse_version("2.0") > parse_version("1.9"));
    assert!(parse_version("1.9") < parse_version("2.0"));
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_no_dot() {
    parse_version("1");
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_too_many_parts() {
    parse_version("1.2.3");
}

#[test]
#[should_panic(expected = "invalid major version")]
fn test_parse_version_non_numeric_major() {
    parse_version("abc.1");
}

#[test]
#[should_panic(expected = "invalid minor version")]
fn test_parse_version_non_numeric_minor() {
    parse_version("1.abc");
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_empty_string() {
    parse_version("");
}

#[test]
fn test_phase_as_str_all_variants() {
    assert_eq!(phase_as_str(&Phase::Explicit), "explicit");
    assert_eq!(phase_as_str(&Phase::Reuse), "reuse");
    assert_eq!(phase_as_str(&Phase::Generate), "generate");
    assert_eq!(phase_as_str(&Phase::Target), "target");
    assert_eq!(phase_as_str(&Phase::Shrink), "shrink");
}
