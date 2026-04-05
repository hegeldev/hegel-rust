# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This is the Rust library for Hegel, a universal property-based testing framework. The library communicates with a Python server (powered by Hypothesis) via Unix sockets to generate test data.

## Build & Test Commands

```bash
just check                          # Full CI: check-format + lint + check-test + check-docs
just test                           # cargo test --all-features
just lint                           # cargo clippy --all-features --tests -- -D warnings
just format                         # cargo fmt
just docs                           # cargo doc --open --all-features --no-deps
just check-conformance              # pytest conformance tests (requires Python environment)
just check-coverage                 # cargo llvm-cov --fail-under-lines 30 (requires cargo-llvm-cov + llvm-tools-preview)
cargo test test_name                # Run single test
```

MSRV is 1.86 (enforced in CI and Cargo.toml). If you bump it, also bump `ci.yml` and `hegel-derive/Cargo.toml`.

## Crate Structure

- `src/lib.rs` — Public API: `hegel()`, `Hegel` builder, `draw()`, `assume()`, `note()`
- `src/protocol.rs` — Binary protocol: packet encoding/decoding, stream multiplexing
- `src/cbor_helpers.rs` — Macros and helpers for `ciborium::Value` (`cbor_map!`, `cbor_array!`, `map_get`, etc.)
- `src/runner.rs` — Spawns hegel CLI, manages socket server
- `src/generators/` — All generator implementations (`mod.rs` has the `Generate` trait + `TestCaseData`)
- `hegel-derive/` — Proc macro crate for `#[derive(Generate)]` (sub-crate with its own `Cargo.toml`)
- `build.rs` — Locates `hegel` binary on PATH, exports `HEGEL_BINARY_PATH` env var (falls back to `"hegel"`)

### Feature Flags

- **`rand`**: Enables `generators::randoms()` for generating `rand::RngCore` implementations

## Architecture

### How It Works

The library creates a Unix socket path and spawns the `hegel` CLI as a subprocess. The server binds to the socket and listens for the client to connect. A single persistent connection is maintained for the program run, supporting multiple test executions.

### Protocol

CBOR-encoded binary protocol over multiplexed streams. For each test:
1. Client sends `run_test` request on control stream (stream 0)
2. Server sends `test_case` events with stream IDs for each test case
3. Client runs the test function, sending `generate`/`start_span`/`stop_span` requests on the test stream
4. Client sends `mark_complete` with status (VALID, INVALID, or INTERESTING)
5. After all test cases, server sends `test_done` with results

### Generate Trait and BasicGenerator

Generators implement `Generate<T>`:
- `do_draw(&self, data: &TestCaseData) -> T` — Produce a value
- `as_basic()` — Returns `Option<BasicGenerator<T>>` with a CBOR schema + parse function

When `as_basic()` returns `Some`, generation uses a single socket request with the schema. When `None` (after `map`/`filter` on non-basic generators, or `flat_map`), it falls back to multiple requests wrapped in spans for shrinking.

Key insight: `map()` on a `BasicGenerator` preserves the schema by composing the transform function, rather than losing it. This is the central optimization.

### Thread-Local State

`TestCaseData` is stored in thread-local `TEST_CASE_DATA` and holds the socket connection, stream, and span depth. `IS_LAST_RUN` tracks whether this is the final replay for counterexample output.

### Span System

Spans (`start_span`/`stop_span`) group related generation calls so Hypothesis can shrink effectively. Labels in `generators::labels` identify span types (LIST, TUPLE, ONE_OF, FILTER, etc.).

### Collections

Server-managed collections use `new_collection`/`collection_more`/`collection_reject` protocol commands. The `Collection` struct in `collections.rs` handles dynamic sizing via the `more()` protocol with lazy initialization.

## Key Patterns

### Adding a New Generator

1. Create a builder struct with configuration fields
2. Implement `Generate<T>` with `do_draw()` and optionally `as_basic()`
3. Export a factory function from `generators/mod.rs`
4. If the generated type should work with `#[derive(Generate)]`, implement `DefaultGenerator` in `generators/default.rs`

### Derive Macro

`#[derive(Generate)]` (in `hegel-derive/`) creates a `<Type>Generator` struct with:
- `new()`: Uses `DefaultGenerator` for all fields
- `with_<field>(gen)`: Builder methods to customize field generators

For enums, it also creates `<Enum><Variant>Generator` for each data variant. Implementation is split across `struct_gen.rs`, `enum_gen.rs`, and `utils.rs`.

### Testing Conventions

- All tests go in `tests/`, never inline in source files. Tests that don't need access to private functions go directly in `tests/` as integration tests. Tests that need access to private functions go in `tests/embedded/`, mirroring the `src/` directory structure (e.g. `src/protocol/packet.rs` → `tests/embedded/protocol/packet_tests.rs`). Embedded tests are included as child modules of their source file via `#[cfg(test)] #[path = "..."] mod tests;`, which gives them access to private items through `use super::*`. This keeps test code out of source files while preserving access to internals that Rust would otherwise forbid.
- When a test needs a throwaway generator, prefer `generators::booleans()` as the simplest option (unless the test needs a larger value space).
- In test code, prefer `.unwrap()` over `.expect("static message")`. A static expect message rarely adds information beyond what the panic already provides (error type + source location). Only use `.expect()` when the message includes a formatted value that aids debugging (e.g., `.expect(&format!("failed to open {}", path))`).

### Code Coverage

This project enforces 100% line coverage for new code. You may not add `// nocov` annotations without explicit human permission. See the `coverage` skill for full details on the coverage philosophy, ratchet mechanism, and how to make code testable.

**CRITICAL: You MUST NOT increase the numbers in `.github/coverage-ratchet.json` without first asking for and then receiving explicit human permission to do so.**

### Conformance Tests

Located in `tests/conformance/`. Rust test binaries in `tests/conformance/rust/src/bin/` are invoked by a Python test runner (`tests/conformance/test_conformance.py`) that validates generators produce values matching their declared constraints.

### Creating Pull Requests

Run the `self-review` skill before creating PRs. Ensure a `RELEASE.md` exists if source code changed (see `RELEASE-sample.md` for format and style guidance).

# Orrery

This worktree is managed by Orrery (cell id: 9ffbbdbd-15e6-4a3b-9742-6db81f72b809).

## When asked to push and/or create a PR:

1. Choose a descriptive branch name based on your changes (e.g. `fix-timeout-handling`, `add-retry-logic`)
2. Rename the current branch: `git branch -m <new-name>`
3. Notify Orrery about the branch rename:
   curl -s -X POST http://localhost:8000/hooks/cells/9ffbbdbd-15e6-4a3b-9742-6db81f72b809/branch-updated -H 'Content-Type: application/json' -d '{"branch": "<new-name>"}'
4. Push: `git push -u origin <new-name>`
5. Create the PR: `gh pr create --title "..." --body "..."`
   Your PR description should be only a summary of the changes, without a `## Summary`
   header. Do not include a "Test plan" section. Wrap the description in a details tag:
   ```
   <details><summary>Claude-written description</summary>

   {description}

   </details>
   ```
6. Notify Orrery about the PR:
   curl -s -X POST http://localhost:8000/hooks/cells/9ffbbdbd-15e6-4a3b-9742-6db81f72b809/pr-created -H 'Content-Type: application/json' -d '{"pr_url": "<url>", "pr_number": <number>}'

Always notify Orrery after renaming the branch and after creating the PR.

## Creating new cells
If the user asks you to create a new cell, create one via:
  curl -s -X POST http://localhost:8000/hooks/create-cell -H 'Content-Type: application/json' -d '{"repo": "<repo_id>"}'
This returns JSON with a `url` field — share that URL with the user so they can navigate to the new cell.
