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
- `index_passes.py` — shortlex enumeration-based shrinkers
  (`try_shortening_via_increment`, `lower_and_bump`). hegel-rust does
  not implement these.
- `mutation.py` — `mutate_and_shrink`, a last-resort pass that mutates
  a prefix then draws random continuations. hegel-rust does not
  implement this either.
- `sorting.py`, `sequence_redistribution.py`, `duplication_passes.py`
  — sequence-normalization passes (all implemented natively).

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
- `tc.forced_choice(n)` — not on the public `TestCase`. The native
  engine exposes per-type forced draws on `NativeTestCase`
  (`weighted`, `draw_integer_forced`, `draw_float_forced`,
  `draw_bytes_forced`, `draw_string_forced`) reachable via
  `hegel::__native_test_internals`; see api-mapping's "Forced draws"
  section for the full table and replay shape.
- `tc.draw_silent(gen)` — exists in hegel-rust; skips the named-draw
  machinery so no `let draw_N = …;` line is printed. The "no line" is
  observable via the `draw_lines` helper (assert empty); the "counter not
  incremented" is not (no counter surface).
- `tc.__draw_named(gen, name, repeatable)` — public entry point to the
  name-tracking machinery behind draw-output labels. pbtkit tests that
  manipulate `tc._named_draw_used` directly (e.g. the "repeatable skips
  taken suffixes" case) *are* portable through this public method; don't
  skip them as "internal Python attribute, no counterpart". Validation
  (non-repeatable reuse panics, inconsistent-repeatable-flag panics) runs
  only at `span_depth == 0`, so it fires at the top-level test body but
  is silenced inside `#[hegel::composite]` — mirror that gating when the
  upstream test hinges on it.

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

Before listing an engine-harness test as skipped, grep
`tests/embedded/native/` for its name — a prior port of a different
upstream file may have already covered it, in which case don't record
it as skipped at all. (The test_core.py port initially skipped three
cases that turned out to live in `tree_tests.rs`.)

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
  **Exception — "should not crash" regressions with no `state.result`
  inspection**: pbtkit's shrink-quality file has a cluster of tests whose
  whole body is `state = State(Random(seed), tf, N); state.run()` (sometimes
  looped over seeds) with *no* assertion on `state.result`. The test
  passes by not crashing. Port these as public-API integration tests —
  `Hegel::new(|tc| { ... }).settings(Settings::new().test_cases(N).database(None)).run()`
  wrapped in `std::panic::catch_unwind(AssertUnwindSafe(|| { ... }))` —
  rather than as embedded Shrinker tests. No native-gating, no
  `Shrinker::new` scaffolding. The `catch_unwind` is essential: the
  engine may still find a counterexample (causing the test body to
  panic), which is benign for a "did shrinking survive?" regression.
  When the test body *does* want the inner counterexample to surface
  (upstream `with pytest.raises(AssertionError):` over `@run_test`),
  use `expect_panic(|| { Hegel::new(...).run() }, ".")` — the `.`
  regex matches any panic message, since the upstream only checked
  that *something* asserted.
- **Asserting on `state.result` choice-sequence values**: when the
  upstream extracts `[n.value for n in state.result if isinstance(n.kind, IntegerChoice)]`
  (or similar) from a shrunk state, the equivalent for a simple
  list-generator test is the drawn `Vec<T>` itself. Port with
  `Minimal::new(...).test_cases(N).run()` and assert on the returned
  value directly rather than reconstructing the choice sequence
  (e.g. `sorted == vec![-2, -1, 0, 1, 2]` instead of
  `int_values == [0, 1, -1, 2, -2]`). Leave a one-line comment noting
  the upstream asserted on choices; we assert on the equivalent value.
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

### `FloatChoice` ordering divergence: raw vs Hypothesis-lex

pbtkit's `FloatChoice` orders floats by `(exp_rank, mantissa, sign)` on
their raw IEEE-754 bits. hegel-rust's `FloatChoice` matches
Hypothesis's lex ordering from `conjecture/floats.py::float_to_lex`,
which bit-reverses the mantissa of subnormals and re-encodes normals
via the (exponent_key, mantissa) reorder table. See the
implementing-native skill's `float_to_lex` note — the divergence is
deliberate and on the Hypothesis side.

Consequence: pbtkit tests exercising `FloatChoice` internals via
`simplest`, `unit`, `sort_key`, `to_index`, or `from_index` will often
assert values that *look* obvious under the raw ordering but land on
different floats under the lex ordering. Examples from
`test_floats.py`:

- `FloatChoice(-10.0, 10.0, False, False).unit`: pbtkit returns `-0.0`
  (raw index 1 is `-0.0`, next to `0.0`); hegel-rust returns `1.0`
  (lex index after `0.0` is the integer encoding of `1.0`).
- `FloatChoice(1e-323, 2e-323, False, False).simplest`: pbtkit picks
  `1e-323` (smaller raw mantissa); hegel-rust picks `2e-323` because
  lex bit-reverses subnormal mantissas (mantissa 4 → reversed bit
  `1<<49`, simpler than mantissa 2 → reversed `1<<50`).
- `test_float_shrinks_across_exponent_boundary`: pbtkit's shrinker
  finds `-2.0 - 1 ULP`; hegel-rust's stops at the simpler `-3.0`. Both
  satisfy `-3.0 ≤ v < -2.0` — widen the assertion to the range.

Port the test, but relax the assertion to either (a) the
hegel-rust-correct value with an in-file note explaining the ordering
divergence, or (b) the range both orderings satisfy. Don't skip —
the test is still exercising real behaviour on both sides.

`_MAX_FINITE_INDEX` is exposed as a module constant in pbtkit but not
re-exported in `hegel::__native_test_internals`. For FloatChoice
`from_index` tests, compute it locally from the lex-ordering formula
`((1<<63) | (2046<<52) | ((1<<52)-1)) * 2 + 1` (the negative variant of
the max subnormal lex index, packed through `float_global_rank`).

## Module-constant monkeypatches — semantic vs coverage-only

pbtkit tests occasionally `monkeypatch.setattr(pbtkit.module, "CONST",
…)` at runtime to tune a threshold (e.g. `BUFFER_SIZE` in `core.py`,
`NAN_DRAW_PROBABILITY` in `floats.py`). hegel-rust's equivalents are
`const` values under `src/native/…` with no runtime-patch surface, so
you can't reproduce the patch. But not all patches are equal — read the
upstream comment next to the `monkeypatch.setattr` call before deciding:

- **Semantic** patches are the point of the test: e.g.
  `test_error_on_unbounded_test_function` patches `BUFFER_SIZE` tiny to
  trigger the unbounded-test-function error path, and the test is
  meaningless without the patch. Skip these — list in both the module
  docstring and `SKIPPED.md`, naming the patched constant.
- **Coverage-only** patches just force a low-probability branch so
  pbtkit's own coverage run hits it: e.g. `test_floats_unbounded`
  patches `NAN_DRAW_PROBABILITY = 0.5` with the comment "Boost NaN
  probability so we reliably cover `_draw_nan`." The assertion the test
  actually makes ("unbounded draws complete without panicking") doesn't
  depend on the patch. Port the test without the patch and add a
  one-line comment explaining the upstream patch was coverage-only.

The clue is usually in the upstream comment immediately above the
`monkeypatch.setattr` line. If it says "reliably cover" / "boost
probability" / similar, the patch is coverage-only.

## `@pytest.mark.requires(...)` and `pytestmark`

pbtkit's `conftest.py` defines a `requires(module)` marker that skips a
test when the named pbtkit feature is disabled via `PBTKIT_DISABLED` —
e.g. `@pytest.mark.requires("collections")`,
`@pytest.mark.requires("shrinking.sorting")`,
`@pytest.mark.requires("shrinking.bind_deletion")`, or a module-level
`pytestmark = pytest.mark.requires(...)`. These are feature gates for
pbtkit's own compiled-mode builds, not test preconditions. The server
backend (Hypothesis) always has the corresponding behaviour, and most
markers also map to an implemented `src/native/shrinker/` pass, so
port such tests unconditionally — strip the marker and don't record it
anywhere.

The exceptions are markers whose named module has **no native
counterpart yet** in `src/native/shrinker/`. Don't enumerate them here —
the list rots as each pass gets ported. Check directly: open the pbtkit
file named by the marker (e.g. `shrinking/advanced_bytes_passes.py` for
`@pytest.mark.requires("shrinking.advanced_bytes_passes")`) and grep
`src/native/shrinker/` for its top-level function names. If no
counterpart exists, the marker is a current exception.

For shrink-quality tests (those under `tests/shrink_quality/` asserting
on the *exact* shrunk minimum), the established pattern is:

1. Port the test normally with its upstream-asserted minimum.
2. **Native-gate with `#[cfg(feature = "native")]`**. Shrink-quality
   assertions pin down the native engine's shrink output; the server
   backend may find a different-but-valid minimum, so running the same
   assertion in server mode is flaky.
3. Commit. The test fails in native mode until the pass is ported.
4. Implement the missing pass in `src/native/shrinker/` as a follow-on
   commit in the same sub-loop (mirror the closest existing sibling —
   `redistribute_string_pairs` is the model for `redistribute_bytes_pairs`,
   etc.). The fixer loop picks up the failing native-mode test and
   dispatches the implementation automatically.

Precedents: `tests/pbtkit/shrink_quality_strings.rs::test_string_length_redistribution`
and `tests/pbtkit/shrink_quality_bytes.rs::test_redistribute_bytes_between_pairs`
both used this pattern (test lands first, `redistribute_{string,bytes}_pairs`
land in the follow-on).

Server-gating with `#[cfg(not(feature = "native"))]` is the alternative
shape — used when the test is *not* a shrink-quality assertion (the exact
minimum doesn't matter, only that Hypothesis's behaviour is correct) and
you want the test running somewhere even before the native pass lands.
If you go this route, file a TODO.yaml entry listing the server-gated
tests as acceptance criteria (removing the gate is the "done" signal)
and note the gate at the top of the module docstring.

Neither shape applies if the required feature has no counterpart on
*either* side (neither Hypothesis nor hegel-rust native implements it) —
follow the native-gated-plus-stub policy in SKILL.md instead.

## Findability and shrink-quality tests

`tests/findability/` and `tests/shrink_quality/` in pbtkit are directly
analogous to `tests/test_find_quality/` and `tests/test_shrink_quality/` in
hegel-rust — same spirit, sometimes same test names. When porting one of
these, check whether an existing hegel-rust file already covers the same
ground before adding a new one.
