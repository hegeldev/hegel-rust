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
| `gs.floats(..., width=N)` (N in 64/32/16)  | width is the Rust element type, not a runtime parameter — `gs::floats::<f64>()` / `gs::floats::<f32>()`. There is no `f16` generator. Drop the `width=[64,32,16]` parametrize and port the `f64` case only (see `tests/hypothesis/float_nastiness.rs`, `numerics.rs` for precedents). |
| `gs.floats(min_value=inf, ...)` / `gs.floats(max_value=-inf, ...)` | Hypothesis infers `allow_infinity=True` when a bound is infinite; hegel-rust keys `allow_infinity` purely off whether bounds are set and rejects infinite bounds regardless. Filter infinite bounds out with `tc.assume(!b.is_infinite())` when porting fuzz-style bounds tests. Tests that assert on the specific Hypothesis error message for inf-bound + `allow_infinity=False` (e.g. `test_numerics.py::test_floats_message`) can't be ported — hegel-rust's default-bound fill-in (`max_value=f64::MAX` when `allow_infinity=False`) masks the error with a different one. |
| `gs.booleans()`                            | `gs::booleans()`                                              |
| `gs.text(min_size=, max_size=, alphabet=)` | `gs::text().min_size(n).max_size(n).alphabet(g)`              |
| `st.text("ascii")` / `st.text("utf-8")` (positional codec-name warning) | `gs::text()` has both `.alphabet(...)` and `.codec(...)` as distinct builder methods, so there is no single-positional-arg surface that could be ambiguous between codec and alphabet. Hypothesis tests asserting on its "it seems like you are trying to use the codec…" warning (`test_validation.py::test_warn_on_strings_matching_common_codecs`) have no warning to observe; skip them with the codec/alphabet-disambiguation rationale. |
| `gs.binary(min_size=, max_size=)`          | `gs::binary().min_size(n).max_size(n)`                        |
| `gs.characters(categories=[...])`          | `gs::characters().categories(&["Lu", ...])`                   |
| `gs.lists(inner, min_size=, max_size=)`    | `gs::vecs(inner).min_size(n).max_size(n)`                     |
| `gs.sets(inner)`                           | `gs::hashsets(inner)`                                         |
| `gs.dictionaries(k, v)`                    | `gs::hashmaps(k, v)`                                          |
| `gs.fixed_dictionaries({k: gen, ...})`     | `gs::fixed_dicts().field(name, gen).build()` — returns `ciborium::Value::Map` (a `Vec<(Value, Value)>`, so insertion order from `.field()` is preserved). No `optional=` kwarg; only string keys; skip or adapt those rows. |
| `gs.lists(inner, unique=True)`             | `gs::vecs(inner).unique(true)`                                |
| `gs.lists(inner, unique_by=f)` / `unique_by=(f, g)` | **missing** — `VecGenerator` exposes only `.unique(bool)`. Skip with rationale. |
| `gs.frozensets(inner)`                     | **missing** — no `gs::frozensets()`. Drop the frozenset parametrize row; port the list/set rows. |
| `gs.tuples(a, b)`                          | `gs::tuples!(a, b)` (macro)                                   |
| `gs.one_of(a, b)`                          | `gs::one_of(vec![a.boxed(), b.boxed()])` (same element type; for mixed types wrap each branch in a local `enum` and `.map(Variant::…)` — see SKILL.md "Think harder before skipping") |
| `gs.sampled_from([x, y])`                  | `gs::sampled_from(vec![x, y])`                                |
| `gs.just(x)`                               | `gs::just(x)`                                                 |
| `gs.nothing()`                             | **missing** — native-gate the test and stub under `src/native/` (see SKILL.md skip-vs-port policy) |
| `gs.decimals(...)`                         | **missing** — Python-stdlib `decimal.Decimal` has no Rust counterpart and no `gs::decimals()` exists. Skip tests whose strategy is `decimals()` with a one-line `decimal.Decimal`-absence rationale in SKIPPED.md. |
| `gs.fractions(...)`                        | **missing** — Python-stdlib `fractions.Fraction` has no Rust counterpart and no `gs::fractions()` exists. Skip as above. |
| `gs.from_regex(pat)`                       | `gs::from_regex(pat)` (add `.fullmatch(true)` if used)        |
| `gs.emails()` / `gs.urls()`                | `gs::emails()` / `gs::urls()`                                 |
| `gs.dates()` etc.                          | `gs::dates()`, `gs::times()`, `gs::datetimes()`, `gs::durations()` |
| `st.data()` (the "draw inside the test" strategy) | **no analog — and that's fine**: the test body's `tc: TestCase` already exposes `tc.draw(...)`. A Hypothesis `@given(st.integers(), st.data()) def t(x, data): data.draw(...)` ports as `Hegel::new(\|tc\| { let x = tc.draw(...); let y = tc.draw(...); })`. Even `@given(st.data(), st.data())` usually ports as two consecutive `tc.draw()` calls — the `Draw 1` / `Draw 2` numbering in failure output still lines up. Only skip when the test genuinely calls `.filter` / `.map` / `.flatmap` *on the strategy object itself* (`st.data().filter(...)`), or uses `repr(st.data())`. |
| `find(st.data(), lambda data: data.draw(g) ...)` | `minimal(hegel::compose!(\|tc\| { tc.draw(g) }), predicate)` — `st.data()` inside a `find()` is a generator with dynamic draws, which is exactly `compose!`. If the Python asserts on `data.conjecture_data.choices`, substitute an assertion on the returned minimal value (the engine-internal accessor has no public counterpart). |

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
| `tc.draw(gen)`             | `tc.draw(gen)` — pass inline generators by value; this matches the established style in `tests/pbtkit/`. A blanket `impl Generator<T> for &G` means `tc.draw(&gen)` also compiles, and you do need the `&` when `gen` is a local variable reused across iterations of a `move` closure (a move would error on the second test case). |
| `data.draw(gen)` (where `data = st.data()`) | `tc.draw(gen)` — the Hypothesis "data" object is the same surface as hegel-rust's `tc` |
| `data.draw(gen, label="X")` | `tc.__draw_named(gen, "X", false)` — the third arg is `repeatable`; `false` matches Hypothesis's per-draw-numbered behaviour |
| `tc.assume(cond)`          | `tc.assume(cond)`                        |
| `tc.note(msg)`              | `tc.note(msg)`                           |
| `tc.choice(n)`             | `tc.draw(gs::integers::<i64>().min_value(0).max_value(n-1))` |
| `tc.weighted(p)`            | **missing** (no public API) — `todo!()`  |
| `tc.mark_status(INTERESTING)` | `panic!(...)` to signal failure        |
| `tc.target(score)`         | **missing** — `todo!()`                  |
| `ConjectureData.for_choices([v, ...])` | `NativeTestCase::for_choices(&[ChoiceValue::…, …], None)` from `hegel::__native_test_internals` (native-only) — see "Replaying fixed choices" below |
| `tc.reject()`              | `tc.reject()` — public method, equivalent to `assume(false)` but returns `!` so following code is statically unreachable |

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
| `minimal(gen, cond, max_examples=N)` | `Minimal::new(gen, \|x: &T\| cond(x)).test_cases(N).run()` — the one-shot `minimal()` helper hardcodes 500; use the `Minimal` builder when you need a different budget. |
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
- `phases` / `Phase.generate` / `Phase.shrink` — no phase control. See
  "Seeded `find()`" below for how to emulate no-shrinking semantics.
- `settings.default` — the Python module-level mutable settings global.
  hegel-rust constructs settings per-test via `Settings::new()`; there
  is no writable default to inspect or swap. Skip tests that read or
  write `settings.default`.
- `find()` + predicate-call-count assertions — tests that drive
  `find(strategy, predicate)` and assert an exact / bounded count on
  a counter incremented inside the predicate (`count == max_examples`,
  `count <= 10*max_examples`, etc.) are unportable. `Hegel::new(...).run()`
  re-enters the test function for span-mutation attempts (up to 5 per
  valid case in native), so the predicate-call shape Python's `find()`
  pins down isn't reproducible through the public Rust surface. Skip
  with a rationale naming the span-mutation re-entry.
- `pytest.skip()` inside a `@given` body aborting shrinking —
  hegel-rust has no per-test "skip-aborts-shrinking" mechanism on the
  public API. Skip.
- `hypothesis.reporting.debug_report(msg)` / `verbose_report(msg)` —
  verbosity-gated user-logging helpers that print only at
  `Verbosity.debug` / `Verbosity.verbose`. hegel-rust's nearest
  analog is `tc.note(msg)`, which is **verbosity-independent** and
  only fires on the final failing-test replay. Tests that assert
  "message appears at debug but not at verbose" (or vice versa)
  cannot be reproduced — skip individually with a rationale naming
  `debug_report` / `verbose_report`.
- `@flaky(max_runs=N, min_passes=M)` — Hypothesis's retry-on-failure
  decorator for tests whose predicate depends on external
  nondeterminism (set iteration order, `PYTHONHASHSEED`, etc.).
  hegel-rust's engine classifies any nondeterministic predicate *inside*
  the property run as a `Flaky test detected` bug and panics before the
  outer retry gets a chance. If the nondeterminism comes from inside the
  predicate, skip with a rationale naming the `@flaky` decorator. If it
  comes from a seedable source (a `Random(seed)`, a time-of-day), seed
  it deterministically in the port instead.

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

### Inspecting `NativeTestCase` state mid-closure

Hypothesis tests sometimes assert on engine-internal bookkeeping on the
`ConjectureData` after a draw — `data.has_discards`, `data.events`,
`data.spans`, etc. To read these from inside a `CachedTestFunction`
closure, grab the current handle via `with_native_tc`:

```rust
use hegel::__native_test_internals::{CachedTestFunction, NativeTestCase, with_native_tc};

let hd = Arc::new(Mutex::new(false));
let hd_c = Arc::clone(&hd);
let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
    tc.draw(gs::integers::<i64>().filter(|x| *x == 0));
    let flag = with_native_tc(|handle| handle.unwrap().lock().unwrap().has_discards);
    *hd_c.lock().unwrap() = flag;
});
ctf.run(NativeTestCase::for_choices(&[ChoiceValue::Integer(1), ChoiceValue::Integer(0)], None));
assert!(*hd.lock().unwrap());
```

`with_native_tc` is re-exported via `__native_test_internals`; it yields
`Option<&NativeTestCaseHandle>` (a `Mutex<NativeTestCase>`). The handle is
always set during `ctf.run`. If the field you want to read isn't populated
yet (e.g. `has_discards` was a no-op before it was wired up to
`stop_span(discard=true)`), the native backend needs a small bookkeeping
change to track it — same shape as any other native-gated port that
surfaces a missing feature.

## Health checks

hegel-rust's `HealthCheck` enum has four variants — `FilterTooMuch`,
`TooSlow`, `TestCasesTooLarge`, `LargeInitialTestCase` — a subset of
Hypothesis's. When a check fires, the native runner **panics** with a
message of the form `FailedHealthCheck: …<VariantName>…`. There is no
dedicated error type to catch.

| Python                                             | Rust                                                                |
|----------------------------------------------------|---------------------------------------------------------------------|
| `pytest.raises(FailedHealthCheck)`                 | `expect_panic(\|\| { ... }, "FailedHealthCheck.*<Variant>")`        |
| `pytest.raises(Unsatisfiable)` / `@fails_with(Unsatisfiable)` over an always-rejecting test | `expect_panic(\|\| { ... }, "FilterTooMuch")` — Hypothesis's `Unsatisfiable` for "every draw rejected" maps to hegel-rust's `FilterTooMuch` health check. **Native-only** (server mode silently passes on all-rejected runs). Other `Unsatisfiable` causes — explicit `nothing()`, deadline exhaustion — have no Rust analog and skip per the api-mapping rows for those features. |
| `suppress_health_check=list(HealthCheck)`          | `.suppress_health_check(HealthCheck::all())`                        |
| `suppress_health_check=[HealthCheck.filter_too_much, HealthCheck.too_slow]` | `.suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow])` |
| `HealthCheck.data_too_large`                       | `HealthCheck::TestCasesTooLarge`                                    |
| `HealthCheck.large_base_example`                   | `HealthCheck::LargeInitialTestCase`                                 |

`HealthCheck::all()` returns `[HealthCheck; 4]`, which satisfies
`IntoIterator<Item = HealthCheck>` — it plugs straight into
`.suppress_health_check(...)` without a `.to_vec()`.

### Native-only enforcement

`TooSlow` and `FilterTooMuch` are enforced by the native runner only —
the Python/server backend does not raise them. A test whose purpose is
to *trip* one of these checks **must be `#[cfg(feature = "native")]`**;
a test that only *suppresses* the check can be unconditional. See
`tests/test_health_check.rs` for the canonical shape and
`tests/hypothesis/health_checks.rs` for a port that splits the two
halves accordingly.

### Python variants with no Rust counterpart

These Python `HealthCheck` variants have no analog — tests targeting
them go to `SKIPPED.md` with a one-line reason:

- `return_value` — Python closures can return non-None; Rust closures
  have declared return types.
- `differing_executors` — detects `@given` on instance methods called
  with different `self`. hegel-rust tests are closures, no class
  dispatch.
- `nested_given` — detects `@given` functions called from inside other
  `@given` functions. hegel-rust has no decorator-based dispatch to
  nest.
- Anything `deadline`-related — no `deadline` setting exists.

Dynamic-typing checks such as `test_it_is_an_error_to_suppress_non_iterables`
(passing a non-iterable / non-`HealthCheck` to `suppress_health_check`)
are prevented at compile time by Rust's `impl IntoIterator<Item = HealthCheck>`
bound — skip them.

## Hypothesis `__notes__` → hegel-rust stderr `let` lines

When a Hypothesis test fails, drawn values (and `note(...)` lines) are
attached to the exception's PEP 678 `__notes__`. Tests that capture the
exception and assert on `__notes__` content port to `TempRustProject`
assertions on stderr — but the *line shape* differs:

| Hypothesis `__notes__` entry          | hegel-rust stderr line             |
|---------------------------------------|------------------------------------|
| `Draw 1: [0, 0]`                      | `let draw_1 = [0, 0];`             |
| `Draw 2: 0`                           | `let draw_2 = 0;`                  |
| `Draw 1 (Some numbers): [0, 0]`       | `let some_numbers = [0, 0];`       |
| `Draw 2 (A number): 0`                | `let a_number = 0;`                |

Notes:

- The label *replaces* the `draw_N` placeholder — there is no `draw_N`
  prefix when a label is given.
- Labels with spaces / capitalisation in Python become snake_case Rust
  identifiers (`"Some numbers"` → `some_numbers`). Pick a label the
  port can pass directly to `tc.__draw_named(..., "name", false)`.
- Values use Rust's `Debug` formatting (`[0, 0]`, not `[0, 0]` — usually
  identical for primitives, but watch strings: `"foo"` Debug-prints with
  quotes whereas Python's `repr` is the same shape, so most cases match).
- Assert with `output.stderr.contains("let draw_1 = [0, 0];")` rather
  than a regex when the value is concrete; `output` comes from
  `TempRustProject::new().main_file(CODE).expect_failure(PANIC).cargo_run(&[])`.

## Stateful filter closures

Python tests frequently construct a `.filter(f)` where `f` closes over a
mutable local (a set, a counter) — e.g. the `unhealthy_filter` pattern
from `test_health_checks.py` that rejects until it has seen 200 values,
then starts accepting. `Generator::filter` takes `F: Fn + Clone`, so the
state must be shared through interior mutability:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

let counter = Arc::new(AtomicUsize::new(0));
let filter = move |_: &i64| counter.fetch_add(1, Ordering::Relaxed) >= THRESHOLD;
```

For a `HashSet`-style `forbidden` set use `Arc<Mutex<HashSet<T>>>`. If
the test runs the closure twice (e.g. part 1 then part 2 of
`test_suppressing_filtering_health_check`) wrap the setup in a
`make_filter` closure and call it each time — `Arc` state persists across
`Hegel::new(...).run()` calls otherwise.

**Filter retry multiplier.** `Filtered::do_draw` retries the predicate
up to 3 times per draw before falling back to `assume(false)`. If you
port a Python test whose threshold is measured in *draws* (e.g. "reject
the first 200 values") the equivalent Rust threshold is measured in
*filter calls* and needs roughly ×3 the number — pick a value that still
exceeds FilterTooMuch's 200-invalid bar but leaves enough budget for the
filter to open up before the test case count is exhausted.

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

### characters() shape differences

A few shape differences between Python `st.characters()` and
`gs::characters()` affect which test cases port:

- **`include_characters` / `exclude_characters` take `&str`** — each
  codepoint is a char in the set. Python accepts a list of
  1-character strings, and parametrizes "one element is a
  multi-character string" to assert input validation. Rust's
  signature makes that case unrepresentable; drop those parametrize
  rows with a note in the module docstring.
- **The Rust client always emits `exclude_categories=["Cs"]`** so
  generated strings stay valid UTF-8. Python tests that rely on
  "`include_characters` alone (with no other constraint) is an
  error" (e.g. `test_whitelisted_characters_alone`) are unreachable —
  the implicit `Cs` exclusion means `include_characters` is never
  the only constraint. Drop the individual case.
- **`include_characters` is a union override, not a range filter**
  (matches Hypothesis `charmap.query`): chars listed there are
  added regardless of `min_codepoint`/`max_codepoint`. A test
  asserting that `include_characters` produces chars outside the
  codepoint range is correct and should port unchanged.
- **Codec round-trip checks collapse.** Python tests of the shape
  `example.encode(codec).decode(codec) == example` port to trivial
  or near-trivial assertions in Rust because `char` is a Unicode
  scalar by construction: `"utf-8"` is always round-trippable (drop
  the assertion; just exercise the schema), `"ascii"` reduces to
  `c.is_ascii()`, and `exclude_categories=["Cs"]` is a no-op (the
  surrogate range is already unreachable). See the
  `test_characters_codec` rows in `tests/hypothesis/core.rs`.
- **Verifying that a drawn char belongs to a Unicode category**
  (e.g. `categories=["N"]` / `exclude_categories=["N"]` rows of
  `test_characters_codec`) is native-only: use
  `hegel::__native_test_internals::unicodedata::general_category(c as u32).as_str()`
  and gate the test with `#[cfg(feature = "native")]`. Don't reach
  for a third-party `unicode-*` crate — see
  `implementing-native/SKILL.md` "Port, don't adapt".

## Validation-panic tests (`InvalidArgument` in Python)

`test_validates_keyword_arguments` in Hypothesis (and similar shapes in
`test_validation.py`, `test_regex.py`, `test_sampled_from.py`,
`test_uuids.py`, …) wraps `check_can_generate_examples(fn(**kwargs))` in
`pytest.raises(InvalidArgument)`. The translation depends on *when* the
check fires:

- **Construction-time** — a few factory functions validate eagerly (e.g.
  `gs::sampled_from(vec![])` and `gs::one_of(vec![])` both panic on empty
  input). Wrap the constructor:

  ```rust
  expect_panic(|| { gs::sampled_from::<i64, _>(Vec::<i64>::new()); },
               "sampled_from cannot be empty");
  expect_panic(
      || {
          let empty: Vec<gs::BoxedGenerator<i64>> = vec![];
          gs::one_of(empty);
      },
      "one_of requires at least one generator",
  );
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

### `@given` decorator-shape tests

A separate cluster of validation tests asserts on shapes Python's `@given`
*decorator* can take but Rust's `#[hegel::test]` *attribute macro* cannot:

| Python `@given` shape                             | Why it skips                                                                  |
|---------------------------------------------------|--------------------------------------------------------------------------------|
| `@given(a=...)` / `@given(...)` (ellipsis)        | Type-hint-based strategy inference. `#[hegel::test]` takes generators inline, not by signature inference. |
| `@given(...)` stacked twice on one function        | Decorator stacking has no Rust analog; one `#[hegel::test]` per fn.            |
| `@given(...)` on a `class`                         | Rust has no class/decorator composition to reject.                             |
| `@given(...) async def ...`                        | hegel-rust has no async-test dispatch, so no specific error to assert.         |
| `@given(a=1, max_examples=5)` (kwarg vs setting collision) | hegel-rust uses `.settings(Settings::new()...)`; no kwarg-merging surface to misuse. |
| `@given(*args)` / `@given(**kw)` / arity mismatch (`@given(integers(), int, int) def foo(x, y)`) / default-arg override (`@given(x=...) def t(x=1)`) / mixed positional+keyword (`@given(booleans(), y=booleans())`) / type-as-strategy (`@given(bool)`) | hegel-rust binds generators to closure parameters via `Hegel::new(\|tc\| { let x = tc.draw(...); })`, not via decorator-arg ↔ function-signature dispatch. None of these mismatch errors exist to assert on. `test_validation.py` concentrates ~14 of them under one rationale. |

Skip these per-test under the `Individually-skipped tests` policy in
SKILL.md, naming the specific decorator shape — they are public-API gaps
*by design*, not missing features. `test_given_error_conditions.py` is
the canonical upstream concentration of these.

## Seeded `find()` (testing determinism)

The default `find(gen, cond)` → `find_any(gen, ...)` mapping drops the
seed — `find_any` doesn't take one. When the upstream test passes
`find(..., random=Random(S))` to pin down determinism across runs, drive
the engine directly:

```rust
use hegel::{Hegel, Settings};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

let found: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
let found_c = Arc::clone(&found);
std::panic::catch_unwind(AssertUnwindSafe(|| {
    Hegel::new(move |tc| {
        let v = tc.draw(&gen);
        if cond(&v) {
            let mut g = found_c.lock().unwrap();
            if g.is_none() { *g = Some(v); }
            drop(g);            // release BEFORE panic — see below
            panic!("HEGEL_FOUND");
        }
    })
    .settings(Settings::new().test_cases(1000).database(None).seed(Some(S)))
    .run();
}))
.ok();
let value = found.lock().unwrap().take().unwrap();
```

Key points:

- **Drop the mutex guard before `panic!`.** Hegel replays the interesting
  case for shrinking; a held guard poisons the mutex and the replay's
  `lock().unwrap()` then panics with a poison error instead of
  `HEGEL_FOUND`, which the engine reports as flaky.
- `if g.is_none() { … }` pins the captured value to what was *first*
  found — shrinking doesn't overwrite it. This is how you emulate
  `phases=[Phase.generate]` (no shrinking) in a framework with no
  `phases` setting.
- `database(None)` prevents replayed failing cases from leaking across
  iterations of the outer loop.
- `Phase` / `phases=[...]` has no hegel-rust analog — drop the setting
  and, if the original relied on no-shrinking semantics, use the
  `if g.is_none()` guard above.

## `@example` stack + `@given` (shared check helper)

A common Hypothesis shape stacks many `@example(...)` decorators above a
single `@given(...)` test that does property-style assertions:

```python
@example(float_constr(1, float_info.max), 0.0)
@example(float_constr(100.0001, 100.0003), 100.0001)
# ... 14 more @example lines
@given(float_constraints(), st.floats())
def test_float_clamper(constraints, input_value):
    clamper = make_float_clamper(...)
    clamped = clamper(input_value)
    assert sign_aware_lte(min_value, clamped)
    # ...
```

The natural Rust translation is **two `#[test]` functions sharing one
check helper**, not one giant test:

```rust
fn check_float_clamper(c: &FloatConstraints, input: f64) {
    let clamper = make_float_clamper(c);
    let clamped = clamper(input);
    // ... assertions
}

#[test]
fn test_float_clamper_examples() {
    check_float_clamper(&float_constr(1.0, f64::MAX), 0.0);
    check_float_clamper(&float_constr(100.0001, 100.0003), 100.0001);
    // ... one call per @example
}

#[test]
fn test_float_clamper_property() {
    Hegel::new(|tc| {
        let constraints = /* draw equivalent of float_constraints() */;
        let input: f64 = tc.draw(gs::floats::<f64>());
        check_float_clamper(&constraints, input);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}
```

Why split rather than `#[hegel::test(explicit_test_case = ...)]` per
example: with a stack of 10+ examples, one `#[test]` per example bloats
the file and the `cargo test` output without buying anything; one
`_examples` test that runs them in sequence reads cleanly and still
fails with a useful line number. If a single example is load-bearing
(e.g. a regression case the upstream named) and worth surfacing on its
own, give it its own `#[test]` — see `test_float_clamper_defensive_lower`
in `tests/hypothesis/float_utils.rs` for that pattern.

If the upstream `@given` uses a strategy with no direct Rust analog
(e.g. Hypothesis's `provider_conformance.float_constraints()`), inline
the strategy as a small `tc.draw(...)` block in the property test
rather than chasing a library helper.

### `@example` + `@given` under `@pytest.mark.parametrize`

When a `parametrize` over N implementations sits on top of the
`@example` + `@given` stack (shape: one Python function covering N×K
combinations), the split-into-two-tests guidance above would blow up
into `2N` `#[test]` functions. Instead, embed the examples as an
explicit loop at the top of the *shared* driver and follow with the
property block:

```rust
fn behaves_like_a_dict_with_losses_hegel<C, F>(make: F)
where F: Fn(usize) -> C + Send + Sync + 'static, C: DictLikeCache + 'static,
{
    for (writes, size) in [ /* @example rows */ ] {
        let mut target = make(size);
        run_dict_like_losses(&mut target, &writes, size);
    }
    Hegel::new(move |tc| { /* @given body */ }).run();
}

#[test] fn test_..._lru()      { behaves_like_a_dict_with_losses_hegel(LRUCache::new); }
#[test] fn test_..._lfu()      { behaves_like_a_dict_with_losses_hegel(|sz| GenericCache::new(sz, LFUScoring).unwrap()); }
// ... one #[test] per parametrize row
```

Each `#[test]` now runs the regression examples *and* the property
phase against its implementation, and `cargo test`'s output still
reports an individual test per implementation (what `pytest` would
show). See `tests/hypothesis/cache_implementation.rs` for the full
shape, which also uses a test-local `DictLikeCache` trait to unify
wrappers with non-identical `insert` signatures (some return
`Result<_, CachePinError>`, some return `()`).

## Python subclass-override hooks → strategy trait on the native type

A common pbtkit/Hypothesis test shape defines a one-off subclass of a
library class (`GenericCache`, `RuleBasedStateMachine`, a database
backend, a provider) that overrides a few hook methods to customise
behaviour for that one test:

```python
class LFUCache(GenericCache):
    def new_entry(self, key, value):   return 1
    def on_access(self, key, value, score):  return score + 1

class ValueScored(GenericCache):
    def new_entry(self, key, value):   return value
```

The Rust `src/native/` counterpart doesn't get to use inheritance.
Factor the hooks into a **strategy trait** with default method bodies,
make the wrapper type generic over it, and give each test a small
struct implementing the trait:

```rust
// In src/native/cache.rs:
pub trait CacheScoring<K, V> {
    fn new_entry(&mut self, k: &K, v: &V) -> i64;
    fn on_access(&mut self, _k: &K, _v: &V, s: i64) -> i64 { s }
    fn on_evict(&mut self, _k: &K, _v: &V, _s: i64) {}
}
pub struct GenericCache<K, V, S: CacheScoring<K, V>> { /* … */ }

// In tests/hypothesis/cache_implementation.rs:
struct LFUScoring;
impl<K, V> CacheScoring<K, V> for LFUScoring {
    fn new_entry(&mut self, _k: &K, _v: &V) -> i64 { 1 }
    fn on_access(&mut self, _k: &K, _v: &V, s: i64) -> i64 { s + 1 }
}
```

Practical points:

- Default method bodies on the trait replace "don't override" on the
  Python side — give every optional hook a default so test structs only
  name the ones they customise.
- Expose the scoring instance as a `pub` field on the wrapper
  (`cache.scoring`) when the test needs to inspect internal state
  after the run (`evicted`, `observed`, etc. in
  `test_always_evicts_the_lowest_scoring_value`). Python subclasses get
  attribute access for free; Rust needs the field to be reachable.
- Monomorphise — each test uses a concrete `GenericCache<K, V, MyScoring>`
  rather than `dyn CacheScoring`. The shared test driver dispatches over
  a tiny test-local `DictLikeCache` trait (three methods: insert / get /
  len); one `impl` per wrapper type. See `tests/hypothesis/cache_implementation.rs`.

### `st.data()`-draw inside an overridden hook

When the Python hook body itself calls `data.draw(...)` (e.g. the
`new_score` closure inside `test_always_evicts_the_lowest_scoring_value`),
the strategy-trait translation has no `tc` in scope. Draw a PRNG seed
from `tc` up-front and let the scoring struct pull values from a
seeded `StdRng` inside each hook call:

```rust
// In the test body:
let seed: u64 = tc.draw(gs::integers::<u64>());
let scoring = DynamicScoring {
    rng: StdRng::seed_from_u64(seed),
    /* … */
};

// In the scoring struct:
fn on_access(&mut self, _k: &i64, _v: &i64, _s: i64) -> i64 {
    (self.rng.next_u64() % 1001) as i64   // matches st.integers(0, 1000)
}
```

A single-seed shrink is coarser than Python's per-draw shrinking, but
it preserves the Python test's semantics — a fresh score on every
`new_entry`/`on_access` call, so the cache's rebalance-after-access
path is actually exercised. A pre-drawn `HashMap<K, Score>` filled in
before the cache is constructed does *not* do this: scores become
static per key and `on_access` collapses into a no-op, which is why
`test_always_evicts_the_lowest_scoring_value` uses the seeded-RNG form
above. Reserve pre-drawing for cases where the hook draws at most
once per key.

## Python idiom translations

Common Python patterns that need non-trivial translation in test
predicates:

| Python                    | Rust                                                      | Why                                                                   |
|---------------------------|-----------------------------------------------------------|-----------------------------------------------------------------------|
| `minimal(text(), bool)`   | `minimal(gs::text(), \|s: &String\| !s.is_empty())`      | Python `bool(s)` is truthy = non-empty                                |
| `x >= "\udfff"` (string comparison) | `s.as_str() >= "\u{e000}"`                      | Rust strings can't contain surrogates; `\u{e000}` is the first valid codepoint past the surrogate range |
| `sum(xs) >= N` where `xs: list[int]` from `integers()` | `xs.iter().copied().map(i128::from).sum::<i128>() >= N as i128` | Python ints are unbounded; `i64` sums overflow on extreme generated values. Promote to `i128` (or `num-bigint`) before summing. |
| `any(xs) and not all(xs)` on `list[list[T]]` | `xs.iter().any(\|inner\| !inner.is_empty()) && !xs.iter().all(\|inner\| !inner.is_empty())` | Python `bool(list)` = non-empty, so `any/all` test inner-list non-emptiness. Rust `Vec` has no truthiness; translate explicitly to `!inner.is_empty()`. |
| `type(x) == type(y)` on mixed-type `one_of` draws | `std::mem::discriminant(&x) == std::mem::discriminant(&y)` | After wrapping mixed-type `one_of` branches in a local enum (see SKILL.md), `type()` equality becomes variant equality. `discriminant` compares the variant tag without unpacking payloads and works even when payload types (e.g. `f64`) aren't `Eq`. |
| `xs.remove(y)` on `list[T]` | `let pos = xs.iter().position(\|v\| *v == y).unwrap(); xs.remove(pos);` | Python's `list.remove` takes a **value** and removes the first match; Rust's `Vec::remove` takes an **index**. Same method name, different semantics — translate via `position` + `remove`. |

### Don't sort before asserting order

When the Python test compares a container to a literal list —
`assert list(cache) == [1, 3]`, `assert list(od.keys()) == ["a", "b"]` —
the *order* is usually part of the assertion (LRU position, insertion
order, shrink order). Port the Rust side as
`assert_eq!(cache.keys(), vec![1, 3]);` and **do not** `ks.sort()` before
comparing. Sorting turns an ordered-equality check into a set-equality
check: a broken LRU emitting `[3, 1]` would still pass, which is
exactly the bug the test exists to catch.

Reflexive sorting is tempting because many elsewhere-in-the-port
assertions on `HashSet` / `HashMap` iteration legitimately need a sort
for determinism. The tell for "don't sort here" is that the upstream
container's Python type is already ordered — `LRUCache`, `OrderedDict`,
a `list` returned from a deterministic algorithm, a span tree's
children. If `keys()` on the Rust side returns a `Vec` in a declared
order (walks an underlying `VecDeque` / `LinkedList`, or yields
indices in shrink order), assert that order directly.

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
