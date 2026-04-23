use std::cell::Cell;

thread_local! {
    static IN_TEST_CONTEXT: Cell<bool> = const { Cell::new(false) };
    static QUIET_MODE: Cell<bool> = const { Cell::new(false) };
}

#[doc(hidden)]
pub(crate) fn with_test_context<R>(f: impl FnOnce() -> R) -> R {
    IN_TEST_CONTEXT.set(true);
    let result = f();
    IN_TEST_CONTEXT.set(false);
    result
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

pub(crate) fn set_quiet_mode(quiet: bool) {
    QUIET_MODE.set(quiet);
}

pub(crate) fn is_quiet_mode() -> bool {
    QUIET_MODE.get()
}
