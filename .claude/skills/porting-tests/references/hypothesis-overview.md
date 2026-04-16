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
- `@example(x)` → hegel-rust has no direct analogue on `hegel::Hegel` but
  the `#[hegel::test(explicit_test_case = ...)]` attribute covers the same
  idea for named cases. Often it's simpler to convert an `@example` into a
  standalone `#[test]` that calls `minimal` or `find_any` on the exact
  value.
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

## Tests to avoid porting

- Anything that introspects `RuleBasedStateMachine` via Python reflection.
- Anything using `hypothesis.extra.*` integrations (`numpy`, `pandas`,
  `django`, `redis`, `lark`, `attrs`, `ghostwriter`).
- Anything that tests Hypothesis's CLI, its `hypothesis write` command, or
  its documentation/observability hooks.

Add such files to `SKIPPED.md` with a short rationale rather than
producing empty Rust stubs.
