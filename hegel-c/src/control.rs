use alloc::string::String;
/// A violated internal invariant of Hegel itself (a bug in Hegel), carrying
/// the formatted diagnostic and the source location that raised it.
///
/// Raised by the [`hegel_internal_assert!`] family as an `Err` value and
/// threaded through each containing call graph: draw-layer violations merge
/// into the draw error channel (`EngineError`), engine-side violations
/// (shrinker, statistics, data tree) surface as a run-level error
/// (`RunError`) read back through `hegel_run_result_error`.
use alloc::string::ToString;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalError {
    message: String,
    location: &'static core::panic::Location<'static>,
}

impl InternalError {
    /// Capture `message` together with the caller's source location.
    #[track_caller]
    pub fn new(message: core::fmt::Arguments<'_>) -> Self {
        InternalError {
            message: message.to_string(),
            location: core::panic::Location::caller(),
        }
    }
}

impl core::fmt::Display for InternalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Internal error in hegel at {}: {}. This is a bug in hegel \
             itself; please report it at \
             https://github.com/hegeldev/hegel-rust/issues",
            self.location, self.message
        )
    }
}

impl core::error::Error for InternalError {}

/// Return an [`InternalError`] from the containing function, converted into
/// its error type with `.into()`. The building block behind the
/// [`hegel_internal_assert!`] family, for invariant violations detected by
/// control flow rather than a testable condition.
macro_rules! hegel_internal_error {
    ($($arg:tt)+) => {
        return Err($crate::control::InternalError::new(::core::format_args!($($arg)+)).into())
    };
}
pub(crate) use hegel_internal_error;

/// Unwrap an `Option` whose `None` case is a violated internal invariant of
/// Hegel itself: yields the contained value, or returns an [`InternalError`]
/// from the containing function (converted into its error type with
/// `.into()`) carrying the formatted message. The `Option`-shaped sibling of
/// [`hegel_internal_assert!`], for values a proven-elsewhere invariant
/// guarantees to be present.
macro_rules! hegel_internal_unwrap {
    ($option:expr, $($arg:tt)+) => {
        match $option {
            ::core::option::Option::Some(value) => value,
            ::core::option::Option::None => {
                $crate::control::hegel_internal_error!($($arg)+)
            }
        }
    };
}
pub(crate) use hegel_internal_unwrap;

/// Assert an internal invariant of Hegel itself. Use in place of `assert!`
/// everywhere under `src/` (enforced by `scripts/check-internal-asserts.py`):
/// a plain `assert!` panics, and the engine must never panic — a violated
/// internal invariant instead returns an [`InternalError`] `Err` (converted
/// into the containing function's error type with `.into()`) carrying the
/// bug-report framing above.
macro_rules! hegel_internal_assert {
    ($cond:expr $(,)?) => {
        if $cond {
        } else {
            $crate::control::hegel_internal_error!(
                "internal assertion failed: {}",
                ::core::stringify!($cond)
            );
        }
    };
    ($cond:expr, $($arg:tt)+) => {
        if $cond {
        } else {
            $crate::control::hegel_internal_error!($($arg)+);
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
                ::core::stringify!($left),
                ::core::stringify!($right),
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
                ::core::stringify!($left),
                ::core::stringify!($right),
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
        if ::core::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert;

/// [`hegel_internal_assert_eq!`] with `debug_assert!`'s cost model.
macro_rules! hegel_internal_debug_assert_eq {
    ($($arg:tt)+) => {
        if ::core::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert_eq!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert_eq;

/// [`hegel_internal_assert_ne!`] with `debug_assert!`'s cost model.
macro_rules! hegel_internal_debug_assert_ne {
    ($($arg:tt)+) => {
        if ::core::cfg!(debug_assertions) {
            $crate::control::hegel_internal_assert_ne!($($arg)+);
        }
    };
}
pub(crate) use hegel_internal_debug_assert_ne;

#[cfg(test)]
#[path = "../tests/embedded/control_tests.rs"]
mod tests;
