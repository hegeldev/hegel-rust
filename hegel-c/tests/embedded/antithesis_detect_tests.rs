use super::*;
use alloc::string::ToString;

#[test]
fn existing_output_dir_is_accepted() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(check_antithesis_output_dir(dir.path().to_str().unwrap()).unwrap());
}

#[test]
fn missing_output_dir_is_a_usage_error() {
    let err =
        check_antithesis_output_dir("/no/such/antithesis/output/dir/for/hegel/tests").unwrap_err();
    assert!(matches!(err, crate::backend::RunError::UsageError(_)));
    let msg = err.to_string();
    assert!(msg.contains("ANTITHESIS_OUTPUT_DIR"), "got: {msg}");
}
