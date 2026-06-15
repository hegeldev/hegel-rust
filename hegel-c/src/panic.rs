/// Extract a string message from a panic payload.
///
/// `pub(crate)` so the libhegel C bindings can use it from their own
/// `catch_unwind` wrapper around `run_native`. Copied from hegeltest's
/// `run_lifecycle::panic_message`.
pub(crate) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}
