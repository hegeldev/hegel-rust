---
name: add-library-support
description: "How to add hegel-rust support for a third-party Rust crate. Use when the user asks to 'add support for <crate>', 'add a <crate> integration', 'add generators for <crate>', or similar. Orchestrates implementing a generator and DefaultGenerator impl for every public type the crate exposes."
---

# Adding Support for a New Library

A procedural skill: orchestrates feature-gated integration with a third-party Rust crate. Implements **full coverage** of the crate's public API — every type that can be generated gets a generator and a `DefaultGenerator` impl.

For each individual type, this skill defers to:
- the **new-generator** skill (for the generator itself)
- the **new-default-generator** skill (for the `DefaultGenerator` impl)

Read both before starting — the per-type pattern is theirs, not this skill's.

## Step 1 — Investigate the public API

Spawn a sub-agent (general-purpose) to enumerate the crate's full public API and produce a research dossier. The sub-agent's job is **research only** — it gathers and reports information, it does not make implementation choices. You (the parent agent) decide how to map each type to a generator using the **new-generator** skill; the sub-agent's output is the raw material for those decisions.

Err on the side of returning more information rather than less. The parent has full agency over schema choice, builder methods, composition strategy, and whether to skip a type — the sub-agent should not pre-decide any of that.

Brief the sub-agent to:

- Enumerate every public type in the crate. Exclude only the things that are obviously not end-user values: internal/private types, error types, traits, marker types.
- For each type, return:
  - Full path (e.g. `jiff::civil::Date`).
  - One- or two-sentence description of what the type represents.
  - **Construction surface**: every public way to build a value — `new`, `from_*`, `try_from_*`, builder types, `FromStr`, `Default`, etc. Include signatures.
  - **Component structure** (for composite types): what fields/parts the type is composed of, and which other types in this crate they reference. The parent uses this to decide composition order and what to delegate.
  - **Constraints / invariants**: documented bounds (e.g. "`Date` is constrained to years -9999..=9999"), validity rules, units, what panics or returns `Err` on invalid input.
  - **Typical usage** in 1–2 lines: how does the type appear in user code? What's its role in the crate?
  - Anything else the sub-agent thinks the parent will want before deciding how to generate this type. Be generous.
- Return the dossier as markdown, one section per type. Do not rank, recommend, or filter — just report.

Once the dossier is back, you decide which types to implement, in what order, and how to map each to a schema. Default to implementing every type the sub-agent returned. Skip a type only if:

- the dossier reveals it's clearly not generatable (errors, traits, marker types that slipped through), or
- **its only sensible generator would be `sampled_from(&ALL_VARIANTS)`.** This applies regardless of whether the variants are domain values (`Weekday`, `Era`) or configuration knobs (`RoundMode`, `Disambiguation`) — the deciding factor is whether the generator is doing anything beyond enumerating variants.

When you skip, note the reason in your final summary.

If the dossier reveals the crate's surface is genuinely too large for full coverage to be reasonable (hundreds of types, deep generic hierarchies), flag that to the user and ask before proceeding — but treat this as the exception, not the default.

## Step 2 — Cargo.toml: optional dep + feature flag

Add the crate as an optional dep:

```toml
[dependencies]
# existing deps...
<lib> = { version = "X.Y", optional = true }

[features]
default = []
<lib> = ["dep:<lib>"]
```

Version: pin to the latest published `major.minor` (Cargo treats `"X.Y"` as `^X.Y`).

`docs.rs` metadata already uses `all-features = true`, so feature-gated items will show up in the published docs without extra config.

## Step 3 — Module layout and public API

Feature-gated third-party integrations live under `src/extras/<lib>/`, exposed publicly as `hegel::extras::<lib>::<gen>`.

Create the source dir:

```
src/extras/
  mod.rs              # contains `#[cfg(feature = "<lib>")] pub mod <lib>;` for each supported lib
  <lib>/
    mod.rs            # module wiring: `mod generators;`, `mod default;`, `pub use generators::*;`
    generators.rs     # generator structs, factory functions, `Generator` impls
    default.rs        # `DefaultGenerator` impls (omit the file if there are none)
```

Wire up the top-level `extras` module in `src/lib.rs` (only needed once, when adding the very first lib):

```rust
pub mod extras;
```

In `src/extras/mod.rs`, add the new lib:

```rust
#[cfg(feature = "<lib>")]
pub mod <lib>;
```

**Naming inside the lib module: drop the lib prefix from function names.** The lib name is in the path. Use `extras::jiff::dates()`, not `extras::jiff::jiff_dates()`.

The lib's generator types and factory functions are `pub` in `src/extras/<lib>/generators.rs` and surfaced through the `pub use generators::*;` in `mod.rs`; no further re-export juggling needed.

`src/generators/mod.rs` does **not** re-export anything from `extras/`. Keep that boundary clean.

## Step 4 — Implement every generator + every DefaultGenerator impl

For each type from Step 1, in dependency order (leaf types first, composite types later so they can reuse the leaves' generators):

1. Apply the **new-generator** skill to add the generator to `src/extras/<lib>/generators.rs`.
2. Apply the **new-default-generator** skill to add the `DefaultGenerator` impl. For feature-gated types the impl goes in `src/extras/<lib>/default.rs` — `src/generators/default.rs` stays first-party only. See the new-default-generator skill for the rationale.

Composite types should typically delegate to the leaf generators rather than building their own schema from scratch. Example sketch:

```rust
// in src/extras/<lib>/generators.rs
pub fn datetimes() -> impl Generator<lib::DateTime> {
    gs::tuples((dates(), times()))
        .map(|(d, t)| lib::DateTime::from_parts(d, t))
}
```

Prefer composition — do not re-derive a schema if one of the leaf generators already has it.

### Engine-side schemas are out of scope

If you find a type that would *dramatically and fundamentally* benefit from a new engine schema (e.g. would otherwise need a wildly contorted `flat_map` or post-hoc filter), stop and surface it to the user. Do not modify the engine's schema interpreters (`hegel-c/src/native/schema/`) yourself. See the new-generator skill for more on this gate.

## Step 5 — Tests

All tests for the lib live under `tests/<lib>/`, organized as several sibling files grouped by topic (e.g. `tests/jiff/civil.rs`, `tests/jiff/duration.rs`, `tests/jiff/tz.rs`). A single `tests/<lib>/main.rs` is the entry point — it declares the shared `common` module and lists each sibling as `mod <name>;`. Cargo auto-discovers `tests/<lib>/main.rs` as one test binary; the sibling files compile as submodules of it.

```rust
// tests/<lib>/main.rs
#![cfg(feature = "<lib>")]

#[path = "../common/mod.rs"]
mod common;

mod civil;
mod duration;
mod tz;
```

Each sibling file imports what it needs directly — no `use super::*;` — and references the shared module as `crate::common::...`:

```rust
// tests/<lib>/civil.rs
use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::<lib> as <lib>_gs;
use hegel::generators as gs;
// ...
```

**Cargo silently ignores files in `tests/<lib>/` that aren't referenced from `main.rs`.** A new sibling file isn't compiled until you add `mod <name>;` to `main.rs`. The `just lint` recipe runs `scripts/check-test-modules.py`, which catches orphan files; rely on it rather than memory.

Tests for *every* generator in the lib (per new-generator skill: sanity, per-builder, composition, panic, randomized-bound) and every `default()` impl (per new-default-generator skill) all go under this directory. Panic tests do **not** go into the global `tests/test_validation.rs` for feature-gated libs — they need the feature flag and live with the rest of the lib's tests.

Group sibling files by topic, not by exact type — e.g. all duration-shaped types in `duration.rs`, all timezone types in `tz.rs`. Start with one or two files and split further as the lib grows.

Update `justfile` if the test command needs to learn the new feature flag — check whether `just test` already runs with `--all-features` or similar before adding anything.

## Step 6 — Verify

Run, in order:

```bash
just check                # formatting, lint, all tests, docs
cargo build --features <lib>
cargo test --features <lib>
cargo doc --features <lib> --no-deps
```

Then verify the no-feature build still compiles cleanly:

```bash
cargo build
cargo test
```

Coverage must stay at 100% on the new code (see the `coverage` skill if anything is uncovered).

`RELEASE.md` is handled by the `self-review` skill at PR time — do not write it here.

## Final checklist

- [ ] Step 1: sub-agent enumerated the full public API; you have the type list
- [ ] Step 2: `Cargo.toml` has the optional dep and the feature
- [ ] Step 3: `src/extras/<lib>/{mod.rs,generators.rs,default.rs}` exist (default.rs only if you have any DefaultGenerator impls); `src/extras/mod.rs` declares it behind `#[cfg(feature = "<lib>")]`; (first-time only) `src/lib.rs` has `pub mod extras;`
- [ ] Step 4: every type from Step 1 has a generator (new-generator skill applied) and a `DefaultGenerator` impl (new-default-generator skill applied) — none skipped; function names inside `extras::<lib>` do not carry a lib prefix
- [ ] Step 5: `tests/<lib>/main.rs` exists with `#![cfg(feature = "<lib>")]`, pulls in `common` via `#[path = "../common/mod.rs"]`, and declares each sibling file with `mod <name>;`. Sibling files (grouped by topic) collectively contain every generator's test set + every `default()` test. `just check-test-modules` passes (no orphans).
- [ ] Step 6: `just check`, `cargo {build,test,doc} --features <lib>`, and the bare `cargo {build,test}` all pass; coverage is 100%
