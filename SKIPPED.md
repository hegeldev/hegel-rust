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

- `test_core.py::test_reuses_results_from_the_database` — asserts
  `len(tmpdir.listdir()) == 1` on pbtkit's `DirectoryDB`
  single-file-per-key layout and an exact `count == prev + 2`
  replay+verify invariant. hegel-rust's `NativeDatabase` uses a
  nested `key/value` hash-directory layout (so the root-`listdir()`
  assertion doesn't translate) and the replay-loop call-count shape
  isn't guaranteed to match pbtkit's literally.
- `test_core.py::test_database_round_trip_with_booleans` — uses
  `tc.weighted(p)`, no hegel-rust counterpart (same public-API
  incompatibility as the other `weighted` skips).
- `test_core.py::test_malformed_database_entry`,
  `test_core.py::test_empty_database_entry`,
  `test_core.py::test_truncated_database_entry` — exercise pbtkit's
  `DirectoryDB` on-disk byte-level serialization format (tag bytes,
  length headers); hegel-rust's `NativeDatabase` uses a different
  serialization layout (`serialize_choices` in
  `src/native/database.rs`), so the exact byte patterns have no
  analog.
- `test_core.py::test_error_on_unbounded_test_function` —
  monkeypatches `pbtkit.core.BUFFER_SIZE` at runtime; hegel-rust's
  `BUFFER_SIZE` is a native-only `const` with no runtime-patch surface.
- `test_core.py::test_function_cache` — uses pbtkit's
  `CachedTestFunction([values])` / `.lookup([values])` shape;
  hegel-rust's `CachedTestFunction` takes a `NativeTestCase` and
  exposes only `run` / `run_shrink` / `run_final`.
- `test_core.py::test_prints_a_top_level_weighted` — uses
  `tc.weighted(p)`, no hegel-rust counterpart (same reason as the
  other `weighted` skips).
- `test_core.py::test_errors_when_using_frozen` — pbtkit's public
  `Frozen` exception; hegel-rust has no equivalent error type.
- `test_core.py::test_forced_choice_bounds` — uses
  `tc.forced_choice(n)`, no public API in hegel-rust.
- `test_core.py::test_errors_on_too_large_choice` — uses
  `tc.choice(2**64)` with a runtime-typed Python int; hegel-rust's
  typed integer generators cap bounds via `T` at compile time, so
  this failure mode is unrepresentable.
- `test_core.py::test_value_punning_on_type_change`,
  `test_core.py::test_bind_deletion_valid_but_not_shorter`,
  `test_core.py::test_delete_chunks_stale_index`,
  `test_core.py::test_shrink_duplicates_with_stale_indices` — drive
  pbtkit's `PbtkitState(random, tf, max_examples).run()` loop and
  depend on the result-truncation-on-accept behaviour of pbtkit's
  shrinker. hegel-rust's shrinker preserves the full candidate sequence
  in `current_nodes` (never shortens it on `consider`), so the specific
  "length shrinks past i" regressions these guard against don't occur
  in hegel-rust's implementation.
- `test_core.py::test_shrink_duplicates_valid_drops_below_two` — relies
  on pbtkit's shrinker truncating `current_nodes` on accept; hegel-rust's
  `consider()` never shortens, so the outer `valid.len() < 2` branch
  these exercise isn't reachable. The inner `current_valid.len() < 2`
  path is covered by the embedded test
  `shrink_duplicates_positive_bin_search_makes_partial_progress`.
- `test_core.py::test_redistribute_binary_search` — calls pbtkit's
  `redistribute_sequence_pair` helper directly with a Python callback;
  no equivalent public function surface in hegel-rust.
- `test_core.py::test_run_test_with_preseeded_result` — uses
  `unittest.mock.patch.object(State, "__init__", ...)` to preseed
  `state.result`; Python-only monkey-patching facility.
- `test_core.py::test_sort_key_type_mismatch` — Python dynamic-typing
  `sort_key(wrong_type)` (same pattern as the already-skipped
  `test_string_sort_key_type_mismatch` /
  `test_bytes_sort_key_type_mismatch`).
- `test_core.py::test_targeting_skips_non_integer` — uses
  `tc.target(score)`, no analog (whole-file skip of
  `test_targeting.py`).
- `test_core.py::test_note_prints_on_failing_example`,
  `test_core.py::test_draw_silent_does_not_print` — use pbtkit's
  `capsys` pytest fixture to inspect the final-replay stdout formatter
  byte-for-byte; hegel-rust's failing-replay output goes to stderr in a
  different shape (`let draw_1 = ...;`), so a byte-level comparison
  with pbtkit's format is unportable. The stderr shape is pinned down
  by the `TempRustProject`-based tests in `tests/test_output.rs`.
- `test_core.py::test_nothing_core` — uses `gs.nothing()`; hegel-rust
  has no empty-generator public API (same reason as the existing
  `test_generators.py::test_cannot_witness_nothing` skip).
- `test_core.py::test_generator_repr` — Python `repr()` output; no
  analog in hegel-rust (same reason as the `test_generators.py`
  equivalent above).

- `test_floats.py::test_floats_database_round_trip` — asserts pbtkit's
  `count == prev + 2` replay invariant on `DirectoryDB`; hegel-rust's
  replay-loop call-count shape isn't guaranteed to match (same reason
  as `test_core.py::test_reuses_results_from_the_database`).
- `test_floats.py::test_floats_deserialize_truncated` — feeds pbtkit's
  `SerializationTag.FLOAT` byte layout directly to its `DirectoryDB`;
  hegel-rust's `NativeDatabase` uses `serialize_choices` with a
  different on-disk layout (same reason as the `test_core.py`
  byte-format-specific skips).
- `test_floats.py::test_float_sort_key_type_mismatch` — Python
  dynamic-typing `sort_key("hello")`; Rust's `sort_key(f64)` signature
  makes the non-float case unrepresentable (same pattern as the
  already-skipped `sort_key_type_mismatch` entries).

- `test_draw_names.py::test_draw_counter_resets_per_test_case`,
  `test_draw_names.py::test_draw_counter_only_fires_when_print_results` —
  access `tc._draw_counter` on pbtkit's `TestCase`, a Python-internal
  attribute with no hegel-rust counterpart.
- `test_draw_names.py::test_choice_output_unchanged` — tests the
  `choice(5): …` output prefix from pbtkit's `tc.choice(n)`; in
  hegel-rust the equivalent is `tc.draw(gs::integers()...)` whose
  output is the generic `let draw_N = …;` format, so the
  pbtkit-specific prefix is unrepresentable.
- `test_draw_names.py::test_weighted_output_unchanged` — uses
  `tc.weighted(p)`; no hegel-rust counterpart (same public-API
  incompatibility as the other `weighted` skips above).
- `test_draw_names.py::test_draw_uses_repr_format` — asserts Python
  `repr()` quoting (`'hello'`); Rust's `Debug` quotes with `"hello"`,
  a format mismatch with no one-to-one mapping.
- `test_draw_names.py::test_draw_named_repeatable_skips_taken_suffixes`
  — mutates `tc._named_draw_used` directly (Python-internal
  attribute).
- `test_draw_names.py::test_draw_named_no_print_when_print_results_false`
  — pbtkit's per-`TestCase` `print_results=False` flag has no
  hegel-rust counterpart (replay-output gating is run-level, keyed
  off the last-run flag, not per-testcase).
- `test_draw_names.py::test_rewriter_try_block_is_repeatable` — Python
  `try`/`except` has no stable Rust syntactic analog (no `try` blocks,
  no bare-block `except`); the "draw inside a try block is repeatable"
  assertion has no direct Rust equivalent.
- `test_draw_names.py::test_rewriter_nested_function_is_repeatable` —
  the upstream comment notes the inner `tc.draw(...)` is a `return`
  expression not an assignment, so the test drains output but asserts
  nothing — no observable behaviour to pin.
- `test_draw_names.py::test_auto_rewriting_without_decorator`,
  `test_draw_names.py::test_importing_draw_names_enables_auto_rewriting`
  — pbtkit's import-time `TestCase` monkey-patching is replaced in
  hegel-rust by the always-on `#[hegel::test]` macro; no "importing
  a module flips a switch" surface to assert on.
- `test_draw_names.py::test_rewrite_draws_with_closure` — tests that
  pbtkit's libcst rewriter preserves Python `__closure__` cell
  references. Rust's proc-macro rewrite operates on tokens, so
  closure-variable preservation is not a meaningful rewriter concern.
- `test_draw_names.py::test_draw_named_stub_raises_before_import` —
  asserts `NotImplementedError` from pbtkit's pre-import stub of
  `draw_named`. Hegel-rust has no such stub; `__draw_named` is
  always available on `TestCase`.
- `test_draw_names.py::test_collector_trystar_marks_repeatable`,
  `test_collector_classdef_marks_repeatable`,
  `test_collector_chained_assignment_skipped` — direct uses of
  `cst.parse_module(...)` + `_DrawNameCollector`: external Python
  library (libcst) integration with no Rust surface.
- `test_draw_names.py::test_rewriter_multiple_targets_in_same_fn` —
  exercises Python chained assignment (`a = b = tc.draw(...)`), a
  Python-syntax construct that doesn't exist in Rust.
- `test_draw_names.py::test_rewriter_tuple_target_when_regular_draw_present`,
  `test_rewriter_nested_funcdef_line_268` — pbtkit libcst line-coverage
  tests for the `_DrawNameCollector` visitor; both behavioural cases
  (tuple target alongside a regular draw; nested `fn` inside a test
  body) are covered by the Section C tuple-target and
  expression-context ports.
- `test_draw_names.py::test_rewriter_kwdefaults_preserved` — asserts
  `rewritten.__kwdefaults__ == {...}`; Python-specific
  keyword-only-default machinery.
- `test_draw_names.py::test_rewriter_draw_with_no_args` — pbtkit's
  `tc.draw()` takes no argument; hegel-rust's `tc.draw(g)` requires a
  generator, so the zero-arg case is unrepresentable in the Rust
  type system.
- `test_draw_names.py::test_rewrite_fallback_on_bad_source` — tests
  pbtkit's `inspect.getsource` fallback (runtime Python source
  reflection); the proc macro has no equivalent failure mode.
- `test_draw_names.py::test_hook_noop_when_original_test_is_none` —
  exercises pbtkit's internal `_draw_names_hook` against a
  `PbtkitState` with `_original_test is None`; an internal hook with
  no Rust counterpart.

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
- `test_typealias_py312.py` — tests `from_type()` resolution of PEP 695
  `type` alias syntax (`type MyInt = int`, parameterized
  `type A[T] = list[T]`, mutually-recursive aliases),
  `register_type_strategy` overrides on aliases, and the internal
  `evaluate_type_alias_type` helper. Rust `type X = Y;` aliases are
  compile-time only with no runtime alias-object surface, and hegel-rust
  has no `from_type` / `register_type_strategy` analog (same family as
  the `test_lookup*.py` skips above).
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

- `test_functions.py` — every test exercises `st.functions(like=..., returns=..., pure=...)`,
  a Hypothesis public-API strategy that generates Python callable mocks. The tests
  depend on Python-specific facilities throughout: generating callables with matching
  `__name__`, `inspect.signature` parameters, `*args`/`**kwargs`, keyword-only arguments,
  lambdas, `TypeError` on arity mismatch, `InvalidState` when calling outside `@given`,
  `hypothesis.reporting.with_reporter` integration, and `hypothesis.find()`. Rust's type
  system forbids runtime-synthesised callables with arbitrary signatures, and hegel-rust
  has no `gs::functions()` generator, no `InvalidState` analog, no reporter-context
  surface, and no standalone `find()` function.

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

- `test_monitoring.py` — the single test exercises Python 3.12+'s
  `sys.monitoring` VM introspection API (PEP 669) via `use_tool_id`/
  `free_tool_id` and `hypothesis.internal.scrutineer.MONITORING_TOOL_ID`
  to verify a `HypothesisWarning` when another tool has already claimed the
  monitoring tool ID. Rust has no `sys.monitoring` counterpart and
  hegel-rust has no scrutineer / branch-coverage infrastructure or
  warning surface.

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

- `test_deadline.py` — every test exercises Hypothesis's public `deadline`
  setting (`@settings(deadline=500)`, `@settings(deadline=None)`,
  `settings(deadline="3 seconds")` raising `InvalidArgument`) and/or the
  `DeadlineExceeded` / `FlakyFailure` error types raised when a test
  exceeds its deadline (including flaky-on-rerun, shrinking-participation,
  "well above the deadline" margin, GC-pause exclusion, and the
  deadline-specific flaky error message). hegel-rust's `Settings` builder
  exposes no `deadline` method (already noted by the `test_health_checks.py`
  `deadline=None` skip entries), there is no `DeadlineExceeded` or
  `FlakyFailure` error type, and `.map()` closures cannot `time.sleep`
  their way into deadline territory via a generator transform.

- `test_statistical_events.py` — every test relies on `hypothesis.statistics.collector`
  / `describe_statistics` (programmatic test-run statistics collection) and/or
  `event()` / `target()` (Hypothesis public APIs for recording custom events and
  targeted PBT scores). hegel-rust exposes none of these: no `event()`, no `target()`,
  no statistics collection or formatting infrastructure.

- `test_targeting.py` — every test calls Hypothesis's public `target(observation, label=...)`
  function and/or stresses its internal `TargetSelector` pool-size logic. hegel-rust
  exposes no `target()` function and no targeted-PBT surface at all (same gap as
  `test_statistical_events.py`), so none of the nine tests are portable.

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

- `test_filtered_strategy.py::test_filtered_branches_are_all_filtered`,
  `test_filtered_strategy.py::test_filter_conditions_may_be_empty`,
  `test_filtered_strategy.py::test_nested_filteredstrategy_flattens_conditions` —
  all three construct Hypothesis's internal `FilteredStrategy` class directly
  and inspect `.branches`, `.flat_conditions`, and `.filtered_strategy`.
  hegel-rust's `Filtered<T, F, G>` is a wrapper generator holding a single
  predicate: chained `.filter()` calls compose as nested wrappers without
  flattening, generators expose no `branches`, and a predicate-less `Filtered`
  is not expressible through the public API.

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

- `test_unicode_identifiers.py` — every test exercises Python-specific
  facilities with no Rust counterpart: `test_can_copy_signature_of_unicode_args`
  and `test_can_copy_signature_of_unicode_name` use
  `hypothesis.internal.reflection.proxies` (a Python decorator that copies
  one function's signature onto another — no Rust equivalent, same gap as
  the whole-file `test_reflection.py` skip);
  `test_can_handle_unicode_identifier_in_same_line_as_lambda_def` uses
  `get_pretty_function_description` to pretty-print a Python lambda's
  source (same Python-reflection gap); `test_regression_issue_1700`
  guards against a Python AST / decorator parsing regression for unicode
  identifiers inside `@given(...)` — a parse-time concern that cannot
  manifest in Rust, where unicode identifiers are handled by the
  compiler before any hegel code runs.

- `test_internal_helpers.py` — the file's single test
  (`test_is_negative_gives_good_type_error`) calls
  `hypothesis.internal.floats.is_negative("foo")` to verify a Python
  `TypeError` whose message contains `repr(x)`. Rust's type system
  prevents passing a non-float to a function that expects one at compile
  time, so the "wrong-type argument produces a good runtime error" case
  is unrepresentable (same pattern as the already-skipped
  `sort_key_type_mismatch` entries).

- `test_arbitrary_data.py::test_errors_when_normal_strategy_functions_are_used`
  — asserts `st.data().filter(...)` / `.map(...)` / `.flatmap(...)` raise
  `InvalidArgument`; there is no `st.data()` strategy object in
  hegel-rust to apply those transforms to (the "data" surface is the `tc`
  argument, not a strategy).
- `test_arbitrary_data.py::test_nice_repr` — tests `repr(st.data()) ==
  "data()"`; Python `repr()` output has no Rust counterpart.

- `test_simple_collections.py::test_find_empty_collection_gives_empty` —
  partial port. The `tuples()`, `lists(none(), max_size=0)`,
  `sets(none(), max_size=0)`, and `fixed_dictionaries({})` rows are
  ported; the remaining rows rely on public-API features with no
  hegel-rust counterpart: `st.nothing()`, `st.frozensets()`,
  `fixed_dictionaries(..., optional=...)`, and non-string
  `fixed_dictionaries` keys.
- `test_simple_collections.py::test_fixed_dictionaries_with_optional_and_empty_keys`
  — uses the `optional=` kwarg on `fixed_dictionaries` and `st.nothing()`,
  neither of which has a hegel-rust counterpart.
- `test_simple_collections.py::test_minimize_dicts_with_incompatible_keys`
  — mixes `int` and `str` keys in one dict; Rust's type system makes a
  heterogeneous-key dict unrepresentable.
- `test_simple_collections.py::test_lists_unique_by_tuple_funcs` — uses
  `unique_by=(key_fn_1, key_fn_2)`; `VecGenerator` exposes only
  `.unique(bool)`, no `.unique_by(key_fn)` setter.
- `test_simple_collections.py::test_can_find_unique_lists_of_non_set_order`
  — Python retries under `@flaky` because its predicate depends on
  process-randomised set iteration order. hegel-rust's engine classifies
  a non-deterministic predicate as a flaky-test bug and raises
  `Flaky test detected` inside the property run, so the test cannot be
  stabilised with an outer retry.
- `test_simple_collections.py::test_find_non_empty_collection_gives_single_zero[frozenset]`,
  `test_simple_collections.py::test_minimizes_to_empty[frozenset]` — only
  the `frozenset` parametrize rows are dropped; there is no
  `gs::frozensets()`. The `list` and `set` rows are ported.

- `test_settings.py` — every test sits on Hypothesis's Python-specific
  settings framework, none of which hegel-rust exposes. Profile
  machinery (`settings.register_profile`, `load_profile`, `get_profile`,
  `settings.default` singleton, `local_settings` context manager) is
  Hypothesis public API with no hegel-rust counterpart; `@settings`
  decorator semantics (stacking order, `@settings` on
  `RuleBasedStateMachine` / non-state-machine classes,
  `@settings()(1)` callable-check) are Python syntax; attribute-access
  patterns (`settings().kittens`, `x.max_examples = "foo"`,
  `settings.max_examples = 10`, `settings.default = ...`) are Python
  dunder access; settings attributes not exposed by hegel-rust's
  `Settings` builder (`deadline`, `phases`, `backend`, `print_blob`,
  `stateful_step_count`, `max_examples`) have no counterpart; runtime
  `InvalidArgument` error-raising on wrong-typed args and
  `note_deprecation` / `HypothesisDeprecationWarning` are
  unrepresentable (Rust's type system catches wrong types at compile
  time and hegel-rust has no deprecation-warning surface);
  `InMemoryExampleDatabase`, `set_hypothesis_home_dir`, and the CI-
  profile subprocess plumbing (`test_check_defaults_to_derandomize_*`,
  `test_will_automatically_pick_up_changes_to_ci_profile_in_ci`) target
  Hypothesis-specific on-disk / global-default behaviour; and
  string/integer → enum coercions (`verbosity="quiet"`, `Verbosity(0)`,
  `Phase(4)`, `HealthCheck(1)`) are Python's dynamic typing. The one
  candidate for a trivial port — `test_can_set_verbosity` — reduces in
  Rust to constructing four enum variants the compiler already
  enforces, adding no coverage. `test_verbosity_is_comparable` would
  require `Verbosity: Ord`, which hegel-rust deliberately does not
  derive.

- `test_traceback_elision.py` — exercises Python's traceback module
  (`traceback.extract_tb`, `e.__traceback__`) and counts frames to verify
  Hypothesis's internal-frame-trimming behaviour (gated on the
  `HYPOTHESIS_NO_TRACEBACK_TRIM` env var). Rust panics and backtraces have
  no equivalent frame-inspection or trim surface, and hegel-rust has no
  `HYPOTHESIS_NO_TRACEBACK_TRIM` analog — all Python-specific.

- `test_asyncio.py` — every test drives Python's `asyncio` library
  (`asyncio.new_event_loop`, `asyncio.run`, `asyncio.coroutine`,
  `asyncio.sleep`, `asyncio.wait_for`) through Hypothesis's
  `TestCase.execute_example` hook (already covered by the whole-file skip
  of `test_executors.py`), plus Python-only syntax (`async def`/`await`,
  `yield from`). Rust's async ecosystem (tokio/async-std) is unrelated to
  Python asyncio, hegel-rust has no `execute_example` class-method hook,
  and tests are closures passed to `Hegel::new(|tc| ...).run()` rather
  than methods on a `TestCase` subclass.

- `test_regressions.py` — a parallel-port attempt on branch
  `port/worker-0` was abandoned after its commits failed to cherry-pick
  cleanly (SKIPPED.md merge conflict); the branch is preserved for a
  later human to inspect. The upstream file is a grab-bag of
  Python-specific regressions with no Rust surface: `pickle.dumps`
  round-trip on Hypothesis error types (`NoSuchExample`,
  `DeadlineExceeded`, `RewindRecursive`, `UnsatisfiedAssumption`,
  `FlakyReplay`, `FlakyFailure`, `BackendCannotProceed`),
  `vars(errors)` module-dict reflection to enumerate custom-`__init__`
  exception classes, `unittest.mock.Mock` injection into
  `@given`-decorated functions, Python global `random` state
  preservation across `@given` runs (same gap as the whole-file skip
  of `test_random_module.py`), and `st.composite` / `st.builds(dict,
  ...)` / `st.fixed_dictionaries` strategies that synthesise
  heterogeneously-typed Python dicts which Rust's type system can't
  represent.

- `test_cache_implementation.py::test_cache_is_threadsafe_issue_2433_regression`
  — uses `st.builds(partial(str))`, a Python-reflection-based strategy
  (runtime `inspect.signature` introspection of the target callable) with
  no hegel-rust counterpart. The thread-safety property it guards is
  specific to Hypothesis's per-thread caching of strategy introspection.

- `test_exceptiongroup.py` — every test raises a Python PEP 654
  `ExceptionGroup` / `BaseExceptionGroup` (Python 3.11+ built-in) from a
  `@given`-decorated function to pin down how Hypothesis unwraps groups
  containing its own error types (`Frozen`, `StopTest`, `Flaky`,
  `FlakyFailure`, `FlakyBackendFailure`); two parametrized tests also
  exercise `ExceptionGroup.split` / `.derive`. Rust panics are singular
  (no grouping construct), `Result` is the idiomatic error channel, and
  hegel-rust has no `Frozen` / `StopTest` / `Flaky*` error types. The
  whole file sits on Python exception-group semantics with no Rust
  counterpart.

- `test_given_error_conditions.py::test_raises_unsatisfiable_if_passed_explicit_nothing`
  — uses `nothing()`, the empty-generator strategy; hegel-rust has no
  `gs::nothing()` public API (same gap as the `test_core.py::test_nothing_core`
  and `test_generators.py::test_cannot_witness_nothing` skips).
- `test_given_error_conditions.py::test_error_if_has_no_hints`,
  `test_given_error_conditions.py::test_error_if_infer_all_and_has_no_hints`,
  `test_given_error_conditions.py::test_error_if_infer_is_posarg`,
  `test_given_error_conditions.py::test_error_if_infer_is_posarg_mixed_with_kwarg`
  — exercise Python's `@given(a=...)` / `@given(...)` ellipsis syntax for
  type-hint-based strategy inference; `#[hegel::test]` takes generators
  directly, so this inference mechanism has no Rust counterpart.
- `test_given_error_conditions.py::test_given_twice_is_an_error` — stacks
  two `@given` decorators on one function; `#[hegel::test]` doesn't
  compose that way.
- `test_given_error_conditions.py::test_given_is_not_a_class_decorator`
  — applies `@given` to a Python class; Rust has no analogous
  class/macro composition.
- `test_given_error_conditions.py::test_specific_error_for_coroutine_functions`
  — asserts a specific error for Python `async def` tests; hegel-rust has
  no async-test dispatch.
- `test_given_error_conditions.py::test_suggests_at_settings_if_extra_kwarg_matches_setting_name`
  — inspects `@given` kwarg handling against Python setting names.
  hegel-rust uses `.settings(Settings::new()...)` rather than kwargs on
  the test macro.

- `test_stateful.py` — a parallel-port attempt on branch `port/worker-0`
  was abandoned after its commits failed to cherry-pick cleanly
  (SKIPPED.md merge conflict); the branch is preserved for a later
  human to inspect.
