# Server Module Refactor

Extract server-specific code from `runner.rs` into `src/server/`, parallel to `src/native/`, eliminating ~65 item-level `#[cfg]` annotations.

## Problem

`runner.rs` is 1464 lines mixing shared types (~250 lines) with server backend implementation (~1100 lines). Every server item has its own `#[cfg(not(feature = "native"))]` annotation — 65 of them. The server-only modules `protocol/`, `utils.rs`, and `uv.rs` are also individually gated in `lib.rs`.

## Design

### New file layout

```
src/
  lib.rs             — one cfg gate: `#[cfg(not(feature = "native"))] mod server;`
  runner.rs          — shared types only (~250 lines, zero cfg annotations on items)
  server/
    mod.rs           — module declarations, re-exports
    runner.rs        — server_run entry point, run_test_case, panic hooks, backtrace formatting
    data_source.rs   — ServerDataSource (protocol-based DataSource impl)
    session.rs       — HegelSession, parse_version, handshake, monitor thread
    process.rs       — hegel_command, resolve_hegel_path, server_log_file, wait_for_exit,
                       startup_error_message, handle_handshake_failure, format_log_excerpt,
                       log helpers, server_crash_message, handle_channel_error,
                       __test_kill_server
    protocol/        — moved from src/protocol/ (connection.rs, packet.rs, stream.rs, mod.rs)
    utils.rs         — moved from src/utils.rs (validate_executable, which)
    uv.rs            — moved from src/uv.rs (find_uv)
  native/            — unchanged
```

### What stays in `runner.rs`

These items are used by both backends and have no server dependency:

- `HealthCheck` enum (without `as_str` — that moves to server)
- `Verbosity` enum
- `Settings` struct + impl + Default
- `Database` enum
- `hegel()` function
- `is_in_ci()` function
- `Hegel<F>` struct + impl (builder methods + `run()`)

### `Hegel::run()` dispatch

The two cfg branches become calls to module-level entry points with matching signatures:

```rust
pub fn run(self) {
    #[cfg(feature = "native")]
    {
        crate::native::runner::native_run(
            self.test_fn, &self.settings,
            self.database_key.as_deref(),
            self.test_location.as_ref(),
        );
    }
    #[cfg(not(feature = "native"))]
    {
        crate::server::runner::server_run(
            self.test_fn, &self.settings,
            self.database_key.as_deref(),
            self.test_location.as_ref(),
        );
    }
}
```

This is the only place cfg annotations appear in `runner.rs`.

### `src/server/mod.rs`

```rust
pub mod runner;
mod data_source;
mod session;
mod process;
pub(crate) mod protocol;
mod utils;
mod uv;
```

Items that `lib.rs` currently re-exports (`__test_kill_server`, `format_log_excerpt`) will be re-exported through `server::process` and then through `lib.rs` with a single cfg gate on the re-export block.

### `src/lib.rs` changes

Before:
```rust
#[cfg(feature = "native")]
pub(crate) mod native;
#[cfg(not(feature = "native"))]
pub(crate) mod protocol;
pub(crate) mod runner;
#[cfg(not(feature = "native"))]
pub(crate) mod utils;
#[cfg(not(feature = "native"))]
mod uv;
```

After:
```rust
#[cfg(feature = "native")]
pub(crate) mod native;
#[cfg(not(feature = "native"))]
pub(crate) mod server;
pub(crate) mod runner;
```

The `protocol`, `utils`, and `uv` re-exports from `lib.rs` are removed — they become internal to `server/`.

### `HealthCheck::as_str`

Currently gated with `#[cfg(not(feature = "native"))]` because only the server runner calls it. After the refactor, it either:
- Moves to a free function in `server/runner.rs` (keeps HealthCheck clean), or
- Stays on HealthCheck without the cfg gate (harmless dead code in native mode, but the compiler may warn)

Recommendation: move to a free function `health_check_to_str()` in `server/runner.rs`. The shared type stays clean.

### Test file changes

**Embedded tests** (`tests/embedded/`):

`tests/embedded/runner_tests.rs` currently has ~300 lines of server-specific tests gated by `#[cfg(all(test, not(feature = "native")))]` on the module include. After the refactor:

- **Stays in `tests/embedded/runner_tests.rs`**: the 3 tests that exercise shared code:
  - `test_settings_verbosity`
  - `test_is_in_ci_some_expected_variant`
  - `test_settings_new_in_ci_disables_database`
  
  These will be included from `runner.rs` with `#[cfg(test)]` (no native gate needed — they test shared types).

- **Moves to `tests/embedded/server/`**: everything else (parse_version, startup errors, log handling, resolve_hegel_path, wait_for_exit, handle_handshake_failure, PROTOCOL_DEBUG, etc.). Split across files mirroring the server submodules:
  - `tests/embedded/server/runner_tests.rs` — (currently empty or minimal)
  - `tests/embedded/server/session_tests.rs` — parse_version tests
  - `tests/embedded/server/process_tests.rs` — startup_error_message, resolve_hegel_path, wait_for_exit, handle_handshake_failure, log tests, handle_channel_error tests

- `tests/embedded/protocol/` — stays where it is but is now included from `server/protocol/*.rs` instead of `src/protocol/*.rs`
- `tests/embedded/uv_tests.rs` — moves to `tests/embedded/server/uv_tests.rs`, included from `server/uv.rs`

**Integration tests** (`tests/`):

No changes needed. `test_server_crash.rs`, `test_server_restart.rs`, `test_bad_server_command.rs`, and `test_install_errors.rs` already have `#![cfg(not(feature = "native"))]` at the file level, which is correct.

## What does NOT change

- `src/native/` — untouched
- `src/backend.rs` — the `DataSource`, `TestRunner` traits stay where they are
- `src/test_case.rs` — untouched
- `src/control.rs` — untouched
- `src/antithesis.rs` — untouched
- `src/generators/` — untouched
- All integration test files in `tests/` — untouched (their existing file-level cfg gates are correct)
- `hegel-macros/` — untouched

## Verification

After the refactor:
1. `cargo check --features native` — compiles with zero warnings
2. `cargo check` — compiles with zero warnings
3. `cargo test --features native --no-fail-fast` — all tests pass
4. `cargo test --no-fail-fast` — all tests pass
5. `grep -r 'cfg.*feature.*native' src/` produces only:
   - `lib.rs`: 2 lines (one for `mod native`, one for `mod server`)
   - `runner.rs`: 2 lines (the two branches in `Hegel::run()`)
