/// Raise an internal-error panic (a bug in Hegel) carrying `message`, with
/// the caller's location and bug-report framing attached.
#[track_caller]
pub(crate) fn raise_internal_error(message: std::fmt::Arguments<'_>) -> ! {
    let location = std::panic::Location::caller();
    panic!(
        "Internal error in hegel at {location}: {message}. This is a bug in hegel \
         itself; please report it at https://github.com/hegeldev/hegel-rust/issues"
    );
}

/// Assert an internal invariant of Hegel itself. Use in place of `assert!`
/// everywhere under `src/` (enforced by `scripts/check-internal-asserts.py`):
/// a plain `assert!` reads as an ordinary test assertion, while a violated
/// internal invariant carries the bug-report framing above.
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

/// [`hegel_internal_assert!`] for inequality, with both values in the message.
macro_rules! hegel_internal_assert_ne {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => $crate::control::hegel_internal_assert!(
                left != right,
                "internal assertion failed: {} != {} (both: {:?})",
                ::std::stringify!($left),
                ::std::stringify!($right),
                left
            ),
        }
    };
}
pub(crate) use hegel_internal_assert_ne;

/// [`hegel_internal_assert!`] with `debug_assert!`'s cost model: compiled
/// out unless `debug_assertions` are enabled. For engine hot paths.
macro_rules! hegel_internal_debug_assert {
    ($($arg:tt)+) => {
        if ::std::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert;

/// [`hegel_internal_assert_eq!`] with `debug_assert!`'s cost model.
macro_rules! hegel_internal_debug_assert_eq {
    ($($arg:tt)+) => {
        if ::std::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert_eq!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert_eq;

/// [`hegel_internal_assert_ne!`] with `debug_assert!`'s cost model.
macro_rules! hegel_internal_debug_assert_ne {
    ($($arg:tt)+) => {
        if ::std::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert_ne!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert_ne;

#[cfg(test)]
#[path = "../tests/embedded/control_tests.rs"]
mod tests;
