# Server Module Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract all server-specific code from `runner.rs` into `src/server/`, eliminating ~65 item-level `#[cfg]` annotations.

**Architecture:** Create `src/server/` as a peer to `src/native/`. Move the protocol layer, server process management, and server-side test runner into submodules. The existing `runner.rs` shrinks to shared types and the `Hegel` builder. A single `#[cfg(not(feature = "native"))]` on `mod server` in `lib.rs` replaces all item-level gates.

**Tech Stack:** Rust, no new dependencies.

---

## File Map

**Create:**
- `src/server/mod.rs` — module declarations, re-exports
- `src/server/runner.rs` — `server_run()`, `run_test_case()`, panic hooks, backtrace
- `src/server/data_source.rs` — `ServerDataSource`, `PROTOCOL_DEBUG`
- `src/server/session.rs` — `HegelSession`, `ServerTestRunner`, `parse_version()`
- `src/server/process.rs` — server process management, log formatting, `__test_kill_server`
- `src/server/protocol/mod.rs` — moved from `src/protocol/mod.rs`
- `src/server/protocol/connection.rs` — moved from `src/protocol/connection.rs`
- `src/server/protocol/packet.rs` — moved from `src/protocol/packet.rs`
- `src/server/protocol/stream.rs` — moved from `src/protocol/stream.rs`
- `src/server/utils.rs` — moved from `src/utils.rs`
- `src/server/uv.rs` — moved from `src/uv.rs`
- `tests/embedded/server/mod.rs` — test module declarations
- `tests/embedded/server/runner_tests.rs` — server runner tests
- `tests/embedded/server/session_tests.rs` — parse_version tests
- `tests/embedded/server/process_tests.rs` — startup, log, resolve_hegel_path tests
- `tests/embedded/server/protocol/` — moved from `tests/embedded/protocol/`
- `tests/embedded/server/uv_tests.rs` — moved from `tests/embedded/uv_tests.rs`

**Modify:**
- `src/lib.rs` — replace 4 cfg-gated module declarations with 1
- `src/runner.rs` — strip to shared types + `Hegel` builder (~250 lines)

**Delete:**
- `src/protocol/` (moved to `src/server/protocol/`)
- `src/utils.rs` (moved to `src/server/utils.rs`)
- `src/uv.rs` (moved to `src/server/uv.rs`)
- `tests/embedded/runner_tests.rs` (split into `tests/embedded/runner_tests.rs` + `tests/embedded/server/`)
- `tests/embedded/protocol/` (moved to `tests/embedded/server/protocol/`)
- `tests/embedded/uv_tests.rs` (moved to `tests/embedded/server/uv_tests.rs`)

---

### Task 1: Create `src/server/` module skeleton and move protocol, utils, uv

Move the three standalone server-only modules into `src/server/` and wire them up. No logic changes — just file moves and path updates.

**Files:**
- Create: `src/server/mod.rs`
- Move: `src/protocol/` → `src/server/protocol/`
- Move: `src/utils.rs` → `src/server/utils.rs`
- Move: `src/uv.rs` → `src/server/uv.rs`
- Modify: `src/lib.rs`
- Move: `tests/embedded/protocol/` → `tests/embedded/server/protocol/`
- Move: `tests/embedded/uv_tests.rs` → `tests/embedded/server/uv_tests.rs`

- [ ] **Step 1: Create `src/server/mod.rs`**

```rust
// Server backend for Hegel.
//
// When the `native` feature is NOT enabled, this module provides the test
// runner that communicates with a Python hegel-core server over Unix sockets.

pub(crate) mod protocol;
pub(crate) mod utils;
pub(crate) mod uv;
```

- [ ] **Step 2: Move protocol files**

```bash
mkdir -p src/server/protocol
git mv src/protocol/mod.rs src/server/protocol/mod.rs
git mv src/protocol/connection.rs src/server/protocol/connection.rs
git mv src/protocol/packet.rs src/server/protocol/packet.rs
git mv src/protocol/stream.rs src/server/protocol/stream.rs
rmdir src/protocol
```

- [ ] **Step 3: Update protocol test `#[path]` attributes**

The protocol source files include embedded tests via `#[path]` attributes. These paths are relative to the source file. After moving from `src/protocol/` to `src/server/protocol/`, the relative path to `tests/embedded/` changes from `../../tests/embedded/` to `../../../tests/embedded/`.

In `src/server/protocol/connection.rs`, change:
```rust
#[path = "../../tests/embedded/protocol/connection_tests.rs"]
```
to:
```rust
#[path = "../../../tests/embedded/server/protocol/connection_tests.rs"]
```

Apply the same depth change in `packet.rs` and `stream.rs`.

- [ ] **Step 4: Move protocol test files**

```bash
mkdir -p tests/embedded/server/protocol
git mv tests/embedded/protocol/connection_tests.rs tests/embedded/server/protocol/connection_tests.rs
git mv tests/embedded/protocol/packet_tests.rs tests/embedded/server/protocol/packet_tests.rs
git mv tests/embedded/protocol/stream_tests.rs tests/embedded/server/protocol/stream_tests.rs
rmdir tests/embedded/protocol
```

- [ ] **Step 5: Move utils.rs and uv.rs**

```bash
git mv src/utils.rs src/server/utils.rs
git mv src/uv.rs src/server/uv.rs
```

- [ ] **Step 6: Update uv.rs test `#[path]` attribute**

In `src/server/uv.rs`, change:
```rust
#[path = "../tests/embedded/uv_tests.rs"]
```
to:
```rust
#[path = "../../tests/embedded/server/uv_tests.rs"]
```

- [ ] **Step 7: Move uv test file**

```bash
mkdir -p tests/embedded/server
git mv tests/embedded/uv_tests.rs tests/embedded/server/uv_tests.rs
```

- [ ] **Step 8: Update `src/lib.rs` module declarations**

Replace:
```rust
#[cfg(feature = "native")]
pub(crate) mod native;
#[cfg(not(feature = "native"))]
pub(crate) mod protocol;
pub(crate) mod runner;
pub mod stateful;
mod test_case;
#[cfg(not(feature = "native"))]
pub(crate) mod utils;
#[cfg(not(feature = "native"))]
mod uv;
```

With:
```rust
#[cfg(feature = "native")]
pub(crate) mod native;
pub(crate) mod runner;
#[cfg(not(feature = "native"))]
pub(crate) mod server;
pub mod stateful;
mod test_case;
```

- [ ] **Step 9: Update cross-module references**

In `src/runner.rs`, the server code references `crate::protocol::`, `crate::utils::`, and `crate::uv::`. These now live under `crate::server::`. Update these references:

- `crate::protocol::` → `crate::server::protocol::`
- `crate::utils::` → `crate::server::utils::`
- `crate::uv::` → `crate::server::uv::`

These references are all inside cfg-gated blocks in `runner.rs` that will move in later tasks, but they need to compile now.

- [ ] **Step 10: Verify both modes compile**

```bash
cargo check --features native
cargo check
```

Both must succeed with no errors.

- [ ] **Step 11: Run tests**

```bash
cargo test --features native --no-fail-fast 2>&1 > /tmp/refactor-native.txt
cargo test --no-fail-fast 2>&1 > /tmp/refactor-server.txt
```

Both must pass. Check for zero FAILED lines in each output file.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "Move protocol, utils, uv into src/server/ module"
```

---

### Task 2: Extract `ServerDataSource` into `src/server/data_source.rs`

Move `ServerDataSource`, its `DataSource` impl, and the `PROTOCOL_DEBUG` static out of `runner.rs`.

**Files:**
- Create: `src/server/data_source.rs`
- Modify: `src/server/mod.rs`
- Modify: `src/runner.rs`

- [ ] **Step 1: Create `src/server/data_source.rs`**

Extract lines from `runner.rs` covering `PROTOCOL_DEBUG`, `struct ServerDataSource`, `impl ServerDataSource`, and `impl DataSource for ServerDataSource`. The file needs its own imports. Remove all `#[cfg(not(feature = "native"))]` annotations — the entire module is already behind the cfg gate.

The `server_crash_message()` call on the error path should use `super::process::server_crash_message` — but that function hasn't been extracted yet. For now, import it as `crate::runner::server_crash_message` (it's still in runner.rs). This will be fixed in Task 4 when process.rs is created.

Actually, simpler: use a temporary `pub(crate)` visibility on `server_crash_message` in `runner.rs` so `data_source.rs` can call it. It reverts to private when the function moves to `process.rs` in Task 4.

The full file content (copy from `runner.rs` lines 57–278, stripping cfg annotations, adjusting imports):

```rust
use std::cell::{Cell, RefCell};
use std::sync::{Arc, LazyLock};

use ciborium::Value;

use crate::backend::{DataSource, DataSourceError};
use crate::cbor_utils::{cbor_map, map_insert};
use crate::runner::Verbosity;
use super::protocol::{Connection, Stream};

pub(super) static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    matches!(
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
});

/// Backend implementation that communicates with the hegel-core server
/// over a multiplexed stream.
pub(crate) struct ServerDataSource {
    connection: Arc<Connection>,
    stream: RefCell<Stream>,
    aborted: Cell<bool>,
    verbosity: Verbosity,
}

impl ServerDataSource {
    pub(crate) fn new(connection: Arc<Connection>, stream: Stream, verbosity: Verbosity) -> Self {
        // ... (exact copy of existing impl, but with server_crash_message()
        //  called as super::process::server_crash_message() — placeholder
        //  until Task 4, use crate::runner::server_crash_message for now)
        // Copy the full body from runner.rs
    }
    // ... send_request and all DataSource trait methods, verbatim from runner.rs
}
```

Copy the full `impl ServerDataSource` and `impl DataSource for ServerDataSource` blocks verbatim from `runner.rs`, removing `#[cfg]` annotations. For the `server_crash_message()` call in `send_request`, temporarily call `super::process::server_crash_message()` — wait, that module doesn't exist yet. Instead, keep `server_crash_message` in `runner.rs` for now as `pub(crate)` and call `crate::runner::server_crash_message()`. Task 4 will move it.

- [ ] **Step 2: Add `data_source` to `src/server/mod.rs`**

```rust
pub(crate) mod data_source;
pub(crate) mod protocol;
pub(crate) mod utils;
pub(crate) mod uv;
```

- [ ] **Step 3: Remove `ServerDataSource` code from `runner.rs`**

Delete the `PROTOCOL_DEBUG` static, `struct ServerDataSource`, `impl ServerDataSource`, and `impl DataSource for ServerDataSource` blocks (the entire range from the PROTOCOL_DEBUG static through the end of the DataSource impl). Also remove the now-unused imports that were only needed by ServerDataSource (e.g. `Cell`, `RefCell`, `LazyLock` — but check they're not used elsewhere in runner.rs first).

- [ ] **Step 4: Update references in `runner.rs`**

Where `runner.rs` constructs `ServerDataSource::new(...)` (in the `ServerTestRunner::run` method), change to `crate::server::data_source::ServerDataSource::new(...)` or add a `use` at the top of the relevant block.

- [ ] **Step 5: Verify both modes compile and test**

```bash
cargo check --features native && cargo check
cargo test --features native --no-fail-fast 2>&1 > /tmp/refactor-t2-native.txt
cargo test --no-fail-fast 2>&1 > /tmp/refactor-t2-server.txt
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Extract ServerDataSource into server/data_source.rs"
```

---

### Task 3: Extract `HegelSession` and `ServerTestRunner` into `src/server/session.rs`

Move the session management and test runner protocol code.

**Files:**
- Create: `src/server/session.rs`
- Modify: `src/server/mod.rs`
- Modify: `src/runner.rs`

- [ ] **Step 1: Create `src/server/session.rs`**

Move from `runner.rs`:
- `parse_version()`
- `struct HegelSession` and `impl HegelSession`
- `struct ServerTestRunner` and `impl TestRunner for ServerTestRunner`
- Constants: `SUPPORTED_PROTOCOL_VERSIONS`, `HEGEL_SERVER_VERSION`
- Static: `SESSION`

The file needs imports for `Connection`, `Stream`, `ServerDataSource`, `Settings`, `Database`, `Verbosity`, `HealthCheck`, plus the backend traits. These come from `super::` (sibling modules in server) and `crate::runner` (shared types) and `crate::backend`.

`HegelSession::init` calls `hegel_command()`, `server_log_file()`, `init_panic_hook()`, `handle_handshake_failure()`. These live in `runner.rs` for now (moving in Tasks 4 and 5). Reference them as `crate::runner::*` temporarily; Tasks 4 and 5 will move them.

`ServerTestRunner::run` calls `handle_channel_error()`, `server_crash_message()` — same approach.

`HealthCheck::as_str()` is called in `ServerTestRunner::run`. Move it to a free function `health_check_as_str()` in this file.

Strip all `#[cfg(not(feature = "native"))]` annotations.

- [ ] **Step 2: Add `session` to `src/server/mod.rs`**

```rust
pub(crate) mod data_source;
pub(crate) mod protocol;
pub(crate) mod session;
pub(crate) mod utils;
pub(crate) mod uv;
```

- [ ] **Step 3: Remove moved items from `runner.rs`**

Delete `parse_version`, `HegelSession`, `ServerTestRunner`, `SUPPORTED_PROTOCOL_VERSIONS`, `HEGEL_SERVER_VERSION`, `SESSION`, and `HealthCheck::as_str()` from `runner.rs`. Also remove their `#[cfg]` annotations and now-unused imports.

- [ ] **Step 4: Update `Hegel::run()` server path in `runner.rs`**

The server path in `Hegel::run()` currently calls `ServerTestRunner` and `run_test_case` directly. After this task, `ServerTestRunner` is in `server::session`. Update the reference.

- [ ] **Step 5: Verify and test**

```bash
cargo check --features native && cargo check
cargo test --features native --no-fail-fast 2>&1 > /tmp/refactor-t3-native.txt
cargo test --no-fail-fast 2>&1 > /tmp/refactor-t3-server.txt
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Extract HegelSession and ServerTestRunner into server/session.rs"
```

---

### Task 4: Extract process management into `src/server/process.rs`

Move all server process lifecycle functions.

**Files:**
- Create: `src/server/process.rs`
- Modify: `src/server/mod.rs`
- Modify: `src/runner.rs`
- Modify: `src/server/data_source.rs` (fix temporary `crate::runner::` references)
- Modify: `src/server/session.rs` (fix temporary `crate::runner::` references)

- [ ] **Step 1: Create `src/server/process.rs`**

Move from `runner.rs`:
- Constants: `HEGEL_SERVER_COMMAND_ENV`, `HEGEL_SERVER_DIR`
- Statics: `SERVER_LOG_PATH`, `LOG_FILE_COUNTER`
- Functions: `hegel_command()`, `server_log_file()`, `wait_for_exit()`, `handle_handshake_failure()`, `startup_error_message()`, `resolve_hegel_path()`, `format_log_excerpt()`, `is_log_unindented()`, `flush_log_indent_run()`, `server_log_excerpt()`, `server_crash_message()`, `handle_channel_error()`, `__test_kill_server()`

`hegel_command()` calls `super::uv::find_uv()` and `super::utils::validate_executable()`.
`resolve_hegel_path()` calls `super::utils::validate_executable()` and `super::utils::which()`.
`__test_kill_server()` accesses `super::session::SESSION` — so `SESSION` must be `pub(super)` in session.rs.

`format_log_excerpt` and `__test_kill_server` are `pub` (re-exported from `lib.rs`). Keep them `pub`.

Strip all `#[cfg]` annotations.

- [ ] **Step 2: Add `process` to `src/server/mod.rs`**

```rust
pub(crate) mod data_source;
pub(crate) mod process;
pub(crate) mod protocol;
pub(crate) mod session;
pub(crate) mod utils;
pub(crate) mod uv;
```

- [ ] **Step 3: Remove moved items from `runner.rs`**

Delete all process management functions, constants, and statics from `runner.rs`.

- [ ] **Step 4: Fix temporary cross-references**

In `src/server/data_source.rs`: change `crate::runner::server_crash_message()` to `super::process::server_crash_message()`.

In `src/server/session.rs`: change any `crate::runner::*` references to `super::process::*` for functions that moved.

- [ ] **Step 5: Update `lib.rs` re-exports**

Change:
```rust
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use runner::__test_kill_server;
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use runner::format_log_excerpt;
```

To:
```rust
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use server::process::__test_kill_server;
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use server::process::format_log_excerpt;
```

- [ ] **Step 6: Verify and test**

```bash
cargo check --features native && cargo check
cargo test --features native --no-fail-fast 2>&1 > /tmp/refactor-t4-native.txt
cargo test --no-fail-fast 2>&1 > /tmp/refactor-t4-server.txt
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "Extract server process management into server/process.rs"
```

---

### Task 5: Extract server runner into `src/server/runner.rs` and clean up `runner.rs`

Move the panic hook, backtrace formatting, `run_test_case`, cbor helpers, and `server_run` entry point. After this task, `runner.rs` contains only shared types.

**Files:**
- Create: `src/server/runner.rs`
- Modify: `src/server/mod.rs`
- Modify: `src/runner.rs`

- [ ] **Step 1: Create `src/server/runner.rs`**

Move from `runner.rs`:
- `PANIC_HOOK_INIT` static
- `LAST_PANIC_INFO` thread-local
- `take_panic_info()`
- `format_backtrace()`
- `init_panic_hook()`
- `run_test_case()`
- `panic_message()`
- `cbor_encode()`
- `cbor_decode()`

Also create the new `server_run()` entry point by extracting the `#[cfg(not(feature = "native"))]` block from `Hegel::run()`:

```rust
use crate::antithesis::{TestLocation, is_running_in_antithesis};
use crate::runner::Settings;
use crate::test_case::TestCase;
// ... other imports

pub fn server_run<F>(
    mut test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_panic_hook();

    let runner = super::session::ServerTestRunner;
    let got_interesting = std::sync::atomic::AtomicBool::new(false);

    let result = runner.run(
        settings,
        database_key,
        &mut |backend, is_final| {
            let tc_result = run_test_case(backend, &mut test_fn, is_final);
            if matches!(&tc_result, crate::backend::TestCaseResult::Interesting { .. }) {
                got_interesting.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            tc_result
        },
    );

    let test_failed = !result.passed || got_interesting.load(std::sync::atomic::Ordering::SeqCst);

    if is_running_in_antithesis() {
        #[cfg(not(feature = "antithesis"))]
        panic!(
            "When Hegel is run inside of Antithesis, it requires the `antithesis` feature. \
            You can add it with {{ features = [\"antithesis\"] }}."
        );

        #[cfg(feature = "antithesis")]
        // nocov start
        if let Some(loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
            // nocov end
        }
    }

    if test_failed {
        let msg = result.failure_message.as_deref().unwrap_or("unknown");
        panic!("Property test failed: {}", msg);
    }
}
```

Strip all `#[cfg(not(feature = "native"))]` annotations.

- [ ] **Step 2: Add `runner` to `src/server/mod.rs`**

```rust
pub(crate) mod data_source;
pub(crate) mod process;
pub(crate) mod protocol;
pub mod runner;
pub(crate) mod session;
pub(crate) mod utils;
pub(crate) mod uv;
```

- [ ] **Step 3: Strip `runner.rs` to shared types only**

Remove from `runner.rs`:
- All server-specific imports (everything with `#[cfg(not(feature = "native"))]`)
- `PANIC_HOOK_INIT`, `LAST_PANIC_INFO`, `take_panic_info`, `format_backtrace`, `init_panic_hook`
- `run_test_case`, `panic_message`, `cbor_encode`, `cbor_decode`
- The `#[cfg(all(test, not(feature = "native")))]` embedded test module

What remains in `runner.rs`:

```rust
use crate::antithesis::TestLocation;
use crate::test_case::TestCase;

// ─── Public types ───────────────────────────────────────────────────────────

pub enum HealthCheck { ... }
impl HealthCheck { pub const fn all() -> ... }
pub enum Verbosity { ... }
pub struct Settings { ... }
impl Settings { ... }
impl Default for Settings { ... }
pub(crate) enum Database { ... }

// ─── Hegel test builder ─────────────────────────────────────────────────────

pub fn hegel<F>(...) { ... }
fn is_in_ci() -> bool { ... }
pub struct Hegel<F> { ... }
impl<F> Hegel<F> where F: FnMut(TestCase) {
    pub fn new(...) -> Self { ... }
    pub fn settings(...) -> Self { ... }
    pub fn __database_key(...) -> Self { ... }
    pub fn test_location(...) -> Self { ... }
    pub fn run(self) {
        #[cfg(feature = "native")]
        { crate::native::runner::native_run(...); }
        #[cfg(not(feature = "native"))]
        { crate::server::runner::server_run(...); }
    }
}

#[cfg(test)]
#[path = "../tests/embedded/runner_tests.rs"]
mod tests;
```

No `#[cfg]` annotations on any item — only the two branches inside `Hegel::run()` and the `#[cfg(test)]` on the test module.

- [ ] **Step 4: Update `tests/embedded/runner_tests.rs`**

Keep only the 3 shared tests:
- `test_settings_verbosity`
- `test_is_in_ci_some_expected_variant`
- `test_settings_new_in_ci_disables_database`

These tests use `super::*` to access `Settings`, `Verbosity`, `Database`, `is_in_ci` from `runner.rs`.

- [ ] **Step 5: Create `tests/embedded/server/` test files**

Move the remaining tests from the old `runner_tests.rs` into server-specific test files, split by which server submodule they test:

**`tests/embedded/server/process_tests.rs`** — tests for:
`wait_for_exit`, `startup_error_message`, `resolve_hegel_path`, `handle_handshake_failure`, `server_log_excerpt`, `server_crash_message`, `handle_channel_error`, `format_log_excerpt` (via `format_log_excerpt` which is now in process.rs), plus the `LOG_TEST_LOCK` helper and `write_server_log`/`remove_server_log` helpers.

Include from `src/server/process.rs`:
```rust
#[cfg(test)]
#[path = "../../tests/embedded/server/process_tests.rs"]
mod tests;
```

**`tests/embedded/server/session_tests.rs`** — tests for `parse_version`:
```rust
// test_parse_version_valid, test_parse_version_no_dot, etc.
```

Include from `src/server/session.rs`:
```rust
#[cfg(test)]
#[path = "../../tests/embedded/server/session_tests.rs"]
mod tests;
```

**`tests/embedded/server/data_source_tests.rs`** — the `PROTOCOL_DEBUG` test:
```rust
// test_protocol_debug_true_when_env_set
```

Include from `src/server/data_source.rs`:
```rust
#[cfg(test)]
#[path = "../../tests/embedded/server/data_source_tests.rs"]
mod tests;
```

**`tests/embedded/server/runner_tests.rs`** — the `validate_executable` test (currently in the old runner_tests.rs, it tests `crate::utils::validate_executable` which is now `crate::server::utils::validate_executable`). Actually this test uses `super::*` so it needs to be included from the file where `validate_executable` lives. Move it to process_tests.rs since `resolve_hegel_path` (in process.rs) is what uses `validate_executable`, or include it from `server/utils.rs`.

Include from `src/server/utils.rs`:
```rust
#[cfg(test)]
#[path = "../../tests/embedded/server/utils_tests.rs"]
mod tests;
```

And create `tests/embedded/server/utils_tests.rs` with just the `validate_executable` test.

- [ ] **Step 6: Verify and test**

```bash
cargo check --features native && cargo check
cargo test --features native --no-fail-fast 2>&1 > /tmp/refactor-t5-native.txt
cargo test --no-fail-fast 2>&1 > /tmp/refactor-t5-server.txt
```

- [ ] **Step 7: Verify cfg annotation count**

```bash
grep -r 'cfg.*feature.*native' src/
```

Expected output — only these lines:
- `src/lib.rs` — `#[cfg(feature = "native")]` on `mod native`
- `src/lib.rs` — `#[cfg(not(feature = "native"))]` on `mod server`
- `src/lib.rs` — `#[cfg(not(feature = "native"))]` on `pub use server::process::__test_kill_server`
- `src/lib.rs` — `#[cfg(not(feature = "native"))]` on `pub use server::process::format_log_excerpt`
- `src/runner.rs` — `#[cfg(feature = "native")]` in `Hegel::run()`
- `src/runner.rs` — `#[cfg(not(feature = "native"))]` in `Hegel::run()`

6 total, down from ~68.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "Extract server runner, split embedded tests, strip runner.rs to shared types"
```

---

### Task 6: Final cleanup and push

- [ ] **Step 1: Run full test suite both modes**

```bash
TMPDIR=$HOME/tmp cargo test --features native --no-fail-fast 2>&1 > $HOME/tmp/final-native.txt
TMPDIR=$HOME/tmp cargo test --no-fail-fast 2>&1 > $HOME/tmp/final-server.txt
```

Verify zero FAILED in both.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --features native -- -D warnings
cargo clippy -- -D warnings
```

- [ ] **Step 3: Push**

```bash
git push origin DRMacIver/native
```

- [ ] **Step 4: Delete spec and plan files**

The spec and plan docs served their purpose. Remove them to keep the repo clean:

```bash
git rm -r docs/superpowers/
git commit -m "Remove implementation spec and plan docs"
git push origin DRMacIver/native
```
