# pbtkit overview

A short tour of pbtkit's architecture, oriented at what a porter of its
tests needs to know. Source: `/tmp/pbtkit/src/pbtkit/`.

## The core

- `core.py` — the `TestCase` class, `ChoiceType` base, `IntegerChoice`,
  `BooleanChoice`, and the `run_test` decorator. `bin_search_down` lives
  here too.
- `generators.py` — high-level combinators: `lists`, `sets`, `dicts`,
  `one_of`, `sampled_from`, `just`, `tuples`, `@composite`.
- `floats.py` — `FloatChoice`, `_draw_unbounded_float`, lex float ordering.
- `text.py` — `StringChoice`, `_draw_string`, `_codepoint_key`.
- `bytes.py` — `BytesChoice`, `_draw_bytes`.
- `database.py` — `DirectoryDB` persistent-example storage.
- `targeting.py` — `tc.target(score)` search.
- `draw_names.py` — `tc.draw` name-tracking for failure output.

## Shrinking

Everything in `shrinking/`:

- `sequence.py` — `shrink_sequence`, used by bytes and text shrinkers.
- `advanced_integer_passes.py`, `advanced_bytes_passes.py`,
  `advanced_string_passes.py` — pass implementations.
- `bind_deletion.py` — the "shrink the controlling integer and delete
  downstream" pass (hegel-rust has this as `bind_deletion` in
  `src/native/shrinker/deletion.rs`).
- `index_passes.py` — shortlex enumeration-based shrinkers (hegel-rust
  does not implement these).
- `sorting.py`, `sequence_redistribution.py`, `duplication_passes.py`,
  `mutation.py` — sequence-normalization and mutation passes.

## Choice types map onto hegel-rust

| pbtkit           | hegel-rust (`src/native/core/choices.rs`) |
|------------------|-------------------------------------------|
| `IntegerChoice`  | `IntegerChoice`                          |
| `BooleanChoice`  | `BooleanChoice`                          |
| `FloatChoice`    | `FloatChoice`                            |
| `BytesChoice`    | `BytesChoice`                            |
| `StringChoice`   | `StringChoice`                           |

All of these expose `simplest()`, `unit()`, `validate()`, `sort_key()` in
both projects. In hegel-rust they are re-exported via
`hegel::__native_test_internals::{IntegerChoice, BooleanChoice, FloatChoice,
BytesChoice, StringChoice}` (native-only, `#[doc(hidden)]`) — so you can
exercise them from a `#[cfg(feature = "native")]` submodule inside the
normal pbtkit integration test. The embedded-tests mirror at
`tests/embedded/native/choices_tests.rs` is also valid but not required.

## Test-side fixtures that *don't* port

- `tc.for_choices([...])` (pbtkit-internal replay shim)
- `tc.weighted(p)` (no equivalent public API in hegel-rust)
- `tc.target(score)` (no public API)
- `tc.mark_status(Status.INTERESTING)` (no public API; `panic!` is the
  hegel-rust equivalent)
- `tc.choice(n)` → `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))`
- `tc.forced_choice(n)` — `forced` is an internal argument on native
  `draw_integer` / `weighted`, not exposed on the public `TestCase`.

## Engine-harness surfaces with no public handle

pbtkit's internal tests routinely drive the engine directly. hegel-rust
has equivalents under `src/native/` but does not expose them, so these
shapes are structurally unportable — add them to the port's
individually-skipped list (`SKIPPED.md` plus a module-docstring bullet)
and move on. They recur across `test_core.py`, `test_spans.py`,
`test_draw_names.py`, `test_floats.py`, `test_text.py`,
`shrink_quality/`, and `findability/`.

- `PbtkitState(random, tf, max_examples).run()` and inspecting
  `state.result` / `state.calls` — the native runner
  (`native/runner.rs`) is driven by `run_native_test` with no
  intermediate-state accessor.
- `SHRINK_PASSES` as an introspectable list, and looking up a single
  pass by `p.__name__` — hegel-rust's shrink passes are `pub(super)`
  methods on `native::shrinker::Shrinker` reachable only via the
  all-at-once `Shrinker::shrink()` entry point.
- `Shrinker(state, initial, is_interesting=fn)` with a custom
  interesting predicate — same reason; no hand-built `Shrinker`
  instantiation from tests.
- `CachedTestFunction([raw_values])` / `.lookup([raw_values])` — pbtkit
  takes a raw choice-value list. hegel-rust's `CachedTestFunction`
  takes a `NativeTestCase` (see `api-mapping.md` "Replaying fixed
  choices" for the shape that does port).
- `pbtkit.caching._cache_key` — hegel-rust's equivalent
  `ChoiceValueKey` lives private to `src/native/tree.rs` with no
  public hook.
- `Frozen` exception raised when a completed `TestCase` is reused —
  no counterpart error type is exported.

## `@pytest.mark.requires(...)` and `pytestmark`

pbtkit's `conftest.py` defines a `requires(module)` marker that skips a
test when the named pbtkit feature is disabled via `PBTKIT_DISABLED` —
e.g. `@pytest.mark.requires("collections")`,
`@pytest.mark.requires("shrinking.sorting")`,
`@pytest.mark.requires("shrinking.bind_deletion")`, or a module-level
`pytestmark = pytest.mark.requires(...)`. These are feature gates for
pbtkit's own compiled-mode builds, not test preconditions. hegel-rust
always has the corresponding behaviour (the listed modules map to
`src/native/shrinker/` passes and the server backend), so port such
tests unconditionally — strip the marker and don't record it anywhere.
The only time one of these should influence the port is if the required
feature genuinely has no counterpart; in that case follow the
native-gated-plus-stub policy in SKILL.md.

## Findability and shrink-quality tests

`tests/findability/` and `tests/shrink_quality/` in pbtkit are directly
analogous to `tests/test_find_quality/` and `tests/test_shrink_quality/` in
hegel-rust — same spirit, sometimes same test names. When porting one of
these, check whether an existing hegel-rust file already covers the same
ground before adding a new one.
