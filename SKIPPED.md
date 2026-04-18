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
- `test_database_backend.py` — tests Hypothesis's full
  `ExampleDatabase` public-API surface: `InMemoryExampleDatabase`,
  `MultiplexedDatabase`, `ReadOnlyDatabase`, `BackgroundWriteDatabase`,
  `GitHubArtifactDatabase`, the `add_listener`/`remove_listener`
  listener API, `choices_to_bytes`/`choices_from_bytes` with
  `_pack_uleb128`/`_unpack_uleb128`, `_metakeys_name`, and the
  multi-value `save`/`fetch`/`delete`/`move` semantics. hegel-rust's
  `NativeDatabase` is a fundamentally different single-value-per-key
  replay store with only `load`/`save`; none of these wrappers,
  variants, or APIs exist. The replay round-trip is covered by
  `tests/test_database_key.rs` and the serialize/load/save round-trips
  by `tests/embedded/native/database_tests.rs`.
