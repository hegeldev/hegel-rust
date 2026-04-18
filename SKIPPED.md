# Skipped upstream test files

Upstream test files that have been deliberately *not* ported, with a one-line
rationale each. The Stop hook's unported-gate (`list-unported.py`) reads this
file and treats listed files as "done".

## pbtkit (`/tmp/pbtkit/tests/`)

- `test_targeting.py` — uses `tc.target(score)`, a pbtkit public-API feature
  (targeted property-based testing) with no hegel-rust analog. Hegel-rust
  exposes no targeting surface on its `TestCase`.
- `test_features.py` — tests Python-specific module-system shims
  (`sys.modules`, dunder access) with no Rust counterpart.
- `test_exercise_shrink_paths.py` — depends on `test_pbtsmith.py` (see
  below) and on `hypothesis.internal.conjecture` (`ConjectureData`) to
  bootstrap shrink-pass inputs. Both are Python-only integrations with no
  Rust counterpart.
- `test_findability_comparison.py` — runs the test programs under
  `hypothesis.internal.conjecture.engine.ConjectureRunner` as the oracle
  to compare against pbtkit's findability. Hypothesis's engine is a Python
  library dependency with no Rust counterpart.
- `test_hypothesis.py` — drives pbtkit via the public `tc.weighted(p)` and
  `tc.target(score)` methods, which hegel-rust deliberately doesn't expose
  on `TestCase` (no public weighted-boolean or targeting API). The
  `tc.choice(n)` / `tc.mark_status(...)` calls do have hegel-rust
  counterparts, but the test's method-dispatch loop can't be expressed
  without the missing two.
- `test_pbtsmith.py` — generates random Python programs via pbtkit's code
  generator and `exec()`s them; this is a Python-syntax/runtime integration
  with no hegel-rust counterpart.
- `test_shrink_comparison.py` — uses `hypothesis.internal.conjecture`
  (`ConjectureRunner`, `ConjectureData`, `calc_label_from_name`,
  `IntervalSet`) to run Hypothesis as an oracle against pbtkit's shrinker.
  Hypothesis's engine is a Python library dependency with no Rust
  counterpart.

Individually-skipped tests (rest of the file is ported):

- `test_text.py::test_string_sort_key_type_mismatch` — exercises Python's
  dynamically-typed `sort_key(non-string)`; Rust's `sort_key(&str)` signature
  makes the "non-string argument" case unrepresentable at compile time.
- `test_generators.py::test_cannot_witness_nothing` — uses `gs.nothing()`;
  hegel-rust has no empty-generator public API.
- `test_generators.py::test_target_and_reduce` — uses `tc.target(score)`;
  no targeting API in hegel-rust (already covered by the whole-file skip
  of `test_targeting.py`).
- `test_generators.py::test_impossible_weighted`,
  `test_generators.py::test_guaranteed_weighted` — both use pbtkit's
  public `tc.weighted(p)` method; hegel-rust deliberately exposes no
  weighted-boolean API on `TestCase` (public-API incompatibility).
- `test_generators.py::test_many_reject`,
  `test_generators.py::test_many_reject_unsatisfiable` — exercise
  pbtkit's free-function `many()` helper and its Unsatisfiable-on-reject
  semantics; hegel-rust's `Collection` is re-exported but the
  force-stop/Unsatisfiable surface isn't shaped the same way.
- `test_generators.py::test_unique_by` — uses `unique_by=key_fn`;
  hegel-rust's `VecGenerator` only exposes `.unique(bool)`, no
  `.unique_by(key_fn)` setter.
- `test_generators.py::test_generator_repr` — tests Python `repr()`
  output; no analog in hegel-rust.

## hypothesis (`/tmp/hypothesis/hypothesis-python/tests/cover/`)

- `test_constants_ast.py` — tests Hypothesis's Python-AST constant
  extractor (`ConstantVisitor`, `constants_from_module`); parses Python
  source code, no Rust counterpart.
- `test_caching.py` — tests Python object identity (`st.text() is
  st.text()`) of Hypothesis's strategy cache; Rust generators are
  builder structs with no `is`-style identity equivalent.
- `test_posonly_args_py38.py` — tests Python 3.8 positional-only arg
  syntax (`/`) on `@st.composite` and `st.builds()`; both are
  Python-syntax / Python-API specific with no Rust counterpart.
- `test_lookup.py` — tests `from_type()` and `st.register_type_strategy()`
  resolution of Python typing constructs (`typing.TypeVar`,
  `typing.ForwardRef`, `typing.Protocol`, `typing.NamedTuple`,
  `typing.Generic`, `typing.NewType`, `enum.Enum`, `typing.Callable`,
  `abc.ABC`, `typing.TypedDict`) via runtime type introspection; neither
  `from_type` nor `register_type_strategy` exists in hegel-rust and the
  derive-macro analog (`#[derive(Generate)]`) is compile-time only.
- `test_lookup_py38.py` — tests `from_type()` resolution of Python typing
  constructs (`typing.Final`, `typing.Literal`, `typing.TypedDict`,
  `typing.Protocol`), Python positional-only/keyword-only arg syntax,
  and Python reflection helpers (`convert_positional_arguments`,
  `get_pretty_function_description`); all Python-API specific with no
  Rust counterpart.
- `test_lookup_py314.py` — tests `from_type()` resolution of Python 3.14's
  parameterized `memoryview[T]` and `collections.abc.Buffer` via the
  Python buffer protocol (`__buffer__` dunder, `memoryview`, `bytearray`);
  `from_type` doesn't exist in hegel-rust and the buffer protocol has no
  Rust counterpart.
- `test_example.py` — tests the fluent `.via("...")` and `.xfail(...)`
  methods chained onto `@example(...)`; hegel-rust's
  `#[hegel::explicit_test_case]` has no equivalent of either.
- `test_map.py` — all three tests rely on Python-specific facilities:
  `test_can_assume_in_map` and `test_assume_in_just_raises_immediately`
  call Hypothesis's standalone thread-local `assume()` inside `.map()`
  closures, but in hegel-rust `assume` is a method on `TestCase` (there
  is no standalone `hegel::assume()` and `ASSUME_FAIL_STRING` is
  `pub(crate)`), so `.map` closures — which receive only the value —
  cannot raise an assumption failure. `test_identity_map_is_noop` uses
  the internal `unwrap_strategies` API and Python `is` object identity
  to check that `s.map(identity) is s`, with no Rust counterpart.
- `test_replay_logic.py::test_does_not_shrink_on_replay_with_multiple_bugs`
  — depends on `report_multiple_bugs=True` (no equivalent setting in
  hegel-rust) and the reported failure arriving as a Python
  `ExceptionGroup`; hegel-rust always surfaces a single panic per run.
- `test_compat.py` — tests `hypothesis.internal.compat`, a Python-language
  compatibility layer: `floor`/`ceil` on Python numeric types,
  `get_type_hints` over `inspect.Signature`/`ForwardRef`/`typing.Union`,
  `dataclass_asdict` over `@dataclass`/`namedtuple`/`defaultdict`,
  `add_note` on frozen-dataclass exceptions, and `extract_bits`. All
  Python-specific with no Rust counterpart.
- `test_random_module.py` — tests Hypothesis's integration with Python's
  global `random` module: `st.random_module()` (seeds Python's global PRNG),
  `register_random()` (registers external `random.Random` instances with
  `entropy.RANDOMS_TO_MANAGE`), `deterministic_PRNG()` (context manager for
  Python random determinism), and the `ReferenceError`/`HypothesisWarning`
  raised when a `Random` instance is passed without a surviving referrer.
  Rust has no global singleton PRNG, no `register_random` analog, and no
  equivalent GC-based weak-reference semantics; hegel-rust's `gs::randoms()`
  is a shrinkable RNG value, a different concept.
- `test_slices.py` — tests `st.slices(size)`, which generates Python
  `slice` objects (built-in type with `.start`/`.stop`/`.step` attributes
  and a `.indices(size)` resolver used with Python's indexing protocol).
  Rust has no `slice`-object type and hegel-rust has no `gs::slices()`
  generator; the tests rely on Python indexing semantics
  (`range(size)[x.start]`, `x.indices(size)`) throughout.
- `test_explicit_examples.py` — all tests rely on Python-specific facilities:
  Python decorator stacking (`@example`/`@given` ordering and `@pytest.mark.parametrize`),
  `unittest.TestCase` integration, Python error APIs (`InvalidArgument`,
  `HypothesisWarning`, `DeadlineExceeded`), Python output-capture helpers
  (`capture_out`, `reporting`, `assert_falsifying_output`), Python's
  `ExceptionGroup`, dunder attributes (`__notes__`, `hypothesis_explicit_examples`),
  and Hypothesis settings absent from hegel-rust (`Phase.explicit`,
  `report_multiple_bugs`, `deadline`). The core explicit-test-case behaviour
  already has thorough coverage in `tests/test_explicit_test_case.rs`.
- `test_database_backend.py` — this file mixes portable public-API tests
  (multi-value `save`/`fetch`/`delete`/`move` semantics, listener API,
  wrappers) with Python-specific ones. Only the latter are skipped here;
  the portable portions have a dedicated port TODO (`TODO.yaml`) tracking
  a line-by-line port to `tests/hypothesis/database_backend.rs`. The
  Python-specific sub-bullets kept here are:
    - `GitHubArtifactDatabase` (tests `test_ga_*`, `TestGADReads`,
      `test_gadb_coverage`) is Python-only infrastructure (urllib,
      zipfile, GitHub Actions artifact endpoints) with no Rust
      counterpart — a permanent skip.
    - `choices_to_bytes`/`choices_from_bytes` with
      `_pack_uleb128`/`_unpack_uleb128` and `_metakeys_name` test the
      bytes of Hypothesis's wire format (ULEB128 packing, metakey name
      conventions). The native engine deliberately uses a different
      serialization layout (`serialize_choices`), so these specific byte
      patterns don't exist in hegel-rust — a public-API design
      difference, not an engine-internal gap.
    - `test_default_database_is_in_memory`,
      `test_default_on_disk_database_is_dir`, and
      `test_database_directory_inaccessible` test Hypothesis's
      `ExampleDatabase()` zero-arg factory and `_db_for_path` path
      resolution. Hegel-rust exposes no equivalent factory — databases
      are constructed directly from a path — so these tests target
      a public-API surface that doesn't exist here.
