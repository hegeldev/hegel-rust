use std::cell::Cell;

// ─── Control-flow unwind payloads ───────────────────────────────────────────
//
// Hegel's internal control flow (a rejected assumption, the engine running
// out of data, `TestCase::repeat` finishing, an invalid-argument usage
// error) unwinds out of the test body through the lifecycle's
// `catch_unwind`. These payload types are raised with
// [`std::panic::resume_unwind`] rather than `panic!`, which skips the panic
// hook entirely: no `thread '...' panicked` line is printed on any thread,
// no backtrace is captured, and no per-unwind hook work happens on the
// rejection-heavy generation path. Classification is by downcast, so no
// user panic — whatever its message — can be mistaken for control flow.

/// A rejected assumption (`tc.assume(false)` / `tc.reject()`): discard this
/// test case as `Invalid` without counting it against the budget.
pub(crate) struct AssumeFailed;

/// The engine ran out of data for this test case: conclude it as `Overrun`.
pub(crate) struct StopTest;

/// `TestCase::repeat`'s loop completed naturally. Because `repeat` returns
/// `!`, it has no normal-return path; this unwind is how it tells the
/// lifecycle "this test case finished successfully, record it as `Valid`".
pub(crate) struct LoopDone;

/// An invalid-argument (usage) error detected inside a running test body:
/// the caller configured the test in a way the framework can't honour (a
/// generator bound with `max < min`, an empty `sampled_from`, a non-finite
/// `tc.target()` score, ...). A mistake in how the test is *written*, not a
/// property that failed on some input — the lifecycle aborts the run with
/// the carried message instead of shrinking it as a counterexample.
pub(crate) struct InvalidArgument(pub(crate) String);

/// Raise a control-flow unwind carrying `payload`. See the module note
/// above for why this is `resume_unwind`, not `panic!`.
pub(crate) fn raise_control<T: std::any::Any + Send>(payload: T) -> ! {
    std::panic::resume_unwind(Box::new(payload))
}

thread_local! {
    static IN_TEST_CONTEXT: Cell<bool> = const { Cell::new(false) };
}

#[doc(hidden)]
pub(crate) fn with_test_context<R>(f: impl FnOnce() -> R) -> R {
    // Restore (rather than clear) on a drop guard: the flag survives a
    // panic unwinding out of `f`, and nested uses don't clear the outer
    // context early.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            IN_TEST_CONTEXT.set(self.0);
        }
    }
    let _restore = Restore(IN_TEST_CONTEXT.get());
    IN_TEST_CONTEXT.set(true);
    f()
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
