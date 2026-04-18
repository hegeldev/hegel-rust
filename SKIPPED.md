# Skipped upstream test files

Upstream test files that have been deliberately *not* ported, with a one-line
rationale each. The Stop hook's unported-gate (`list-unported.py`) reads this
file and treats listed files as "done".

## pbtkit (`/tmp/pbtkit/tests/`)

- `test_targeting.py` — uses `tc.target(score)`; hegel-rust has no targeting
  API yet.
- `test_features.py` — tests Python-specific module-system shims
  (`sys.modules`, dunder access) with no Rust counterpart.
- `test_spans.py` — relies on pbtkit-internal span introspection
  (`tc.spans`, `tc.nodes`, `PbtkitState`, `_span_mutation_hook`) not
  exposed by hegel-rust.
- `test_exercise_shrink_paths.py` — exercises each `SHRINK_PASSES` pass via
  `PbtkitState`, pure pbtkit engine internals.
- `test_findability_comparison.py` — compares pbtkit vs Hypothesis by
  running both engines in the same process; neither oracle is available in
  hegel-rust.
- `test_hypothesis.py` — raw pbtkit API stress tests (`tc.weighted`,
  `tc.choice`, `tc.mark_status`) that don't map to the generator-based
  public API.
- `test_pbtsmith.py` — generates random Python programs via pbtkit's code
  generator and execs them; no `hegelsmith` equivalent yet.
- `test_shrink_comparison.py` — compares shrinker quality via
  `ConjectureRunner`/`ConjectureData`/`ChoiceNode`, pure engine internals.
- `test_choice_index.py` — tests pbtkit's `to_index`/`from_index` shortlex
  enumeration; hegel-rust doesn't implement an index-based shrink pass.

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
  `test_generators.py::test_guaranteed_weighted` — both use
  `tc.weighted(p)`; hegel-rust has no public weighted-boolean API.
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
- `test_database_backend.py` — the multi-value
  `save`/`fetch`/`delete`/`move` semantics that the bulk of this file
  exercises are covered in Rust by
  `tests/embedded/native/database_tests.rs` (via `NativeDatabase`,
  which now mirrors `DirectoryBasedExampleDatabase`). The remainder
  targets public-API surface that hegel-rust doesn't have and tracks
  as separate TODOs:
    - `GitHubArtifactDatabase` (tests `test_ga_*`, `TestGADReads`,
      `test_gadb_coverage`) is Python-only infrastructure (urllib,
      zipfile, GitHub Actions artifact endpoints) with no Rust
      counterpart — a permanent skip.
    - `InMemoryExampleDatabase`, `ReadOnlyDatabase`,
      `MultiplexedDatabase`, `BackgroundWriteDatabase`, and the
      `add_listener`/`remove_listener` change-listener API are
      engine-internal gaps; they live as focused TODOs in
      `TODO.yaml` and will be ported (or split into narrower skips)
      there, not here.
    - `choices_to_bytes`/`choices_from_bytes` with
      `_pack_uleb128`/`_unpack_uleb128` and `_metakeys_name` test
      internals of Hypothesis's wire format that aren't part of the
      native engine's serialization format (`serialize_choices` uses
      a distinct layout; see `tests/embedded/native/database_tests.rs`).
    - `test_default_database_is_in_memory`,
      `test_default_on_disk_database_is_dir`, and
      `test_database_directory_inaccessible` test Hypothesis's
      `ExampleDatabase()` factory / `_db_for_path`, which has no
      hegel-rust counterpart (hegel databases are constructed
      directly from a path).
