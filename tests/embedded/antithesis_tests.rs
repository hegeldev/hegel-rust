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
