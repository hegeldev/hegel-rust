//! Helper for [`super::tree::CachedTestFunction`].
//!
//! Most of the native engine — generation, shrinking, replay, multi-origin
//! tracking — lives in [`super::test_runner::NativeTestRunner`] and is
//! driven from [`crate::run_lifecycle::drive`] alongside the server backend.
//! `CachedTestFunction` is the legacy "run a test_fn" wrapper still used by
//! embedded tests that drive the engine directly, and it needs the helper
//! below to extract panic payloads.

/// Extract a string message from a panic payload.
pub(crate) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}
