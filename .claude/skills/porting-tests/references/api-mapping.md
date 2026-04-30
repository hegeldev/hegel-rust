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
| `gs.floats(..., allow_subnormal=True/False)` | **missing** — `gs::floats()` has no `.allow_subnormal(bool)` builder method. Skip tests whose assertion depends on the subnormal range being excluded (`test_subnormal_floats.py::test_subnormal_validation`, `test_allow_subnormal_defaults_correctly`). Internal helpers `next_up_normal`/`next_down_normal` *are* ported in `src/native/floats.rs` and reachable via `__native_test_internals` for native-gated tests. |
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
| `gs.nothing()`                             | **missing** — native-gate the test and stub under `src/native/` (see SKILL.md skip-vs-port policy). **Exception:** for the shape `flat_map(lambda _: nothing())` used to prove *validation happens on draw, not construction* (e.g. `test_validation.py::test_validation_happens_on_draw`), substitute any always-bad-at-draw generator — `gs::integers::<i64>().min_value(1).max_value(0)` works cleanly — and assert with the same `expect_draw_panic` helper as the other validation-timing tests. The test's semantic point (inner strategy from a `flat_map` callback is only validated when drawn) is preserved without needing a native `gs::nothing()`. |
| `gs.decimals(...)`                         | **missing** — Python-stdlib `decimal.Decimal` has no Rust counterpart and no `gs::decimals()` exists. Skip tests whose strategy is `decimals()` with a one-line `decimal.Decimal`-absence rationale in SKIPPED.md. |
| `gs.fractions(...)`                        | **missing** — Python-stdlib `fractions.Fraction` has no Rust counterpart and no `gs::fractions()` exists. Skip as above. |
| `gs.from_regex(pat)`                       | `gs::from_regex(pat)` (add `.fullmatch(true)` if used)        |
| `gs.emails()` / `gs.urls()`                | `gs::emails()` / `gs::urls()`. **`emails(domains=st)` has no counterpart** — `EmailGenerator` exposes no `domains` builder method. Skip any test whose assertion depends on a restricted domain set (e.g. `test_can_restrict_email_domains`); port the domain-agnostic tests as normal. |
| `gs.dates()` etc.                          | `gs::dates()`, `gs::times()`, `gs::datetimes()`, `gs::durations()` |
| `st.deferred(lambda: ...)` (recursive / mutually-recursive strategies) | `gs::deferred::<T>()` returns a *definition* object, not a generator. Call `.generator()` on it to get a drawable handle, then `.set(body)` exactly once to install the body. Recursive self-reference goes through cloned handles, so a Python `tree = st.deferred(lambda: st.tuples(st.integers(), tree, tree)) \| st.just(None)` ports as `let def = gs::deferred::<Tree>(); let tree = def.generator(); def.set(hegel::one_of!(gs::just(Tree::Leaf), gs::tuples!(gs::integers::<i64>(), tree.clone(), tree.clone()).map(\|(v, l, r)\| Tree::Node(v, Box::new(l), Box::new(r)))));`. Mutually-recursive uses the same shape with multiple definitions — see `tests/test_deferred.rs` for worked examples. **`T: Send + Sync` is required.** `JustGenerator<T>`, `BoxedGenerator<T>`, and `.map`-d combinators bound their value type `Send + Sync`, so a recursive type with interior mutability must use `Arc<Mutex<...>>` rather than `Rc<RefCell<...>>` even when the test is single-threaded. Python originals that mutate a `dataclass` field in-place (e.g. `Branch(children: dict)` with `node.children.setdefault(c, …)`) port to `Branch { children: Arc<Mutex<HashMap<K, V>>> }`; the mutex is uncontended and just satisfies the trait bounds. See `tests/hypothesis/nocover_explore_arbitrary_languages.rs::Node::Branch`. |
| `st.data()` (the "draw inside the test" strategy) | **no analog — and that's fine**: the test body's `tc: TestCase` already exposes `tc.draw(...)`. A Hypothesis `@given(st.integers(), st.data()) def t(x, data): data.draw(...)` ports as `Hegel::new(\|tc\| { let x = tc.draw(...); let y = tc.draw(...); })`. Even `@given(st.data(), st.data())` usually ports as two consecutive `tc.draw()` calls — the `Draw 1` / `Draw 2` numbering in failure output still lines up. Only skip when the test genuinely calls `.filter` / `.map` / `.flatmap` *on the strategy object itself* (`st.data().filter(...)`), or uses `repr(st.data())`. |
| `find(st.data(), lambda data: data.draw(g) ...)` | `minimal(hegel::compose!(\|tc\| { tc.draw(g) }), predicate)` — `st.data()` inside a `find()` is a generator with dynamic draws, which is exactly `compose!`. If the Python asserts on `data.conjecture_data.choices`, substitute an assertion on the returned minimal value (the engine-internal accessor has no public counterpart). |

Generator transforms (all require `Generator` trait in scope):

| Python                        | Rust                                      |
|-------------------------------|-------------------------------------------|
| `inner.map(f)`                | `inner.map(\|x\| f(x))`                   |
| `inner.filter(p)`             | `inner.filter(\|x: &T\| p(x))`            |
| `inner.flatmap(f)`            | `inner.flat_map(\|x\| f(x))` — if the Python callback branches on `x` to produce **different generator types** (e.g. `lambda b: booleans() if b else text()`), the Rust closure must return one type: wrap each branch in a local `enum` and `.boxed()` the per-branch generator, exactly like the mixed-type `one_of` pattern (see SKILL.md). Example: `gs::booleans().flat_map(\|b\| if b { gs::booleans().map(Value::Bool).boxed() } else { gs::text().map(Value::Text).boxed() })`. Worked example in `tests/hypothesis/nocover_flatmap.rs::test_mixed_list_flatmap`. |
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
| `tc.weighted(p)`            | **missing** from the public API. For `p ∈ {0.0, 1.0}` substitute `gs::just(false)` / `gs::just(true)`. For rare probabilities, a **native-gated** port can drive `NativeTestCase::weighted(p, None)` directly via `with_native_tc` from inside a `compose!` body — see "Calling native draws from a `compose!` body" below. |
| `tc.mark_status(INTERESTING)` | `panic!(...)` to signal failure        |
| `tc.target(score)`         | **missing from the public `TestCase` API.** For porting Hypothesis's `conjecture/test_optimiser.py`-shape tests (or pbtkit `test_targeting.py`) that *build their own runner* to exercise `target_observations` / `optimise_targets`, drive the native-only `TargetedRunner` / `TargetedTestCase` surface from `hegel::__native_test_internals` — `target_observations` is a `pub HashMap<String, f64>` on the test case. See `tests/hypothesis/conjecture_optimiser.rs` for the worked harness. A `tc.target(...)` call on a regular `Hegel::new(...).run()` user test still has no analog. |
| `ConjectureData.for_choices([v, ...])` | `NativeTestCase::for_choices(&[ChoiceValue::…, …], None)` from `hegel::__native_test_internals` (native-only) — see "Replaying fixed choices" below |
| `tc.reject()`              | `tc.reject()` — public method, equivalent to `assume(false)` but returns `!` so following code is statically unreachable |

## Top-level API

| Python                                    | Rust                                      |
|-------------------------------------------|-------------------------------------------|
| `hypothesis.currently_in_test()`          | `hegel::currently_in_test_context()` — re-exported from `hegel::control`. Works in server and native mode, and inside `@rule`s of a `#[hegel::state_machine]`-driven machine (see `tests/hypothesis/control.rs`). |
| `hypothesis.note(msg)` / `hypothesis.assume(cond)` / `hypothesis.reject()` / `hypothesis.event(msg)` (module-level free functions) | **missing as free functions.** `note` / `assume` / `reject` exist only as `TestCase::` methods, so there is no "out-of-context" call site to validate — tests of the shape `test_raises_if_note_out_of_context`, `test_deprecation_warning_if_{assume,reject}_out_of_context`, `test_cannot_event_with_no_context` are unportable (the type system forecloses the error path). `event()` has no public analog at all. Skip each individually with a one-line "method on `TestCase`, no free-function out-of-context path" rationale. |
| `hypothesis.control.BuildContext` / `current_build_context()` / `cleanup()` | **missing.** Hypothesis exposes test-context entry/exit as an openable, nestable context-manager with a user-facing cleanup-hook registry and a `current_build_context()` accessor. hegel-rust's test context is a thread-local bool (`currently_in_test_context()`) — no openable object, no nesting, no cleanup hooks, no `BuildContext` accessor. Whole cluster of `test_control.py` tests (`test_can_nest_build_context`, `test_cleanup_executes_on_leaving_build_context`, `test_current_build_context_is_current`, etc.) skip under this. |
| `hypothesis.reporting.with_reporter(list.append)` | **missing** — there is no reporter-override public API. `tc.note()` output is verbosity-independent and only fires on the final failing replay; tests that capture notes into a list during generation (e.g. `test_prints_all_notes_in_verbose_mode`, `test_note_pretty_prints`) have no observation surface. Same gap that blocks most of `test_reporting.py`. |

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
| `@fails` + `@given(gen) def test(x): [assume(g); ...; assert p]` (i.e. `tests.common.utils.fails` = `fails_with(AssertionError)` — the test *expects* Hypothesis to find a counterexample) | `find_any(gen, \|x: &T\| g(x) && !p(x))` — negate the final assert and fold each `assume(...)` guard into the condition (both must hold for the counterexample to be valid). A multi-statement body collapses to a single boolean expression. See `tests/hypothesis/nocover_floating.rs` for worked examples. |
| `TRY_HARDER = settings(max_examples=1000, suppress_health_check=[HealthCheck.filter_too_much])` stacked on a `@fails` test | `FindAny::new(gen, cond).max_attempts(1000).suppress_health_check(HealthCheck::FilterTooMuch).run()` — `TRY_HARDER` is the standard Hypothesis override for hunting rare values (NaN, ±∞); the `FilterTooMuch` suppression isn't a "health-check bypass" in the sense of the SKILL rule — it's carried over from the original. On non-`@fails` tests the same override maps to `Settings::new().test_cases(1000).suppress_health_check([HealthCheck::FilterTooMuch])` on the `Hegel::new(...).run()` call. **Filter-rewriting caveat:** Hypothesis rewrites certain `.filter(pred)` shapes (e.g. `floats().filter(math.isnan)` → NaN-only strategy mixing all sign × signaling variants; `integers().filter(lambda x: x >= K)` → bounded-integers strategy) into dedicated rare-value strategies. hegel-rust's `.filter()` is always a generic 3-try rejection sampler with no such pass. If an upstream `@fails` test needs *every* variant of a rare class to turn up inside TRY_HARDER's 1000-attempt budget, the rewrite is doing the work — raising `max_attempts` further won't recover it. Skip the test with a filter-rewriting rationale and file a TODO for the missing rewrite / specialised generator. See `nocover/test_floating.py::test_can_find_negative_and_signaling_nans` in `SKIPPED.md` for a worked example. |
| `find(gen, cond)`                   | `find_any(gen, \|x: &T\| cond(x))`         |
| `find_any(gen)` / `find_any(gen, lambda _: True)` (trivially-true — just smoke-testing the generator produces a value) | `check_can_generate_examples(gen)` for small structures; `AssertSimpleProperty::new(gen, \|_\| true).test_cases(1).run()` for generators with large `min_size` or expensive shrinking. **Do NOT translate as `find_any(gen, \|_\| true)`** — Python's `debug.py::find_any` uses `phases=no_shrink` internally, so it never shrinks the found value. Rust's `find_any` panics to signal the found case, causing Hypothesis to shrink it fully. For a `min_size=400` list that's thousands of shrink attempts on trivially-passing inputs (71s → 0.6s with `AssertSimpleProperty`). |
| `minimal(gen, cond)`                | `minimal(gen, \|x: &T\| cond(x))`          |
| `minimal(gen, cond, max_examples=N)` | `Minimal::new(gen, \|x: &T\| cond(x)).test_cases(N).run()` — the one-shot `minimal()` helper hardcodes 500; use the `Minimal` builder when you need a different budget. **Pitfall:** when `N` comes from a `settings=settings(max_examples=N, ...)` kwarg passed to `minimal()` inside a `@given(st.data())` test, it belongs on the `Minimal` builder — *not* on the outer `Hegel.settings()`. The two settings coexist: `@settings(...)` on the decorator configures `@given`'s test-case budget, `settings=settings(...)` on the `minimal()` call configures the nested shrink search. An outer test with no explicit `max_examples` is still running Hypothesis's default (100), so collapsing the two by putting `test_cases(N)` on the outer `Hegel` silently 10×-shrinks the outer budget. Port each site independently. See `tests/hypothesis/nocover_boundary_exploration.rs` for a worked example (outer = default 100, inner `Minimal::new(...).test_cases(10)`). |
| `try: minimal(gen, cond) except Unsatisfiable: reject()` (or `: pass`) | `catch_unwind(AssertUnwindSafe(\|\| { minimal(gen, \|v: &T\| cond(v)); })).ok();` — `minimal()` panics with `"Could not find any examples"` when its budget can't satisfy `cond`. Catching the panic translates Python's tolerate-and-skip idiom; the outer `Hegel::new(...).run()` moves on to the next test case. **Not the same as the `FilterTooMuch` row in the Health checks section** — that's for tests that *assert* Unsatisfiable fires, while this is for tests that *tolerate* a nested `minimal()` occasionally being unsatisfiable. Use when satisfiability depends on runtime input (a per-test-case RNG seed, a generated element); when satisfiability is static per parametrize row, drop the row instead (see `tests/hypothesis/nocover_collective_minimization.rs`, which lists the vacuously-unsatisfiable rows it dropped). If the outer test has work after the `minimal()` call, match on `catch_unwind(...)` and call `tc.reject()` in the `Err` arm so later assertions don't run on stale state. |
| `with pytest.raises(X): ...`        | `expect_panic(\|\| { ... }, "regex")`      |
| `capture_out()` / `capsys` / `capfd` | `TempRustProject::new().main_file(CODE).cargo_run(&[])` — access `.stderr`/`.stdout` on the `RunOutput` |
| `capture_out() + pytest.raises(X)`  | `TempRustProject::new().main_file(CODE).expect_failure("pattern")` — builds, runs, asserts non-zero exit + pattern in stderr, returns `RunOutput` |

**Derandomised helpers collapse seed-parametrize axes.** Upstream tests
(especially under `tests/quality/`) routinely stack
`@pytest.mark.parametrize("seed", [...])` on a `minimal(...)` /
`find(...)` / `ConjectureRunner(..., random=Random(seed))` call to
assert the same shrink target is reached from several random starts. In
hegel-rust, `Minimal` / `FindAny` / `assert_all_examples` all run
derandomised, so the seed axis carries no information — collapse it to
one `#[test]` per remaining parametrize row instead of emitting N
identical assertions. The other parametrize axes still expand normally.
See `tests/hypothesis/quality_poisoned_lists.rs` for a worked example
(4 seeds × 3 sizes × 2 probabilities × 2 strategy classes → 12 tests,
not 48).

**Parametrize rows that are different strategies → `BoxedGenerator<'static, T>`.**
When the parametrize axis is a *list of strategies* producing the same element
type (e.g. `@pytest.mark.parametrize("base", [st.integers(1, 20),
st.integers(0, 19).map(lambda x: x + 1), st.sampled_from(range(1, 21)), ...])`),
the four rows have four different Rust generator types. Python erases the
difference dynamically; Rust needs an explicit `BoxedGenerator<'static, T>` to
unify them. Split into one `#[test]` per row, each calling a shared driver:

```rust
fn run_chained_filters_agree(base: BoxedGenerator<'static, i64>) { /* ... */ }

#[test]
fn test_chained_filters_agree_integers_1_20() {
    run_chained_filters_agree(gs::integers::<i64>().min_value(1).max_value(20).boxed());
}
#[test]
fn test_chained_filters_agree_sampled_from_0_19_mapped() {
    let values: Vec<i64> = (0..20).collect();
    run_chained_filters_agree(gs::sampled_from(values).map(|x| x + 1).boxed());
}
```

The same type-erasure is the fix for **growing a generator chain in a loop**
(`for x in xs: s = s.filter(lambda y: y != x)` in Python). Each `.filter(...)`
produces a new concrete Rust type; re-binding `s` across iterations requires
`BoxedGenerator<'static, T>` and a trailing `.boxed()`:

```rust
let mut s: BoxedGenerator<'static, i64> = base.clone();
for f in &forbidden {
    let f = *f;
    s = s.filter(move |x: &i64| *x != f).boxed();
}
```

See `tests/hypothesis/nocover_filtering.rs` for both shapes together.

**Large scalar-axis parametrize → `for`-loop inside one `#[test]`.** When the
parametrize axis is just a list of scalars (numeric boundaries, powers of ten,
bit patterns) and every row applies the *same* assertion logic, don't emit one
`#[test]` per row — loop inside a single `#[test]` and label failures with the
axis value:

```rust
for boundary in boundaries() {
    assert_eq!(
        minimal(gs::integers::<i64>(), move |x: &i64| *x >= boundary),
        boundary,
        "boundary = {boundary}"
    );
}
```

Rule of thumb: >5-6 rows → loop; ≤4 rows → one `#[test]` per row (names can
encode the axis, e.g. `..._straddle_zero` vs `..._subnormal_pair`). Unlike the
seed-axis collapse above, each iteration here is a genuinely distinct
assertion — the `"boundary = {..}"` message is load-bearing because the loop
hides which value tripped. Move-captured loop variables need `move` on the
inner closure. See `tests/hypothesis/nocover_simple_numbers.rs` for 3×axes
collapsed this way (boundaries, k∈0..10) next to a 4-row axis given one
`#[test]` per row.

## Features deliberately missing from hegel-rust

These show up in lots of pbtkit/Hypothesis tests. When you hit one, leave
the test as `todo!()` with a clear comment and **add a TODO.md entry** for
adding the feature. Don't invent a workaround in the test.

- `tc.weighted(p)` — weighted booleans. (Native-gated tests have an
  escape hatch via `with_native_tc`; see "Calling native draws from a
  `compose!` body" below. The *public* API gap is still real.)
- `tc.target(score)` — score-directed search on the *public* `TestCase`.
  (A native-only test-harness surface — `TargetedRunner` /
  `TargetedTestCase` / `BufferSizeLimit` from `__native_test_internals`
  — exists for porting Hypothesis's `conjecture/test_optimiser.py`-shape
  tests; see `tests/hypothesis/conjecture_optimiser.rs`. It is not
  wired into `Hegel::new(...).run()`, so user-test `tc.target(score)`
  calls still have no analog.)
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
- `find()` + database-accumulation assertions — tests that drive
  `find(strategy, predicate, settings=settings(database=db))` and assert
  that `db` accumulates more than one entry (`len(all_values(db)) > 1`,
  `len(non_covering_examples(db)) > 0`, or that the count shrinks back
  to zero across runs as the predicate becomes always-false / always-
  invalid) are unportable. Hypothesis's `find()` driver auto-saves every
  distinct interesting example reached during search and shrinking, plus
  pareto-front entries; `NativeConjectureRunner::run()` only mutates the
  database via the reuse phase (delete-invalid + replay-existing), and
  `pareto_front()` is `todo!()`. The public `Hegel::new(...).run()` path
  saves only the final shrunk counterexample, never intermediates. Skip
  with a rationale naming the missing auto-save side; the cluster in
  `nocover/test_database_usage.py` (`test_saves_incremental_steps_*`,
  `test_clears_out_database_*`, `test_trashes_invalid_examples`) is the
  worked example. Becomes portable once the native engine grows
  pareto / interesting-example auto-save.
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
- **`BaseException` parametrize rows** (`KeyboardInterrupt`, `SystemExit`,
  `GeneratorExit`). Python distinguishes `BaseException` subclasses —
  which Hypothesis propagates unchanged without catch/shrink/replay —
  from `Exception` subclasses that go through the normal replay path.
  Rust panics are singular; there is no `BaseException`/`Exception`
  split, so every panic travels the catch-shrink-replay path. When an
  upstream test parametrizes over `[KeyboardInterrupt, SystemExit,
  GeneratorExit, ValueError]` (or similar), port only the
  `Exception`-subclass rows (usually `ValueError` → plain `panic!`) and
  skip the BaseException rows individually with a "Rust panics don't
  distinguish `BaseException` from `Exception`" rationale. A
  counter-based "panic on the Nth run" probe still maps to an
  `Arc<AtomicUsize>` body and `expect_panic(..., "Flaky test detected")`
  — that's the `ValueError` half of `test_baseexception.py`, ported in
  `tests/hypothesis/nocover_baseexception.rs`.
- `LazyStrategy` / `defines_strategy` memoisation — Hypothesis's
  `@defines_strategy` decorator wraps each `st.*` factory in a
  `LazyStrategy` that computes and caches the underlying
  `SearchStrategy` on first use. Tests that pin down this caching
  behaviour (e.g. `nocover/test_deferred_errors.py::test_does_not_recalculate_the_strategy`,
  which counts factory invocations across repeated draws) have no
  Rust counterpart: `gs::*` factories return eagerly-constructed
  generator structs, so there is no laziness/memoisation layer to
  observe. Skip individually with a rationale naming `LazyStrategy` /
  `defines_strategy`.
- **Pluggable `Provider`s** (`BytestringProvider`, `URandomProvider`).
  Hypothesis `ConjectureData` takes a `provider=...` arg that swaps the
  draw-source — `BytestringProvider` drives draws from a raw byte string,
  `URandomProvider` from `/dev/urandom`. `src/native/` has one provider:
  the `SmallRng` embedded in `NativeTestCase::new_random`, i.e. only
  `HypothesisProvider`'s analog. `NativeTestCase::for_choices` takes
  concrete `ChoiceValue`s, not bytes, so `BytestringProvider`-driven
  tests have no port. When a `conjecture/test_provider_contract.py`-shape
  file parametrizes over providers, port only the `HypothesisProvider`
  row (see `tests/hypothesis/conjecture_provider_contract.rs`) and
  individually-skip the `BytestringProvider` / `URandomProvider` rows
  with a "no pluggable-provider surface in `src/native/`" rationale.
  When the Python row is `@given(st.randoms())` — i.e. the property is
  "for any RNG seed, draws satisfy the invariant" — port as a small
  fixed seed array (`const SEEDS: &[u64] = &[0, 1, 2, 3, 17, 42, 12345, u64::MAX];`)
  iterated inside one `#[test]` per constraint shape, each constructing
  `NativeTestCase::new_random(SmallRng::seed_from_u64(seed))`. A handful
  of seeds exercises the random-draw path without re-plumbing an
  `@given`-over-RNG wrapper that hegel-rust's derandomised helpers don't
  have.

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

### Calling native draws from a `compose!` body

When an upstream strategy needs an engine-level primitive the public API
doesn't expose — the usual case is `data.draw_boolean(p)` for a rare `p`
— reach through `with_native_tc` to the live `NativeTestCase` from
inside a `compose!` closure. The whole test file goes
`#![cfg(feature = "native")]`:

```rust
#![cfg(feature = "native")]

use hegel::__native_test_internals::with_native_tc;
use hegel::compose;
use hegel::generators::{self as gs, Generator};

// STOP_TEST_STRING is pub(crate); reproduce the literal here.
const STOP_TEST_STRING: &str = "__HEGEL_STOP_TEST";

fn weighted_boolean(p: f64) -> bool {
    with_native_tc(|handle| {
        match handle
            .expect("weighted_boolean called outside native test context")
            .lock()
            .unwrap()
            .weighted(p, None)
        {
            Ok(v) => v,
            Err(_) => panic!("{STOP_TEST_STRING}"),
        }
    })
}

fn poisoned(p: f64) -> impl Generator<Poisoned> {
    compose!(|tc| {
        if weighted_boolean(p) { Poisoned::Poison }
        else { Poisoned::Value(tc.draw(gs::integers::<i64>())) }
    })
}
```

The `Err(_) => panic!("{STOP_TEST_STRING}")` bridge is load-bearing: the
handle returns `DataSourceError::StopTest` when the replay buffer is
exhausted, and the engine's outer loop recognises only the exact panic
payload `__HEGEL_STOP_TEST` as "end of replay, not a real test failure."
Returning `Result::Err` up through a `compose!` body or letting the
underlying `StopTest` bubble as anything else gets classified as a real
assertion failure and the shrinker chases a phantom bug. `src/native/featureflags.rs`
uses the same pattern; mirror it rather than inventing a variant.

## Driving the native shrinker (`@shrinking_from(initial)`)

Conjecture tests in `hypothesis-python/tests/conjecture/test_shrinker.py`
use a `@shrinking_from(initial)` fixture that runs a `ConjectureRunner`,
caches its choice sequence, and hands the test a live `Shrinker` to call
methods on. In hegel-rust (native only) the same shape is expressed by
building a `Shrinker` directly from a hand-written initial choice list,
skipping the runner. Put this helper *in the test file*, not in
`tests/common/utils.rs` (see SKILL.md "Don't modify"):

```rust
use hegel::__native_test_internals::{ChoiceNode, ChoiceValue, NativeTestCase, Shrinker};

fn shrinking_from<F>(initial: Vec<ChoiceValue>, user_test_fn: F) -> Shrinker<'static>
where
    F: FnMut(&mut NativeTestCase) -> bool + 'static,
{
    let mut user_test_fn = user_test_fn;

    let mut ntc = NativeTestCase::for_choices(&initial, None);
    let is_interesting = user_test_fn(&mut ntc);
    assert!(is_interesting, "initial choices did not trigger mark_interesting");
    let initial_nodes = ntc.nodes.clone();

    let test_fn = Box::new(move |candidate: &[ChoiceNode]| {
        let values: Vec<ChoiceValue> = candidate.iter().map(|n| n.value.clone()).collect();
        let mut ntc = NativeTestCase::for_choices(&values, Some(candidate));
        let is_interesting = user_test_fn(&mut ntc);
        (is_interesting, ntc.nodes)
    });

    Shrinker::new(test_fn, initial_nodes)
}
```

The `user_test_fn` returns `true` for "mark_interesting" (Python's
implicit `raise InterestingException` in the `@shrinking_from` body).
Call `shrinker.shrink()` to run the full pipeline, then assert on
`shrinker.current_nodes` to check the minimal choice sequence. See
`tests/hypothesis/conjecture_shrinker.rs` for worked examples.

`Shrinker::new`, `ChoiceNode`, `ChoiceKind`, and `ShrinkRun` are
re-exported via `__native_test_internals`. If a port needs another
private shrinker API (a specific pass method, a state accessor), add
the re-export in `src/lib.rs` as part of the same commit — no separate
source-stub needed. If the underlying item is `pub` but gated on
`#[cfg(test)]` (as `Shrinker::new` was before this port), **remove
the gate too** — integration tests compile against the library without
`--test`, so `#[cfg(test)]` items aren't visible to them. Adding the
re-export alone produces a misleading "private type in public
interface" or "function not found" error at the test crate.

### Spans inside the test body

Python `data.start_span(label)` / `data.stop_span()` brackets port to a
`with_span` helper that captures `tc.nodes.len()` before and after the
body and calls `NativeTestCase::record_span(start, end, label)`. The
native shrinker's passes don't consume span metadata (unlike
Hypothesis's `pass_to_descendant` / `reorder_spans`), so the recorded
spans are faithfulness-only; the shrink pipeline still runs end-to-end.
Don't skip a test just because it uses spans — the shape is portable.

`stop_span(discard=True)` maps to setting `tc.has_discards = true`
(the field is `pub`) when the discarded branch is taken.

### The `@run_to_nodes` pytest helper

Several `test_shrinker.py` tests decorate their body with
`@run_to_nodes`, a module-level pytest helper that runs a
`ConjectureRunner` to discover an initial interesting choice
sequence automatically. This is NOT the same as `shrinker.node_program`
or any other shrinker API — it's test-setup sugar for producing the
first interesting case. The port usually just hand-seeds an initial
choice list that exercises the same body path and passes it to
`shrinking_from(initial, body)`. See `test_handle_empty_draws` in
`tests/hypothesis/conjecture_shrinker.rs` for a worked example
(seeded `(1, 0)` plus `has_discards = true` to reproduce a discarded
first iteration).

### Quality tests that inspect or splice choice sequences

`tests/quality/` files that run `ConjectureRunner.generate_new_examples()`
+ `shrink_interesting_examples()` and then read `data.choices` /
`data.nodes` (rather than just asserting on a generated value) port
through the same `Shrinker::new(...).shrink()` shape as `@shrinking_from`,
skipping the generate phase. Hand-seed a deterministic initial choice
sequence that exercises the body, shrink it, and assert on
`shrinker.current_nodes`. Use `Minimal` only when the test asserts on a
*generated value*; use the `Shrinker` shape when it asserts on the
choice sequence or splices new choices into it.

Because the generate phase is skipped, the `@pytest.mark.parametrize("seed", ...)`
axis collapses — extending the rule from the seed-collapse note above to
the `ConjectureRunner`-style shape (`quality_poisoned_trees.rs` goes from
2 seeds × 3 sizes to 3 tests).

**Splice + re-shrink** (the
`runner.cached_test_function(choices[:i] + new_choices + choices[i+k:])`
idiom): hand-build a `Vec<ChoiceValue>` with the spliced region, pass it
to `NativeTestCase::for_choices(&spliced, None)` to replay once (and
verify it's still interesting), then `Shrinker::new(test_fn, ntc.nodes)
.shrink()` it. To find leaf-node indices to splice at, walk
`shrunk_nodes` filtering on `ChoiceKind::Integer(k) if k.max_value == …`.

**Assertions on the shrunk *generated value*, not the seed.** If the
upstream test asserts on a property of the generated value after
shrinking (e.g. `assert len(tree) == size`, "the shrunk tree has exactly
the minimum leaves"), the equivalent in a hand-seeded port is to replay
the shrunk choices through your `draw_*` function and assert on *that*:
`NativeTestCase::for_choices(&values_of(&shrinker.current_nodes), None)`
then re-run the draw. Do **not** apply the assertion to the initial
hand-seeded draw — by construction the seed already satisfies the
shape, so the assertion is tautological and silently stops being a
regression check.

**Recursive `SearchStrategy.do_draw` → iterative pre-order traversal.**
Python lets `do_draw` call `data.draw(self) + data.draw(self)` recursively,
but a Rust port can't nest `ntc.draw_*` calls inside a closure that holds
`&mut NativeTestCase`. Rewrite as a single loop with a pending-leaves
counter:

```rust
let mut pending = 1usize;
while pending > 0 {
    pending -= 1;
    if ntc.weighted(p, None).ok()? { pending += 2; }
    else { /* draw leaf, push result */ }
}
```

This preserves the Python pre-order choice ordering (split node, then
left subtree before right subtree) — the invariant the shrinker relies
on for the splice-at-leaf-index pattern to land at the right place.

### Tests this shape can't reach

`fixate_shrink_passes([ShrinkPass(pass_name)])` is not by itself a
reason to skip. Running `Shrinker::shrink()` end-to-end usually
converges on the same minimum as the single Python pass the fixate
narrows to, so *port the test against the full pipeline and keep the
minimum-choice assertion*. Drop any incidental call-count or
`valid_examples` assertions that the native `Shrinker` has no
counterpart for. Only skip when the test's subject is specifically a
single-pass invariant that the full pipeline violates — for example,
`test_redistribute_with_forced_node_integer` asserts that
`redistribute_numeric_pairs` preserves a `forced=10` node, which the
full pipeline can still lower via unrelated passes.

Unbounded `data.draw_integer(min_value=0)` / `data.draw_bytes()`
(no `max`) are NOT by themselves a reason to skip. Native
`tc.draw_integer(min, max)` and `tc.draw_bytes(min_sz, max_sz)`
require concrete bounds, but picking a max that comfortably covers
what the test's initial choice list actually produces works fine —
common choices are `(1i128 << 24) - 1`, `i32::MAX as i128`, or the
largest parametrize row for sizes. The shrink assertion still holds
because shrinking drives towards zero within whatever max you pick.
Only skip if the test's invariant genuinely depends on the max being
unbounded (rare).

The parts of `test_shrinker.py` that genuinely don't port through
`shrinking_from` go to `SKIPPED.md` for one of these concrete reasons:

- **Public `draw` feature missing from the native API.** Examples:
  `draw_integer(..., shrink_towards=N)`, `draw_integer(..., forced=N)`
  as a public-facing constraint (`draw_integer_forced` exists but
  takes a different shape). Port once the feature lands, or leave
  listed. (`Sampler` *is* available — see the section below.)
- **Pass-level mutator API called directly.** Tests that call
  `shrinker.mark_changed(i)` / `shrinker.lower_common_node_offset()` /
  `shrinker.pass_to_descendant()` as methods on the Shrinker, not via
  `fixate_shrink_passes`. The native `Shrinker` doesn't expose these
  as public methods.
- **Span-consuming passes.** Tests that exercise `pass_to_descendant`
  or `reorder_spans` specifically (vs. using spans only as structure
  in the test body). The native shrinker's passes ignore span
  metadata, so these can't reach the Python-test-expected minimum.
- **Instrumentation the native `Shrinker` lacks.** `shrinker.calls`,
  `shrinker.max_stall`, `StopShrinking`, `initial_coarse_reduction()`,
  `shrinker.node_program("X" * i)` (the adaptive node-program pass
  called as a method). No counterparts in the native shrinker;
  termination is bounded by `MAX_SHRINK_ITERATIONS` with no
  observation hook. (Do NOT conflate this with the `@run_to_nodes`
  pytest helper — that ports via hand-seeding; see above.)
- **Monkey-patched runner/shrinker entry points.** Tests that stub
  `ConjectureRunner.generate_new_examples` or `Shrinker.shrink` to
  control the engine's first example or shrink path. No
  monkey-patching surface in the native engine.
- **Subclass the base `Shrinker` with a custom `run_step`.** The
  generic base-class `hypothesis.internal.conjecture.shrinking.common.Shrinker`
  ports to concrete structs (`IntegerShrinker`, `OrderingShrinker`)
  with fixed `run_step` implementations and no subclass-pluggable
  base.

When skipping for any of the above, name the concrete missing
feature in the `SKIPPED.md` entry (not "no counterpart") so the
gap stays traceable.

## Standalone value shrinkers (`conjecture/shrinking/*.py`)

Distinct from the node-sequence `Shrinker` above, Hypothesis's
`hypothesis.internal.conjecture.shrinking` subpackage ships per-value
minimisers — `Integer`, `Ordering`, `Collection`, `Bytes`, `String` —
each a `Shrinker` subclass exposing an `(initial, predicate)` →
minimised-value API. hegel-rust ports them as concrete structs in
`src/native/shrinker/value_shrinkers.rs`, re-exported via
`__native_test_internals`:

| Hypothesis                                           | hegel-rust (native only)                                                    |
|------------------------------------------------------|------------------------------------------------------------------------------|
| `Integer.shrink(n, lambda x: pred(x))`               | `let mut s = IntegerShrinker::new(n, pred); s.run(); s.current().clone()`    |
| `Ordering.shrink(xs, pred)`                          | `let mut s = OrderingShrinker::new(xs, pred); s.run(); s.current().to_vec()` |
| `Ordering.shrink(xs, pred, full=True)`               | `OrderingShrinker::new(xs, pred).full(true)` — kwargs become builder methods |
| `Bytes.shrink(initial, pred, min_size=n)`            | `BytesShrinker::shrink(initial, pred, n)` — classmethod → unit struct assoc. fn |
| `String.shrink(initial, pred, intervals=iv, min_size=n)` | `StringShrinker::shrink(initial, pred, &iv, n)` — returns `Vec<char>`    |
| `Collection(xs, pred, ElementShrinker=Integer, min_size=n)` | `CollectionShrinker::new(xs, pred, n)` — `ElementShrinker` is implicit (always Integer for now) |
| `shrinker.left_is_better(a, b)` (bool)               | same name on the Rust shrinker                                               |
| `shrinker.calls` (int)                               | `shrinker.calls()` — counts distinct inputs seen, same semantics             |

Classmethod-style `.shrink(...)` entry points (`Bytes`, `String`) map
to **unit structs with an associated `shrink` function**, not to a
`::new(...).run()` pair — mirroring the Python one-shot API so test
bodies read the same way. Instance-style ones (`Integer`, `Ordering`,
`Collection`) use `::new(...)` because the tests also observe
`current` / `calls` / `left_is_better` between steps.

**Python predicate idioms.** Closures in these tests routinely call
`set(x)` or `Counter(x)` on a byte/int sequence to express "same
multiset of elements". Translate with small local helpers — don't add
them to `tests/common/utils.rs`:

```rust
fn bytes_set(v: &[u8]) -> std::collections::HashSet<u8> { v.iter().copied().collect() }
fn bytes_counter(v: &[u8]) -> std::collections::HashMap<u8, usize> {
    let mut m = std::collections::HashMap::new();
    for &b in v { *m.entry(b).or_insert(0) += 1; }
    m
}
```

The closures capture the expected `HashSet`/`HashMap` up front (built
from `start`) and compare against it inside the predicate. Rust's
`move` closure + `HashMap: Eq` handles it cleanly.

**`IntervalSet.from_string("abcdefg")`** maps to
`IntervalSet::new(vec![('a' as u32, 'g' as u32)])` — the public
constructor takes a sorted `Vec<(u32, u32)>` of inclusive ranges. For
disjoint characters pass one singleton range each: `vec![('a' as u32,
'a' as u32), ('z' as u32, 'z' as u32)]`.

**`@pytest.mark.parametrize` with lambda predicates** over shrinker
inputs: expand to one `#[test]` per row (lambdas are not first-class
enough across threads to loop over a `Vec<Box<dyn Fn>>` with `FnMut`
shrinker predicates). Name each test after what the row is checking
— `test_shrink_bytes_sum_at_least_nine`, not `test_shrink_bytes_case_4`.

**Missing pieces to skip when porting these files.** `debug=True`,
`random=Random(0)`, `name="…"`, and `__repr__` assertions are all
Hypothesis-only surface. `test_shrinking_interface.py` is unportable
for this reason alone. The `Collection` shrink pipeline (delete /
reorder / minimise-duplicates / minimise-each-element) **is** ported;
earlier skip rationales about "no Collection port" no longer apply.

**Direct `shrinker.run_step()` calls ARE portable** — promote
`run_step` from `fn` to `pub fn` in `src/native/shrinker/value_shrinkers.rs`
as a one-line source change and call it the same way the Python test
does. `test_order_shrinking.py` ports this way (see
`tests/hypothesis/conjecture_order_shrinking.rs` and the `pub fn
run_step` on `OrderingShrinker`). Don't skip a test on the grounds
that "run_step isn't exposed"; expose it. Same applies to the other
value shrinkers if a test reaches for their `run_step`.

## Forced draws (engine-internal, native only)

Hypothesis's `conjecture/test_forced.py` exercises the "pass `forced=X`
to a draw, the draw returns X, and the emitted choice sequence replays
back to X without `forced`" invariant. hegel-rust exposes the forcing
side on `NativeTestCase` via per-type helpers:

| Hypothesis                                              | hegel-rust (native only)                                              |
|---------------------------------------------------------|------------------------------------------------------------------------|
| `data.draw_boolean(p, forced=True)`                     | `ntc.weighted(p, Some(v))`                                             |
| `data.draw_integer(a, b, forced=n)`                     | `ntc.draw_integer_forced(a, b, n)`                                     |
| `data.draw_float(min, max, nan?, inf?, forced=f)`       | `ntc.draw_float_forced(min, max, allow_nan, allow_infinity, f)`        |
| `data.draw_bytes(min_sz, max_sz, forced=bs)`            | `ntc.draw_bytes_forced(min_sz, max_sz, bs)`                            |
| `data.draw_string(lo_cp, hi_cp, min_sz, max_sz, forced=s)` | `ntc.draw_string_forced(lo_cp, hi_cp, min_sz, max_sz, s)`           |

All four panic if `forced` violates the declared constraints (outside
range, wrong length, disallowed NaN, etc.) — mirroring `weighted`.
`draw_integer` takes `(min, max)` only; rows with `shrink_towards=` /
`weights=` are unportable until those constraints land natively.

### Forced → replay roundtrip

The canonical `test_forced_values` shape is:

```rust
let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
let drawn = ntc.draw_float_forced(min, max, nan, inf, forced).ok().unwrap();
assert!(choice_equal_float(drawn, forced));

let choices: Vec<ChoiceValue> = ntc.nodes.iter().map(|n| n.value.clone()).collect();
let mut replay = NativeTestCase::for_choices(&choices, None);
let replayed = replay.draw_float(min, max, nan, inf).ok().unwrap();
assert!(choice_equal_float(replayed, forced));
```

`choice_equal_float` (re-exported via `__native_test_internals`) is
bit-exact: `0.0 != -0.0`, distinct NaN payloads compare unequal, etc.
Use it instead of `==` whenever the test is asserting NaN / zero-sign
preservation. For plain equality on integers / bytes / strings the
default `==` / `assert_eq!` is fine.

### Adding a new forced-draw helper

If a port needs a forced-draw shape not in the table above (e.g. a
future `draw_integer` with `shrink_towards`), follow the pattern in
`src/native/core/state.rs`:

1. Validate the forced value against the `XChoice { … }` constraint
   struct (`kind.validate(&forced)`).
2. `self.pre_choice()?` — same prologue as the non-forced draw.
3. Push a `ChoiceNode { kind: ChoiceKind::X(kind), value:
   ChoiceValue::X(forced.clone()), was_forced: true }`.
4. Return the forced value unchanged.

The `was_forced = true` marker is what makes the choice replay
deterministically under `for_choices`.

## `ChoiceKind` / `ChoiceValue` direct API (`test_choice.py`)

Separate from the `for_choices` replay surface, Hypothesis's
`conjecture/test_choice.py` exercises the **value-level** choice API
directly: `compute_max_children(constraints)`, `choice_permitted(value,
constraints)`, and per-kind `to_index` / `from_index` round-trips used
by `datatree`. hegel-rust exposes the same surface as methods on the
per-kind constraint struct, re-exported via `__native_test_internals`:

| Hypothesis (Python)                          | hegel-rust (native only)                                                    |
|----------------------------------------------|------------------------------------------------------------------------------|
| `compute_max_children(kind, constraints)`    | `compute_max_children(&ChoiceKind::X(XChoice { … }))` — free function       |
| `choice_permitted(value, constraints)`       | `ChoiceKind::X(XChoice { … }).validate(&ChoiceValue::X(v))`                 |
| `choice_to_index(v, constraints)`            | `XChoice { … }.to_index(v)` — returns `BigUint`                             |
| `choice_from_index(i, "kind", constraints)`  | `XChoice { … }.from_index(BigUint::from(i))` — returns `Option<V>`          |
| `choice_from_index(0, "kind", constraints)`  | equivalently `XChoice { … }.simplest()` — the sort-order anchor             |
| `MAX_CHILDREN_EFFECTIVELY_INFINITE`          | same name, re-exported via `__native_test_internals`                        |
| `next_down(f)` / `next_up(f)`                | same names, re-exported via `__native_test_internals`                       |

`BigUint` (from `num-bigint`) is re-exported too. Use `BigUint::from(n)`
(with `n: u64`) for literals; the derive-equality `assert_eq!` works
directly.

**`ChoiceValue::String` wraps `Vec<u32>`, not `String`.** The string
variant stores a codepoint sequence so replay can round-trip codepoints
outside the UTF-8 range. Convert Python string literals at
construction: `ChoiceValue::String("abc".chars().map(|c| c as u32).collect())`.
The other variants are unsurprising — `Integer(i128)`, `Float(f64)`,
`Bytes(Vec<u8>)`, `Boolean(bool)`.

**Empty alphabet via the surrogate range.** Python's `intervals=""`
(zero-char alphabet, used to test `compute_max_children` collapsing to
1) has no `StringChoice` equivalent — `min_codepoint` / `max_codepoint`
define a range, not an arbitrary set. Use the surrogate block
`[0xD800, 0xDFFF]` to get `alpha_size() == 0`: every codepoint in that
range is filtered out as invalid UTF-16. Port bytes-alphabet-zero the
same way (`min_size=0, max_size=0` → empty range of length zero).

**Native absent fields and which rows survive.** Several Python
constraint fields have no slot on the native `XChoice` structs; the
parametrized rows keying on them must be dropped (not skipped — they
aren't representable):

- `IntegerChoice`: no `weights`, no `shrink_towards`. The
  `shrink_towards=N` invariant degenerates to `to_index(simplest()) == 0`
  (anchor is the range-endpoint nearest zero).
- `FloatChoice`: no `smallest_nonzero_magnitude`.
- `BooleanChoice`: no `p`. `compute_max_children(BooleanChoice) == 2`
  always; Python's `p=0.0` / `p=1.0` rows (which collapse to 1) are
  unrepresentable. The `p=0.5` / `p=0.001` / `p=0.999` rows all collapse
  to 2 — port those.

Name the dropped fields concretely in both the module docstring and the
SKIPPED.md entry so a reviewer can see what happened to the missing
rows.

**`Status::OVERRUN` → `Status::EarlyStop`.** Python asserts
`data.status is Status.OVERRUN` after drawing from an empty
`for_choices([])` prefix. Native has no distinct `Overrun`; `pre_choice`
sets `Status::EarlyStop` on the same path. Assert
`data.status == Some(Status::EarlyStop)`.

### Engine surfaces with no native counterpart

`test_choice.py` exercises a cluster of `ChoiceNode` / datatree helpers
that simply aren't on native. Don't waste time looking — the list
below was charted on this port. Individually-skip each test (with
SKIPPED.md entries) rather than trying to stub them:

- `all_children(kind, constraints)` — iterator over every valid value
  of a `ChoiceKind`. `compute_max_children` is ported but the
  enumerator is not. Blocks `*_and_all_children_agree`,
  `*_are_permitted_values`, `*_injective`, `*_from_value_injective`.
- `ChoiceTemplate("simplest", count=n)` — prefix primitive that tells
  `for_choices` to produce the simplest value of each step's kind.
  `NativeTestCase::for_choices` takes only concrete `ChoiceValue`s.
- `ChoiceNode.copy(with_value=…)` **raising on forced nodes** (blocks
  `test_cannot_modify_forced_nodes` only) — native
  `ChoiceNode::with_value` propagates `was_forced` through rather than
  panicking. The non-forced branch of `test_copy_choice_node` (which
  has `assume(not node.was_forced)` filtering) ports fine against
  `ChoiceNode::with_value`; only the assertion that copying a *forced*
  node errors is unrepresentable.
- `choices_size([values])` — byte-width of a choice sequence.
- `choices_key([values])` — dedup key distinguishing `True` from `1`,
  etc. Rust's `Vec<ChoiceValue>` already distinguishes by enum variant,
  so the *invariant* ports (as `assert_ne!` on vectors); the *helper*
  doesn't.
- Cross-type `PartialEq` (`node != 42`) — Rust's type system forbids
  the comparison at compile time. Unrepresentable.
- Assertions trivially satisfied by Rust types —
  `ChoiceKind::{to,from}_index` returns `BigUint` (unsigned), so
  `index >= 0` is a tautology with no observable behaviour. Skip
  `test_choice_indices_are_positive`-shape tests with that rationale.

### Porting shape

The file is dominated by `@pytest.mark.parametrize` rows and
`@example` stacks on `@given(choice_types_constraints())` PBTs. The
conjecture-file convention (`conjecture_*.rs`) is to split each row
into its own `#[test]` with a name encoding the row — e.g.
`test_compute_max_children_string_empty_alphabet`,
`test_choice_permitted_integer_in_range`. See
`tests/hypothesis/conjecture_choice.rs` for the worked expansion.

The PBT bodies of `@given(choice_types_constraints())` port as a
handful of hand-picked `#[test]` witnesses (integer full-range,
boolean, bytes-unbounded, string-full-unicode) — enough to exercise
the mainline branches of `compute_max_children` / `validate` /
`to_index` without an enumerator.

## `ConjectureData` direct API (`test_test_data.py`)

`conjecture/test_test_data.py` exercises Hypothesis's `ConjectureData`
test object directly. Most of its tests rely on engine surface that
has no native counterpart; the portable subset is small. Read the
upstream cluster *before* committing to a port — three of 33 tests
ported on the first pass.

What ports through `NativeTestCase::for_choices(&[...], None)` plus
direct `weighted` / `draw_bytes_forced` / `record_span` calls and
`nodes[i].trivial()` reads:

- Status-after-overrun: an extra draw past the end sets
  `status == Some(Status::EarlyStop)` and `weighted(...)` returns `Err`.
- Pre/post-freeze comparisons: native has no separate `freeze()`
  step, so `[trivial() before freeze == trivial() after freeze]`
  collapses to a single read of `nodes[i].trivial()`.
- Triviality lookups by `(start, end)`: replay the choice sequence,
  then `d.spans.iter().find(|s| s.start == u && s.end == v)`.
  **Hypothesis auto-creates a span around each `data.draw(strategy)`
  call; native primitives (`ntc.weighted`, `ntc.draw_bytes_forced`,
  etc.) do NOT.** Mirror the auto-spans by calling
  `record_span(start, end, label)` after each "logical draw" — without
  it the `(start, end)` lookup misses and the test fails for the wrong
  reason.

What does NOT port (individually-skip with a gap-named rationale):

| Hypothesis API used                                  | Native gap                                                                 |
|------------------------------------------------------|----------------------------------------------------------------------------|
| `data.freeze()` / `data.frozen` flag                 | no public `freeze()` on `NativeTestCase`; status is the only freeze marker |
| `data.mark_interesting()` / `data.mark_invalid()`    | live on `NativeConjectureData`, whose `for_choices` constructor is private |
| `data.note(...)`, `data.output`, `data.events`       | no `note`/`output`/`events` API on either native test-case type            |
| `data.draw(strategy)` auto-recording spans           | no draw-by-strategy method on `NativeTestCase`; strategies route through `Generator::do_draw` Hegel-side |
| `data.examples` / `Span.parent`/`.children`/`.depth` | `Span` is `{ start, end, label }` (`src/native/core/state.rs`); no tree shape, no `discarded` flag |
| `DataObserver`                                       | no observer hook on native draw paths                                      |
| `MAX_DEPTH` recursion-depth limit                    | no depth limit on native (and no draw-by-strategy method to bound)         |
| `data.as_result()` / `data.is_overrun`               | no `as_result`; closest analog is `status == Some(Status::EarlyStop)`      |
| `data.structural_coverage()` / `tags`                | no coverage-tag tracking                                                   |
| `ConjectureData(prefix=…, random=None, max_choices=N)` | only `for_choices(...)` and `new_random(...)` constructors exist         |

When porting this file, list each skipped test by name in **both**
the module docstring and `SKIPPED.md` (under "Individually-skipped
tests") so the unported-gate sees them and a future agent can pick
them up as each native gap closes — the gap names above are the
acceptance criteria for un-skipping.

## Conjecture utilities (`conjecture/utils.py`)

A small native-only surface for the `nocover/test_conjecture_utils.py`
shape (and any future tests that reach into Hypothesis's `cu.*`
helpers). Re-exported via `__native_test_internals` from
`src/native/conjecture_utils.rs`:

| Hypothesis (`hypothesis.internal.conjecture.utils`) | hegel-rust (native only)                                    |
|------------------------------------------------------|--------------------------------------------------------------|
| `Sampler(weights)`                                   | `Sampler::new(&weights)` — `weights: &[f64]`                |
| `sampler.sample(data)`                               | `sampler.sample(&mut ntc) -> Result<usize, StopTest>`        |
| `cu._calc_p_continue(avg, max)`                      | `calc_p_continue(avg, max) -> f64`                           |
| `cu._p_continue_to_avg(p, max)`                      | `p_continue_to_avg(p, max) -> f64`                           |
| `cu.SMALLEST_POSITIVE_FLOAT`                         | same name; `f64::from_bits(1)`                               |

`Sampler::sample` follows the standard native draw shape — takes
`&mut NativeTestCase`, returns `Result<usize, StopTest>`, and consumes
choice nodes (one `draw_integer` for the table index, one `weighted`
for the alternate-or-base coin). The Python-side `forced=` /
`observe=` kwargs are not exposed; no port has needed them yet.

The Hypothesis test uses `provider_conformance.integer_weights()` to
generate `dict[int, float]` inputs for `Sampler`. Only the dict
*values* matter — `Sampler` ignores keys — and the values are arbitrary
positive floats whose absolute magnitudes don't matter (`Sampler`
normalises internally). When porting such a strategy, generate
integer "buckets" and divide by the sum so the inputs shrink as
integers rather than via the float-shrinker:

```rust
gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000))
    .min_size(1).max_size(20)
    .map(|buckets: Vec<u64>| {
        let total: f64 = buckets.iter().map(|&b| b as f64).sum();
        buckets.iter().map(|&b| b as f64 / total).collect()
    })
```

This generalises: any `dict[K, float]` / `list[float]` strategy whose
*distribution* is the only observable input (weights, normalised
probabilities, score arrays) ports cleanly via integer-buckets-then-
normalise rather than chaining `gs::floats()`.

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

### Shrink-quality stacks where rows gap individually

An exception to "consolidate into one `_examples` test": shrink-quality
tests where each `@example` row asserts the shrinker converges on a
specific minimum, and the native `Shrinker` reaches that minimum on
some rows but hits `MAX_SHRINK_ITERATIONS` on others (engine gap —
single-choice passes can't follow a predicate that couples two
choices, etc.). In that shape, **split each `@example` row into its
own `#[test]`** so the gapping rows can be `#[ignore]`d individually
and the passing rows stay as regression coverage. A consolidated
`_examples` test would have to be ignored wholesale, losing the
passing rows. All ignored rows share one TODO.yaml entry (acceptance
criteria: un-ignore `test_foo_1` … `test_foo_k`). See
`tests/hypothesis/quality_zig_zagging.rs` for the shape — five rows
pass today, six are `#[ignore]`d under a single
`pair-locked-zig-zag-shrink` TODO.

Related: if the `@given` random-fuzz pass in the same test only
asserts on a counter the native `Shrinker` doesn't expose (the usual
case is `runner.shrinks <= budget`), drop the `@given` entirely —
without the budget check it collapses to the same
minimum-correctness assertion as the explicit rows and adds no
coverage. Note the drop in the module docstring.

Same outcome — same fix — when the `@given` body itself calls a
derandomised helper (`minimal(...)`, `find_any(...)`) rather than
asserting `runner.shrinks <= budget`. The upstream signal is
`@settings(suppress_health_check=[HealthCheck.nested_given])` sitting
above a `@given` whose body is one `minimal(...)` call: each random
input re-runs the full 500-case derandomised shrink, the assertion
is just "the shrink reaches `ceil(f)`", and the `@example` rows
already cover the representative boundaries. Port only the `@example`
rows as individual `#[test]`s and drop the outer `@given` loop.
`HealthCheck.nested_given` has no hegel-rust analog (see the
Health-check section), so the `suppress_health_check` setting also
drops. Note both drops in the module docstring. Precedent:
`tests/hypothesis/quality_float_shrinking.rs` ports the two
`@given`-over-`minimal` tests in `test_float_shrinking.py` this way.

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

The same seeded-PRNG-from-outer-`tc` technique applies any time a
closure needs dynamic oracle values but has no `tc` in scope — the
other frequent case is a **predicate passed to `minimal()` /
`find_any()`**. Those helpers spawn their own nested runner, so the
predicate is called with no outer `tc` visible. A `@given(st.data())`
upstream whose predicate body reads `data.draw(st.booleans())` (e.g.
`nocover/test_boundary_exploration.py`'s `predicate` for an
arbitrary-but-consistent oracle per input) ports to the same seed +
`StdRng` pattern, with a `RefCell<HashMap<Input, Value>>` cache inside
the closure when the Python version uses `cache.setdefault` to keep
the oracle consistent per input:

```rust
let seed: u64 = tc.draw(gs::integers::<u64>());
let rng = RefCell::new(StdRng::seed_from_u64(seed));
let cache: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
let predicate = move |x: &String| -> bool {
    if let Some(&v) = cache.borrow().get(x) { return v; }
    let v = rng.borrow_mut().next_u64() & 1 == 0;
    cache.borrow_mut().insert(x.clone(), v);
    v
};
minimal(gs::text().min_size(5), predicate);
```

If the seeded oracle sometimes makes the predicate unsatisfiable,
wrap the `minimal()` call in `catch_unwind(...).ok()` per the
`try: minimal(...) except Unsatisfiable` row in the Helpers table
above.

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
| `find_any(s) is not find_any(s)` / any `is`/`is not` check between two draws | **unportable — skip individually** | Python `is`/`is not` is object-identity. hegel-rust `tc.draw(...)` returns owned/cloned values by type, so distinctness is structural rather than identity-observable (`==` would always be *true*, never *false*, for equal payloads). Tests that pin down "repeated draws must not reuse the same strategy object" (e.g. `nocover/test_flatmap.py::test_flatmap_does_not_reuse_strategies`) have no Rust analog. |
| `xs.remove(y)` on `list[T]` | `let pos = xs.iter().position(\|v\| *v == y).unwrap(); xs.remove(pos);` | Python's `list.remove` takes a **value** and removes the first match; Rust's `Vec::remove` takes an **index**. Same method name, different semantics — translate via `position` + `remove`. |
| `min(a, b)` / `max(a, b)` on floats that may be NaN | `if a < b { a } else { b }` / `if a > b { a } else { b }` | **`f64::min` / `f64::max` silently drop NaN in favour of the other operand; Python's `min`/`max` propagate it.** Load-bearing whenever the test asserts that a NaN input stays NaN through a clamp (e.g. `cathetus(h, nan)` = `nan`). Using `f64::min` here will silently break the NaN case and no other test will catch it. |
| `not (x < end)` as a shrink-interesting predicate on a float draw | `#[allow(clippy::neg_cmp_op_on_partial_ord)] let cond = \|x: f64\| !(x < end);` | Python's `not (x < end)` is not the same as `x >= end` when `x` is NaN: all NaN comparisons return False, so `not (x < end)` is True for NaN while `x >= end` is False. Float-shrink tests (`test_can_shrink_downwards`, `test_shrinks_to_canonical_nan`) use the `not <` form specifically to make NaN "interesting" so the shrinker has to canonicalise it. Clippy's `neg_cmp_op_on_partial_ord` wants to rewrite `!(x < end)` → `x >= end`; suppressing the lint is correct, rewriting silently changes which inputs trigger the test. |
| `n % K == r` on a signed-integer draw where `r != 0` | `n.rem_euclid(K) == r` | Python `%` always returns a non-negative result when `K > 0`; Rust `%` takes the sign of the dividend. For `n = -39, K = 50` Python gives `11`, Rust gives `-39`. A literal `%` translation silently rejects every negative `n` that upstream would have accepted, changing the `assume()` acceptance rate (and with it, test timing / health-check behaviour). `rem_euclid` matches Python. **`r == 0` is safe** — `0 == -0`, so the sign mismatch never surfaces and `%` is fine there. |

### Python `math` / `sys.float_info` → Rust f64

| Python                                       | Rust                                 | Notes                                                                 |
|----------------------------------------------|--------------------------------------|-----------------------------------------------------------------------|
| `sys.float_info.min`                         | `f64::MIN_POSITIVE`                  | Smallest positive **normal**. **Not** `f64::MIN` — that's `-f64::MAX`. |
| `sys.float_info.max`                         | `f64::MAX`                           |                                                                       |
| `sys.float_info.epsilon`                     | `f64::EPSILON`                       |                                                                       |
| `sys.float_info.min * sys.float_info.epsilon` | `f64::from_bits(1)`                 | Smallest positive **subnormal**. The Python idiom exploits that the multiplication yields exactly bit pattern 1; in Rust skip the arithmetic and name the bit pattern. |
| `math.inf` / `math.nan`                      | `f64::INFINITY` / `f64::NAN`         |                                                                       |
| `-math.nan` / `float('-nan')`                | `f64::NAN.copysign(-1.0)`            | Python's unary minus on NaN flips the sign bit deterministically. **`-f64::NAN` in Rust has implementation-defined sign** and cannot be relied on for bit-exact roundtrips — use `copysign` when the test preserves NaN sign. Generalises to `nan.copysign(sign)` for `sign ∈ {-1.0, 1.0}` over a parametrize row. |
| `1e999` / `-1e999` (Python literal)          | `f64::INFINITY` / `f64::NEG_INFINITY` | Python parses overflowing float literals to infinity at compile time — not an error. An `@example(1e999)` row ports as `f64::INFINITY`, not a panic test. |
| `hypothesis.internal.floats.SIGNALING_NAN`   | `f64::from_bits(0x7FF8_0000_0000_0001)` | No Rust stdlib constant. Define as a `const SIGNALING_NAN: u64 = 0x7FF8_0000_0000_0001;` at the top of the ported file and use `f64::from_bits(SIGNALING_NAN)` at call sites. Bit pattern must match upstream exactly: some tests parametrize over `[nan, -nan, SIGNALING_NAN, -SIGNALING_NAN]` and expect four distinct starting points, so using a custom "IEEE-canonical" sNaN like `0x7FF0_…_0001` collapses rows. |
| `hypothesis.internal.floats.SMALLEST_SUBNORMAL` | `f64::from_bits(1)`               | Same — no stdlib constant. `f64::MIN_POSITIVE` is the smallest *normal*, which is different. |
| `math.isnan(x)` / `math.isinf(x)` / `math.isfinite(x)` | `x.is_nan()` / `x.is_infinite()` / `x.is_finite()` | Methods, not free functions.                                          |
| `math.fabs(x)`                               | `x.abs()`                            | Method.                                                               |
| `math.sqrt(x)`                               | `x.sqrt()`                           | Method.                                                               |
| `math.hypot(a, b)`                           | `a.hypot(b)`                         | Method on `f64`, not a free function.                                 |
| `int(x)` where `x: float` and the result is compared back as a float (e.g. `x == int(x)`, `x + 1 != x`) | `x.trunc()` | Stay in float-space. **Do not** translate as `x as i64` — the cast saturates at `i64::{MIN,MAX}` on large-magnitude floats, silently swallowing the `x + 1 == x` / `x == trunc(x)` regressions that `findability/test_floats.py` exists to surface. |
| `isinstance(x, float)` as a test assertion   | trivially true                       | `gs::floats::<f64>()` is statically typed, so there is no runtime type-check to assert. Port the test body as a smoke test that draws from the generator (mirrors the upstream surface without a contentful assertion). |

### `str(x)` / `repr(x)` collapse for floats

Python has two distinct float formatters — `str(x)` produces the shorter
common form, `repr(x)` is the round-trip-guaranteeing form — and tests
such as `test_can_find_float_that_does_not_round_trip_through_str` vs
`..._through_repr` exist because the two paths have separate
counterexamples in Python.

In Rust, both `format!("{x}")` and `format!("{x:?}")` produce
round-trippable representations for `f64`, so the two tests collapse
into the same assertion. Both **still fail as expected** — the
counterexample both paths find is NaN (`NaN != NaN`), which is not a
formatter property. Port both tests to mirror the upstream surface,
and add a one-line comment on the `{:?}` variant noting the distinction
is vestigial in Rust so a future reader isn't left wondering why the
two are redundant.

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

## Recursive `gs::deferred` outputs: tame the Rust stack

Native-mode generation through `gs::deferred` produces much deeper
recursive trees than the Hypothesis server (whose wire protocol bounds
depth) and lacks the leaf-bias for high-branching grammars that
Hypothesis applies in `ConjectureData.draw`. Python tests that walk
those trees recursively port literally just fine on the server backend
but blow the Rust stack on debug builds under `--features native`,
*before* the shrinker has a chance to converge. Two distinct sites
need iterative rewrites:

1. **User-side recursive predicates and evaluators.** A
   `match self; recurse children` walk is the natural Python port and
   the natural Rust port, but it overflows on the deep
   trees native generation reaches. Rewrite as a `Vec<&Node>`
   work-stack loop. For boolean predicates (e.g. `div_subterms`),
   one stack suffices. For post-order combiners (e.g. `evaluate`
   that adds / divides children), push two-phase commands —
   `Eval(node)` / `ReduceAdd` / `ReduceDiv` — onto a work stack and
   accumulate intermediate values onto a separate `Vec<i128>`.
   See `tests/hypothesis/quality_shrink_quality.rs::evaluate` for the
   shape.

2. **Auto-derived `Drop` for `Box<…>`-recursive enums.** Rust's
   default `Drop` for `enum Expr { Add(Box<Expr>, Box<Expr>), … }`
   is itself recursive: `Box::drop` runs the inner `Expr`'s `Drop`,
   which runs its children's `Box::drop`, and so on. Deep trees from
   native `gs::deferred` overflow on free, *even when the test body
   is `#[ignore]`d* (Drop runs as the value goes out of scope at the
   end of `minimal()`). Add an explicit `impl Drop` that pulls
   children into a `Vec<Expr>` work-stack, replacing each
   pulled-out child with a leaf so the post-loop auto-drop only
   walks leaves:

   ```rust
   impl Drop for Expr {
       fn drop(&mut self) {
           let mut stack: Vec<Expr> = Vec::new();
           if let Expr::Add(l, r) | Expr::Div(l, r) = self {
               stack.push(std::mem::replace(l.as_mut(), Expr::Int(0)));
               stack.push(std::mem::replace(r.as_mut(), Expr::Int(0)));
           }
           while let Some(mut node) = stack.pop() {
               if let Expr::Add(l, r) | Expr::Div(l, r) = &mut node {
                   stack.push(std::mem::replace(l.as_mut(), Expr::Int(0)));
                   stack.push(std::mem::replace(r.as_mut(), Expr::Int(0)));
               }
           }
       }
   }
   ```

If the test's witness pattern actually depends on the missing
leaf-bias (the calculator-benchmark fixed-point shape — N-of-M
recursive vs leaf branches, M > 1), the shrinker still won't
converge under native. `#[ignore]` it with
`#[cfg_attr(feature = "native", ignore = "...")]` and file the
`gs::deferred` leaf-bias TODO. The iterative-drop and iterative-walk
shapes above are still required so the file *compiles and links*
without crashing during teardown — they're independent of whether
the test asserts.

## Glob-importing from `hegel`

Python's `from hypothesis import *` / `from hypothesis.strategies import *`
ports as `use hegel::*;` / `use hegel::generators::*;` — but `hegel::*`
re-exports the `test` proc macro, which shadows the built-in `#[test]`
attribute if the glob lives at module scope. Scope the glob imports
inside the test function body:

```rust
#[test]
fn test_can_glob_import_from_hegel() {
    use hegel::generators::*;
    use hegel::*;
    // ...
}
```

Only relevant when porting a test file whose *purpose* is to exercise
the glob surface (e.g. `nocover/test_imports.py`); normal ports should
keep using the explicit `use hegel::generators::{self as gs, Generator};`
form per the main SKILL.

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
