RELEASE_TYPE: patch

This patch fixes `hegel_test_case_from_blob` ignoring the `stateful_step_count` setting ([#396](https://github.com/hegeldev/hegel-rust/issues/396)). A stateful counterexample that needed more than 50 steps did not reproduce.
