//! Server-backend entry point.
//!
//! The server backend's distinguishing piece is `ServerTestRunner`, which
//! talks to the Hypothesis subprocess over the protocol; the per-test-case
//! lifecycle (panic hook, `catch_unwind`, `mark_complete`, antithesis
//! integration, final re-raise) is shared with the native backend and lives
//! in [`crate::run_lifecycle`].

use crate::antithesis::TestLocation;
use crate::run_lifecycle::drive;
use crate::runner::Settings;
use crate::test_case::TestCase;

/// Encode a `ciborium::Value` to CBOR bytes.
pub(super) fn cbor_encode(value: &ciborium::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR encoding failed");
    bytes
}

/// Decode CBOR bytes to a `ciborium::Value`.
pub(super) fn cbor_decode(bytes: &[u8]) -> ciborium::Value {
    ciborium::from_reader(bytes).expect("CBOR decoding failed")
}

pub fn server_run<F>(
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    drive(
        super::session::ServerTestRunner,
        test_fn,
        settings,
        database_key,
        test_location,
    );
}
