RELEASE_TYPE: minor

This release stops the native engine from aborting the host process on an invalid schema. Previously, a generation request the engine could not interpret — for example a misspelled type such as `{"type": "ipv4"}`, an unparseable regex, or a character set that excludes every codepoint — would `panic!`. When the engine is driven in-process over the C FFI (`libhegel`), that panic crossed the `extern "C"` boundary and aborted the whole host process (SIGABRT), so a single bad schema from a binding could take down an embedding application. The engine now reports these as recoverable errors: `hegel_generate` returns `HEGEL_E_INVALID_ARG` with a diagnostic in `hegel_last_error_message`, matching the documented contract, and the process keeps running. The pure-Rust API is unchanged — it still panics at the API surface, since its generators only ever build valid schemas.

Run-loop health-check failures (`FilterTooMuch`, `TooSlow`, and flaky-test detection) are likewise no longer raised as panics inside the engine; they are surfaced as a normal failing run, which a libhegel caller can inspect instead of having the worker abort.

This is a minor release because `hegel::backend::DataSourceError` gains a new `InvalidArgument(String)` variant and is now `#[non_exhaustive]`. Code that matches on `DataSourceError` exhaustively will need to add a wildcard arm:

```rust
match err {
    DataSourceError::StopTest => { /* ... */ }
    DataSourceError::Assume => { /* ... */ }
    DataSourceError::ServerError(msg) => { /* ... */ }
    // now required:
    _ => { /* ... */ }
}
```
