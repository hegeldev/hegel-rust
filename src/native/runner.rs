//! Helper for [`super::tree::CachedTestFunction`].
//!
//! Most of the native engine — generation, shrinking, replay, multi-origin
//! tracking — lives in [`super::test_runner::NativeTestRunner`] and is
//! driven from [`crate::run_lifecycle::drive`] alongside the server backend.
//! `CachedTestFunction` is the legacy "run a test_fn" wrapper still used by
//! embedded tests that drive the engine directly, and it needs the helper
//! below to extract panic payloads.

/// Re-export of [`crate::run_lifecycle::panic_message`] so existing
/// `super::runner::panic_message` imports inside `native/` continue working.
/// The canonical implementation lives in `run_lifecycle.rs` (N1 dedup).
pub(crate) use crate::run_lifecycle::panic_message;
