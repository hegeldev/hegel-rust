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

All ported tests live inside one of two integration-test binaries,
already wired up in `Cargo.toml`:

- `tests/pbtkit/main.rs` — `cargo test --test pbtkit`
- `tests/hypothesis/main.rs` — `cargo test --test hypothesis`

Each `main.rs` exists as an empty harness and declares submodules:

```rust
//! Tests ported from pbtkit/tests/

#[path = "../common/mod.rs"]
mod common;

mod bytes;
mod collections;
// ... one `mod foo;` per ported file, alphabetical
```

Your file goes in `tests/pbtkit/<name>.rs` (or `tests/hypothesis/<name>.rs`)
as a submodule. Use the original Python filename minus the `test_` prefix
and `.py` extension.

Examples:
- `resources/pbtkit/tests/test_text.py` → `tests/pbtkit/text.rs` (module `text`)
- `resources/pbtkit/tests/test_core.py` → `tests/pbtkit/core.rs`
- `resources/hypothesis/hypothesis-python/tests/cover/test_floats.py` → `tests/hypothesis/floats.rs`

For subdirectories in the source (e.g.
`resources/pbtkit/tests/findability/test_types.py`), flatten to a prefix:
`tests/pbtkit/findability_types.rs`.

### Wiring in

After writing `tests/pbtkit/<name>.rs`, add `mod <name>;` to
`tests/pbtkit/main.rs` (alphabetically). Do NOT touch the `[[test]]`
declarations in `Cargo.toml` — they're already set up.

**Do NOT declare `mod common;` inside your submodule file** — it's
declared by `main.rs`. Your file accesses helpers via
`use crate::common::utils::...`.

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

Default: **port**. The skip-list is narrow and strict; redundancy is fine,
mis-skips are not.

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
blocks the port, then commit. "Too complex", "engine internal", and
"no Rust counterpart" (by itself) are NOT valid reasons — those are
covered below.

### NOT reasons to skip

Agents have a strong bias to rationalise ports away. These are not valid
reasons to skip a file, drop a test from the port, or list a case as
"omitted" in the module docstring:

- **"Has no Rust counterpart"** (for an internal API). That's the reason
  to port — see the next section.
- **"This is covered by tests/foo.rs already"** / **"redundant with an
  existing test"**. Redundancy is fine. A later rationalisation pass will
  deduplicate; don't pre-empt it. Porting the test a second time costs
  very little; incorrectly skipping one costs real coverage.
- **"The test targets pbtkit's serialization tag / database format /
  internal harness"** when hegel-rust has the equivalent internal
  facility under `src/native/`. Native-gate it and port.
- **"The test requires a shrinking pass marked `@pytest.mark.requires(...)`
  that hegel-rust may or may not have"**. If hegel-rust has it, port
  normally. If it doesn't yet, native-gate the test and stub/implement
  the pass per the next section.

### Port — native-gated plus source-level stub

Tests that exercise pbtkit / Hypothesis *engine internals* —
`ChoiceNode`, `PbtkitState`, `ConjectureRunner`, `SHRINK_PASSES`,
`CachedTestFunction`, `IntegerChoice`, `FloatChoice`, `StringChoice`,
`TC.for_choices`, `to_index`/`from_index`, database serialization tags,
span introspection, etc. — have counterparts under `src/native/` and
are reachable only in native mode. Port these; do NOT skip them.

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

### Skipping individual tests within an otherwise-ported file

Occasionally one test (or one parametrize row) in an otherwise
fully-ported file is unrepresentable — usually because its input
exercises a Python type / shape that Rust's type system forbids, or
because it tests a failure mode unreachable through the Rust public
API (e.g. a client-side invariant the runner adds silently, such as
`gs::characters()`'s implicit `exclude_categories=["Cs"]`).

Record these in **both** places:

1. At the top of the Rust module, under an `//! Individually-skipped
   tests:` docstring listing each skipped test with a one-line
   reason. Future readers comparing the port against the Python
   original see what's missing and why.
2. In `SKIPPED.md` under the `Individually-skipped tests (rest of
   the file is ported):` section for that upstream (pbtkit or
   hypothesis), as `test_file.py::test_name — reason.`. The
   unported-gate and port audits scan this file; a skip that lives
   only in a module docstring is invisible to them.

Do **not** add the whole file to SKIPPED.md's whole-file section —
that tells the unported-gate the file is done and stops it dispatching
further work on it.

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
- **`tc.weighted(0.0)` / `tc.weighted(1.0)`** — the public API is
  missing, but the *forced* cases substitute cleanly: `tc.draw(gs::just(false))`
  / `tc.draw(gs::just(true))`. Don't skip tests just because they mention
  `tc.weighted`; check the probability first. (pbtkit uses this forcing
  idiom in `test_core.py`, `test_draw_names.py`, `test_generators.py`,
  and `test_hypothesis.py`.) Real probabilistic `tc.weighted(0.9)` etc.
  stay skipped.
- **Mixed-type `one_of` / `sampled_from`** — e.g.
  `st.one_of(st.integers(), st.tuples(st.booleans()))` or
  `st.sampled_from([1, "two", 3.0])`. Python's dynamic typing lets
  branches produce different types; Rust's `gs::one_of` requires a
  single element type. Don't skip — define a small local `enum` with
  one variant per branch, `.map(Variant::…)` each inner generator,
  and `gs::one_of(vec![…])` the lot. Example:

  ```rust
  #[derive(Debug, Clone)]
  enum Value { Int(i64), BoolTuple((bool,)) }

  let gen = gs::one_of(vec![
      gs::integers::<i64>().map(Value::Int).boxed(),
      gs::tuples!(gs::booleans()).map(Value::BoolTuple).boxed(),
  ]);
  ```

  The test body then matches on `Value` to exercise each branch. The
  enum is scaffolding for the port, not a new public API.

## Don't add `suppress_health_check` that wasn't in the original

If a ported test starts tripping a health check (`TooSlow`,
`FilterTooMuch`, `LargeBaseExample`, etc.) only after the port, do
NOT reach for `.suppress_health_check([...])`. A tripped health
check is usually signalling a real performance or
generation-rejection problem in the engine or generator — silence
it and the next test that walks the same path will silently pay
the same cost.

Before adding any suppression:

1. Check the upstream source. If the original did not call
   `suppress_health_check` (or the pbtkit equivalent), your port
   must not either. If the original DID, mirror it exactly — same
   checks, no extras.
2. If there's no upstream (native-only coverage tests), the same
   rule holds: a health check trip on a native-only test means the
   underlying path is genuinely slow / genuinely rejects too much,
   and that's the bug to fix.
3. File a TODO.yaml entry describing the slow or rejection-heavy
   path and what needs investigating. Leave the test failing, or
   native-gate it, rather than suppressing the check.

The exception is when the *purpose* of the test is to exercise the
health-check mechanism itself (e.g.
`native_too_slow_suppressed`) — those are obvious on inspection.

## When a test fails because Rust ≠ Python semantics, STOP

If a test port keeps tripping over disagreements between a Rust crate
`src/native/` is using and the Python module Hypothesis is using —
e.g. a regex crate that disagrees with Python's `re` on `\Z` vs `\z`,
a unicode crate that disagrees with CPython's `unicodedata` on some
edge codepoint, a bignum crate that disagrees with Python `int` on
shift semantics — **do not paper over it with per-test translation
shims**. That's how we end up with `translate_python_escapes`,
`normalise_category`, and other sticks of gum holding the boundary
together until it collapses.

The fix is to stop, file a TODO (or pick up the relevant existing
one), and port the Python module directly into `src/native/` as a
standalone Rust module. `src/native/unicodedata.rs` and
`src/native/bignum.rs` are worked precedents. The full rationale is
in `.claude/skills/implementing-native/SKILL.md` under "Port, don't
adapt" — read it before reaching for another third-party crate at
the semantics boundary.

In the meantime, add the failing tests to SKIPPED.md with a rationale
that names the underlying Python module needing to be ported (so the
skip is visibly blocked on a known follow-up, not "no Rust
counterpart"). **Whenever you add skips for this reason, update the
corresponding TODO.yaml entry for the port so that removing those exact
SKIPPED.md entries is part of its acceptance criteria.** If no TODO
exists yet, file one and include the skip list in the acceptance
criteria from the start. Without that link the skips become invisible
debt — the port "completes" while the tests it was meant to unblock
quietly stay skipped.

## Stateful (rule-based) tests

If the upstream file uses `hypothesis.stateful` —
`RuleBasedStateMachine`, `@rule`, `@invariant`, `Bundle`, `@initialize`,
`@precondition`, `consumes`, `multiple`, or `run_state_machine_as_test` —
follow `.claude/skills/porting-stateful/SKILL.md` for the stateful-specific
API mapping. The layout, naming, verification and skip policy rules in this
file still apply.

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
2. Update `tests/pbtkit/main.rs` (or `tests/hypothesis/main.rs`) to
   include your new module via `mod <name>;`. Both `main.rs` files and
   the matching `[[test]]` declarations in `Cargo.toml` already exist —
   do not add or modify them.
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

## Coverage witnesses the Python original doesn't have

If your port adds or pulls in `src/native/` code with a defensive
branch (a `clamp`-to-bound fallthrough, an `unreachable`-adjacent
fall-through that returns a sentinel, a `.max(0)` guard on an
arithmetic result), Python's `@example` cases often don't exercise it
— Python coverage tools don't flag it and the upstream author never
needed to. Rust's 100% line-coverage ratchet does flag it.

Don't delete the defensive branch; it's there for a reason. Don't add
`// nocov`; that needs human permission. Instead add a single small
test with a pathological input that actually hits the branch. Mark it
clearly as non-upstream so a later reviewer diffing the port against
the Python doesn't think it's missing from their audit:

```rust
// Exercise the defensive `return lower` branch of make_float_clamper:
// when the constraint is pathological (no value can satisfy both sm > max
// and -sm >= min) the clamper falls back to min_value rather than a
// value below it. Python coverage doesn't test this; Rust's ratchet does.
#[test]
fn test_float_clamper_defensive_lower() { ... }
```

One focused `#[test]` per defensive branch, same file as the port,
commit message notes the extra.

The same witness pattern applies when the upstream's *predicate shape*
is what starves coverage, not a defensive branch in the code. Budget /
call-count tests (e.g. `test_shrink_budgeting.py`, which asserts
`shrinker.calls <= 10` with a `lambda x: x == value` predicate that
accepts only the initial value) deliberately reject every
improvement, so the *mainline* improvement paths of the code under
test go unhit. Add one witness per path the budget predicate skips —
a permissive predicate to hit the short-circuit improvement arm, a
threshold predicate to walk the binary-search / mask arms, inputs
that trigger the skip-branch of a "continue if already in order"
loop, etc. Same file, same "non-upstream" comment style, commit
message notes the extras.

## Keep this skill current

As you port, you'll figure things out that aren't documented yet. When
you do, update the relevant file as part of the same commit (or a
separate follow-up commit during the same sub-loop). Additions should
be terse — tables over prose, real code over hand-waving — and only
for things not already covered somewhere else in the skill.

Where new content belongs:

- `references/api-mapping.md` — a Python→Rust translation missing from
  the cheat sheet, or a transform whose shape in Rust isn't obvious.
- `references/pbtkit-overview.md` /
  `references/hypothesis-overview.md` — structural or organizational
  facts about the upstream that would help a future porter orient.
- This file (`SKILL.md`) — a new porting-workflow rule, a recurring
  gotcha, or a clarification to the skip-vs-port policy.

Do NOT record per-file notes (one-off quirks of a single upstream file
don't belong in the skill; they belong in the port's commit message).
Do NOT rewrite existing content to match your preferences; add to it
only when there's something genuinely new.

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
