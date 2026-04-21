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
both projects. They are `pub(crate)` in hegel-rust, so tests that exercise
them go in `tests/embedded/native/choices_tests.rs` rather than a pbtkit
integration test.

## Test-side fixtures that *don't* port

- `tc.for_choices([...])` (pbtkit-internal replay shim)
- `tc.weighted(p)` (no equivalent public API in hegel-rust)
- `tc.target(score)` (no public API)
- `tc.mark_status(Status.INTERESTING)` (no public API; `panic!` is the
  hegel-rust equivalent)
- `tc.choice(n)` → `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))`

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
