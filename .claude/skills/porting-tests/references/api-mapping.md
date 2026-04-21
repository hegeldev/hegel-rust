# API mapping: pbtkit / Hypothesis → hegel-rust

Cheat-sheet for translating Python test bodies into Rust. See the overview
docs (`pbtkit-overview.md`, `hypothesis-overview.md`) for the higher-level
structure.

## Generators

| Python                                     | Rust                                                          |
|--------------------------------------------|---------------------------------------------------------------|
| `gs.integers(a, b)`                        | `gs::integers::<i64>().min_value(a).max_value(b)`             |
| `st.integers(min_value=a, max_value=b)`    | same                                                          |
| `gs.floats(min=a, max=b, allow_nan=True)`  | `gs::floats::<f64>().min_value(a).max_value(b).allow_nan(true)` |
| `gs.booleans()`                            | `gs::booleans()`                                              |
| `gs.text(min_size=, max_size=, alphabet=)` | `gs::text().min_size(n).max_size(n).alphabet(g)`              |
| `gs.binary(min_size=, max_size=)`          | `gs::binary().min_size(n).max_size(n)`                        |
| `gs.characters(categories=[...])`          | `gs::characters().categories(&["Lu", ...])`                   |
| `gs.lists(inner, min_size=, max_size=)`    | `gs::vecs(inner).min_size(n).max_size(n)`                     |
| `gs.sets(inner)`                           | `gs::hashsets(inner)`                                         |
| `gs.dictionaries(k, v)`                    | `gs::hashmaps(k, v)`                                          |
| `gs.tuples(a, b)`                          | `gs::tuples!(a, b)` (macro)                                   |
| `gs.one_of(a, b)`                          | `gs::one_of(vec![a.boxed(), b.boxed()])` (same element type; for mixed types wrap each branch in a local `enum` and `.map(Variant::…)` — see SKILL.md "Think harder before skipping") |
| `gs.sampled_from([x, y])`                  | `gs::sampled_from(vec![x, y])`                                |
| `gs.just(x)`                               | `gs::just(x)`                                                 |
| `gs.nothing()`                             | **missing** — native-gate the test and stub under `src/native/` (see SKILL.md skip-vs-port policy) |
| `gs.from_regex(pat)`                       | `gs::from_regex(pat)` (add `.fullmatch(true)` if used)        |
| `gs.emails()` / `gs.urls()`                | `gs::emails()` / `gs::urls()`                                 |
| `gs.dates()` etc.                          | `gs::dates()`, `gs::times()`, `gs::datetimes()`, `gs::durations()` |

Generator transforms (all require `Generator` trait in scope):

| Python                        | Rust                                      |
|-------------------------------|-------------------------------------------|
| `inner.map(f)`                | `inner.map(\|x\| f(x))`                   |
| `inner.filter(p)`             | `inner.filter(\|x: &T\| p(x))`            |
| `inner.flatmap(f)`            | `inner.flat_map(\|x\| f(x))`              |
| `@gs.composite def g(draw):`  | `hegel::compose!(\|tc\| { ... })` macro   |
| `gs.builds(ctor, a, b)`       | `gs::tuples!(a, b).map(\|(a,b)\| ctor(a,b))` |

## TestCase methods

| Python                     | Rust                                     |
|----------------------------|------------------------------------------|
| `tc.draw(gen)`             | `tc.draw(&gen)`                          |
| `tc.assume(cond)`          | `tc.assume(cond)`                        |
| `tc.note(msg)`              | `tc.note(msg)`                           |
| `tc.choice(n)`             | `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))` |
| `tc.weighted(p)`            | **missing** (no public API) — `todo!()`  |
| `tc.mark_status(INTERESTING)` | `panic!(...)` to signal failure        |
| `tc.target(score)`         | **missing** — `todo!()`                  |
| `ConjectureData.for_choices([v, ...])` | `NativeTestCase::for_choices(&[ChoiceValue::…, …], None)` from `hegel::__native_test_internals` (native-only) — see "Replaying fixed choices" below |
| `tc.reject()`              | `tc.assume(false)` is the closest (but see pbtkit-overview.md) |

## Settings

| Python                              | Rust                                       |
|-------------------------------------|--------------------------------------------|
| `settings(max_examples=N)`          | `Settings::new().test_cases(N)`            |
| `settings(seed=S)`                  | `Settings::new().seed(Some(S))`            |
| `settings(derandomize=True)`        | `Settings::new().derandomize(true)`        |
| `settings(database=DirectoryDB(p))` | `Settings::new().database(Database::Path(p))` (native backend only) |
| `settings(database=None)`           | `Settings::new().database(None)`           |
| `settings(suppress_health_check=...)` | `Settings::new().suppress_health_check(...)` |
| `settings(verbosity=Verbosity.debug)` | `Settings::new().verbosity(Verbosity::Debug)` |
| `settings(deadline=ms)`             | **missing** — drop the setting or `todo!()` |

## Helpers in `crate::common::utils`

| Python idiom                        | Rust helper                                |
|-------------------------------------|--------------------------------------------|
| `@given(gen) def test(x): assert p(x)` | `assert_all_examples(gen, \|x: &T\| p(x))` |
| `find(gen, cond)`                   | `find_any(gen, \|x: &T\| cond(x))`         |
| `minimal(gen, cond)`                | `minimal(gen, \|x: &T\| cond(x))`          |
| `with pytest.raises(X): ...`        | `expect_panic(\|\| { ... }, "regex")`      |
| `capture_out()` / `capsys` / `capfd` | `TempRustProject::new().main_file(CODE).cargo_run(&[])` — access `.stderr`/`.stdout` on the `RunOutput` |
| `capture_out() + pytest.raises(X)`  | `TempRustProject::new().main_file(CODE).expect_failure("pattern")` — builds, runs, asserts non-zero exit + pattern in stderr, returns `RunOutput` |

## Features deliberately missing from hegel-rust

These show up in lots of pbtkit/Hypothesis tests. When you hit one, leave
the test as `todo!()` with a clear comment and **add a TODO.md entry** for
adding the feature. Don't invent a workaround in the test.

- `tc.weighted(p)` — weighted booleans.
- `tc.target(score)` — score-directed search.
- `tc.reject()` distinguished from `tc.assume(false)`.
- `tc.forced_choice(v)` — direct replay fixture.
- `gs::nothing()` — the empty generator.
- `deadline` setting.
- Index-based shrink passes (`max_index`, `to_index`, `from_index`) on
  `StringChoice` / `BytesChoice`.

## Replaying fixed choices (`ConjectureData.for_choices`)

Hypothesis's conjecture tests often exercise a strategy against a
handwritten choice sequence via `data = ConjectureData.for_choices([...])`
followed by `s.do_draw(data)`. In hegel-rust (native mode only) the same
pattern is expressed by running the strategy inside a `CachedTestFunction`
closure and replaying a `NativeTestCase::for_choices` as the input:

```python
# Hypothesis
data = ConjectureData.for_choices([])
assert st.just("hello").do_draw(data) == "hello"
```

```rust
// hegel-rust
#[cfg(feature = "native")]
#[test]
fn test_just_does_not_draw() {
    use hegel::__native_test_internals::{CachedTestFunction, NativeTestCase};
    use hegel::TestCase;
    use std::sync::{Arc, Mutex};

    let seen = Arc::new(Mutex::new(None::<String>));
    let seen_c = Arc::clone(&seen);
    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        let v: String = tc.draw(gs::just("hello".to_string()));
        *seen_c.lock().unwrap() = Some(v);
    });
    let ntc = NativeTestCase::for_choices(&[], None);
    let (_status, nodes, _span_tree) = ctf.run(ntc);

    assert_eq!(seen.lock().unwrap().as_deref(), Some("hello"));
    assert!(nodes.is_empty()); // strategy consumed zero choice nodes
}
```

Key points:

- The closure takes the public `tc: TestCase` and calls `tc.draw(...)`
  exactly as a normal test body does — this is what lets you drive
  public-API strategies from a fixed choice sequence rather than only
  low-level `ntc.draw_bytes` / `ntc.draw_integer`.
- Captured state goes through `Arc<Mutex<_>>` because the closure is
  `move` and `ctf.run` consumes its input but does not return the
  closure's result.
- `ctf.run(ntc)` returns `(status, choice_nodes, span_tree)`. Assert on
  `nodes.is_empty()` to verify the strategy draws nothing, or on the
  nodes directly to verify what it drew.
- This is native-only — gate with `#[cfg(feature = "native")]`. In server
  mode there is no equivalent replay surface; skip that half of the test
  or make the whole test native.

Non-replay uses of `NativeTestCase::for_choices` (driving `ntc.draw_bytes`,
`ntc.draw_integer`, etc. directly without a strategy) don't need
`CachedTestFunction` — see `tests/hypothesis/simple_strings.rs::test_fixed_size_bytes_just_draw_bytes`
for that simpler shape.

## text() with characters() parameters

In Python, `text()` accepts a `characters()` strategy as its alphabet,
passing character-level constraints through. In Rust, `gs::text()`
exposes these as builder methods directly — there is no separate
`characters()` composition step.

| Python                                          | Rust                                         |
|-------------------------------------------------|----------------------------------------------|
| `text(characters(exclude_characters="\n"))`     | `gs::text().exclude_characters("\n")`        |
| `text(characters(max_codepoint=127))`           | `gs::text().max_codepoint(127)`              |
| `text(characters(exclude_categories=("Cc",)))`  | `gs::text().exclude_categories(&["Cc"])`     |

## Validation-panic tests (`InvalidArgument` in Python)

`test_validates_keyword_arguments` in Hypothesis (and similar shapes in
`test_validation.py`, `test_regex.py`, `test_sampled_from.py`,
`test_uuids.py`, …) wraps `check_can_generate_examples(fn(**kwargs))` in
`pytest.raises(InvalidArgument)`. The translation depends on *when* the
check fires:

- **Construction-time** — a few factory functions validate eagerly (e.g.
  `gs::sampled_from(vec![])` panics on empty input). Wrap the constructor:

  ```rust
  expect_panic(|| { gs::sampled_from::<i64, _>(Vec::<i64>::new()); },
               "sampled_from cannot be empty");
  ```

- **Draw-time (the common case)** — bounds checks (`min > max`), mutually
  exclusive flags (`allow_nan=true` with `min_value`), and server-rejected
  shapes only panic when the engine actually runs. `expect_panic` over a
  bare constructor won't trigger them. A small local helper keeps each
  test a one-liner:

  ```rust
  fn expect_generator_panic<T, G>(generator: G, pattern: &str)
  where
      G: Generator<T> + 'static + std::panic::UnwindSafe,
      T: std::fmt::Debug + Send + 'static,
  {
      expect_panic(
          move || {
              Hegel::new(move |tc| { tc.draw(&generator); })
                  .settings(Settings::new().test_cases(1).database(None))
                  .run();
          },
          pattern,
      );
  }

  #[test]
  fn test_integers_rejects_min_greater_than_max() {
      expect_generator_panic(
          gs::integers::<i64>().min_value(2).max_value(1),
          "max_value < min_value",
      );
  }
  ```

  The `database(None)` avoids replay noise; `test_cases(1)` keeps it fast.
  Put this helper *in the test file*, not in `tests/common/utils.rs` (see
  SKILL.md "Don't modify").

**Drop wrong-typed-kwarg cases.** Hypothesis parametrizes heavily over
values that violate the Python signature — `min_value=math.nan`,
`min_value="fish"`, `regex=123`, `alphabet=[1]`, `v="4"`, `unique_by=(...)`,
`elements="hi"`, etc. Rust's type system rejects these at compile time, so
there is no runtime behaviour to assert. List the dropped categories
once in the module docstring rather than per-case — a reviewer checking
the original against the port can see the whole class is accounted for.

## Python idiom translations

Common Python patterns that need non-trivial translation in test
predicates:

| Python                    | Rust                                                      | Why                                                                   |
|---------------------------|-----------------------------------------------------------|-----------------------------------------------------------------------|
| `minimal(text(), bool)`   | `minimal(gs::text(), \|s: &String\| !s.is_empty())`      | Python `bool(s)` is truthy = non-empty                                |
| `x >= "\udfff"` (string comparison) | `s.as_str() >= "\u{e000}"`                      | Rust strings can't contain surrogates; `\u{e000}` is the first valid codepoint past the surrogate range |

## File naming

Upstream Python filename → Rust module name drops the `test_` prefix:

- `pbtkit/tests/test_floats.py` → `tests/pbtkit/floats.rs` (module `floats`)
- `hypothesis-python/tests/cover/test_regex.py` → `tests/hypothesis/regex.rs`
- Subdirectory: `pbtkit/tests/findability/test_types.py` → flattened as
  `tests/pbtkit/findability_types.rs`.

Don't nest directories under `tests/pbtkit/`; flatten to a prefix in the
filename so there's one test binary per repo. Existing example:
`tests/test_find_quality/main.rs` uses the directory pattern, but for new
ports we prefer flat prefixes to keep the test binary count stable.
