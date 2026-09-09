RELEASE_TYPE: patch

`#[hegel::main]` binaries now suppress the `TooSlow` and `TestCasesTooLarge` health checks, which judge how a run accumulates valid test cases and have nothing to judge in a run of one.

Stateful test cases no longer stop at random after tens of thousands of rounds: the engine's per-round stop probability is now 2^-32 instead of 2^-16, so a large `stateful_step_count` is honored in full.
