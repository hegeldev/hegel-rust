RELEASE_TYPE: minor

This release improves how failing runs are reported and separates "the
property failed" from "the run itself failed".

When a run finds multiple distinct bugs, the report now leads with a
`Property-based test failed with N distinct failures.` headline, followed
by one self-contained block per failure: the counterexample's drawn values
immediately followed by its panic message and (when enabled) backtrace.
Previously all draw lines printed first and all diagnostics after, with no
way to tell which counterexample belonged to which stack trace.

Failures of the run itself — failed health checks, flaky tests,
nondeterministic generation, and stale `reproduce_failure` blobs — are no
longer framed as property-test failures. They now panic with their own
message, without the `Property test failed:` prefix, and over libhegel
they no longer appear as counterexamples. If you match on panic messages
from such runs, drop the prefix.

The libhegel C API changes accordingly: `hegel_run_result_passed` is
replaced by the three-state `hegel_run_result_status` (passed / failed /
errored), run-level errors carry their message through the new
`hegel_run_result_error`, and `hegel_failure_diagnostic` is removed — the
rendered panic block is printed by the Rust panic API at the moment the
panic is caught, and never contained anything meaningful over FFI.

This release also fixes two panic-classification bugs: a user panic whose
message happened to equal one of Hegel's internal sentinel strings was
silently misclassified (in the worst case turning a failing test case into
a passing one), and a rejected assumption on a spawned thread printed a
spurious `thread '<unnamed>' panicked` line for every affected test case.
Internal control flow now unwinds with typed payloads that skip the panic
hook entirely, which also removes per-rejection hook overhead from
rejection-heavy tests.

Finally, `Quiet` verbosity no longer prints the final replay's drawn
values, and a violated assumption in an `#[hegel::explicit_test_case]` now
panics with a readable `Explicit test case: assumption violated` message
instead of an internal marker string.

This release also exposes libhegel in flake.nix.
