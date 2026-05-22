RELEASE_TYPE: patch

This patch makes `Verbosity::Verbose` (and `Verbosity::Debug`) actually show what's happening inside a run. Previously these levels only added a single `Running test case` line; everything else (drawn values, notes, panic messages) was suppressed until the final replay.

At `Verbose` or higher:

- `tc.note(...)` and the per-draw `let x = ...;` lines now print on every test case, not only the final replay of a failing example.
- When a test case panics, the full panic diagnostic (thread, location, message, and backtrace if `RUST_BACKTRACE` is set) is emitted as soon as the panic happens, so you can see which inputs caused which failure as the run progresses.
- Each test case rejected by the backend prints a short reason line — `Test case stopped: failed assumption` for `assume`/`reject` calls, and `Test case stopped: out of data` when the backend exhausts its choice budget.
