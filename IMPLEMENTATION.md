# Native Backend: Remaining Implementation Tasks

The original 11-phase plan is now complete — all 558 tests pass under `--features native`.
This document describes the remaining quality gaps between the native backend and the
server backend, and the work required to close them. All tasks use a strict TDD protocol.

## Reference Repositories (local checkouts)

- **pbtkit** (primary reference): `/tmp/pbtkit/src/pbtkit/`
- **hegel-core** (schema format): `/tmp/hegel-core/src/hegel/schema.py`
- **Hypothesis** (complex internals): `/tmp/hypothesis/hypothesis-python/src/hypothesis/`

## TDD Protocol (mandatory for all tasks)

For each feature:
1. Write a test that exercises the feature
2. Run `cargo test --features native <test_name>` — it must **fail**
3. Run `cargo test <test_name>` (no native) — it must **pass** (or be gated, see below)
4. Commit the failing test on its own
5. Implement the feature
6. Run both again to verify correct behaviour in both modes
7. Commit the implementation

---

## Priority 1 (Critical): Failure Database

The server backend stores shrunk counterexamples keyed by `database_key` and replays them
on subsequent runs, giving fast feedback for known failures. The native backend has no
persistence — every run starts from scratch. This is the most important missing feature.

### Current state

`tests/test_hegel_test.rs::test_database_persists_failing_examples` is gated with
`#[cfg(not(feature = "native"))]`. The gate is the placeholder; the feature must be built.

### TDD steps

Write a new test in `tests/test_hegel_test.rs` (using `TempRustProject::new().feature("native")`):

```rust
#[cfg(feature = "native")]
#[test]
fn native_database_persists_failing_examples() {
    let code = r#"
        use hegel::generators as gs;
        #[hegel::test]
        fn find_large(tc: hegel::TestCase) {
            let x: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(1000));
            assert!(x < 900, "x = {x}");
        }
    "#;
    let project = TempRustProject::new().feature("native");
    // First run: finds and stores the counterexample
    let first = project.main_file(code).cargo_test_output(&[]);
    assert!(first.contains("FAILED") || first.contains("assertion"),
        "expected failure on first run, got: {first}");
    // Second run: replays the stored counterexample immediately (should still fail)
    let second = project.cargo_test_output(&[]);
    assert!(second.contains("FAILED") || second.contains("assertion"),
        "expected failure on second run (replay), got: {second}");
}
```

Verify `cargo test --features native native_database_persists_failing_examples` fails before
implementing.

### Implementation approach

Study how Hypothesis stores choices:
- `hypothesis-python/src/hypothesis/database.py` — `DirectoryBasedExampleDatabase` uses a
  content-addressed directory layout. The key is a hash of the database key; the value is
  a serialized byte sequence.
- `hypothesis-python/src/hypothesis/core.py` — search for `database` to see where it is
  written (after shrinking) and read (before new generation begins).

For native:
1. After a counterexample is found and shrunk in `native_run()`, serialize the
   `Vec<ChoiceValue>` (e.g. as JSON or CBOR) to a file under
   `$HEGEL_DATABASE_DIR/<key_hash>/<choice_hash>` (matching hegel's existing convention).
2. At the start of `native_run()`, if stored choices exist for the current `database_key`,
   try replaying them first before running any random cases.
3. Add a `NativeDatabase` struct in `src/native/database.rs`.
4. Respect `HEGEL_DATABASE_DIR` env var (same as server backend).

### Un-gate the existing test

Once the feature works, remove `#[cfg(not(feature = "native"))]` from
`test_database_persists_failing_examples` and verify both modes pass.

---

## Priority 1 (Critical): Flaky Test Detection

The server backend re-runs the shrunk counterexample after shrinking. If the replay passes,
the test is reported as flaky rather than as a genuine failure. This prevents non-deterministic
code from causing false CI red.

### Current state

`tests/test_flaky_global_state.rs` is entirely gated `#[cfg(not(feature = "native"))]` with
the comment "Non-determinism detection is a server-side feature; skip in native mode". This
gate must be replaced by a working implementation.

### TDD steps

The existing tests in `test_flaky_global_state.rs` are the target tests. Before implementing,
remove the file-level gate and run:

```
cargo test --features native 2>&1 | grep -E "flaky|FAILED"
```

This should show the tests failing. Confirm they pass without `--features native`.

If the tests need structural adjustment for native (e.g. because they rely on server-specific
output format), keep the logic and adjust the output assertions.

### Implementation approach

Study Hypothesis for the flaky path:
- `hypothesis-python/src/hypothesis/core.py` — search for `Flaky`. After shrinking, Hypothesis
  re-runs the test with the shrunk choices. If the second run does not fail, it raises `Flaky`.

For native in `src/native/runner.rs`:
1. After shrinking is complete and a `counterexample` is found, run the test function again
   with those exact choices in replay mode.
2. If the replay **passes** (no panic): print a flaky warning and do not report a failure.
   The current test run should be considered unsatisfying (call it a warning, not an error).
3. If the replay **fails**: proceed with normal failure reporting.

The current code near the end of `native_run()` already has a replay step — extend it to
distinguish the two outcomes.

### Un-gate the existing tests

Once implemented, remove `#![cfg(not(feature = "native"))]` from
`tests/test_flaky_global_state.rs` and verify all tests there pass under `--features native`.

---

## Priority 2: Fix Double Panic Output

When a property test fails, the native runner currently panics twice: once from the test
body (caught by the panic hook, which prints the message) and once from
`panic!("Property test failed: {}", msg)` at the end of `native_run()`. This produces
duplicate panic output that is confusing to users.

### TDD steps

Add a test in `tests/test_output.rs` (gated `#[cfg(feature = "native")]`) that compiles
a subprocess with `.feature("native")` and asserts the failure message appears exactly once:

```rust
#[cfg(feature = "native")]
#[test]
fn native_single_panic_on_failure() {
    let code = r#"
        use hegel::generators as gs;
        #[hegel::test]
        fn always_fails(tc: hegel::TestCase) {
            let _x: bool = tc.draw(gs::booleans());
            panic!("deliberate failure");
        }
    "#;
    let output = TempRustProject::new().feature("native").main_file(code).cargo_test_output(&[]);
    let count = output.matches("deliberate failure").count();
    assert_eq!(count, 1, "expected exactly one occurrence of failure message, got:\n{output}");
}
```

Verify it fails (count > 1) before fixing.

### Implementation

In `src/native/runner.rs`, find the final `panic!("Property test failed: ...")` call.
Replace it with `std::panic::resume_unwind(payload)` where `payload` is the original
`Box<dyn Any + Send>` captured by `catch_unwind`. This re-raises the original panic
object without creating a second panic message.

Make sure the original panic payload is threaded through to the re-raise site.

---

## Priority 2: Gate Remaining Server-Only Tests

Several test files test server infrastructure that is entirely irrelevant to the native
backend. They should be gated so they are excluded when `--features native` is active,
rather than silently succeeding because the subprocess they compile doesn't use native either.

### Files to gate entirely

Add `#![cfg(not(feature = "native"))]` at the top of:

- `tests/test_bad_server_command.rs` — tests spawning of the hegel CLI binary
- `tests/test_install_errors.rs` — tests install/binary detection

### Verify

Run `cargo test --features native` and confirm the gated test binaries are no longer compiled
or run. Run `cargo test` (no native) to confirm they still run normally.

---

## Priority 3: TempRustProject Native Coverage

`TempRustProject` compiles subprocess test binaries **without** `--features native` by
default. This means most integration tests that use it test the server backend even when
the outer test suite runs with `--features native`. Most "passing" integration tests do not
actually test native code paths.

### Principle

For every observable behavior the native backend is supposed to share with the server
backend (output format, assertion messages, draw labels, etc.), there should be a
`TempRustProject::new().feature("native")` variant that exercises the native path.

### Audit approach

Go through these files and for each `TempRustProject::new()` call, assess whether the test
exercises native behavior:

- `tests/test_output.rs`
- `tests/test_stateful.rs`
- `tests/test_hegel_test.rs` (non-database tests)

For behavioral tests (output format, draw labels, failure messages), add a `_native`
variant gated `#[cfg(feature = "native")]` that calls `.feature("native")`. For server-
infrastructure tests (database key, install errors), the gate from Priority 2 is sufficient.

### TDD protocol

For each new `_native` variant:
1. Write the test.
2. Run it — if the native backend has a behavioral gap it should fail.
3. Fix the gap first (don't just make the test pass by weakening assertions).
4. Commit both test and fix together.

---

## Priority 3: Health Checks

The server backend reports `FilterTooMuch` when more than ~90% of drawn examples are
filtered by `assume()`, and `TooSlow` when test execution exceeds a time budget.
The native backend has no such checks, which means runaway assumptions silently hang.

### TDD for FilterTooMuch

Write a test that exercises heavy filtering and asserts a `FilterTooMuch`-style error:

```rust
#[cfg(feature = "native")]
#[test]
fn native_filter_too_much_detected() {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            let x: u64 = tc.draw(hegel::generators::integers::<u64>()
                .min_value(0).max_value(1_000_000));
            tc.assume(x == 42); // almost always filtered
        })
        .run();
    });
    let payload = result.unwrap_err();
    let msg = payload.downcast_ref::<String>().map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("FilterTooMuch") || msg.contains("filter") || msg.contains("assume"),
        "expected FilterTooMuch error, got: {msg}"
    );
}
```

Reference: `hypothesis-python/src/hypothesis/core.py` — search for `filter_too_much`.
The threshold is approximately 200 filtered attempts without a valid case.

Also look at the existing `tests/test_health_check.rs` to understand the expected behavior
the server already tests. For tests in that file that are currently gated out of native
mode, un-gate them one by one as each health check is implemented.

---

## Priority 4: Shrink Quality

The native shrinker is substantially simpler than Hypothesis's. The shrink quality tests
that currently pass may be accepting suboptimal minimal counterexamples.

### Where to look

The current shrink quality tests were written for the server backend and may not reflect
what a good native shrinker should achieve. Rather than treating the current tests as the
bar, look at the reference implementations:

- **pbtkit shrink tests**: `/tmp/pbtkit/tests/` — look for tests that assert a specific
  minimal counterexample (e.g., "finds 0", "finds []", "finds empty string").
- **Hypothesis shrink tests**: `/tmp/hypothesis/hypothesis-python/tests/` — especially
  `test_shrink_quality.py` and `test_minimization.py`.

Port representative shrink quality tests from these suites into `tests/test_native.rs` or
a new `tests/test_shrink_quality_native.rs`. For each:

1. Write the test.
2. Run `cargo test --features native` — if the native shrinker is suboptimal, it fails.
3. Identify which shrink pass is missing by comparing to pbtkit's shrinker.
4. Port the missing pass from pbtkit's `shrinking/` directory.

Key shrink passes to evaluate (in `/tmp/pbtkit/src/pbtkit/shrinking/`):
- `sorting.py` — sort and swap passes
- `bind_deletion.py` — bind-point deletion (high impact for collections)
- `duplication_passes.py` — duplicate value shrinking
- `advanced_integer_passes.py` — integer redistribution
- `index_passes.py` — generic index-based passes

---

## Priority 5: Unimplemented Special Schemas

Check whether the following schemas panic or produce wrong results under `--features native`:

- `email()` — RFC 5322 email addresses
- `domain()` — DNS domain names
- `url()` — URLs
- `ip_addresses()` with `v=4` or `v=6`
- `dates()`, `times()`, `datetimes()` — if exposed through the public API

For each:
1. Write a test that draws from the generator and checks basic structural constraints
   (email contains `@`, domain is non-empty, IPv4 is parseable, etc.).
2. Verify it fails under `--features native`.
3. Implement the schema handler in `src/native/schema/`.

Reference: `/tmp/hegel-core/src/hegel/schema.py` for the schema field names.

---

## Tracking Progress

After each task, run:

```bash
cargo test --features native --no-fail-fast 2>&1 > /tmp/native-test-run.txt
grep -E "FAILED|^test result:" /tmp/native-test-run.txt
```

And verify the non-native suite is not broken:

```bash
cargo test --no-fail-fast 2>&1 | grep -E "FAILED|^test result:"
```

The goal is:
1. No tests failing under `--features native` (zero `FAILED` lines).
2. All server-infrastructure tests still running and passing under `cargo test` (no native).
3. Both backends tested by `TempRustProject` variants where behavior is shared.
