RELEASE_TYPE: patch

`#[hegel::main]` binaries now run their single test case with no bound on the number of choices made. They also suppress the `TooSlow` and `TestCasesTooLarge` health checks.

Stateful test cases no longer stop at random after tens of thousands of rounds: the engine's per-round stop probability is now 2^-32 instead of 2^-16, so a large `stateful_step_count` is honored in full.
