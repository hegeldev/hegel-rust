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
- `target(score)` → no hegel-rust API yet; leave as `todo!()`.
- `find(strategy, predicate)` → `minimal(generator, predicate)` from
  `crate::common::utils`.
- `capsys` / `capfd` fixtures → use `TempRustProject` from
  `tests/common/project.rs` to run the body as a subprocess and capture
  stderr (see `tests/test_output.rs` for the pattern).

## Shared test fixtures

### `tests.common.standard_types`

A heterogeneous `list[SearchStrategy]` defined in
`hypothesis-python/tests/common/__init__.py`, used to parametrize
"behaves consistently across strategy types" tests (e.g.
`nocover/test_collective_minimization.py`, `cover/test_draw_example.py`,
`nocover/test_fixtures.py`). Python iterates it via
`@pytest.mark.parametrize("spec", standard_types, ids=repr)`; Rust
can't, because each entry has a different concrete strategy type.

Port as one `#[test]` per representative strategy, sharing a generic
check helper. Cover the breadth of the Python list (booleans, bounded
and unbounded integers, floats with various bound configurations, text,
binary, tuples, `sampled_from`, nested lists) rather than mirroring
every entry 1:1 — `standard_types` includes strategies with no hegel-rust
analog (`complex_numbers()`, `fractions()`, `decimals()`, `randoms()`,
`frozensets()`, `recursive()`) that you skip per the api-mapping table.

### `try/except Unsatisfiable: pass` in `standard_types` loops

When the test body is wrapped in `try: ... except Unsatisfiable: pass`,
strategies that can only produce a single value — `just("a")`,
`tuples()`, `lists(none(), max_size=0)`, `fixed_dictionaries({})`,
`none()` — hit the `Unsatisfiable` branch because the predicate
(typically "at least 2 distinct values") is vacuously false. These rows
carry no signal; omit them from the Rust port with a one-line note in
the module docstring naming the class of strategies dropped. Don't
translate the `try/except` guard itself — it was only there to keep
Python's parametrize sweep from failing on the single-value entries.

## Tests to avoid porting

- Anything that introspects `RuleBasedStateMachine` via Python reflection.
- Anything using `hypothesis.extra.*` integrations (`numpy`, `pandas`,
  `django`, `redis`, `lark`, `attrs`, `ghostwriter`).
- Anything that tests Hypothesis's CLI, its `hypothesis write` command, or
  its documentation/observability hooks.

Add such files to `SKIPPED.md` with a short rationale rather than
producing empty Rust stubs.
