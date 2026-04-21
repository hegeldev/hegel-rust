RELEASE_TYPE: minor

Add `Mode::SingleTestCase` setting for running exactly one test case with no shrinking or replay. Available via `Settings::mode()`, `#[hegel::test(mode = Mode::SingleTestCase)]`, and the `--single-test-case` CLI flag.

This mode is mostly intended for long-running workloads, so in this mode, `repeat` becomes a simple infinite loop, and stateful testing will keep running rules indefinitely.
