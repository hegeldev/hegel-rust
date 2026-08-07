RELEASE_TYPE: patch

This patch updates the native engine as part of making libhegel safe to unload and portable beyond std platforms. Two of the engine changes are visible from hegel-rust:

- Bugs in hegel itself now surface as run-level errors carrying a bug-report diagnostic instead of panics raised inside the engine. A panic that escapes that reporting — which would indicate a further bug in hegel — now aborts the process at the engine boundary instead of being converted into a run-level error.
- Engine diagnostics are written directly to the stderr file descriptor, so the Rust test harness's output capture no longer intercepts them.
