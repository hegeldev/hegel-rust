use super::*;

#[test]
fn existing_output_dir_is_accepted() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(check_antithesis_output_dir(dir.path().to_str().unwrap()));
}

#[test]
fn missing_output_dir_panics() {
    // A configured-but-absent ANTITHESIS_OUTPUT_DIR is a launch
    // misconfiguration, surfaced as a plain panic.
    let result = std::panic::catch_unwind(|| {
        check_antithesis_output_dir("/no/such/antithesis/output/dir/for/hegel/tests")
    });
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(msg.contains("ANTITHESIS_OUTPUT_DIR"), "got: {msg}");
}
