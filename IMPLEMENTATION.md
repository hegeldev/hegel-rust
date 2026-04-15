# Native Backend Implementation Plan

## Context

The `native` feature flag enables a pbtkit-style test runner that replaces the Python server.
Currently 249 tests fail in native mode (309 pass). This document is the execution plan for
making all tests pass, one failing test at a time.

## Reference Repositories (local checkouts)

- **pbtkit** (primary reference): `/tmp/pbtkit/src/pbtkit/`
- **hegel-core** (schema format): `/tmp/hegel-core/src/hegel/schema.py`
- **Hypothesis** (complex internals): `/tmp/hypothesis/hypothesis-python/src/hypothesis/`

Prefer pbtkit as it is simpler and more modularised. Use Hypothesis only when pbtkit
doesn't cover the needed functionality (e.g. for `from_regex`).

## Execution Strategy (ralph-loop)

Each iteration of the loop:

1. **Run the tests**: `cargo test --features native --no-fail-fast 2>&1 > /tmp/native-test-run.txt`
2. **If all tests pass, stop.**
3. **Pick one failing test** — aim for the easiest / most impactful (unblocks the most other tests).
4. **Study the failure** — determine which `todo!()` or missing feature causes it.
5. **Implement the fix** by studying pbtkit (or Hypothesis if necessary).
6. **Verify** the fix passes and no other tests regressed: `cargo test --features native --no-fail-fast`
7. **Commit** the work.
8. **Stop** (the next loop iteration picks up from step 1).

## Tests to Skip in Native Mode

These tests are entirely about server management and should be skipped (gated with
`#[cfg(not(feature = "native"))]` or similar):

- `tests/test_bad_server_command.rs` — all 7 tests (server binary validation)
- `tests/test_install_errors.rs` — all 2 tests (uv installation)
- `tests/embedded/runner_tests.rs` — already gated (server runner internals)
- `tests/embedded/uv_tests.rs` — uv binary management
- `tests/embedded/protocol/` — all protocol tests (connection, packet, stream)
- `tests/test_flaky_global_state.rs` — tests server-side flaky detection
- `tests/test_database_key.rs` — tests server-side database replay
- `tests/test_hegel_test.rs::test_database_persists_failing_examples` — server database

Additionally:
- `tests/test_health_check.rs` — health checks are a server-side feature. Suppress tests
  that depend on server health check behaviour. Individual tests here that don't depend on
  server features (like `test_does_not_hang_on_assume_false` from test_hang.rs) need native
  fixes instead.

No other tests should be skipped. If a test exercises a generator feature, implement the
feature natively.

## Failure Categories and Implementation Order

### Phase 1: Fix integer edge-case discovery (unblocks ~10 tests)

**Problem**: `find_any` tests like `test_i32`, `test_u32`, `test_i64`, `test_u64` fail because
uniform random generation over the full range rarely hits specific boundary regions (e.g. values
above 2^31 for u32). The server uses Hypothesis's edge-case boosting.

**Fix**: Port pbtkit's `edge_case_boosting.py`. When drawing integers, with probability
`BOUNDARY_PROBABILITY = 0.01` per special value, return a boundary value (min, max, 0).

**Reference**: `/tmp/pbtkit/src/pbtkit/edge_case_boosting.py` and the boosting logic in
`/tmp/pbtkit/src/pbtkit/core.py` lines 395-413.

**Files**: `src/native/core.rs` (modify `draw_integer`)

**Also fix**: `cbor_to_i128` must handle CBOR BigNum tags (Tag 2 = positive bignum,
Tag 3 = negative bignum) for i128/u128 support.

**Files**: `src/native/schema.rs` (modify `cbor_to_i128`)

### Phase 2: Implement `float` schema (unblocks ~50 tests)

**Problem**: All float tests hit `todo!("float schema")`.

**Fix**: Port pbtkit's `floats.py`. Key concepts:
- `FloatChoice` with IEEE 754 bit-level generation
- Bounded floats: uniform in range
- Unbounded: random bit patterns covering full f64/f32 space
- Special values: NaN, +/-Infinity, +/-0.0
- Float shrinking: sign flip, magnitude reduction, exponent reduction, mantissa reduction

**Reference**: `/tmp/pbtkit/src/pbtkit/floats.py`

**Schema fields** (from hegel-core `schema.py`):
- `width`: 32 or 64
- `min_value`, `max_value`: optional bounds
- `allow_nan`, `allow_infinity`: bool flags
- `exclude_min`, `exclude_max`: bool flags

**Files**: `src/native/schema.rs` (add `interpret_float`), `src/native/core.rs` (add
`FloatChoice`, `draw_float`), `src/native/shrinker.rs` (add float shrink passes)

### Phase 3: Implement `list` schema (unblocks ~80 tests)

**Problem**: All collection tests (vecs, hashsets, flatmap, compose with vecs) hit
`todo!("list schema")`. This is the single biggest blocker.

**Fix**: Port pbtkit's `collections.py` `many` class. The `list` schema needs:
1. Determine length using geometric distribution (like pbtkit's `many.more()`)
2. Generate each element recursively via `interpret_schema`
3. Handle `unique: true` by rejecting duplicates
4. Respect `min_size` / `max_size`

**Reference**: `/tmp/pbtkit/src/pbtkit/collections.py` (the `many` class)

**Schema fields** (from hegel-core `schema.py`):
- `elements`: sub-schema for each element
- `min_size`, `max_size`: size bounds
- `unique`: boolean

**Also implement**: The `new_collection` / `collection_more` / `collection_reject` protocol
commands, used by generators whose `as_basic()` returns `None` (e.g. after `map`/`filter`).
These use the same `many` logic.

**Files**: `src/native/schema.rs` (add `interpret_list`, collection command handling),
`src/native/core.rs` (may need `many` equivalent)

### Phase 4: Implement `dict` schema (unblocks ~15 tests)

**Problem**: HashMap tests hit `todo!("dict schema")`.

**Fix**: Similar to list but generates key-value pairs. Reject duplicate keys.

**Schema fields**: `keys`, `values` (sub-schemas), `min_size`, `max_size`

**Reference**: `/tmp/pbtkit/src/pbtkit/generators.py` `dictionaries` function,
`/tmp/hegel-core/src/hegel/schema.py` lines 106-112

**Files**: `src/native/schema.rs` (add `interpret_dict`)

### Phase 5: Implement `string` schema (unblocks ~40 tests)

**Problem**: All text/character tests hit `todo!("string schema")`.

**Fix**: Port pbtkit's `text.py`. Key concepts:
- `StringChoice` with codepoint range and length bounds
- Alphabet construction from codepoint ranges, categories, include/exclude lists
- Surrogate filtering (0xD800-0xDFFF excluded for Rust)
- hegel-core's HEGEL_STRING_TAG (CBOR tag 91) encoding for the response

**Reference**: `/tmp/pbtkit/src/pbtkit/text.py`

**Schema fields** (from hegel-core `schema.py`):
- `min_size`, `max_size`
- `codec` (ascii, utf-8, latin-1)
- `min_codepoint`, `max_codepoint`
- `categories`, `exclude_categories` (Unicode general categories)
- `include_characters`, `exclude_characters`

**Unicode categories**: Need a lookup table or crate (e.g. `unicode-general-category`) to
filter codepoints by Unicode general category (L, N, P, S, Z, C, and sub-categories like
Lu, Ll, Nd, etc.).

**Files**: `src/native/schema.rs` (add `interpret_string`), `src/native/core.rs` (add
`StringChoice`, `draw_string`), possibly new dependency for Unicode categories

### Phase 6: Implement `binary` schema (unblocks ~5 tests)

**Problem**: Binary/bytes tests hit `todo!("binary schema")`.

**Fix**: Port pbtkit's `bytes.py`. Generate random byte sequences of random length.

**Reference**: `/tmp/pbtkit/src/pbtkit/bytes.py`

**Schema fields**: `min_size`, `max_size`

**Files**: `src/native/schema.rs` (add `interpret_binary`), `src/native/core.rs` (add
`BytesChoice`, `draw_bytes`)

### Phase 7: Implement `regex` schema (unblocks ~2 tests)

**Problem**: `from_regex` tests hit `todo!("regex schema")`.

**Fix**: This is the most complex schema. Requires:
1. **Port Python's `re._parser`** to Rust using a suitable parser library. The parser
   converts regex strings into an AST.
   - Reference: `/tmp/hypothesis/hypothesis-python/src/hypothesis/strategies/_internal/regex.py`
   - Also: `https://github.com/python/cpython/blob/main/Lib/re/_parser.py`
2. **Convert the AST to generators** following Hypothesis's `regex.py` strategy, which
   maps regex AST nodes to generators (character classes, repetitions, alternations, etc.)
3. This can be implemented entirely in terms of existing native draw operations (draw_integer
   for character selection, weighted for optionals, etc.)

**Alternative**: Use the `regex-syntax` crate (already a transitive dependency via `regex`)
to parse regex into an AST, then walk the AST to generate matching strings.

**Files**: `src/native/regex.rs` (new), `src/native/schema.rs` (add `interpret_regex`)

### Phase 8: Implement remaining simple schemas (unblocks ~5 tests)

These schemas are used by derived generators with string/special fields:

- `email` — generate RFC 5322 email addresses (can use string generators + fixed templates)
- `url` — generate URLs (similar template approach)
- `domain` — generate domain names with max_length constraint
- `ipv4` / `ipv6` — generate IP addresses as strings
- `date` / `time` / `datetime` — generate ISO 8601 date/time strings

**Reference**: `/tmp/hegel-core/src/hegel/schema.py` shows these all delegate to Hypothesis
strategies. For native mode, implement simple generators using draw_integer for components.

**Files**: `src/native/schema.rs` (add handlers for each)

### Phase 9: Implement stateful testing support (unblocks ~3 tests)

**Problem**: State machine tests use `new_pool` / `pool_add` / `pool_generate` / `pool_consume`
protocol commands.

**Fix**: Implement variable pools in the native backend. A pool is a list of variable IDs
that can be added to, drawn from (via draw_integer), and consumed.

**Reference**: `/tmp/hegel-core/src/hegel/server.py` `Variables` class (lines 93-138)

**Files**: `src/native/schema.rs` (add pool command handling)

### Phase 10: Fix test infrastructure issues

Several tests need native-specific fixes unrelated to schemas:

- **`test_does_not_hang_on_assume_false`** (test_hang.rs): The native runner panics with
  "Unsatisfiable" when all test cases are invalid. The test expects this to be a health
  check failure, not a hard panic. Need to match the server's health check behavior or
  adjust the test expectation.

- **`test_flaky_global_state`**: Uses global state to detect flakiness — a server-side
  feature. Should be skipped in native mode.

- **`test_database_persists_failing_examples`**: Tests failure database replay — a server
  feature. Should be skipped in native mode.

- **`test_text_invalid_codec_panics`**: Tests that an invalid codec name produces a
  specific error message. Will work once string schema is implemented.

- **Output tests** (`test_output.rs`): These test the final replay output format. Should
  mostly work once schemas are implemented, but may need minor adjustments to match
  expected output.

### Phase 11: Shrinking quality

After all schemas are implemented, many shrink quality tests may still fail because the
native shrinker is simpler than Hypothesis's. Progressively port additional shrink passes
from pbtkit:

- `/tmp/pbtkit/src/pbtkit/shrinking/sorting.py` — sort and swap passes
- `/tmp/pbtkit/src/pbtkit/shrinking/bind_deletion.py` — bind-point deletion
- `/tmp/pbtkit/src/pbtkit/shrinking/duplication_passes.py` — duplicate shrinking
- `/tmp/pbtkit/src/pbtkit/shrinking/mutation.py` — random mutation to escape local optima
- `/tmp/pbtkit/src/pbtkit/shrinking/sequence.py` — sequence-specific shrinking
- `/tmp/pbtkit/src/pbtkit/shrinking/advanced_integer_passes.py` — integer redistribution
- `/tmp/pbtkit/src/pbtkit/shrinking/index_passes.py` — generic index-based passes

Also consider porting:
- `/tmp/pbtkit/src/pbtkit/caching.py` — choice tree caching for shrink performance
- `/tmp/pbtkit/src/pbtkit/span_mutation.py` — structural mutation generation

## Dependency Graph

```
Phase 1 (integer edge cases + bignum) ← no deps, unblocks integer tests
Phase 2 (float) ← no deps, unblocks all float tests  
Phase 3 (list) ← no deps, unblocks most collection/flatmap/shrink tests
Phase 4 (dict) ← after Phase 3 (reuses collection logic)
Phase 5 (string) ← no deps, unblocks all text tests
Phase 6 (binary) ← no deps, unblocks binary tests
Phase 7 (regex) ← after Phase 5 (uses string generation)
Phase 8 (email/url/etc.) ← after Phase 5 (uses string generation)
Phase 9 (stateful) ← after Phase 3 (uses collections)
Phase 10 (infra fixes) ← can be done anytime
Phase 11 (shrink quality) ← after Phases 1-9
```

Phases 1, 2, 3, 5, 6 are independent and can be tackled in any order. Phase 3 (list)
is the highest-impact single item (unblocks ~80 tests). Phase 2 (float) is next (~50).

## Verification

After each phase, run:
```bash
cargo test --features native --no-fail-fast 2>&1 > /tmp/native-test-run.txt
grep "FAILED\|^test result:" /tmp/native-test-run.txt
```

The total failure count should decrease monotonically. Final target: 0 failures
(excluding properly skipped server-management tests).

Also verify non-native mode is not broken:
```bash
cargo check --tests
```
