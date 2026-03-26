use std::cell::Cell;

thread_local! {
    static IN_TEST_CONTEXT: Cell<bool> = const { Cell::new(false) };
}

#[doc(hidden)]
pub(crate) fn with_test_context<R>(f: impl FnOnce() -> R) -> R {
    IN_TEST_CONTEXT.set(true);
    let result = f();
    IN_TEST_CONTEXT.set(false);
    result
}

/// Extract a message from a panic payload.
pub(crate) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Returns `true` if we are currently inside a Hegel test context.
///
/// This can be used to conditionally execute code that depends on a
/// live test case (e.g., generating values, recording notes).
///
/// # Example
///
/// ```no_run
/// if hegel::currently_in_test_context() {
///     // inside a test
/// }
/// ```
pub fn currently_in_test_context() -> bool {
    IN_TEST_CONTEXT.get()
}
