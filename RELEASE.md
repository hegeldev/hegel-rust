RELEASE_TYPE: patch

This release improves resilience when the hegel server subprocess exits unexpectedly.

- When a server crash is detected, the next call to `hegel()` now transparently starts a fresh server instead of failing with a `PoisonError` or a generic panic.
- Server crash error messages now include the last few lines of `.hegel/server.log` so the root cause is visible without inspecting the log file manually.
- Panics from test functions (including server crashes) are now properly propagated rather than being replaced with a generic "could not find any examples" error.
