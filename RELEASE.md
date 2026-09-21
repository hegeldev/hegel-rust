RELEASE_TYPE: patch

This patch fixes usage errors and internal errors raised inside a test body — a non-finite `tc.target()` score, a generator bound with `max < min`, an empty `sampled_from`, ... — aborting the run with no output at all. The run still fails as before, but the error message is now printed like any other panic, instead of the process (or `#[hegel::test]`) exiting with a bare failure and nothing explaining why ([#47](https://github.com/hegeldev/hegel-rust/issues/47)).
