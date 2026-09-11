use super::*;
use alloc::string::ToString;

#[test]
fn existing_output_dir_is_accepted() {
    let dir = tempfile::TempDir::new().unwrap();
    check_antithesis_output_dir(dir.path().to_str().unwrap()).unwrap();
}

#[test]
fn missing_output_dir_is_a_usage_error() {
    let err =
        check_antithesis_output_dir("/no/such/antithesis/output/dir/for/hegel/tests").unwrap_err();
    assert!(matches!(err, crate::backend::RunError::UsageError(_)));
    let msg = err.to_string();
    assert!(msg.contains("ANTITHESIS_OUTPUT_DIR"), "got: {msg}");
}

/// A fake environment holding only `ANTITHESIS_OUTPUT_DIR`.
fn env_with_output_dir(dir: &str) -> impl Fn(&str) -> Option<String> {
    let dir = dir.to_string();
    move |key| (key == "ANTITHESIS_OUTPUT_DIR").then(|| dir.clone())
}

#[test]
fn not_in_antithesis_when_the_variable_is_unset() {
    check_environment_from(|_| None).unwrap();
    assert!(!antithesis_env_var_set_from(|_| None));
    assert!(antithesis_output_dir_from(|_| None).is_none());
}

#[test]
fn in_antithesis_when_the_variable_names_an_existing_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    let env = env_with_output_dir(dir.path().to_str().unwrap());
    check_environment_from(&env).unwrap();
    assert_eq!(antithesis_env_var_set_from(&env), !cfg!(windows));
    assert_eq!(antithesis_output_dir_from(&env).is_some(), !cfg!(windows));
}

#[test]
fn other_variables_do_not_count() {
    let env = |key: &str| (key == "ANTITHESIS_OUTPUT").then(|| "/tmp".to_string());
    check_environment_from(env).unwrap();
    assert!(!antithesis_env_var_set_from(env));
}

#[cfg(not(windows))]
#[test]
fn a_missing_directory_is_a_usage_error_via_the_environment() {
    let env = env_with_output_dir("/no/such/antithesis/output/dir/for/hegel/tests");
    let err = check_environment_from(env).unwrap_err();
    assert!(matches!(err, crate::backend::RunError::UsageError(_)));
}
