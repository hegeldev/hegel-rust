# Hypothesis overview

A short tour of Hypothesis's test structure, oriented at what a porter of
its tests needs to know. Source: `/tmp/hypothesis/hypothesis-python/`.

## Test layout

Tests live under `hypothesis-python/tests/`:

- `cover/` — library-behaviour tests. This is the primary porting target.
  Tests here exercise generator behaviour (`integers()`, `lists()`,
  `floats()`, etc.) and the user-facing API. Most port.
- `conjecture/` — engine internals (the Hypothesis shrinker, `ConjectureData`,
  example-database serialization). These map onto `src/native/` in
  hegel-rust; port as `#![cfg(feature = "native")]` integration tests or
  as embedded tests under `tests/embedded/`.
- `quality/` — shrink-quality tests. Similar to `pbtkit/tests/shrink_quality/`;
  port into `tests/test_shrink_quality/` when the pattern overlaps.
- `nocover/` — tests that are intentionally excluded from Hypothesis's
  own coverage measurement. Port judgmentally.
- `pytest/` — tests for Hypothesis's pytest plugin (conftest hooks,
  fixture ordering, `capsys`-based output capture, subprocess-invoked
  pytest runs). Most skip with "pytest plugin integration — hegel-rust
  has no pytest plugin" rationale. Occasional files here are just
  generic `@given` / `@fails` smoke tests that happen to live in this
  directory (e.g. `test_runs.py`) — those port trivially. Read each
  file before deciding; don't bulk-skip the directory.
- Python-specific dirs (`django/`, `numpy/`, `pandas/`, `attrs/`, …) —
  **skip** unless the dependency has a Rust analogue, which it usually
  doesn't.

## Strategy → Generator map

Hypothesis "strategies" correspond to hegel-rust "generators".

| Hypothesis (`from hypothesis import strategies as st`) | hegel-rust |
|--------------------------------------------------------|------------|
| `st.integers(min_value=a, max_value=b)` | `gs::integers::<i64>().min_value(a).max_value(b)` |
| `st.floats(min_value=a, max_value=b, allow_nan=False)` | `gs::floats::<f64>().min_value(a).max_value(b).allow_nan(false)` |
| `st.booleans()` | `gs::booleans()` |
| `st.text(alphabet=..., min_size=, max_size=)` | `gs::text().alphabet(...).min_size(...).max_size(...)` |
| `st.binary(min_size=, max_size=)` | `gs::binary().min_size(...).max_size(...)` |
| `st.lists(inner, min_size=, max_size=, unique=)` | `gs::vecs(inner).min_size(...).max_size(...).unique()` |
| `st.sets(inner)` | `gs::hashsets(inner)` |
| `st.dictionaries(keys, values)` | `gs::hashmaps(k, v)` |
| `st.tuples(a, b, ...)` | `gs::tuples!(a, b, ...)` |
| `st.one_of(a, b, ...)` | `gs::one_of(vec![a.boxed(), b.boxed()])` |
| `st.sampled_from([...])` | `gs::sampled_from(vec![...])` |
| `st.just(x)` | `gs::just(x)` |
| `st.none()` | `gs::just(())` or `gs::unit()` |
| `st.from_regex(pat, fullmatch=True)` | `gs::from_regex(pat).fullmatch(true)` |
| `st.characters(categories=..., whitelist_categories=..., ...)` | `gs::characters().categories(&[...])` |

## `tests/snapshots/` — syrupy `.ambr` snapshot tests

Every file in `hypothesis-python/tests/snapshots/` uses syrupy to pin
Hypothesis's `Falsifying example: inner(arg=...)` stderr output with a
corresponding `__snapshots__/<file>.ambr` file. The body is always
shaped:

```python
def test_X(snapshot):
    @SNAPSHOT_SETTINGS  # generate + shrink phases, derandomize, no DB
    @given(arg=st.whatever())
    def inner(arg):
        assert <invariant>

    assert run_test_for_falsifying_example(inner) == snapshot
```

The underlying claim is about the **shrunk counterexample**, not the
stderr format. Port as `minimal(gen, |v: &T| !<invariant>(v))` and
assert on the returned value directly against the `arg=...` shown in
the `.ambr` file. No `TempRustProject` / stderr capture is needed.

Backend-divergent shrink targets are common here: the native engine
shrinks one way, the server engine's choice protocol shrinks another.
If both find valid (but different) minima, branch with inline
`#[cfg(feature = "native")]` inside the test and assert each target
rather than skipping either backend — see
`tests/hypothesis/snapshots_shrinking.rs::test_shrunk_string` (native
→ `'A'` matches upstream, server → `'À'`). Skipping or gating to one
backend here discards real coverage.

## Test-side fixtures that *don't* port

- `@given(st....)` decorator → the test body goes into a closure passed
  to `hegel::Hegel::new(...)` or `assert_all_examples(...)`.
- `@example(x)` → hegel-rust has no direct analogue on `hegel::Hegel`.
  When a `@given` test is preceded by a stack of `@example`s, split into
  two `#[test]` functions sharing a check helper — see "@example stack +
  @given" in `api-mapping.md`. For a single named example,
  `#[hegel::test(explicit_test_case = ...)]` or a standalone `#[test]`
  calling `find_any` / `minimal` is fine.
- `settings(max_examples=N)` → `Settings::new().test_cases(N)`.
- `settings(deadline=...)` → hegel-rust has no deadline API; drop the
  setting or leave the test as `todo!()` if deadline is load-bearing.
- `assume(cond)` → `tc.assume(cond)` (same spelling).
- `note(msg)` → `tc.note(msg)`.
- `target(score)` on the public `TestCase` → still no hegel-rust API;
  leave user-facing `tc.target(...)` calls as `todo!()`. For
  `conjecture/test_optimiser.py`-shape tests that build their own
  runner and assert on `target_observations` /
  `best_observed_targets` / `optimise_targets`, use the native-only
  `TargetedRunner` / `TargetedTestCase` / `BufferSizeLimit` surface
  exposed via `__native_test_internals` — see
  `tests/hypothesis/conjecture_optimiser.rs`.
- `find(strategy, predicate)` → `minimal(generator, predicate)` from
  `crate::common::utils`.
- `capsys` / `capfd` fixtures → use `TempRustProject` from
  `tests/common/project.rs` to run the body as a subprocess and capture
  stderr (see `tests/test_output.rs` for the pattern).
- `@skipif_threading` / `@skipif_time_unpatched` (from
  `tests.common.utils`) → elide. The guard skips the test under
  Hypothesis's free-threaded-Python (`PYTHON_GIL=0`) test profile, a
  CPython-only concern with no hegel-rust analogue. Don't try to mirror
  it — porting a `@skipif_threading`-decorated test means dropping the
  decorator entirely.
- pytest's `monkeypatch` fixture used to swap a Hypothesis
  module-level global or a runtime attribute on a strategy/database
  instance — `monkeypatch.setattr(hypothesis.core, "global_force_seed",
  N)`, `database.fetch = None`, `monkeypatch.setattr(ConjectureRunner,
  "generate_new_examples", ...)`, etc. These tests exploit Python's
  module-mutability and dunder-attribute-assignment to override
  internal references for a single test. Rust has no equivalent
  surface: there's no writable module-level `global_force_seed`
  (seeds go through `Settings::new().seed(Some(n))`), no runtime
  attribute reassignment on `NativeDatabase` / generator structs,
  and no swap-this-method-on-a-class-instance hook. The test usually
  asserts a *negative* (e.g. "`fetch` was not called" via assigning a
  non-callable sentinel and observing no error) which has no Rust-side
  observation either. Skip with a rationale naming the patched
  reference. `pbtkit-overview.md`'s "Module-constant monkeypatches"
  section covers a related but distinct pattern (threshold/probability
  patches for coverage, where the patch is sometimes droppable);
  Hypothesis-side patches are typically semantic and skip wholesale.

## Shared test fixtures

### `tests.common.standard_types`

A heterogeneous `list[SearchStrategy]` defined in
`hypothesis-python/tests/common/__init__.py`, used to parametrize
"behaves consistently across strategy types" tests (e.g.
`nocover/test_collective_minimization.py`, `cover/test_draw_example.py`,
`nocover/test_fixtures.py`). Python iterates it via
`@pytest.mark.parametrize("spec", standard_types, ids=repr)`; Rust
can't, because each entry has a different concrete strategy type.

Port as one `#[test]` per entry, sharing a generic check helper. Dedup
only the exact duplicates (`standard_types` lists `floats()` four
times); otherwise port every entry that has a hegel-rust analog —
`sampled_from`, `one_of`, `fixed_dicts`, `flat_map`, `filter`, bounded
and unbounded `integers()`/`floats()`, and the various tuples and
nested lists all have direct counterparts. `tests/hypothesis/draw_example.rs`
is the reference mapping for how each entry translates.

Entries with no hegel-rust counterpart (skip per the api-mapping table):
`complex_numbers()`, `fractions()`, `decimals()`, `recursive()`, and
any flatmap that bottoms out in one of those. `randoms()` is
feature-gated (`#[cfg(feature = "rand")]`).

### `try/except Unsatisfiable: pass` in `standard_types` loops

When the test body is wrapped in `try: ... except Unsatisfiable: pass`,
any strategy whose outputs all share one repr hits the `Unsatisfiable`
branch because the "≥ 2 distinct reprs" predicate is vacuously false.
These rows carry no signal and must be omitted — Rust's `Minimal::run`
panics when nothing satisfies the predicate, so translating them
literally turns a silent skip into a test failure. Categories to drop:

- Always-empty collections: `lists(none(), max_size=0)`, `tuples()`,
  `sets(none(), max_size=0)`, `frozensets(none(), max_size=0)`,
  `fixed_dictionaries({})`.
- Single-point strategies: `just(...)`, `none()`,
  `floats(min_value=x, max_value=x)`.
- Strategies whose per-example output is a single repeated value:
  `lists(floats(0.0, 0.0))`, `integers().flatmap(lambda v: lists(just(v)))`.

Don't translate the `try/except` guard itself — it was only there to
keep Python's parametrize sweep from failing on the single-value
entries. List each dropped entry in the module docstring.

### Debug-format determinism in `standard_types` loops

When the test body uses `repr()`/`format!("{:?}", ...)` as the identity
(as collective-minimization does), strategies that return `HashMap` or
`HashSet` — `dictionaries(...)`, `frozensets(...)`, `sets(frozensets(...))`
— must also be dropped: Rust's default hasher randomises iteration
order, so two equal maps can print differently and the "≤ 3 distinct
reprs" assertion fails spuriously. Note this in the docstring
separately from the unsatisfiable drops.

## Tests to avoid porting

- Anything that introspects `RuleBasedStateMachine` via Python reflection.
- Anything using `hypothesis.extra.*` integrations (`numpy`, `pandas`,
  `django`, `redis`, `lark`, `attrs`, `ghostwriter`).
- Anything that tests Hypothesis's CLI, its `hypothesis write` command, or
  its documentation/observability hooks.

Add such files to `SKIPPED.md` with a short rationale rather than
producing empty Rust stubs.
