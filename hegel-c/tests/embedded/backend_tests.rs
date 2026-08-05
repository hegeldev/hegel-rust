use super::*;

#[test]
fn data_source_error_display_messages() {
    assert!(
        DataSourceError::StopTest
            .to_string()
            .contains("ran out of data")
    );
    assert!(
        DataSourceError::Assume
            .to_string()
            .contains("rejected the current draw")
    );
    assert_eq!(
        DataSourceError::InvalidArgument("bad schema".to_string()).to_string(),
        "bad schema"
    );
}

#[test]
fn internal_error_converts_into_data_source_error_and_displays_framed() {
    let e = crate::control::InternalError::new(format_args!("broken invariant"));
    let ds = DataSourceError::from(e);
    assert!(matches!(ds, DataSourceError::Internal(_)));
    let msg = ds.to_string();
    assert!(msg.contains("broken invariant"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn usage_error_displays_its_message_verbatim() {
    let run = RunError::UsageError("mark_complete was never called".to_string());
    assert_eq!(run.to_string(), "mark_complete was never called");
}

#[test]
fn internal_error_converts_into_run_error_and_displays_framed() {
    let e = crate::control::InternalError::new(format_args!("broken invariant"));
    let run = RunError::from(e);
    assert!(matches!(run, RunError::Internal(_)));
    let msg = run.to_string();
    assert!(msg.contains("broken invariant"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}
