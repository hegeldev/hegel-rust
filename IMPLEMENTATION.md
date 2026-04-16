# Native Backend: Remaining Implementation Tasks

The original 11-phase plan is now complete.
This document describes the remaining quality gaps between the native backend and the
server backend, and the work required to close them. All tasks use a strict TDD protocol.

## Completed (previously failing tests, now fixed)

- `test_database_key::test_database_key_replays_failure` — **DONE**: NativeDatabase
  implemented with binary serialization; replay-before-generate and save-after-shrink.
- `test_output::test_failing_test_output_with_backtrace` — **DONE**: format_backtrace_native
  filters to short-backtrace range; manual print avoids default-handler blank-line separator.
- `test_output::native_single_panic_on_failure` — **DONE**: replaced
  `panic!("Property test failed: ...")` with `resume_unwind` so only one panic message
  appears on stderr. Simple flaky detection added (replay passes → flaky warning).
- `test_health_check::native_filter_too_much_detected` — **DONE**: FilterTooMuch health
  check triggers after 200 consecutive invalid examples with no valid examples found.

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

Start with pbtkit, then cross-reference Hypothesis for the storage format:

- **pbtkit**: `/tmp/pbtkit/src/pbtkit/` — look for database or storage related modules
  (e.g. `database.py` or similar). pbtkit's implementation is simpler and a better starting
  point than Hypothesis's.
- **Hypothesis**: `hypothesis-python/src/hypothesis/database.py` — `DirectoryBasedExampleDatabase`
  uses a content-addressed directory layout. The key is a hash of the database key; the
  value is a serialized byte sequence.
- **Hypothesis**: `hypothesis-python/src/hypothesis/core.py` — search for `database` to see
  where it is written (after shrinking) and read (before new generation begins).

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

**Partial — simple form done, Hypothesis-specific form remains gated.**

The server backend re-runs the shrunk counterexample after shrinking. If the replay passes,
the test is reported as flaky rather than as a genuine failure. The native backend now
implements this: if the final replay is non-interesting (passes), it panics with
"Flaky test detected: Your test produced different outcomes...".

### What is still gated

`tests/test_flaky_global_state.rs::test_flaky_global_state` remains gated
`#[cfg(not(feature = "native"))]`. This test uses a global atomic counter to change the
`min_value` of a generator on every invocation. It expects "Your data generation is
non-deterministic" — which Hypothesis detects via its **datatree** mechanism:

The datatree tracks the transition at each choice node. If the same choice byte sequence
previously led to one value but now (due to changed global state) leads to a different
value or outcome, Hypothesis raises `FlakyStrategyDefinition` ("Your data generation
is non-deterministic").

**pbtkit does not implement the datatree mechanism** (confirmed: `test_flaky_global_state`
relies exclusively on Hypothesis internals). This test is intentionally left gated because
implementing a full datatree in the native backend would be a major undertaking and is
not required for feature parity with the *pbtkit* reference implementation.

If this specific detection is needed in future, the approach would be: during replay,
track which choices were made with which schema (min/max values), and compare them to the
original; if the schema changes at the same position, report non-determinism.

---

## Priority 2: Fix Double Panic Output — **DONE**

Replaced `panic!("Property test failed: ...")` with `resume_unwind(payload)` using a
thread-local (`LAST_PANIC_PAYLOAD`) to thread the `Box<dyn Any + Send>` payload from the
catch_unwind site to the call site. `resume_unwind` bypasses the panic hook so there is
no second "thread '...' panicked" line on stderr.

Simple flaky detection was added at the same time: if the final replay is non-interesting
(passes), the native runner panics with "Flaky test detected: ..." instead of re-raising.

Regression test: `test_output::native_single_panic_on_failure`.

---

## Priority 2: Gate Remaining Server-Only Tests

**Already done** for `test_bad_server_command.rs`, `test_install_errors.rs`, and
`test_antithesis.rs`. The remaining gating work is described below.

### test_antithesis.rs

`tests/test_antithesis.rs` is gated `#![cfg(not(feature = "native"))]` because:
1. `test_antithesis_jsonl_written_when_env_set` — the native backend does not wire
   through the Antithesis JSONL assertions.
2. `test_antithesis_panics_without_feature` — the server backend panics when
   `ANTITHESIS_OUTPUT_DIR` is set without the `antithesis` feature; the native backend
   does not have this guard.

Both gaps should be fixed as part of Antithesis native support (see Priority 5 below).

### Any remaining server-only tests

Run `cargo test --features native` after each task and investigate any newly exposed failures.
Gate tests that test server infrastructure (binary spawning, socket connections, protocol).
Leave tests that expose behavioral gaps unfixed until those gaps are implemented.

---

## Priority 3: TempRustProject Native Coverage

**Structural fix already done**: `TempRustProject::new()` now automatically adds the
`native` feature to subprocesses when the outer test suite is compiled with
`--features native`. This means all existing `TempRustProject` tests now exercise the
native code path automatically, and new failures will surface as the subprocesses actually
run native code.

The two currently failing tests (`test_database_key`, `test_output`) are the direct result
of this structural fix exposing genuine gaps.

### Remaining coverage work

For every observable behavior the native backend is supposed to share with the server
backend (output format, assertion messages, draw labels, etc.), there should be a
`TempRustProject::new()` test that now runs in native mode under `--features native`.

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

### FilterTooMuch — **DONE**

After 200 consecutive invalid (assume()-filtered) test cases with no valid examples yet
found, native_run panics with "FailedHealthCheck: FilterTooMuch". Suppressed when
`HealthCheck::FilterTooMuch` is in `suppress_health_check`.

Tests: `native_filter_too_much_detected` and `native_filter_too_much_suppressed` in
`tests/test_health_check.rs`.

### TooSlow — not yet implemented

The server backend reports TooSlow when test execution exceeds a time budget. The native
backend does not implement this. To add it:
1. Track wall-clock time per test case
2. If a test case exceeds a threshold (e.g., 200ms), panic with FailedHealthCheck: TooSlow
3. Suppress when HealthCheck::TooSlow is in suppress_health_check

The existing `test_health_check.rs` tests use `suppress_health_check = [HealthCheck::TooSlow]`
which currently work because native silently ignores TooSlow. Un-gating TooSlow detection
would require those tests to actually be fast enough.

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

### TDD approach

For each schema, **port the corresponding Hypothesis tests first** — do not write new tests
from scratch. The Hypothesis test suite has extensive property-based tests for each of these
generators that cover edge cases and structural invariants:

- `hypothesis-python/tests/` — search for `test_email`, `test_urls`, `test_ip`, `test_dates`,
  etc. Port the ones that assert structural properties (valid format, parseable, within bounds)
  into a new `tests/test_special_schemas_native.rs`.
- Focus on tests that check _what the generator can produce_ rather than Hypothesis-internal
  plumbing.

Then for each ported test:
1. Run it — it must fail under `--features native`.
2. Verify it passes without `--features native`.
3. Implement the schema handler in `src/native/schema/`.
4. Confirm both modes pass.

Reference: `/tmp/hegel-core/src/hegel/schema.py` for the schema field names.

---

## Priority 5: Antithesis Native Support

Two tests in `tests/test_antithesis.rs` are gated because the native backend does not
support the Antithesis SDK integration:

1. **JSONL output**: When compiled with `--features antithesis`, the native backend must
   write assertion declarations and evaluations to `$ANTITHESIS_OUTPUT_DIR/sdk.jsonl`.
   Study how the server path triggers these writes (likely in `src/antithesis.rs` or via
   the `HealthCheck` mechanism) and wire the same calls into `native_run()`.

2. **Guard without feature**: When `ANTITHESIS_OUTPUT_DIR` is set but the `antithesis`
   feature is not compiled in, the backend should panic with an informative error. Add
   this check to the native startup path (currently only the server path has it).

### TDD steps

Remove `#![cfg(not(feature = "native"))]` from `tests/test_antithesis.rs`. Run
`cargo test --features native` — both tests should fail. Implement, then remove the gate.

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
