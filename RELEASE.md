RELEASE_TYPE: patch

This patch updates the native engine as part of making libhegel safe to unload and portable beyond std platforms. Three of the engine changes are visible from hegel-rust:

- Bugs in hegel itself now surface as run-level errors carrying a bug-report diagnostic instead of panics raised inside the engine. A panic that escapes that reporting — which would indicate a further bug in hegel — now aborts the process at the engine boundary instead of being converted into a run-level error.
- Engine diagnostics are written directly to the stderr file descriptor, so the Rust test harness's output capture no longer intercepts them.
- The engine's floating-point math now goes through the `libm` crate instead of the platform math library, which can round differently in the last bit, so on some platforms fixed-seed runs may generate different values than previous releases. Stored failures are unaffected: database replay is value-based, and seed reproducibility has always been build-specific.
