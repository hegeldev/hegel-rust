use super::*;

#[test]
fn check_antithesis_output_dir_accepts_an_existing_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(check_antithesis_output_dir(dir.path().to_str().unwrap()));
}

#[test]
fn check_antithesis_output_dir_panics_on_a_missing_directory() {
    let result =
        std::panic::catch_unwind(|| check_antithesis_output_dir("/nonexistent/antithesis-output"));
    let msg = result
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("to exist when running inside of Antithesis"),
        "{msg}"
    );
}
