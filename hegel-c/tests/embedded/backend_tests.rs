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
    // InvalidArgument surfaces its carried diagnostic verbatim.
    assert_eq!(
        DataSourceError::InvalidArgument("bad schema".to_string()).to_string(),
        "bad schema"
    );
}
