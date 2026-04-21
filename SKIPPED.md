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
- `test_bytes.py::test_bytes_sort_key_type_mismatch` — same pattern as the
  string equivalent: Rust's `sort_key(&[u8])` signature makes the
  "non-bytes argument" case unrepresentable at compile time.
- `test_bytes.py::test_targeting_with_bytes` — uses `tc.target(score)`;
  no targeting API in hegel-rust (already covered by the whole-file skip
  of `test_targeting.py`).
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

- `test_recursive.py` — all tests exercise `st.recursive(base, extend, max_leaves=N)`, a
  public API that generates dynamically-typed recursive data structures (e.g.
  `bool | list[bool | list[...]]`). The return type varies at runtime based on the
  `extend` function, which is natural in Python's dynamic type system but has no clean
  Rust analog: a generic `gs::recursive()` combinator would require type erasure
  (`Box<dyn Any>`) or a concrete per-use-case recursive enum, neither of which
  matches this API surface. Hegel-rust has no `gs::recursive()` equivalent.

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
- `test_lookup_py310.py` — tests `from_type()` resolution of Python 3.10's
  native union syntax (`int | list[str]`); `from_type` doesn't exist in
  hegel-rust and Python union-type introspection has no Rust counterpart.
- `test_lookup_py37.py` — tests `from_type()` resolution of PEP 585 generic
  types (`tuple[Elem]`, `list[Elem]`, `dict[Elem, Value]`,
  `collections.deque[Elem]`, `collections.abc.Iterable[Elem]`,
  `re.Match[str]`, etc.) via `@given(...)` with type annotations; neither
  `from_type` nor runtime type-annotation resolution exists in hegel-rust.
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
- `test_verbosity.py::test_prints_initial_attempts_on_find` — uses `hypothesis.find()`,
  a public API with no hegel-rust counterpart (hegel-rust exposes no standalone `find()`
  function; the equivalent is `Hegel::new(...).run()`).
- `test_feature_flags.py::test_eval_featureflags_repr`,
  `test_feature_flags.py::test_repr_can_be_evalled` — both rely on Python's
  `eval(repr(flags))` round-trip; Rust has no equivalent of `eval`, and
  `FeatureFlags`'s Debug output is not round-trippable by design.
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
- `test_falsifying_example_output.py` — both tests rely on Python-specific facilities:
  `test_inserts_line_breaks_only_at_appropriate_lengths` uses the `@example` decorator,
  `Phase.explicit`, and `__notes__` (PEP 678 exception annotation) to inspect Hypothesis's
  "Falsifying explicit example: test(x=..., y=...)" output format; `test_vararg_output`
  uses Python `*args` function signatures and likewise inspects `__notes__`. Neither the
  `@example` API, `Phase.explicit`, nor `__notes__` have hegel-rust counterparts, and
  hegel-rust's failure output format (`let draw_1 = ...; panicked at...`) is
  completely different from Hypothesis's.
- `test_reflection.py` — all tests exercise Python-specific reflection utilities:
  `convert_keyword_arguments`/`convert_positional_arguments`/`define_function_signature`
  (Python `inspect.Signature`/`Parameter` manipulation), `function_digest`/`repr_call`/
  `get_pretty_function_description`/`source_exec_as_module`/`proxies`/`is_mock`/
  `is_first_param_referenced_in_function`/`is_identity_function`/`required_args`
  (Hypothesis internal Python reflection helpers), `lambda_sources._function_key`/
  `_normalize_code`/`_clean_source` (Python bytecode and source-code manipulation),
  `LazyStrategy.__repr__` warnings, `unittest.mock` objects, `sys.path`, and
  `functools.partial/wraps`. None of these have Rust counterparts.
- `test_fuzz_one_input.py` — all tests exercise `test.hypothesis.fuzz_one_input(buffer)`,
  a Python-specific public API that lets `@given`-decorated tests serve as AFL/libFuzzer
  corpus targets (feeding raw bytes as test input). Hegel-rust has no `fuzz_one_input`
  equivalent and no analogous attribute-on-decorated-function surface. One test also
  accesses `test.hypothesis._given_kwargs` (Python dunder-adjacent attribute). Neither
  the fuzzer-integration API nor the attribute-access pattern have Rust counterparts.

- `test_pretty.py` — tests `hypothesis.vendor.pretty`, a vendored IPython
  pretty-printer that operates entirely on Python object protocols
  (`__repr__`, `_repr_pretty_` dunder dispatch, `id()`-based cycle
  detection) and Python-specific types (`dict`, `set`, `frozenset`,
  `Counter`, `OrderedDict`, `defaultdict`, `deque`, `@dataclass`,
  `Enum`/`Flag`, `functools.partial`, `re.compile`, `struct`,
  metaclasses, `super()`). Hegel-rust has no pretty-printer module and
  no equivalent dunder-dispatch surface — all Python-specific.

- `test_lazy_import.py` — the single test checks that Hypothesis does not import
  Python test runners (`pytest`, `nose`, `unittest2`) by running a Python subprocess
  and inspecting `sys.modules`. Both `sys.modules` and the subprocess-Python approach
  are Python-specific facilities with no Rust counterpart.

- `test_seed_printing.py` — all tests exercise Python/pytest-specific seed-reporting
  infrastructure: `monkeypatch.setattr(core, "running_under_pytest", ...)` and
  `monkeypatch.setattr(core, "global_force_seed", ...)` (patching Python module globals),
  `test._hypothesis_internal_use_generated_seed` (Python dunder-adjacent attribute),
  `@seed(N)` decorator syntax in output, `--hypothesis-seed=N` pytest CLI flag,
  `capture_out` (Python stdout capture), and `InMemoryExampleDatabase` health-check
  interaction. The seed-reporting UX is fundamentally Python/pytest-specific with no
  hegel-rust counterpart.

- `test_sideeffect_warnings.py` — all tests exercise Hypothesis's Python-specific
  import-time initialization infrastructure: `_hypothesis_globals.in_initialization`
  (a Python module attribute tracking import phase), `hypothesis.configuration`
  internals (`_first_postinit_what`, `notice_initialization_restarted`,
  `check_sideeffect_during_initialization`), `HypothesisSideeffectWarning` (a
  Python warning type), and `pytest.warns`/`monkeypatch` pytest fixtures. This
  tests Python module-loading side-effect detection during entrypoint loading,
  a concept with no Rust counterpart.

- `test_mock.py` — all tests exercise Python's `unittest.mock` integration
  (`mock.patch`, `mock.MagicMock`) and pytest fixtures (`pytestconfig`,
  `pytest.Config`) interacting with Hypothesis's `@given` decorator. Neither
  `unittest.mock` nor pytest fixtures have Rust counterparts.

- `test_filter_rewriting.py` — all tests exercise Hypothesis's filter rewriting
  optimization, which inspects Python predicates at runtime (lambda AST source
  parsing via `hypothesis.internal.reflection`, `functools.partial` attribute
  introspection, recognition of specific Python built-ins like `math.isfinite`,
  `str.isidentifier`, `re.compile().method`) and rewrites `.filter()` calls into
  tighter bounds on internal strategy types (`IntegersStrategy`, `FloatStrategy`,
  `TextStrategy`, `FilteredStrategy`). The tests verify the rewriting by checking
  `isinstance` on internal Python strategy classes and reading their `.start`,
  `.end`, `.min_value`, `.max_value`, `.min_size`, `.max_size` attributes via
  `unwrap_strategies`. Rust closures cannot be introspected at runtime, so filter
  rewriting is inherently Python-specific; hegel-rust's `.filter()` is pure
  rejection sampling with no predicate analysis.

- `test_database_backend.py` — this file mixes portable public-API tests
  (multi-value `save`/`fetch`/`delete`/`move` semantics, listener API,
  wrappers) with Python-specific ones. The portable portions are ported
  in `tests/hypothesis/database_backend.rs`. Only the Python-specific
  sub-bullets remain skipped:
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
    - `test_warns_when_listening_not_supported` exercises
      `HypothesisWarning`, a Python `warnings.warn` category emitted
      from `ExampleDatabase.add_listener` when the subclass doesn't
      override `_start_listening`. hegel-rust's default `add_listener`
      silently drops the listener (no warning surface) — a public-API
      design difference.

- `test_statistical_events.py` — every test relies on `hypothesis.statistics.collector`
  / `describe_statistics` (programmatic test-run statistics collection) and/or
  `event()` / `target()` (Hypothesis public APIs for recording custom events and
  targeted PBT scores). hegel-rust exposes none of these: no `event()`, no `target()`,
  no statistics collection or formatting infrastructure.

- `test_observability.py` — every test sits on Hypothesis's observability public
  API surface, none of which hegel-rust exposes:
  `capture_observations` / `TestCaseObservation` / `InfoObservation` /
  `add_observability_callback` / `remove_observability_callback` /
  `with_observability_callback` / `observability_enabled` / `TESTCASE_CALLBACKS`
  (the per-thread / all-threads observation callback registry that emits
  per-test-case JSON observations with `arguments`, `representation`, `timing`,
  `status_reason`, `metadata`, etc.); `event()` and `target()` (custom-event /
  targeted-PBT recording, same gap as `test_statistical_events.py`);
  `choices_to_json` / `nodes_to_json` (observability-only JSON serialization of
  `ChoiceNode` sequences); `to_jsonable` (Python-only observability serialization
  helper, same gap as the `test_searchstrategy.py::test_jsonable*` skips);
  `fuzz_one_input` (AFL/libFuzzer corpus integration, same gap as the whole-file
  skip of `test_fuzz_one_input.py`); and `@reproduce_failure` (encoded-failure
  replay decorator, same gap as the whole-file skip of `test_reproduce_failure.py`).

- `test_detection.py` — all tests use `is_hypothesis_test()`, a Python public API
  that checks whether a function was decorated with `@given` by inspecting a Python
  function attribute. Hegel-rust tests are closures passed to `Hegel::new(|tc| {...}).run()`,
  not decorated functions, so the concept of runtime test-detection has no Rust counterpart.
  The stateful test additionally uses `RuleBasedStateMachine.TestCase().runTest`, which is
  Python unittest metaclass machinery.

- `test_custom_reprs.py` — every test exercises Python's `__repr__` dunder on
  Hypothesis strategy objects (`repr(st.integers())`, `repr(st.sampled_from(...))`,
  `repr(st.builds(...))`, `repr(st.characters())`, etc.) and/or inspects
  `__notes__` (PEP 678 exception annotations) and `unwrap_strategies` to verify
  repr formatting in failure output. Rust generators have no equivalent repr
  surface — `Debug` output is structurally different and hegel-rust's failure
  output format (`let draw_1 = ...`) doesn't include strategy reprs.

- `test_complex_numbers.py` — all tests use `st.complex_numbers()`, a Hypothesis
  public-API strategy that generates Python `complex` values. Rust has no built-in
  complex number type and hegel-rust has no `gs::complex_numbers()` generator.

- `test_annotations.py` — all tests exercise Python reflection and annotation
  manipulation: `inspect.signature`/`inspect.Parameter` introspection,
  `define_function_signature` (rewrites Python function signatures),
  `get_pretty_function_description` (pretty-prints Python lambdas),
  `convert_positional_arguments` (Python argument conversion), and `@given`/
  `@st.composite` decorator annotation editing. None of these Python
  introspection APIs have Rust counterparts.

- `test_sampled_from.py::test_cannot_sample_sets` — Rust's type system prevents
  passing non-sequence types to `sampled_from`; the Python runtime type check has
  no Rust counterpart.
- `test_sampled_from.py::test_can_sample_enums` — Python `enum.Enum`/`enum.Flag`
  auto-iteration integration; `sampled_from(EnumClass)` iterates members natively
  in Python, no Rust equivalent.
- `test_sampled_from.py::test_efficient_lists_of_tuples_first_element_sampled_from`
  — uses `unique_by=fn`; `VecGenerator` only has `.unique(bool)`, no
  `.unique_by(key_fn)` setter.
- `test_sampled_from.py::test_unsatisfiable_explicit_filteredstrategy_sampled`,
  `test_sampled_from.py::test_unsatisfiable_explicit_filteredstrategy_just` —
  construct `FilteredStrategy` directly with Python `bool` as predicate
  (truthiness semantics); no Rust counterpart for either the internal class or
  the truthiness-as-filter pattern.
- `test_sampled_from.py::test_transformed_just_strategy` — uses
  `ConjectureData.for_choices`, `JustStrategy`, `do_draw`/`do_filtered_draw`/
  `filter_not_satisfied` (Hypothesis strategy-protocol internals with no
  hegel-rust counterpart at any level).
- `test_sampled_from.py::test_issue_2247_regression` — Python int/float equality
  (`0 == 0.0`) with dynamic typing; Rust's type system prevents mixed-type
  sequences.
- `test_sampled_from.py::test_mutability_1`,
  `test_sampled_from.py::test_mutability_2` — Python list mutability after
  strategy creation; Rust's ownership model makes this untestable.
- `test_sampled_from.py::test_suggests_elements_instead_of_annotations` — Python
  enum type-annotation vs values error message; no Rust counterpart.
- `test_sampled_from.py::TestErrorNoteBehavior3819` — Python `__notes__` (PEP 678
  exception annotations) and dynamic typing (strategies as `sampled_from`
  elements); no Rust counterpart.

- `test_reproduce_failure.py` — exercises Hypothesis's
  `encode_failure`/`decode_failure`/`@reproduce_failure` public API for
  serialising a failing choice sequence into a base64+zlib blob that a
  later `@given` run can replay. Hegel-rust has no counterpart: there is
  no `encode_failure`/`decode_failure` function, no `@reproduce_failure`
  decorator, and no `DidNotReproduce` error. The project also pulls in
  no base64 or zlib dependency. Every test in the file sits on top of
  that API surface, so nothing is portable today.

- `test_charmap.py` — tests Python-internal charmap infrastructure with no
  Rust counterpart. Most tests exercise `hypothesis.internal.charmap`
  plumbing (the `cm._charmap` module global, `cm.charmap_file()` on-disk
  cache, `cm.query(categories=...)` returning `IntervalSet`, the
  `CategoryName` `Literal` type) plus Python-only monkeypatching of
  `os.utime`, `os.path.exists`, `os.rename`, and `tempfile.mkstemp`.
  Hegel-rust's native charmap (`src/native/unicodedata.rs`) is a
  build-time run-length-encoded table with no file cache, no `query`
  entry point, and no `IntervalSet` return type. The four
  `IntervalSet.union` tests (`test_union_empty`,
  `test_union_handles_totally_overlapped_gap`,
  `test_union_handles_partially_overlapped_gap`,
  `test_successive_union`) target `hypothesis.internal.intervalsets.IntervalSet`,
  a standalone set-of-codepoint-ranges type; hegel-rust has no
  `IntervalSet` type and no interval-union operation on its alphabet
  representation (`StringAlphabet` in `src/native/schema/text.rs` stores
  `Vec<(u32, u32)>` but never merges two such lists), so these tests
  have no Rust target either.

- `test_simple_characters.py::test_include_exclude_with_multiple_chars_is_invalid`
  — Python passes a list of strings where each element must be a single
  character; Rust's `include_characters`/`exclude_characters` take `&str`, so
  the "one element is a multi-character string" failure mode is unrepresentable.
- `test_simple_characters.py::test_whitelisted_characters_alone` — asserts that
  `characters(include_characters=...)` with no other constraint raises. The
  hegel-rust client always emits `exclude_categories=["Cs"]` to keep strings
  surrogate-free, so "include alone" is unreachable through the Rust public API.

- `test_executors.py` — all tests exercise Hypothesis's `execute_example` protocol,
  a Python class-method hook that lets classes (e.g. `unittest.TestCase` subclasses)
  customize how `@given`-decorated method bodies are executed. Hegel-rust has no
  class-based test dispatch — tests are closures passed to `Hegel::new(|tc| {...}).run()`,
  so there is no `execute_example` surface or equivalent wrapping mechanism.

- `test_searchstrategy.py::test_or_errors_when_given_non_strategy` — Python `|`
  operator overloading on strategies; Rust has no operator-overloaded `|` for
  generators.
- `test_searchstrategy.py::test_just_strategy_uses_repr`,
  `test_searchstrategy.py::test_can_map_nameless`,
  `test_searchstrategy.py::test_can_flatmap_nameless` — Python `repr()` output
  and `functools.partial`; hegel-rust generators have no repr surface.
- `test_searchstrategy.py::test_flatmap_with_invalid_expand` — Python dynamic
  typing; Rust's `.flat_map()` requires its closure to return a generator at
  compile time, so the "returns a non-strategy" case is unrepresentable.
- `test_searchstrategy.py::test_use_of_global_random_is_deprecated_in_given`,
  `test_searchstrategy.py::test_use_of_global_random_is_deprecated_in_interactive_draws`
  — both tests wrap `random.choice` in a strategy to trigger Hypothesis's
  deprecation warning for using the Python global PRNG; Rust has no global
  singleton PRNG and hegel-rust has no deprecation-warning surface.
- `test_searchstrategy.py::test_jsonable`,
  `test_searchstrategy.py::test_jsonable_defaultdict`,
  `test_searchstrategy.py::test_jsonable_namedtuple`,
  `test_searchstrategy.py::test_jsonable_small_ints_are_ints`,
  `test_searchstrategy.py::test_jsonable_large_ints_are_floats`,
  `test_searchstrategy.py::test_jsonable_very_large_ints`,
  `test_searchstrategy.py::test_jsonable_override`,
  `test_searchstrategy.py::test_jsonable_to_json_nested`,
  `test_searchstrategy.py::test_to_jsonable_handles_reference_cycles` — all
  test `hypothesis.strategies._internal.utils.to_jsonable`, a Python-only
  observability serialization helper (symbolic realization, Python-specific
  containers like `defaultdict` / `namedtuple`, reference-cycle detection via
  `id()`, `@dataclass.to_json` protocol). hegel-rust has no observability /
  `to_jsonable` counterpart.
- `test_searchstrategy.py::test_deferred_strategy_draw` — `st.deferred()`
  (a lazy forward-reference strategy used for recursive definitions) has no
  hegel-rust analog; Rust's static type system doesn't support
  forward-referenced recursive strategies without explicit per-use-case
  enum scaffolding, and `gs::deferred()` doesn't exist.

- `test_interactive_example.py` — every test exercises `strategy.example()`, a
  Hypothesis public-API method that draws a single value from a strategy
  outside of any `@given` / `find` run. Hegel-rust generators expose no
  `.example()` equivalent: all generation happens inside
  `Hegel::new(|tc| tc.draw(&gen)).run()`, and there is no standalone
  "one value from a generator" surface. The remaining tests additionally
  depend on Python-specific facilities (`warnings.catch_warnings` +
  `NonInteractiveExampleWarning`, pytester, pexpect-driven REPL subprocess,
  `PYTEST_CURRENT_TEST` env-var plumbing) with no Rust counterpart.

- `test_health_checks.py::test_returning_non_none_is_forbidden`,
  `test_health_checks.py::test_stateful_returnvalue_healthcheck` — check
  Hypothesis's `return_value` health check on
  `@given`/`@rule`/`@initialize`/`@invariant`-decorated functions. Rust
  closures have declared return types already; the check is Python-specific
  and hegel-rust has no corresponding `HealthCheck` variant.
- `test_health_checks.py::test_the_slow_test_health_check_can_be_disabled`,
  `test_health_checks.py::test_the_slow_test_health_only_runs_if_health_checks_are_on`
  — use the `deadline=None` setting and `skipif_time_unpatched`, a
  pytest-specific time-freezing fixture. hegel-rust has no `deadline`
  setting on `Settings`.
- `test_health_checks.py::test_differing_executors_fails_health_check` —
  tests the `differing_executors` health check on `@given`-decorated
  instance methods called with different `self` receivers. hegel-rust
  tests are closures passed to `Hegel::new(...).run()` with no
  class/instance dispatch and no analogous health-check variant.
- `test_health_checks.py::test_it_is_an_error_to_suppress_non_iterables`,
  `test_health_checks.py::test_it_is_an_error_to_suppress_non_healthchecks`
  — Python dynamic typing: pass a non-iterable or non-`HealthCheck` to
  `suppress_health_check`. Rust's type system prevents these at compile
  time (`impl IntoIterator<Item = HealthCheck>`).
- `test_runner_strategy.py` — every test exercises `st.runner()`, a Hypothesis
  public-API strategy that returns the surrounding `unittest.TestCase` instance
  (or a supplied default outside a class). Hegel-rust has no class-based test
  dispatch — tests are closures passed to `Hegel::new(|tc| ...).run()` — so
  there is no `self` instance to return and no `gs::runner()` counterpart. The
  stateful case additionally relies on `RuleBasedStateMachine.TestCase`
  unittest metaclass machinery.

- `test_health_checks.py::test_nested_given_raises_healthcheck`,
  `test_health_checks.py::test_triply_nested_given_raises_healthcheck`,
  `test_health_checks.py::test_can_suppress_nested_given`,
  `test_health_checks.py::test_cant_suppress_nested_given_on_inner`,
  `test_health_checks.py::test_suppress_triply_nested_given` — all
  exercise `HealthCheck.nested_given`, which detects a `@given`-decorated
  function being called from inside another `@given` function. hegel-rust
  has no decorator-based test dispatch to nest and no `nested_given`
  variant on its `HealthCheck` enum.

- `test_error_in_draw.py` — every test inspects Python-specific
  error-annotation surfaces with no Rust counterpart:
  `test_error_is_in_finally` asserts the drawn-value list appears in
  PEP 678 `__notes__` after a `try/finally` re-raise (Rust has no
  `finally` and no `__notes__`); `test_warns_on_bool_strategy` uses
  `pytest.warns(HypothesisWarning)` triggered by `if st.booleans():`
  (Rust's type system makes "use a generator as a bool"
  unrepresentable, and there is no `HypothesisWarning` analog);
  `test_adds_note_showing_which_strategy` and
  `test_adds_note_showing_which_strategy_stateful` match
  `pytest.raises(...).match(f"while generating 'value' from {rep}")`
  against `__notes__` containing `st.from_type(X)`'s `__repr__`
  (hegel-rust has no `from_type`, no strategy `__repr__` surface, and
  its failure output is `let draw_1 = ...;` with no strategy repr or
  PEP 678 notes).
