RELEASE_TYPE: patch

This patch fixes the replay of stateful counterexamples that need more than 50 steps ([#396](https://github.com/hegeldev/hegel-rust/issues/396)). Previously, the replay of the shrunk counterexample stopped at 50 steps and incorrectly triggered a flaky test error.
