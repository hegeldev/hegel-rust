RELEASE_TYPE: minor

This release removes `Mode::SingleTestCase` and, with it, the ways it silently changed test semantics: state machines no longer run rules forever in any configuration (they are always bounded by `stateful_step_count`), and `tc.repeat(...)` always uses the engine-driven loop protocol. The `Mode` enum, `Settings::mode`, and the `--single-test-case` CLI flag are gone.

Instead, `#[hegel::main]` binaries now always run exactly one test case per invocation. The run otherwise behaves like any property test: invalid test cases (a failed `assume()`) are retried until one valid case has run, health checks apply, and failures are shrunk, reported, and persisted to the failure database, so a failure found by one invocation is replayed by the next. The test-case count is the one thing that cannot be changed — the `test_cases` attribute argument is rejected at compile time, the `--test-cases` CLI flag has been removed, and `HEGEL_TEST_CASES` has no effect on these binaries.

If you were using `Mode::SingleTestCase` for a long-running workload (for example under Antithesis), use a `#[hegel::main]` binary; to run more rules per state machine, raise `stateful_step_count`. If you were passing `--test-cases` to a main binary, run the binary once per test case instead.
