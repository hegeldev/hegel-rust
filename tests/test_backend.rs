use hegel::backend::DataSourceError;

#[test]
fn test_data_source_error_display() {
    let stop = DataSourceError::StopTest;
    let assume = DataSourceError::Assume;

    assert!(stop.to_string().contains("StopTest"));
    assert!(assume.to_string().contains("Assume"));
}
