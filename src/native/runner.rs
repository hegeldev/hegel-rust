//! Helpers for the legacy [`super::tree::CachedTestFunction`] code path.
//!
//! Most of the native engine — generation, shrinking, replay, multi-origin
//! tracking — now lives in [`super::test_runner::NativeTestRunner`] and is
//! driven from [`crate::run_lifecycle::drive`] alongside the server backend.
//! The two helpers below are still needed by `CachedTestFunction::execute`,
//! which is kept around as a self-contained "run a test_fn" wrapper for
//! embedded tests that want to drive the engine without going through the
//! cross-backend lifecycle.

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

/// Legacy stub kept so [`super::tree::CachedTestFunction::execute`]
/// compiles. The cross-backend lifecycle in [`crate::run_lifecycle`] now
/// owns the panic-info plumbing for tests run through `Hegel::run`; the
/// CachedTestFunction code path is only exercised by embedded tests that
/// don't care about the printed output.
pub(crate) fn store_final_panic_info(_msg: &str) {}
