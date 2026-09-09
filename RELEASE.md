RELEASE_TYPE: patch

`#[hegel::main]` binaries now suppress the `TooSlow` and `TestCasesTooLarge` health checks. Stateful test cases no longer stop at random after tens of thousands of rounds: the engine's per-round stop probability is now 2^-32 instead of 2^-16.
