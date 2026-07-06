---
name: new-generator
description: "How to add a new generator to hegel-rust. Use when the user asks to implement, add, or write a generator for a type — e.g. 'add a generator for Url', 'implement a UUID generator', 'write a generator for jiff::civil::Date'. Covers the generator struct, builder methods, Generator trait impl, argument validation, mod.rs wiring, rustdoc, and the required test set. Pair with the new-default-generator skill to also wire up gs::default::<T>()."
---

# Adding a New Generator

A reference + checklist for implementing a single new generator. The pattern is very consistent across the crate — `src/generators/time.rs` (`DurationGenerator`) is the cleanest single-builder example, and `src/generators/numeric.rs` (`FloatGenerator`) is the cleanest multi-builder example. Read whichever is closer to your case before starting.

This skill covers writing the generator itself. To make `gs::default::<T>()` work for the new type, also run the **new-default-generator** skill after this one.

## When engine-side draw work would be required

Engine-side draw mods (the typed draws in `hegel-c/src/native/draws/` and their `hegel_generate_*` C ABI) are **out of scope** for this skill.

If you find that the generator would *dramatically and fundamentally* benefit from a new engine-side draw (e.g. it needs bundled data like the Unicode tables or the TLD list, or every interesting builder method would need a new ABI parameter), stop and surface that to the user. Do not start the engine-side work yourself. Otherwise, compose the existing generators and `TestCase` draw methods — `gs::integers()`, `gs::text()`, `tc.generate_date()`, etc. — even if the result is slightly less direct.

## File placement

**First-party generators** (always-on, not tied to a third-party crate) live under `src/generators/`:

- If a topical module already exists for this kind of generator, add it there.
  - Time-shaped → `src/generators/time.rs`
  - Numeric → `src/generators/numeric.rs`
  - String-shaped → `src/generators/strings.rs`
  - Tiny / one-off (no obvious topic) → `src/generators/misc.rs`
- Otherwise create a new module file `src/generators/<name>.rs`.

**Feature-gated third-party integrations** (jiff, chrono, uuid, url, rand, etc.) live under `src/extras/<lib>/generators.rs`, exposed publicly as `hegel::extras::<lib>::<gen>` (via `pub use generators::*;` in the lib's `mod.rs`). See the **add-library-support** skill for the full layout. Inside an `extras::<lib>` module, function names drop the lib prefix — use `dates()`, not `<lib>_dates()` — because the lib name is already in the path.

## Implementation pattern

A generator is four pieces. Use the existing modules as templates verbatim — naming, ordering, and idioms are consistent across the crate.

### 1. The generator struct

A struct holding the generator's configuration. One field per builder option. `Option<T>` for fields that have a meaningful "unset" state, plain `T` for fields with a sensible default (like `min` = 0).

Naming: `<Type>Generator` (e.g. `DurationGenerator`, `FloatGenerator`). Public.

### 2. Builder methods

One `pub fn` per option, each returning `Self` so they chain. Take the value by value (not reference) when reasonable. Each method gets a `///` rustdoc comment describing what it constrains.

If two builder methods are mutually exclusive, document the exclusion on both methods (see `TextGenerator::categories` / `exclude_categories`).

### 3. Validation `invalid_argument!`s

**Every invalid combination of builder values must be caught at draw time with `invalid_argument!`, with a clear message.** Validation runs at the start of `do_draw` (or, for string-shaped generators, inside the cached-handle builder). Examples to model:
- `IntegerGenerator` and `FloatGenerator` in `src/generators/numeric.rs` — bound ordering (`min <= max`), and a descriptive message for the sign-aware-empty float range
- `DurationGenerator` in `src/generators/time.rs` — bound ordering for nanoseconds
- `TextGenerator` in `src/generators/strings.rs` — combination check for `alphabet` vs character methods

These messages are part of the public API: tests assert against them. Pick a stable, descriptive substring.

### 4. `Generator<T>` impl

```rust
impl Generator<T> for FooGenerator {
    fn do_draw(&self, tc: &TestCase) -> T {
        if self.min > self.max {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let n = integers::<i64>()
            .min_value(self.min)
            .max_value(self.max)
            .do_draw(tc);
        parse_foo(n)
    }
}
```

Compose existing generators (`integers()`, `text()`, …) or, for a leaf that maps directly onto an engine draw, the `pub(crate)` typed draw methods on `TestCase` (`generate_integer_i64`, `generate_date`, `generate_string`, …). A generator that makes *several* draws should wrap them in a span (`tc.start_span(label)` / `tc.stop_span(false)`) with a label from `test_case::labels` so the shrinker treats the value as a unit. String-shaped generators that need a `ffi::StringGenerator` handle cache it in a `OnceLock` (see `TextGenerator`) so the alphabet/pattern construction happens once.

### 5. Factory function + module export

Public factory function `pub fn foos() -> FooGenerator` that returns the generator with default-everything. Plural (`durations`, `floats`, `integers`). Inside an `extras::<lib>` module, drop the lib prefix — `dates()` not `<lib>_dates()`.

The factory function gets a `///` doc comment with a short prose description and a runnable example using `#[hegel::test]` in a `no_run` block. Model on the `durations()` factory in `src/generators/time.rs`.

Module wiring depends on placement:

- **First-party generator in a new module file:** add `pub use foo::{FooGenerator, foos};` to `src/generators/mod.rs`.
- **First-party generator added to an existing module:** the existing `pub use` block in `mod.rs` may need a new entry — extend the list rather than adding a new line.
- **Feature-gated generator under `src/extras/<lib>/`:** add the generator type and factory function as `pub` items in `src/extras/<lib>/generators.rs`. They surface as `hegel::extras::<lib>::FooGenerator` and `hegel::extras::<lib>::foos()` through the `pub use generators::*;` already in the lib's `mod.rs`. No `pub use` re-exports in `src/generators/mod.rs`.

## rustdoc requirements

Required on every new generator:

- `///` on the generator struct, including a `Created by [`foos()`].` cross-reference.
- `///` on every builder method describing what it constrains.
- `///` on the factory function with a runnable `#[hegel::test]` example wrapped in ` ```no_run ` (use `no_run` so the example compiles but doctests don't execute a full property run).

The example block in the factory function is a *requirement*, not aspirational — see the `durations()` factory in `src/generators/time.rs` for the canonical shape.

## Tests

All five test categories below are required for every new generator (except #4 which lives in the new-default-generator skill).

Test file location:

- **First-party generator** (under `src/generators/`) → `tests/test_<name>.rs`, modeled on `tests/test_time.rs`.
- **Feature-gated generator** (under `src/extras/<lib>/`) → tests live in a topic-grouped sibling file under `tests/<lib>/` (e.g. `tests/jiff/civil.rs`), declared as `mod <name>;` in `tests/<lib>/main.rs`. Cargo silently skips files not declared in `main.rs`; `just lint` runs `scripts/check-test-modules.py` to catch orphans. See the **add-library-support** skill.

### Test 1 — Sanity (required)

A single test that the generator runs at all with no configuration. Use `check_can_generate_examples`, **not** `assert_all_examples` over a trivial `|_| true` predicate:

```rust
#[test]
fn test_foos_default() {
    check_can_generate_examples(gs::foos());
}
```

`check_can_generate_examples` is the right tool for "does this even run" — `assert_all_examples` with a trivial predicate is misleading.

### Test 2 — Per-builder-method (required, one test each)

One test per builder method (or per meaningful combination, when two methods only make sense together). Each test exercises the method and asserts the constraint is respected on every drawn value.

```rust
#[test]
fn test_foos_max_value() {
    let max = ...;
    assert_all_examples(gs::foos().max_value(max), move |v| *v <= max);
}
```

Model on `tests/test_strings.rs` — most builder methods on `text()` and `characters()` get their own dedicated test.

### Test 3 — Composition in `vecs` (required)

A single test that nests the generator inside `gs::vecs(...)`, exercising the generator inside the engine-managed collection protocol.

```rust
#[test]
fn test_foos_in_vec() {
    let max = ...;
    assert_all_examples(
        gs::vecs(gs::foos().max_value(max)).max_size(5),
        move |v| v.iter().all(|x| *x <= max),
    );
}
```

### Test 4 — `gs::default::<T>()` works

Lives in the **new-default-generator** skill, not here. Skip if you're not also wiring up the default impl.

### Test 5 — Panic on invalid config (required, one per validation)

For every `invalid_argument!` in the generator, a test that triggers it by drawing. Validation happens at draw time, so the test draws inside a run (either the `expect_draw_panic` helper in `tests/test_validation.rs`, or a `#[hegel::test]` with `#[should_panic]`):

```rust
#[hegel::test]
#[should_panic(expected = "max_value < min_value")]
fn test_foos_min_greater_than_max(tc: hegel::TestCase) {
    tc.draw(gs::foos().min_value(10).max_value(5));
}
```

For **first-party generators**, panic tests live in `tests/test_validation.rs` (not in `tests/test_<name>.rs`), grouped with the other validation tests. Model on the existing `test_integers_*` and `test_floats_*` panic tests there.

For **feature-gated generators**, panic tests live alongside the rest of the lib's tests under `tests/<lib>/`. They can't go in `test_validation.rs` because they need the feature flag to compile.

The `expected` substring must match a stable part of the panic message — keep it short and free of formatting.

### Test 6 — Randomized-bound property test (recommended)

A single `#[hegel::test]` per generator that itself draws values for any/all builder options, applies them, draws a value from the configured generator, and asserts the value is within the expected range. This is a strictly more powerful version of test #2 — it catches bugs at parameter combinations a fixed-bound test wouldn't reach.

```rust
#[hegel::test]
fn test_foos_property(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<i64>().min_value(...).max_value(...));
    let hi = tc.draw(gs::integers::<i64>().min_value(lo).max_value(...));
    let v = tc.draw(gs::foos().min_value(lo).max_value(hi));
    assert!(v >= lo && v <= hi);
}
```

Model on `tests/test_strings.rs:test_text_codepoint_range` and `test_characters_codepoint_range`.

When the generator's options interact in nontrivial ways (e.g. `floats()` with `allow_nan` × `min_value`), use `tc.assume(...)` to filter out combinations the generator rejects rather than picking them apart by hand.

### Tests not to add

Explicit edge-case tests are **not** part of the standard test set for new generators. Don't add them.

## Final checklist

Before declaring the generator done:

- [ ] Generator struct with `///` doc and `Created by [`foos()`].` cross-reference
- [ ] Every builder method has `///` doc
- [ ] Every invalid configuration is caught with `invalid_argument!` at draw time
- [ ] `Generator<T>` impl composing existing generators / `TestCase` draw methods, with spans around multi-draw structures
- [ ] Factory function with `///` doc and runnable `#[hegel::test]` example in `no_run`
- [ ] Module wiring done: re-exported from `src/generators/mod.rs` (first-party) or `pub` in `src/extras/<lib>/generators.rs` (feature-gated; surfaced via the existing `pub use generators::*;` in `mod.rs`)
- [ ] Test 1 (sanity, `check_can_generate_examples`)
- [ ] Test 2 (one test per builder method)
- [ ] Test 3 (composition in `vecs`)
- [ ] Test 5 (one panic test per assert; first-party → `tests/test_validation.rs`, feature-gated → `tests/<lib>/`)
- [ ] Test 6 (randomized-bound property test, recommended)
- [ ] `just check` passes (formatting, lint, tests, docs)
- [ ] Coverage is 100% on the new code (see the `coverage` skill if anything is uncovered)

If you're also wiring up `gs::default::<T>()`, run the **new-default-generator** skill next.
