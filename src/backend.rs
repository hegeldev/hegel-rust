//! Outcome types the per-test-case lifecycle works in.
//!
//! hegeltest drives the engine through libhegel's C ABI (see [`crate::ffi`]),
//! so the data-source / test-runner traits that used to live here now belong
//! to the engine crate. What remains is the small currency the lifecycle
//! itself speaks: the result of running one test case, and the failure it
//! carries. [`crate::run_lifecycle::run_test_case`] builds these from a caught
//! panic; [`crate::test_case::TestCase::mark_complete`] translates them into a
//! `hegel_mark_complete` status (plus, for a failure, its bug origin), and the
//! richer failure data (panic message, reproduce blob) is read back out of the
//! run result afterward.

/// A single failing test case the lifecycle has classified.
///
/// This is data about the failure, not its presentation: the rendered
/// diagnostic block (panic location, message, backtrace) is printed by the
/// lifecycle at the moment the panic is caught and never travels with the
/// failure.
#[derive(Debug, Clone)]
pub struct Failure {
    /// The raw panic message from the failing test (the string passed to
    /// `panic!`). Used as-is for the legacy single-failure outer panic message.
    pub panic_message: String,
    /// Opaque per-bug origin tag — currently `"Panic at file:line:col"` from
    /// the captured panic site (with `<unknown>` for the location when
    /// `take_panic_info` returns nothing). Passed to the engine through
    /// `hegel_mark_complete` so it can group test cases by which bug they
    /// trigger and shrink each origin to its own minimal counterexample.
    pub origin: String,
    /// A base64 "failure blob" encoding the minimal counterexample's choice
    /// sequence. Populated by the lifecycle from the run result for a failing
    /// run; `None` while a case is still in flight. Paste into
    /// `#[hegel::reproduce_failure("…")]` to replay it.
    pub reproduce_blob: Option<String>,
}

/// Result of running a single test case.
#[derive(Debug, Clone)]
pub enum TestCaseResult {
    /// Test case passed normally.
    Valid,
    /// Test case was rejected because an assumption failed.
    Invalid,
    /// Test case was rejected because the engine ran out of data.
    Overrun,
    /// Test case found a bug.
    Interesting(Failure),
}
