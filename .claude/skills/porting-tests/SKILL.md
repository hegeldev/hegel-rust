---
name: porting-tests
description: "Port Python property-based tests from pbtkit or hypothesis to Rust in this repo. Use when the Stop hook names a specific upstream file to port, or when you manually decide to port one."
---

# Porting Python tests to hegel-rust

You are porting ONE Python test file from pbtkit (`/tmp/pbtkit/tests/`) or
Hypothesis (`/tmp/hypothesis/hypothesis-python/tests/cover/`) to a Rust test
file in this repo. The Stop hook gives you the exact upstream path. Port only
that file in this commit — do not batch.

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

## What to skip (cleanly, with no stub)

Entire test functions should be dropped — or the whole file added to
`SKIPPED.md` with a one-line rationale — when they test something that has no
meaningful counterpart in Rust at all:

1. Python internals (`__repr__`, `__iter__`, `sys.modules`, `pickle`).
2. Python syntactic constructs that don't exist in Rust.
3. Integration with other Python libraries (numpy, pandas, django, redis, attrs, etc.).

## What to leave as `todo!()` (don't drop)

Tests that exercise a hegel-rust feature that *could* exist but currently
doesn't, or that you can't port cleanly in a small change:

1. **Missing public API** (`target()`, `.weighted()` on TestCase, `.reject()`
   vs `.assume(false)` distinction, deadlines). When you hit one of these,
   also add a TODO.md entry for adding that API to hegel-rust and reference
   it from the `todo!()` stub.
2. **Tests that *should* work but hit a Rust type-inference or API shape
   problem** you can't resolve in a small change — leave a `todo!()` with
   the error you saw.

### Think harder before writing todo!()

Agents have a strong bias to mark anything unfamiliar as untestable. Many
tests that look like pbtkit internals *do* have a portable shape in
hegel-rust:

- **pbtkit engine-internal tests** (`ChoiceNode`, `TC.for_choices`,
  `PbtkitState`, `ConjectureRunner`, `SHRINK_PASSES`, `CachedTestFunction`,
  `IntegerChoice`, `FloatChoice`, etc.) — hegel-rust has equivalents in
  `src/native/`. These types are `pub(crate)` — **you are expected to test
  them via embedded tests**. Look at `tests/embedded/native/choices_tests.rs`
  and `tree_tests.rs` for the pattern: an embedded test is wired into the
  source file via
  `#[cfg(test)] #[path = "../../../tests/embedded/native/foo_tests.rs"] mod tests;`.
  Write the embedded test and modify the target source file to add the wiring
  (but don't change other code in the src file). This is the PREFERRED
  outcome over `todo!()` for engine behavior tests.

- **stdout/stderr capture** (tests using `capsys` in Python) — hegel-rust
  has a `TempRustProject` helper used in `tests/test_output.rs` and
  `tests/test_draw_named.rs` that runs test code as a subprocess and
  captures stderr. Don't say "too heavy a port".

- **Database replay** (writing a failing case, replaying it) — hegel-rust
  has a `Database` type usable via
  `Settings::new().database(Database::Path(...))`. The round-trip is only
  exercised by the native backend, so gate such tests with
  `#[cfg(feature = "native")]`. Look at `tests/test_database_key.rs`. Only
  use `todo!()` if the test is about pbtkit's specific serialized format
  (e.g. `test_malformed_database_entry`).

- **`tc.choice(n)` in Python** → in hegel-rust this is
  `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))`.

- **Full 64-bit integer range** → `gs::integers::<u64>()` etc.

- **`@gs.composite`** → `#[hegel::composite]` or `hegel::compose!` macros.

When you *do* use `todo!()`, the comment should name the specific API that's
missing, not a vague "engine internal". A future reader should be able to
tell whether the blocker is "hegel-rust needs a new public method" vs "this
genuinely can't work without exposing private types".

## Conjecture tests (Hypothesis internal engine)

Tests under `hypothesis-python/tests/conjecture/` test Hypothesis's engine
internals. The hegel-rust equivalent lives in `src/native/`. Port these as
native-only by placing them at `tests/hypothesis/conjecture_*.rs` with
`#![cfg(feature = "native")]` at the top.

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
4. Run `cargo test --test pbtkit <your_module>` (server mode). Every non-todo
   test you wrote must pass. `todo!()` stubs will fail — that's expected.
5. Run `cargo test --features native --test pbtkit --no-run` — the suite
   must still compile under native mode. Don't worry about test failures in
   native mode from pre-existing canary panics; the Stop hook handles those.

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
- But runtime failures can occasionally be genuine hegel-rust bugs, especially
  under `#[cfg(feature = "native")]` where the engine is less mature. If
  after re-reading the Python original and your translation you believe the
  test is correct, note this in a new TODO.md entry — don't force the
  assertion to pass by fudging values.

### Handling tests that can't be completed

If a `#[test]` cannot be written fully, **do NOT delete the test**. Instead,
leave it in place with a `todo!()` body and a comment explaining what went
wrong:

```rust
#[test]
fn test_weighted_coin() {
    // TODO: hegel-rust has no public `weighted()` API on TestCase.
    // Original Python used `tc.weighted(0.7)`. See TODO.md entry
    // "add weighted() to TestCase".
    todo!()
}
```

This keeps the intent visible and lets future work pick it up. The `todo!()`
will cause the test to fail when run, but the file will still compile — which
is what the verification step requires.

## Commit

After verification passes, commit with a focused message like:

```
Port pbtkit/test_text.py to tests/pbtkit/text.rs

7 tests ported via the public generator API. 12 left as todo!() stubs
(engine internals not exposed through the public API).
```

One port per commit. Update `tests/pbtkit/main.rs` in the same commit.

## Don't modify

- `tests/common/utils.rs`.
- Any existing ported file.
- `src/` code, **except** for adding a single
  `#[cfg(test)] #[path = "..."] mod tests;` wiring at the bottom of a file to
  hook up a new embedded test module. Don't change the source code itself.
