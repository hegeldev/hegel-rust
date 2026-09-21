mod common;

use common::exec::fixture;

#[test]
fn target_error_reraised_to_user() {
    let output = fixture(env!("CARGO_BIN_EXE_fixture_target_nan"))
        .expect_failure("requires a finite score; got non-finite value")
        .run();
    assert!(
        !output.stderr.contains("Property test failed"),
        "a usage error must not be framed as a property failure, got:\n{}",
        output.stderr
    );
}
