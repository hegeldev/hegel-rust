mod common;

use common::exec::fixture;

#[test]
fn target_error_reraised_to_user() {
    let output = fixture(env!("CARGO_BIN_EXE_fixture_target_nan")).run();
    assert!(output.stderr.contains("got non-finite value"));
}
