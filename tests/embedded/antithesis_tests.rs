use super::*;

#[test]
fn require_antithesis_feature_panics_when_in_antithesis_without_feature() {
    let result = std::panic::catch_unwind(|| require_antithesis_feature(true, false));
    assert!(result.is_err());
    let msg = result
        .unwrap_err()
        .downcast::<String>()
        .unwrap_or_else(|e| Box::new(e.downcast::<&str>().unwrap().to_string()));
    assert!(msg.contains("antithesis"));
}

#[test]
fn require_antithesis_feature_does_not_panic_outside_antithesis() {
    require_antithesis_feature(false, false);
}

#[test]
fn require_antithesis_feature_does_not_panic_when_feature_enabled() {
    require_antithesis_feature(true, true);
}

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
