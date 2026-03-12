use std::cell::Cell;

thread_local! {
    static IN_TEST_CONTEXT: Cell<bool> = const { Cell::new(false) };
}

/// Mark whether we are currently inside a Hegel test context.
pub(crate) fn set_in_test_context(value: bool) {
    IN_TEST_CONTEXT.with(|c| c.set(value));
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
    IN_TEST_CONTEXT.with(|c| c.get())
}
