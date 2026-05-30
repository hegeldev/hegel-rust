---
name: new-generator
description: "How to add a new generator to hegel-rust. Use when the user asks to implement, add, or write a generator for a type — e.g. 'add a generator for Url', 'implement a UUID generator', 'write a generator for jiff::civil::Date'. Covers the generator struct, builder methods, Generate trait impl, schema asserts, mod.rs wiring, rustdoc, and the required test set. Pair with the new-default-generator skill to also wire up gs::default::<T>()."
---

# Adding a New Generator

A reference + checklist for implementing a single new generator. The pattern is very consistent across the crate — `src/generators/time.rs` (`DurationGenerator`) is the cleanest single-builder example, and `src/generators/numeric.rs` (`FloatGenerator`) is the cleanest multi-builder example. Read whichever is closer to your case before starting.

This skill covers writing the generator itself. To make `gs::default::<T>()` work for the new type, also run the **new-default-generator** skill after this one.

## When server-side schema work would be required

Server-side schema mods (in `hegel-core`) are **out of scope** for this skill. Do not modify the server.

If you find that the generator would *dramatically and fundamentally* benefit from a new or extended server schema (e.g. the type cannot reasonably be expressed via existing schemas, or every interesting builder method would need a new schema field), stop and surface that to the user. Do not start the server-side work yourself. Otherwise, compose existing schemas — `{"type": "integer"}`, `{"type": "string"}`, `{"type": "date"}`, etc. — even if the result is slightly less ergonomic on the wire.

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

A struct holding the generator's configuration. One field per builder option. `Option<T>` for fields that have a meaningful "unset" state (so `build_schema` can omit the corresponding schema field), plain `T` for fields with a sensible default (like `min` = 0).

Naming: `<Type>Generator` (e.g. `DurationGenerator`, `FloatGenerator`). Public.

### 2. Builder methods

One `pub fn` per option, each returning `Self` so they chain. Take the value by value (not reference) when reasonable. Each method gets a `///` rustdoc comment describing what it constrains.

If two builder methods are mutually exclusive, document the exclusion on both methods (see `TextGenerator::categories` / `exclude_categories`).

### 3. `build_schema` + `assert!`s

A private `fn build_schema(&self) -> ciborium::Value` that turns the configuration into a CBOR schema using the `cbor_map!` / `cbor_array!` / `map_insert` / `map_extend` helpers from `crate::cbor_utils`.

**Every invalid combination of builder values must be caught here with `assert!` or `panic!`, with a clear message.** Examples to model, all inside `build_schema`:
- `IntegerGenerator` and `FloatGenerator` in `src/generators/numeric.rs` — bound ordering (`min <= max`), and a descriptive `panic!` for the sign-aware-empty float range
- `DurationGenerator` in `src/generators/time.rs` — bound ordering for nanoseconds
- `TextGenerator` in `src/generators/strings.rs` — combination check for `alphabet` vs character methods

These messages are part of the public API: tests assert against them with `#[should_panic(expected = "...")]`. Pick a stable, descriptive substring.

### 4. `Generator<T>` impl

```rust
impl Generator<T> for FooGenerator {
    fn do_draw(&self, tc: &TestCase) -> T {
        super::generate_from_schema(tc, &self.build_schema())
        // or, for types needing a custom parse:
        // parse_foo(super::generate_raw(tc, &self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            // transform raw ciborium::Value into T
        }))
    }
}
```

**Always implement `as_basic` returning `Some` when the generator can be expressed as a single server schema.** This is the central optimization of the crate — `map()` on a basic generator preserves the schema. The only time `as_basic` returns `None` (or is omitted) is when generation fundamentally requires multiple draws or runtime decisions that can't be encoded as one schema.

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
- `///` on the factory function with a runnable `#[hegel::test]` example wrapped in ` ```no_run ` (use `no_run` because doctests don't have a hegel server available).

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

A single test that nests the generator inside `gs::vecs(...)`. Critical: this exercises the *non-basic* code path (because at the time of writing some collection contexts go through different machinery), which the standalone tests don't cover.

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

### Test 5 — Panic on invalid config (required, one per assert)

For every `assert!` / `panic!` in `build_schema`, a `#[should_panic(expected = "...")]` test that triggers it. Force `as_basic()` to evaluate the schema:

```rust
#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_foos_min_greater_than_max() {
    let g = gs::foos().min_value(10).max_value(5);
    g.as_basic();
}
```

For **first-party generators**, panic tests live in `tests/test_validation.rs` (not in `tests/test_<name>.rs`), grouped with the other validation tests. Model on the existing `test_integers_*` and `test_floats_*` panic tests there.

For **feature-gated generators**, panic tests live alongside the rest of the lib's tests under `tests/<lib>/`. They can't go in `test_validation.rs` because they need the feature flag to compile.

The `expected` substring must match a stable part of the panic message — keep it short and free of formatting.

### Test 7 — Randomized-bound property test (recommended)

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

### Test 6 / 8 — Skip

Explicit edge-case tests are **not** part of the standard test set for new generators. Don't add them.

## Final checklist

Before declaring the generator done:

- [ ] Generator struct with `///` doc and `Created by [`foos()`].` cross-reference
- [ ] Every builder method has `///` doc
- [ ] `build_schema` has `assert!` / `panic!` for every invalid configuration
- [ ] `Generator<T>` impl with `as_basic` returning `Some` (unless fundamentally non-basic)
- [ ] Factory function with `///` doc and runnable `#[hegel::test]` example in `no_run`
- [ ] Module wiring done: re-exported from `src/generators/mod.rs` (first-party) or `pub` in `src/extras/<lib>/generators.rs` (feature-gated; surfaced via the existing `pub use generators::*;` in `mod.rs`)
- [ ] Test 1 (sanity, `check_can_generate_examples`)
- [ ] Test 2 (one test per builder method)
- [ ] Test 3 (composition in `vecs`)
- [ ] Test 5 (one panic test per assert; first-party → `tests/test_validation.rs`, feature-gated → `tests/<lib>/`)
- [ ] Test 7 (randomized-bound property test, recommended)
- [ ] `just check` passes (formatting, lint, tests, docs)
- [ ] Coverage is 100% on the new code (see the `coverage` skill if anything is uncovered)

If you're also wiring up `gs::default::<T>()`, run the **new-default-generator** skill next.
