RELEASE_TYPE: patch

`#[hegel::main]` binaries now always suppress the `TooSlow` health check. That check judges how quickly a run accumulates valid test cases, which a run of exactly one test case cannot be judged on, and a single long-running test case (such as a large stateful machine) would otherwise fail it after 30 seconds.
