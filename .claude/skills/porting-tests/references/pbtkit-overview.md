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

## Engine-harness surfaces — port as embedded tests

pbtkit's internal tests routinely drive the engine directly. hegel-rust
has equivalents under `src/native/` but they're `pub(crate)` / `pub(super)`
so they can't be called from the pbtkit integration test in
`tests/pbtkit/`. The port location is **`tests/embedded/native/*_tests.rs`**
instead: embedded tests are wired into the source via
`#[cfg(test)] #[path = "..."] mod tests;`, giving them `use super::*`
access to everything the module sees. Existing precedent:
`tests/embedded/native/shrinker_tests.rs` ports `test_bin_search_down_lo_satisfies`,
`test_swap_adjacent_blocks_equal_blocks`, and many more; `tree_tests.rs`
ports `test_cache_key_distinguishes_negative_zero` /
`test_cache_key_distinguishes_nan_variants`.

Shapes that port this way (do NOT skip them — see SKILL.md "NOT reasons to skip"):

- `SHRINK_PASSES` lookup by name, `Shrinker(state, initial, is_interesting=fn)` —
  hegel-rust's `Shrinker::new(Box::new(|nodes| (is_interesting, len)), initial_nodes)`
  plus direct calls to the individual `pub(super)` pass methods
  (`shrinker.delete_chunks()`, `shrinker.swap_adjacent_blocks()`,
  `shrinker.bind_deletion()`, etc.). Co-locate with an existing shrink-pass
  embedded test.
- `pbtkit.caching._cache_key` — use `ChoiceValueKey::from(&ChoiceValue::...)`
  inside `tests/embedded/native/tree_tests.rs`.
- `CachedTestFunction([raw_values])` / `.lookup([raw_values])` — the
  hegel-rust shape is `NativeTestCase::for_choices(&[ChoiceValue], None)`
  fed through `ctf.run(ntc)` / `ctf.run_shrink(candidate_nodes)`. Live in
  `tests/embedded/native/tree_tests.rs`.
- `PbtkitState(random, tf, max_examples).run()` + inspecting `state.result`
  — there's no state-equivalent handle on `native_run`, but the behaviour
  the upstream tests care about is almost always the shrinker's output.
  Drive `Shrinker::new(...).shrink()` (or the specific pass) directly
  from an embedded test and assert on `shrinker.current_nodes`.
- `Frozen` exception on a reused completed `TestCase` — hegel-rust's
  equivalent is `Status` plus the guards inside `NativeTestCase`
  methods; exercise them from an embedded test.

### Shrinker model divergence: `current.nodes` is not truncated on accept

pbtkit's `Shrinker.consider` routes through `state.test_function`, which
populates `test_case.nodes` with only the *drawn* prefix; that trimmed
sequence becomes `current.nodes` on accept. hegel-rust's
`Shrinker::consider` (see `src/native/shrinker/mod.rs`) stores the full
input `nodes.to_vec()` verbatim, with no truncation to actually-consumed
length.

Consequence: several pbtkit regression tests are specifically designed
around "a previously-accepted candidate leaves `current.nodes` shorter
than an index pass expects, and the pass must guard against the stale
index". Those failure modes don't arise in hegel-rust — the indices
stay valid because the sequence doesn't shrink underneath them. Port
the general-purpose pass regressions normally (they exercise the same
deletion / redistribution / sorting logic), but record any test whose
*whole point* is the stale-index-after-truncation regression as an
individual skip, naming this divergence.

Known-affected cases from `test_core.py`:
`test_value_punning_on_type_change`, `test_bind_deletion_valid_but_not_shorter`,
`test_delete_chunks_stale_index`, `test_shrink_duplicates_with_stale_indices`,
`test_shrink_duplicates_valid_drops_below_two`.

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
