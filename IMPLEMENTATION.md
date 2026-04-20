# Native Backend: Remaining Implementation Tasks

This document tracks what remains to reach full feature parity between the
native backend and the server backend. Completed work is summarised briefly;
remaining tasks have full TDD instructions.

## Completed

All of the following are implemented, tested, and passing in both modes:

- **Core test loop** — random generation, shrinking (10 passes), final replay.
- **All schema interpreters** — integer, boolean, float, string, binary, list,
  dict, tuple, one_of, sampled_from, regex, constant, null.
- **Special schemas** — date, time, datetime, ipv4, ipv6, domain, email, url.
- **Database persistence** — `NativeDatabase` with binary serialization; replay
  before generation, save after shrink. `test_database_persists_failing_examples`
  runs in both modes.
- **Backtrace filtering** — `format_backtrace_native` filters to short range
  and renumbers frames.
- **Single panic output** — `resume_unwind` avoids double panic message.
- **Flaky detection (simple form)** — final replay passes → "Flaky test
  detected" panic.
- **FilterTooMuch health check** — 200 consecutive invalid cases with no valid
  examples triggers panic (suppressible).
- **TooSlow health check** — single test case >200ms triggers panic
  (suppressible).
- **Span mutation** — span grouping, donor swap, 5 attempts per valid test case.
- **Variable pools** — `new_pool`, `pool_add`, `pool_generate`, `pool_consume`
  commands for stateful testing.
- **Antithesis integration** — JSONL assertion output when
  `ANTITHESIS_OUTPUT_DIR` is set; panic without `antithesis` feature.
  `test_antithesis.rs` runs in both modes.
- **TempRustProject auto-inheritance** — subprocesses automatically get the
  `native` feature when the outer test suite uses `--features native`.
- **Shrink quality** — all 74 shrink quality tests pass.
- **Server module refactor** — server-specific code extracted into `src/server/`.

## Remaining Tasks

### 1. TestCasesTooLarge / LargeInitialTestCase health checks

The `HealthCheck` enum exposes four variants, but the native backend only
implements `FilterTooMuch` and `TooSlow`. The remaining two are:

- **TestCasesTooLarge** — the server backend reports this when the total size of
  generated data exceeds a threshold. The native backend silently accepts
  arbitrarily large test cases.
- **LargeInitialTestCase** — the server backend reports this when the smallest
  natural input is very large (hinting at a generator problem).

These are low-priority because they catch generator-configuration issues rather
than correctness bugs, and because the threshold semantics differ between
pbtkit and Hypothesis.

#### TDD approach

1. Write tests in `tests/test_health_check.rs` that construct generators
   producing very large outputs and verify the health check fires.
2. In `native_run()`, track the total number of choice nodes per test case
   and compare against a threshold (Hypothesis uses 200 * `max_examples`).
3. For `LargeInitialTestCase`, check the size of the first valid test case
   against a threshold (Hypothesis uses 200).

### 2. Flaky detection via datatree (Hypothesis-specific)

`tests/test_flaky_global_state.rs::test_flaky_global_state` is gated with
`#[cfg(not(feature = "native"))]`. This test uses a global atomic counter to
change a generator's `min_value` on each invocation. The server backend detects
this via Hypothesis's datatree mechanism, which tracks per-choice-node
transitions.

**pbtkit does not implement the datatree** — this is intentionally out of
scope for parity with pbtkit. To implement it:

- During replay, track which choices were made with which schema parameters
  (min/max values). If the schema changes at the same position compared to the
  original run, report "Your data generation is non-deterministic."
- This is a significant undertaking for marginal benefit.

### 3. Float lex ordering unit tests

`float_to_index` and `index_to_float` in `src/native/core/float_index.rs` are
critical for shrinking but have no direct unit tests. They are exercised
indirectly through shrink quality tests.

#### TDD approach

Add `tests/embedded/native/float_index_tests.rs`:

- Round-trip: `index_to_float(float_to_index(v)) == v` for representative
  values (0.0, 1.0, 0.5, 1.5, 2.0, f64::MAX, f64::INFINITY, subnormals).
- Ordering: `float_to_index(0.0) < float_to_index(1.0) < float_to_index(2.0)`.
- Integer floats map to themselves: `float_to_index(n as f64) == n` for
  small `n`.
- `encode_exponent` / `decode_exponent` round-trip.

### 4. Richer special schema generators

The native special schema generators (email, domain, url, date, etc.) produce
valid but limited outputs:

- Dates always use day 1–28 (avoids month-length logic but misses 29/30/31).
- Emails are lowercase-letters-only (misses dots, plus-addressing, etc.).
- URLs have no query parameters, fragments, or percent-encoding.
- Domains are ASCII-only (no IDN).

This is acceptable for most property tests but means the native backend finds
fewer edge cases than the server backend for these specific generators.

Improvement is low priority — the generators work correctly for their contracted
output space, and users who need richer generation can use `from_regex()` or
custom generators.

### 5. Coverage audit for native code

The project enforces 100% line coverage for new code. The native module
(~5,300 lines) needs a coverage audit to verify:

- No dead code paths.
- All error branches are exercised.
- Coverage ratchet values in `.github/coverage-ratchet.json` account for the
  native feature.

Run `python3 scripts/check-coverage.py --native` and investigate any uncovered
lines.

## Reference Repositories

- **Hypothesis** (behavioural ground truth): `/tmp/hypothesis/hypothesis-python/src/hypothesis/`
- **pbtkit** (cleaner reference implementation of the same ideas — read for structure, defer to Hypothesis on conflicts): `/tmp/pbtkit/src/pbtkit/`
- **hegel-core** (schema format): `/tmp/hegel-core/src/hegel/schema.py`
