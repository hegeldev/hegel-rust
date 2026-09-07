RELEASE_TYPE: patch

libhegel is now documented as fork-compatible ([#186](https://github.com/hegeldev/hegel-rust/issues/186)). No libhegel call starts a thread, retains a file descriptor, or holds a lock after it returns, so a process may fork between calls. In particular a C harness can run each test case's body in a forked child for crash isolation, with the parent making every libhegel call and mapping the child's exit status onto `hegel_mark_complete`. The contract is in a new "Forking" section of the `hegel.h` preamble, with a worked example in `examples/fork.c`.
