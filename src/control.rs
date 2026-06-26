use std::cell::Cell;

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

/// A violated internal invariant: a bug in Hegel itself, detected inside a
/// running test body. Aborts the run with a bug-report message — it must
/// never be classified as a counterexample and shrunk. Raised by the
/// [`hegel_internal_assert!`]-family macros.
pub(crate) struct InternalError(pub(crate) String);

/// Raise a control-flow unwind carrying `payload`. See the module note
/// above for why this is `resume_unwind`, not `panic!`.
pub(crate) fn raise_control<T: std::any::Any + Send>(payload: T) -> ! {
    std::panic::resume_unwind(Box::new(payload))
}

/// Raise an internal-error unwind (a bug in Hegel) carrying `message`,
/// with the caller's location and bug-report framing attached. Outside a
/// test context there is no lifecycle to catch a payload, so the message
/// is panicked directly.
#[track_caller]
pub(crate) fn raise_internal_error(message: std::fmt::Arguments<'_>) -> ! {
    let location = std::panic::Location::caller();
    let full = format!(
        "Internal error in hegel at {location}: {message}. This is a bug in hegel \
         itself; please report it at https://github.com/hegeldev/hegel-rust/issues"
    );
    if currently_in_test_context() {
        raise_control(InternalError(full));
    } else {
        panic!("{full}");
    }
}

/// Assert an internal invariant of Hegel itself. Use in place of `assert!`
/// everywhere under `src/` (enforced by `scripts/check-internal-asserts.py`):
/// a plain `assert!` that fires inside a running test body unwinds like a
/// test failure and gets shrunk as a counterexample, while a violated
/// internal invariant must abort the run with a bug-report message.
macro_rules! hegel_internal_assert {
    ($cond:expr $(,)?) => {
        if $cond {
        } else {
            $crate::control::raise_internal_error(::std::format_args!(
                "internal assertion failed: {}",
                ::std::stringify!($cond)
            ));
        }
    };
    ($cond:expr, $($arg:tt)+) => {
        if $cond {
        } else {
            $crate::control::raise_internal_error(::std::format_args!($($arg)+));
        }
    };
}
pub(crate) use hegel_internal_assert;

/// [`hegel_internal_assert!`] for equality, with both values in the message.
macro_rules! hegel_internal_assert_eq {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => $crate::control::hegel_internal_assert!(
                left == right,
                "internal assertion failed: {} == {} (left: {:?}, right: {:?})",
                ::std::stringify!($left),
                ::std::stringify!($right),
                left,
                right
            ),
        }
    };
}
pub(crate) use hegel_internal_assert_eq;

/// Raise an internal error (a bug in Hegel) directly, formatting like
/// [`format!`]. The non-assertion counterpart of
/// [`hegel_internal_assert!`], for invariant violations detected by
/// control flow rather than a boolean check.
macro_rules! hegel_internal_error {
    ($($arg:tt)+) => {
        $crate::control::raise_internal_error(::std::format_args!($($arg)+))
    };
}
pub(crate) use hegel_internal_error;

thread_local! {
    static IN_TEST_CONTEXT: Cell<bool> = const { Cell::new(false) };
}

#[doc(hidden)]
pub(crate) fn with_test_context<R>(f: impl FnOnce() -> R) -> R {
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

#[cfg(test)]
#[path = "../tests/embedded/control_tests.rs"]
mod tests;
