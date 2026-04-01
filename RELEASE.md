RELEASE_TYPE: patch

This release improves resilience when the hegel server subprocess exits unexpectedly.

- When a server crash is detected, the next call to `hegel()` now transparently starts a fresh server instead of failing with a `PoisonError` or a generic panic.
- Server crash error messages now include the last few lines of `.hegel/server.log` so the root cause is visible without inspecting the log file manually.
- `find_any` no longer swallows real panics: if the condition function panics or the server crashes, the panic is re-raised rather than replaced with "could not find any examples".
