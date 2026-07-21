# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code), Codex 5.5, and other coding agents when working with code in this repository. The legacy `.claude` path is a symlink to `.agents`, and `.claude/CLAUDE.md` resolves back to this file so Claude and Codex share the same instructions.

## Overview

This repository is the Rust implementation of Hegel, a universal property-based testing framework. It contains two crates: the native engine (a port of Hypothesis's conjecture engine) lives in the `hegel-c` workspace member and is built as the `libhegel` C-ABI library, and the `hegeltest` frontend (the root crate) drives that engine through the same C ABI every other language binding uses. Everything runs in-process — there is no external server or Python dependency.

## Build & Test Commands

```bash
just check                          # run full CI checks (lint + tests + all-features tests)
just test                           # run tests
just lint                           # run clippy, rustfmt --check, and the repo lint scripts
just format                         # format
just docs                           # build and open docs
just check-coverage                 # check coverage (requires cargo-llvm-cov + llvm-tools-preview)
just c-test                         # hegel-c smoke tests + example C programs
just miri                           # fast core of the suite under Miri
cargo test test_name                # run a single test
```

MSRV is 1.86 (enforced in CI and Cargo.toml). If you bump it, also bump `ci.yml`, `hegel-macros/Cargo.toml`, and `hegel-c/Cargo.toml`.

## Workspace Structure

### `hegeltest` (root crate) — the Rust frontend

- `src/lib.rs` — Public API surface: `hegel()`, the `Hegel` builder, `TestCase`, the `Generator` trait, and the proc-macro re-exports (`#[hegel::test]`, `#[hegel::main]`, `#[derive(DefaultGenerator)]`, `#[composite]`, `#[state_machine]`, `#[reproduce_failure]`, …)
- `src/ffi.rs` — The libhegel C-ABI boundary: the only module that touches the raw `hegel_*` functions; the rest of the frontend works against its safe wrappers (`SettingsHandle`, `RunHandle`, `CTestCase`, `RunResult`)
- `src/run_lifecycle.rs` — Cross-cutting per-test-case lifecycle: panic hook, `catch_unwind` wrapping, translating panics into `TestCaseResult`, and the final re-raise
- `src/backend.rs` — The result types the lifecycle speaks (`TestCaseResult`, `Failure`)
- `src/test_case.rs` — `TestCase` (the handle test bodies draw from) and its thread-local state, the `Collection` helper, and the span `labels` module
- `src/runner.rs` — `Hegel` builder plus `Settings`, `HealthCheck`, `Phase`, `Mode`, `Backend`, `Verbosity`
- `src/cli.rs` — CLI argument parsing for standalone `#[hegel::main]` binaries
- `src/generators/` — All first-party generator implementations (the `Generator` trait lives in `generators.rs`)
- `src/extras/` — Feature-gated third-party integrations (`chrono`, `jiff`, `serde_json`, `rand`)
- `src/stateful.rs` — Stateful (model-based) testing via `#[state_machine]`
- `src/explicit_test_case.rs` — Explicit test-case support (`#[explicit_test_case]`)
- `src/control.rs` — Control-flow unwind payloads (`AssumeFailed`, `StopTest`) and their handling
- `src/antithesis.rs` — Antithesis integration
- `hegel-macros/` — Proc-macro crate (sub-crate with its own `Cargo.toml`)

### `hegel-c` — the engine, built as `libhegel`

- `src/lib.rs` — The exported `hegel_*` C functions: settings, run lifecycle, test-case handles (including clones), draws, spans, collections, pools, state machines, targeting, results/failures. The checked-in header `hegel-c/include/hegel.h` is generated from this file by cbindgen (`just c-header`)
- `src/backend.rs` — The `DataSource` trait the engine implements and the C ABI drives
- `src/native/` — The engine proper: `core/` (choice sequence, test-case state, shrink ordering), `draws/` (the typed draw implementations: float specs, string generators, regex, internet, date/time/uuid/ip), `shrinker/`, `test_runner.rs` (owns a run: database replay, generation, targeting, shrinking, final replay), plus the failure database, data tree / novel-prefix generation, RNG, regex generation (`re/`), interval sets + Unicode tables, and blob encoding
- `src/embed.rs` — Low-level embedding entry point for driving the engine natively from Rust
- Released as `libhegel-<goos>-<goarch>.<ext>` assets on each GitHub release; the source is published to crates.io as `hegeltest-c` mostly to reserve the name

### Feature Flags (root crate)

- **`rand`**, **`chrono`**, **`jiff`**, **`serde_json`**, **`serde_json_raw_value`**: gate the corresponding `extras::` generator modules
- **`__bench`**: internal, re-exports engine internals for `benches/`; not part of the public API

The Antithesis integration (`src/antithesis.rs`) is not feature-gated: it is always compiled and activates at runtime when `ANTITHESIS_OUTPUT_DIR` is set.

## Architecture

### How It Works

The engine owns a test run. `hegel_run_start` starts an engine worker; the frontend pulls test cases off it with `hegel_next_test_case`, runs the user's test body against each test-case handle, and reports each outcome back with `hegel_mark_complete`. Database replay, generation, targeting, shrinking, and the final replay all happen inside the engine (`hegel-c/src/native/test_runner.rs`); the frontend's `run_lifecycle::drive` wires the pull loop to the user's test function, catches panics, and reads the aggregate result — including reproduce blobs — off the finished run.

### The C ABI

The frontend and engine communicate exclusively through the `hegel_*` functions wrapped by `src/ffi.rs`. Draws are typed: `hegel_generate_integer` / `hegel_generate_integer_big`, `hegel_generate_float`, `hegel_generate_boolean`, `hegel_generate_bytes`, `hegel_generate_string` (through an opaque `hegel_string_generator_t` handle built by typed constructors for text/regex/email/url/domain), and structured `hegel_generate_date`/`_time`/`_datetime`/`_uuid`/`_ipv4`/`_ipv6`. Alongside the draws: `hegel_start_span`/`hegel_stop_span` group related draws for the shrinker, `hegel_new_collection`/`hegel_collection_more`/`hegel_collection_reject` drive dynamically-sized collections, `hegel_target` records targeting scores, and `hegel_mark_complete` reports the outcome (VALID, INVALID, OVERRUN, or INTERESTING). Failed calls report diagnostics on an explicit `HegelContext` handle rather than thread-local state.

### Generator Trait

Generators implement `Generator<T>` (`src/generators/generators.rs`) with one required method, `do_draw(&self, tc: &TestCase) -> T`. Leaf generators call the typed draw methods on `TestCase` (`generate_integer_i64`, `generate_float`, `generate_string`, …), which wrap the C ABI; composite generators (tuples, collections, `one_of`, `map`/`filter`/`flat_map`) compose other generators' `do_draw` inside labeled spans. String-shaped generators cache a validated `ffi::StringGenerator` handle in a `OnceLock` so the alphabet/pattern work happens once, not per draw.

### Span System

Spans (`start_span`/`stop_span`) group related generation calls so the shrinker can shrink effectively. Labels in `test_case::labels` identify span types (LIST, TUPLE, ONE_OF, FILTER, etc.); the label space is open — any stable `u64` works. The engine also emits its own spans around every draw (see `hegel_label_t` values 17–30): same-label spans are what the engine's mutation machinery duplicates to propose repeated values, so leaf draws each get a kind-specific span.

### Collections

Engine-managed collections use the `new_collection`/`collection_more`/`collection_reject` calls: the engine decides how many elements to produce and the client pulls them one at a time. The `Collection` struct in `src/test_case.rs` handles dynamic sizing via the `more()` protocol with lazy initialization.

## Key Patterns

### Adding a New Generator

Follow the skills: **new-generator** for the generator itself (struct, builder methods, `Generator` impl, wiring, rustdoc, required tests), **new-default-generator** to wire up `gs::default::<T>()` / `#[derive(DefaultGenerator)]` support, and **add-library-support** for a whole third-party crate integration under `src/extras/`.

### Derive Macro

`#[derive(DefaultGenerator)]` (in `hegel-macros/`) creates a `<Type>Generator` struct with:
- `new()`: Uses `DefaultGenerator` for all fields
- `<field>(gen)`: Builder methods to customize field generators

For enums, it also creates `<Enum><Variant>Generator` for each data variant. Implementation is split across `struct_gen.rs`, `enum_gen.rs`, and `utils.rs`.

### Testing Conventions

- All tests go in `tests/`, never inline in source files. Tests that don't need access to private functions go directly in `tests/` as integration tests. Tests that need access to private functions go in `tests/embedded/`, mirroring the `src/` directory structure. This applies to both crates (e.g. `src/ffi.rs` → `tests/embedded/ffi_tests.rs`; `hegel-c/src/native/core/state.rs` → `hegel-c/tests/embedded/native/state_tests.rs`). Embedded tests are included as child modules of their source file via `#[cfg(test)] #[path = "..."] mod tests;`, which gives them access to private items through `use super::*`. This keeps test code out of source files while preserving access to internals that Rust would otherwise forbid.
- Always import generators as `use hegel::generators as gs;` (or `use hegel::generators::{self as gs, Generator};` when the `Generator` trait is needed). Use `gs::` in all generator calls, e.g. `gs::booleans()`, `gs::integers::<i32>()`. This applies to code inside string literals (e.g. `TempRustProject` snippets) as well.
- When a test needs a throwaway generator, prefer `gs::booleans()` as the simplest option (unless the test needs a larger value space).
- In test code, prefer `.unwrap()` over `.expect("static message")`. A static expect message rarely adds information beyond what the panic already provides (error type + source location). Only use `.expect()` when the message includes a formatted value that aids debugging (e.g., `.expect(&format!("failed to open {}", path))`).
- When a return value isn't used, don't bind it to `_` — just call the function as a bare statement. Only use `let _ =` when needed to suppress a `#[must_use]` warning.

### Code Coverage

This project enforces 100% line coverage for new code. You may not add `// nocov` annotations without explicit human permission. See the `coverage` skill for full details on the coverage philosophy, ratchet mechanism, and how to make code testable.

**CRITICAL: You MUST NOT increase the numbers in `.github/coverage-ratchet.json` without first asking for and then receiving explicit human permission to do so.**


### Comments

Do not write comments other than nocov and other comments that have specific functions. Before committing, check if you are about to stage comments you have written, and if you are then delete them before committing.
