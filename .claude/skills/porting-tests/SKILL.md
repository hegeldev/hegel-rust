---
name: porting-tests
description: "Port Python property-based tests from pbtkit or hypothesis to Rust in this repo. Use when scripts/port-loop.py dispatches you on a specific upstream file, or when you manually decide to port one."
---

# Porting Python tests to hegel-rust

You are porting ONE Python test file from pbtkit
(`resources/pbtkit/tests/`) or Hypothesis
(`resources/hypothesis/hypothesis-python/tests/cover/`) to a Rust test file
in this repo. `scripts/port-loop.py` gives you the exact upstream path and
destination. Port only that file in this commit — do not batch.

## Structure

All ported tests live inside a single integration-test binary per upstream
source, whose entry point is `tests/pbtkit/main.rs` or `tests/hypothesis/main.rs`.
That main.rs declares submodules:

```rust
//! Tests ported from pbtkit/tests/

#[path = "../common/mod.rs"]
mod common;

mod bytes;
mod collections;
// ... one `mod foo;` per ported file, alphabetical
```

Your file goes in `tests/pbtkit/<name>.rs` (or `tests/hypothesis/<name>.rs`) as
a submodule. Use the original Python filename minus the `test_` prefix and
`.py` extension.

Examples:
- `pbtkit/tests/test_text.py` → `tests/pbtkit/text.rs` (module name: `text`)
- `pbtkit/tests/test_core.py` → `tests/pbtkit/core.rs`
- `hypothesis-python/tests/cover/test_floats.py` → `tests/hypothesis/floats.rs`

For subdirectories in the source (e.g. `pbtkit/tests/findability/test_types.py`),
flatten to a prefix: `tests/pbtkit/findability_types.rs`.

### Wiring in

After writing `tests/pbtkit/<name>.rs`, add `mod <name>;` to `tests/pbtkit/main.rs`
(alphabetically). Create `tests/pbtkit/main.rs` from this template if it
doesn't exist:

```rust
//! Tests ported from pbtkit/tests/

#[path = "../common/mod.rs"]
mod common;

mod <your_module>;
```

**Do NOT declare `mod common;` inside your submodule file** — it's declared by
`main.rs`. Your file accesses helpers via `use crate::common::utils::...`.

## File template

```rust
//! Ported from <original_path>

use crate::common::utils::{assert_all_examples, find_any, minimal};
use hegel::generators::{self as gs, Generator};

#[test]
fn test_foo() {
    // ...
}
```

**Critical import**: `use hegel::generators::{self as gs, Generator};`. The
`Generator` trait is required for `.map()`, `.filter()`, `.flat_map()`,
`.boxed()` on any generator. Without it you get `"X is not an iterator"` errors.

## Available test helpers (from `crate::common::utils`)

- `assert_all_examples(generator, predicate)` — assert all 100 generated values satisfy the predicate.
- `find_any(generator, condition) -> T` — return the first generated value matching the condition (panics after 1000 attempts).
- `minimal(generator, condition) -> T` — return the minimal (most-shrunk) value matching the condition.
- `assert_no_examples(generator, condition)` — assert no generated value matches.
- `check_can_generate_examples(generator)` — smoke test that the generator runs.
- `expect_panic(|| { ... }, "regex")` — assert the closure panics with a message matching the regex.

For lower-level control:

```rust
use hegel::{Hegel, Settings, Verbosity};

Hegel::new(|tc| {
    let x: i64 = tc.draw(&gs::integers());
}).settings(Settings::new().test_cases(100).database(None)).run();
```

## Generator API quick-reference

See `references/api-mapping.md` for the full pbtkit/Hypothesis → hegel-rust
cheat sheet. In brief:

- `gs::integers::<T>()`, `gs::floats::<T>()`, `gs::booleans()`, `gs::text()`, `gs::binary()`.
- `gs::vecs(inner)`, `gs::hashsets(inner)`, `gs::hashmaps(k, v)`, `gs::arrays::<G, T, N>(inner)`.
- `gs::just(x)`, `gs::unit()`, `gs::optional(inner)`, `gs::sampled_from(vec)`, `gs::one_of(vec![g.boxed()])`.
- `gs::from_regex(pat)`, `gs::characters()`, `gs::dates()`, `gs::times()`, `gs::datetimes()`, `gs::durations()`.
- Transforms on generators (require `Generator` trait in scope): `.map`, `.filter`, `.flat_map`, `.boxed`.

## Common type-inference pitfalls

Closure parameters sometimes need explicit types. When the Python tests use
`@given(st.lists(st.integers()))` and take `xs`, the Rust port often needs
`&Vec<i64>`:

```rust
// wrong:  |xs| xs.iter().sum::<i64>() > 10,
// right:
assert_all_examples(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
    xs.iter().sum::<i64>() > 10
});
```

For tuples: `|(x, y): &(i64, i64)| ...`. For floats: `|f: &f64| ...` (note
it's passed by `&T`). If unsure about element types, make them explicit with
`gs::integers::<i64>()` rather than `gs::integers()` bare.

## Skip vs. port decision

### Add to SKIPPED.md — public-API incompatibility only

An upstream file goes in `SKIPPED.md` ONLY when its tests rely on *public
API* that has no hegel-rust counterpart:

1. Python-specific facilities: `pickle`, `__repr__`, `__iter__`,
   `sys.modules`, dunder access patterns, Python syntax checks.
2. Integrations with other Python libraries: numpy, pandas, django,
   attrs, redis, etc.
3. Hypothesis/pbtkit public-API features with genuinely no analog (rare
   — most of the public API *does* have a counterpart).

Add the filename to the appropriate section of `SKIPPED.md` with a
one-line rationale naming the specific public API or integration that
blocks the port, then commit. "Too complex" and "engine internal" are
NOT valid reasons — those are covered below.

### Port — native-gated plus source-level stub

Tests that exercise pbtkit / Hypothesis *engine internals* —
`ChoiceNode`, `PbtkitState`, `ConjectureRunner`, `SHRINK_PASSES`,
`CachedTestFunction`, `IntegerChoice`, `FloatChoice`, `TC.for_choices`,
etc. — have counterparts under `src/native/` and are reachable only in
native mode. Port these; do NOT skip them.

1. Write the test in its usual destination (`tests/<kind>/<module>.rs`),
   or as an embedded test in `tests/embedded/native/...` if it needs
   private access — the embedded pattern wires the test into the source
   via
   `#[cfg(test)] #[path = "../../../tests/embedded/native/foo_tests.rs"] mod tests;`.
   Look at `tests/embedded/native/choices_tests.rs` and
   `tree_tests.rs` for the pattern.
2. Native-gate it. Put `#![cfg(feature = "native")]` at the top of the
   file if every test in it is native-only; otherwise mark individual
   tests with `#[cfg(feature = "native")]`.
3. If the test depends on a native-mode feature that isn't implemented
   yet:
   - If the feature is easy to add, implement it properly in
     `src/native/...`.
   - If it's hard, stub the missing function body in `src/native/...`
     with `todo!("specific thing missing")` — a runtime panic, not a
     compile error. A subsequent fixer-task invocation will pick it
     up once the test fails at runtime.
   - The test MUST compile cleanly in both server and native modes.
     `todo!()` lives in the source code, never in the test body.

### Port — normal (the common case)

For tests that only use the public generator API (`gs::integers`,
`gs::vecs`, `Hegel::new(...).run()`, etc.) — just port them. No
native-gating, no stubs.

### Think harder before skipping

Agents have a strong bias to mark anything unfamiliar as unportable.
Specific shapes that *look* skip-worthy but aren't:

- **stdout/stderr capture** (`capsys` in Python) — hegel-rust has a
  `TempRustProject` helper used in `tests/test_output.rs` and
  `tests/test_draw_named.rs` that runs test code as a subprocess and
  captures stderr.
- **Database replay** (writing a failing case, replaying it) — hegel-rust
  has `Database::Path(...)` via
  `Settings::new().database(Database::Path(...))`. The round-trip is only
  exercised natively, so native-gate it. See `tests/test_database_key.rs`.
- **`tc.choice(n)` in Python** → in hegel-rust,
  `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))`.
- **Full 64-bit integer range** → `gs::integers::<u64>()` etc.
- **`@gs.composite`** → `#[hegel::composite]` or `hegel::compose!`.

## Conjecture tests (Hypothesis internal engine)

Tests under `resources/hypothesis/hypothesis-python/tests/conjecture/`
test Hypothesis's engine internals. Place these at
`tests/hypothesis/conjecture_*.rs` with `#![cfg(feature = "native")]`
at the top, following the native-gated-plus-source-stub rules above.

## Style

- Keep each `#[test]` close in spirit to the Python original, with a similar
  name (prefix `test_`).
- Use `.unwrap()` over `.expect("static message")`.
- Don't bind unused return values to `_`.
- Minimal comments — only when a translation choice is non-obvious.
- Don't add new helpers to `tests/common/utils.rs`.

## Verification step (REQUIRED before commit)

Before you finish:

1. Write the file.
2. Update `tests/pbtkit/main.rs` (or `tests/hypothesis/main.rs`) to include
   your new module. Create the main.rs if it doesn't exist.
3. Run `cargo test --test pbtkit --no-run` (or `--test hypothesis`). The
   suite MUST compile.
4. Run `cargo test --test pbtkit <your_module>` (server mode). Every test
   you wrote must either pass or fail with a runtime `todo!()` raised
   from `src/native/` (see the skip-vs-port section). Tests themselves
   must not contain `todo!()`.
5. Run `cargo test --features native --test pbtkit --no-run` — the suite
   must still compile under native mode.

**Interpreting failures:**

- **Compilation errors** are *always* a problem in your test code, not in
  hegel-rust. Common fixes:
  - `X is not an iterator` / `method 'boxed' not found` → you forgot
    `use hegel::generators::{self as gs, Generator};`
  - `type annotations needed` → add closure parameter type (`|xs: &Vec<i64>|`).
  - `file not found for module 'common'` → you left a spurious
    `mod common;` in the submodule file; remove it.
- **Runtime test failures** are *usually* a problem in your test translation
  (wrong expected value, wrong bounds, misread the Python intent). Look at
  your test first.
- But runtime failures can occasionally be genuine hegel-rust bugs,
  especially under `#[cfg(feature = "native")]` where the engine is less
  mature. If after re-reading the Python original and your translation you
  believe the test is correct, leave the test asserting the correct
  behavior — don't fudge the assertion to make it pass. The fixer loop
  will pick up the failure and fix the engine.

### Handling tests that can't pass yet

If a test calls a native-mode feature that isn't implemented yet, do NOT
`todo!()` the test body. Instead, per the skip-vs-port section above:

1. Write the full test body (native-gated if it exercises engine internals).
2. Implement the missing feature in `src/native/...`, or stub the missing
   function body with `todo!("specific thing missing")` if it's too large
   for this commit.
3. Commit. The test will compile and (if the source was stubbed) fail at
   runtime — that's expected; a fixer-task invocation will pick it up.

## Commit

After verification passes, commit with a focused message like:

```
Port pbtkit/test_text.py to tests/pbtkit/text.rs

7 tests ported via the public generator API. 12 native-gated tests
exercise src/native/choices, which currently stubs `choice_with_weight`
as `todo!()`; fixer-task invocations will fill the stub in.
```

One port per commit. Update `tests/pbtkit/main.rs` in the same commit.

## Don't modify

- `tests/common/utils.rs`.
- Any existing ported file.
- `src/` code beyond the scope of this port. You MAY:
  - Add a `#[cfg(test)] #[path = "..."] mod tests;` wiring at the bottom
    of a source file to hook up a new embedded test module.
  - Add or stub a missing native-mode function under `src/native/...`
    as required by the port (see the skip-vs-port section).
