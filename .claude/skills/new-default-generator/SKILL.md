---
name: new-default-generator
description: "How to add a DefaultGenerator impl for a type so gs::default::<T>() works. Use when the user asks to wire up default() for a type, add a default generator, or make a type usable with #[derive(Generate)]. Pair after the new-generator skill when adding a fresh generator, or use standalone when the underlying generator already exists or can be composed from existing generators."
---

# Adding a New Default Generator

A small skill: a `DefaultGenerator` impl is just a trait implementation plus one test. Use this skill in two situations:

1. **Paired after the new-generator skill**, when you've just added a new generator and want `gs::default::<T>()` to return it.
2. **Standalone**, when a generator already exists (or can be expressed by composing existing generators) and only the trait impl is missing. For example, if youwant `gs::default::<IpAddr>()` to work via `gs::ip_addresses().map(...)`, with no new generator needed.

If neither generator exists nor can be composed from existing ones, inform the user.

## File placement

- For primitive / `std` types: `src/generators/default.rs`. Group with similar impls (numeric near numeric, etc.).
- For types from a feature-gated third-party crate (e.g. `jiff`, `chrono`): `src/extras/<lib>/default.rs`. Create the file if it doesn't exist yet and wire it up in the lib's `mod.rs` with `mod default;`.

## The trait impl

```rust
impl DefaultGenerator for Foo {
    type Generator = FooGenerator;
    fn default_generator() -> Self::Generator {
        foos()
    }
}
```

When the default uses composition rather than a dedicated generator type, the associated `Generator` becomes a `BoxedGenerator`:

```rust
impl DefaultGenerator for std::path::PathBuf {
    type Generator = BoxedGenerator<'static, PathBuf>;
    fn default_generator() -> Self::Generator {
        text().map(PathBuf::from).boxed()
    }
}
```

For feature-gated impls, the impl lives in `src/extras/<lib>/default.rs`. The lib's `mod.rs` re-exports the generator type and factory via `pub use generators::*;`, so `default.rs` can import them through `super::`:

```rust
// in src/extras/jiff/default.rs
use jiff::civil::Date;

use crate::generators::DefaultGenerator;

use super::{DateGenerator, dates};

impl DefaultGenerator for Date {
    type Generator = DateGenerator;
    fn default_generator() -> Self::Generator {
        dates()
    }
}
```

No `#[cfg(feature = "<lib>")]` attribute is needed on the impl itself — the entire module is already gated by the feature in `src/extras/mod.rs`.

## Imports

For first-party types, add the new generator type and factory function to the existing `use super::{...}` block at the top of `default.rs`.

For feature-gated types, extend the existing `use super::{...};` block at the top of `src/extras/<lib>/default.rs` with the new generator type and factory function. `DefaultGenerator` comes from `use crate::generators::DefaultGenerator;` in the same file.

## The required test

A single test in the same test file as the generator's own tests:

- **First-party type** → `tests/test_<name>.rs`
- **Feature-gated type** → `tests/<lib>.rs` (with `#![cfg(feature = "<lib>")]` at the top, per the add-library-support skill)

```rust
#[test]
fn test_foo_default_generator() {
    check_can_generate_examples(gs::default::<Foo>());
}
```

Use `check_can_generate_examples`, not `assert_all_examples` over a trivial predicate. The point is to prove the impl is wired up — the generator's own tests already cover correctness of the produced values.

If the standalone-case generator has a meaningful invariant on its default output, you may strengthen the assertion. Optional, not required.

## Final checklist

- [ ] `DefaultGenerator` impl in the right place: `src/generators/default.rs` for first-party types, `src/extras/<lib>/default.rs` for feature-gated types
- [ ] For first-party: factory + generator type imported at the top of `default.rs`
- [ ] For feature-gated: factory + generator type imported via `use super::{...};` in `src/extras/<lib>/default.rs`, plus `use crate::generators::DefaultGenerator;`
- [ ] One test calling `check_can_generate_examples(gs::default::<T>())`
- [ ] `just check` passes
