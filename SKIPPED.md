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
- `findability/test_types.py` — port-loop workers produced commits that
  conflicted irreconcilably with the `src/native/` backend on integration
  (Cargo.toml, src/lib.rs, src/native/mod.rs, src/native/runner.rs,
  src/runner.rs, tests/test_native.rs). Abandoned pending human review of
  the workers' `port/worker-0` and `port/worker-1` branches.
- `test_flatmap.py` (in `shrink_quality/`) — port-loop worker produced
  commits that conflicted irreconcilably with the `src/native/` backend on
  integration (Cargo.toml, src/lib.rs, src/native/mod.rs, src/native/runner.rs,
  src/runner.rs, tests/test_native.rs). Abandoned pending human review of
  the worker's `port/worker-1` branch.
- `test_composite.py` — port-loop worker produced commits that conflicted
  irreconcilably with the `src/native/` backend on integration (Cargo.toml,
  src/lib.rs, src/native/mod.rs, src/native/runner.rs, src/runner.rs,
  tests/test_native.rs). Abandoned pending human review of the worker's
  `port/worker-0` branch.
- `test_mixed_types.py` (in `shrink_quality/`) — port-loop worker produced
  commits that conflicted irreconcilably with the `src/native/` backend on
  integration (Cargo.toml, src/lib.rs, src/native/mod.rs, src/native/runner.rs,
  src/runner.rs, tests/test_native.rs). Abandoned pending human review of
  the worker's `port/worker-0` branch.
- `test_floats.py` (in `shrink_quality/`) — port-loop worker produced
  commits that conflicted irreconcilably on integration (merge conflict in
  `.claude/skills/porting-tests/references/pbtkit-overview.md` against the
  pbtkit-only shrink-pass gating update on the supervisor branch).
  Abandoned pending human review of the worker's `port/worker-0` branch.

Individually-skipped tests (rest of the file is ported):

- `shrink_quality/test_composite.py::test_lower_and_bump_j_past_end_after_shortening`
  — invokes pbtkit's `lower_and_bump(shrinker)` shrink pass directly with
  a pre-seeded `TC.for_choices(...)` and `Shrinker(...)`; hegel-rust's
  shrinker exposes no public or `__native_test_internals` entry-point
  for a single shrink pass on a seeded test case.
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

- `test_deferred_strategies.py` — every test exercises `st.deferred(lambda: ...)`,
  a public-API lazy forward-reference strategy used for recursive definitions
  (e.g. `tree = st.deferred(lambda: st.integers() | st.tuples(tree, tree))`).
  Hegel-rust has no `gs::deferred()` equivalent — Rust's static type system
  requires an explicit per-use-case recursive enum, so the `Strategy`-object
  forward-declaration pattern has no direct analog (same gap as the whole-file
  skip of `test_recursive.py` and the individually-skipped
  `test_searchstrategy.py::test_deferred_strategy_draw`). Most tests also assert
  on strategy-composition-class introspection (`.branches`, `.is_empty`,
  `.has_reusable_values`), which hegel-rust's typed-wrapper generators don't
  expose at any level.

- `test_constants_ast.py` — tests Hypothesis's Python-AST constant
  extractor (`ConstantVisitor`, `constants_from_module`); parses Python
  source code, no Rust counterpart.
- `test_codemods.py` (in `codemods/`) — tests
  `hypothesis.extra.codemods`, a Python source-code refactoring tool
  built on `libcst` (the LibCST Python CST library) that rewrites legacy
  Hypothesis API calls. Entire file depends on `libcst.codemod.CodemodTest`
  and tests Python-syntax transformations (keyword arguments,
  `HealthCheck.all()` → `list(HealthCheck)`, etc.); no Rust counterpart.
- `test_codemod_cli.py` (in `codemods/`) — tests the `hypothesis codemod`
  CLI entry point that drives the same `hypothesis.extra.codemods` tool
  over Python source files (single-file, multi-file, and stdin modes).
  Runs `hypothesis codemod ...` as a subprocess and asserts the Python
  file contents get rewritten; no Rust counterpart to either the CLI or
  the Python-syntax transformation.
- `test_local_constants.py` (in `conjecture/`) — tests the consumption
  side of the same Python-AST constant-collection feature as
  `test_constants_ast.py` above. Every test monkey-patches
  module-level attributes on
  `hypothesis.internal.conjecture.providers` (`_get_local_constants`,
  `_sys_modules_len`, `_seen_modules`, `is_local_module_file`) or
  `sys.modules` itself (`monkeypatch.setitem(sys.modules, ...,
  SimpleNamespace())`); the feature scans Python source via
  `ast.parse` and relies on Python's module-file import system. No
  Rust counterpart — Python-specific facilities (`sys.modules`,
  `monkeypatch`, `ast`).
- `test_junkdrawer.py` (in `conjecture/`) — every test targets a
  Python-language-specific utility container or facility in
  `hypothesis.internal.conjecture.junkdrawer` that exists only to work
  around Python limitations that don't apply in Rust, or targets a
  `hypothesis.internal.floats` helper whose behaviour is already
  exercised through its Rust caller. `LazySequenceCopy` (O(1) list
  copy via a dict+`SortedList` mask) is redundant in Rust where
  ownership and `Vec::clone()` / `Cow::Borrowed` handle the same job;
  `IntList` (auto-upgrading `array.array` typecode storage) is
  redundant in Rust where typed `Vec<T>` is used directly; the
  `ensure_free_stackframes` / `stack_depth_of_caller` pair exercise
  Python's `sys.setrecursionlimit` / `HypothesisWarning` machinery and
  `sys._getframe` introspection, none of which have Rust counterparts
  (Rust uses the OS stack with no user-facing limit API, and has no
  runtime frame-chain API); `startswith` / `endswith` are wrappers
  around Python `bytes.startswith` / `bytes.endswith` that in Rust are
  built-in `[T]::starts_with` / `[T]::ends_with`, so the tests reduce
  to exercising stdlib; `replace_all` and `binary_search` are engine
  helpers not mirrored in `src/native/` (the native shrinker uses
  inline logic rather than a standalone helper); and the single
  `test_clamp` test targets `hypothesis.internal.floats.clamp`, which
  is already a private helper inside `src/native/floats.rs` and whose
  sign-aware `-0.0` / `0.0` / NaN behaviour is already covered through
  `make_float_clamper` in `tests/hypothesis/float_utils.rs`
  (`test_float_clamper_examples` uses the same boundary cases).
- `test_inquisitor.py` (in `conjecture/`) — every test exercises
  Hypothesis's "inquisitor" output feature (source-level comments
  like `# or any other generated value` and `# The test always failed
  when commented parts were varied together` appended to falsifying
  examples). hegel-rust has no inquisitor (no references in `src/`
  or `tests/`) and its failure output format is entirely different
  (same rationale as the skipped `test_falsifying_example_output.py`).
  All tests also depend on Python-specific facilities: `__notes__`
  (PEP 678 exception annotation) via a `fails_with_output` helper
  that compares the notes text; `st.builds(MyClass, ...)` (Python
  class construction; hegel-rust's `#[derive(Generate)]` is
  compile-time only and the failure-report formatter would emit
  Rust syntax, not `MyClass(0, True)`); `st.fixed_dictionaries({"x":
  ..., "y": ...})` (Python string-keyed heterogeneous dicts with no
  Rust analog); and `st.data()` with
  `data.conjecture_data.draw_boolean(forced=True)` (hegel-rust has
  no `gs::data()` public entry point for runtime draws and does not
  expose the internal forced-draw primitive through any strategy).
- `test_intlist.py` (in `conjecture/`) — port-loop worker produced
  commits that conflicted irreconcilably with the `src/native/` backend
  on integration (Cargo.toml, src/lib.rs, src/native/mod.rs,
  src/native/runner.rs, src/runner.rs, tests/test_native.rs).
  Abandoned pending human review of the worker's `port/worker-0` branch.
- `test_discovery_ability.py` (in `quality/`) — port-loop worker
  produced commits that failed to cherry-pick onto the supervisor
  branch (TODO.yaml integration conflict). Abandoned pending human
  review of the worker's `port/worker-1` branch.
- `test_caching.py` — tests Python object identity (`st.text() is
  st.text()`) of Hypothesis's strategy cache; Rust generators are
  builder structs with no `is`-style identity equivalent.
- `test_cacheable.py` (in `nocover/`) — every test depends on Python-specific
  strategy facilities with no Rust counterpart:
  `test_is_cacheable` / `test_is_not_cacheable` read the
  `SearchStrategy.is_cacheable` introspection attribute (same family as
  `.is_empty` / `.branches` strategy-class introspection which
  hegel-rust's typed-wrapper generators don't expose);
  `test_non_cacheable_things_are_not_cached` /
  `test_cacheable_things_are_cached` compare strategy instances with
  `==`/`!=` to pin down Hypothesis's strategy cache (same gap as
  `test_caching.py` above); `test_local_types_are_garbage_collected_issue_493`
  uses `weakref.ref` + `gc.collect()` to assert Python garbage-collection
  behaviour on a locally-defined `@given`-decorated class — no Rust analog.
- `test_conjecture_int_list.py` (in `nocover/`) — the sole stateful test
  (`IntListRules(RuleBasedStateMachine)` + `TestIntList = IntListRules.TestCase`)
  targets `hypothesis.internal.conjecture.junkdrawer.IntList`, a
  Python-language-specific compact-integer container that stores values in
  an `array.array` with a dynamically-upgrading typecode (`B`/`H`/`I`/`L`/`Q`/`O`)
  to save memory on small non-negative integers and fall back to unbounded
  Python `int` via a `list[int]` when a value overflows the largest
  fixed-width code. Same rationale as the already-skipped `test_junkdrawer.py`
  (in `conjecture/`): `IntList` is redundant in Rust where typed `Vec<u64>`
  / `Vec<u128>` / `Vec<BigUint>` is used directly and there is no
  auto-upgrading typecode machinery to model. The upstream state machine
  also uses `st.runner()` (Hypothesis's `@st.composite` hook that returns
  the currently-running `RuleBasedStateMachine` instance so a sampling
  strategy can see the model), which hegel-rust's `hegel::stateful` does
  not expose — there is no equivalent way for a strategy used inside a
  `#[rule]` to observe the state-machine struct it is running against.
- `test_conventions.py` (in `nocover/`) — the sole test asserts
  `repr(UniqueIdentifier("hello_world")) == "hello_world"`, exercising
  Python's `__repr__` dunder on a `hypothesis.utils.conventions` sentinel
  type. `UniqueIdentifier` is a Python-only marker used as a default-arg
  sentinel that prints as its own name; hegel-rust has no `UniqueIdentifier`
  type and no `__repr__` dunder surface to test.
- `test_drypython_returns.py` (in `nocover/`) — regression test for
  Hypothesis's integration with the third-party `returns` Python library
  (issue #3060). Every test exercises Python-only type-introspection
  facilities: `typing.Generic[T]` / `TypeVar` subclasses, `__mro__` dunder
  access on user classes, `isinstance(mappable, MappableN)` checks on
  generic types, `st.builds(target_func)` / `st.builds(type_)`
  function-signature introspection, and `temp_registered(T, strategy)`
  to override `from_type` resolution for a class. Neither `from_type`,
  `register_type_strategy`, nor runtime type-annotation resolution
  exists in hegel-rust (same family as the `test_lookup*.py` /
  `test_type_lookup*.py` skips).
- `test_eval_as_source.py` (in `nocover/`) — every test exercises
  `hypothesis.internal.reflection.source_exec_as_module`, which dynamically
  executes a Python source string as a Python module (via `exec`/`compile`
  and caches the resulting module object). This is a Python-runtime
  facility with no Rust counterpart: Rust has no runtime source-evaluation
  or module-object model, and hegel-rust does not expose any equivalent
  reflection helper.
- `test_fancy_repr.py` (in `nocover/`) — every test asserts Python's
  `repr()` of a strategy against a literal string
  (`repr(st.floats()) == "floats()"`,
  `repr(st.integers().map(lambda x: x * 2)) == "integers().map(lambda x: x * 2)"`,
  etc.), including lambda-source reconstruction via
  `get_pretty_function_description`. hegel-rust generators are Rust
  builder structs with no repr surface (same family as `test_bad_repr.py`
  repr skips, `test_custom_reprs.py`, `test_pretty_repr.py`, and
  `test_eval_as_source.py`).
- `test_integer_ranges.py` (in `nocover/`) — the sole test
  (`test_bounded_integers_distribution_of_bit_width_issue_1387_regression`)
  draws from `integers(0, 1e100)` (upper bound ≈ 332 bits) and observes
  that the distribution is shaped for ~7/8 of draws within a 128-bit
  window and uniform across the full range for the remaining ~1/8.
  hegel-rust's public `gs::integers::<T>()` takes a fixed-size type via
  the `Integer` trait (`i8..=i128`, `u8..=u128`), capping `max_value` at
  `i128::MAX` ≈ 1.7e38, so a range wider than 128 bits cannot be
  expressed. The native backend saturates at i128 too — `interpret_integer`
  routes `bignum_overflows_i128(max_cbor)` to a u128 selector + two-halves
  draw, with no arbitrary-precision path. With any expressible range the
  test's distribution-shape assertion (`huge = sum(x > 1e97 ...)`,
  `assert huge != 0 or len(values) < 800`) is unobservable.
- `test_interesting_origin.py` (in `nocover/`) — the sole test,
  parametrized over three `go_wrong_*` helpers that raise `ValueError`
  with different Python exception-chaining patterns (naive, `raise X from
  err`, `raise X from None`), asserts that `@given` surfaces an
  `ExceptionGroup` under `settings(report_multiple_bugs=True)` — i.e. it
  pins down that `InterestingOrigin.from_exception` distinguishes the
  three chain shapes via `__cause__` / `__context__` traversal and
  Hypothesis groups them into one PEP 654 `ExceptionGroup`. Rust panics
  are singular with no chaining construct, hegel-rust's `Settings`
  exposes no `report_multiple_bugs` setting (same gap as the
  `test_replay_logic.py::test_does_not_shrink_on_replay_with_multiple_bugs`,
  `test_exceptiongroup.py`, `test_slippage.py`, and `test_escalation.py`
  skips), and `src/native/` has no `InterestingOrigin` counterpart.
- `test_labels.py` (in `nocover/`) — every test asserts on Hypothesis's
  per-strategy `SearchStrategy.label` attribute (`st.integers().label is
  st.integers().label`, `st.integers().label != st.text().label`,
  `st.one_of(a, b).label != st.one_of(b, a).label`, etc.) plus
  `ConjectureData.for_choices([0])` / `cd.spans[1].label` span-label
  introspection and `st.deferred(...)`. hegel-rust has no per-strategy
  `.label()` on generators — span labels in `src/test_case.rs::labels`
  are per-span-*type* u64 constants (`LIST`, `TUPLE`, `FILTER`, etc.),
  not per-strategy identifiers derived from composition. `st.deferred`
  has no analog either (same gap as the individually-skipped
  `test_searchstrategy.py::test_deferred_strategy_draw`), and hegel-rust
  generators are Rust generic wrappers with no runtime introspection
  surface to derive a Hypothesis-style per-strategy label from (same
  strategy-class-structure family as `.is_cacheable` / `.branches` /
  `.is_empty` skips).
- `test_large_examples.py` (in `nocover/`) — port abandoned due to
  integration conflict when cherry-picking onto the supervisor branch.
- `test_pretty_repr.py` (in `nocover/`) — both tests exercise Python's
  `repr()` / `eval()` round-trip on strategy objects.
  `test_repr_evals_to_thing_with_same_repr` asserts `repr(eval(repr(s)))
  == repr(s)`, i.e. that a strategy's `__repr__` produces Python source
  that evaluates back to an equivalent strategy via `eval(r,
  strategy_globals)`. `test_sampled_transform_reprs` asserts
  `repr(eval("sampled_from([1, 2, 3]).filter(foo).map(bar)")) ==
  "sampled_from([1, 2, 3]).filter(foo).map(bar)"` across three
  parametrize rows. Both depend on Python's `__repr__` dunder,
  `eval()` of source, and function-name introspection (module-level
  `foo`/`bar`/`baz` referenced by name in the repr). hegel-rust
  generators are Rust builder structs with no repr surface and no
  runtime source-evaluation path (same family as the `test_bad_repr.py`
  repr skips, `test_custom_reprs.py`, and `test_eval_as_source.py`).
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
- `test_lookup_py39.py` — tests `from_type()` resolution of Python typing
  constructs (`typing.Annotated[int, metadata]`, `typing.Union[list[int], int]`,
  `typing.Protocol[T]` with `typing.runtime_checkable`,
  `collections.abc.Callable[[None], None]`), `register_type_strategy` /
  `temp_registered` overrides on builtin and user types, `st.builds()`
  function-signature introspection, and Python `repr()` assertions on
  strategies (`repr(st.from_type(...)) == "integers()"`). Neither
  `from_type`, `register_type_strategy`, nor runtime type-annotation
  resolution exists in hegel-rust (same family as the other
  `test_lookup*.py` skips above).
- `test_typealias_py312.py` — tests `from_type()` resolution of PEP 695
  `type` alias syntax (`type MyInt = int`, parameterized
  `type A[T] = list[T]`, mutually-recursive aliases),
  `register_type_strategy` overrides on aliases, and the internal
  `evaluate_type_alias_type` helper. Rust `type X = Y;` aliases are
  compile-time only with no runtime alias-object surface, and hegel-rust
  has no `from_type` / `register_type_strategy` analog (same family as
  the `test_lookup*.py` skips above).
- `test_type_lookup.py` — tests `st.from_type()` and
  `st.register_type_strategy()` resolution of Python typing constructs
  (`typing.Generic[T]`, `typing.TypeVar`, `Sequence[int]`, `Union[str, int]`,
  `Callable[..., str]`, `X | Y` union syntax), abstract classes via
  `abc.ABC` / `@abc.abstractmethod`, `enum.Enum` subclasses, `st.builds()`
  function-signature introspection, `@given(a=infer)` with runtime
  `__annotations__` mutation, `inspect.Signature` / `get_type_hints`, and
  internal attributes (`LazyStrategy`, `_global_type_lookup`,
  `_all_strategies`). Neither `from_type`, `register_type_strategy`, nor
  runtime type-annotation resolution exists in hegel-rust (same family as
  the `test_lookup*.py` skips above).
- `test_type_lookup_forward_ref.py` — tests `st.builds(fn)` resolution of
  `TypeVar(..., bound="MyType")` string forward references and
  `temp_registered(ForwardRef("MyType"), ...)` overrides. Python's
  `TypeVar` / `ForwardRef` / runtime type-annotation introspection have
  no Rust counterpart, and hegel-rust has no `st.builds()` /
  `register_type_strategy` analog (same family as the `test_lookup*.py`
  skips above).
- `test_type_lookup_future_annotations.py` (in `nocover/`) — tests
  `st.from_type(B)` resolution of `typing.TypedDict` subclasses with
  forward-referenced field types under `from __future__ import
  annotations` (PEP 563 stringified annotations), `int | str` PEP 604
  union syntax, and `InvalidArgument` raised on locally-scoped aliases.
  `from_type`, `TypedDict`, runtime annotation evaluation, and PEP 604
  unions all have no Rust counterpart (same family as the
  `test_lookup*.py` skips above).
- `test_annotated_types.py` — tests `st.from_type()` resolution of
  `typing.Annotated[T, ...]` metadata using the third-party Python
  `annotated_types` library (`at.Gt`, `at.Ge`, `at.Lt`, `at.Le`,
  `at.Interval`, `at.Len`, `at.Predicate`, `at.Timezone`, `at.MultipleOf`,
  `__is_annotated_types_grouped_metadata__`), plus `typing.TypedDict` with
  `NotRequired`, Python `slice` objects as metadata, and internal
  attributes (`unwrap_strategies`, `FilteredStrategy.flat_conditions`,
  `_get_constraints`). Neither `from_type`, `typing.Annotated`
  introspection, nor the `annotated_types` library has any Rust
  counterpart (same family as the `test_lookup*.py` skips above).
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
- `test_reporting.py::test_does_not_print_debug_in_verbose`,
  `test_reporting.py::test_does_print_debug_in_debug`,
  `test_reporting.py::test_does_print_verbose_in_debug` — exercise
  `hypothesis.reporting.debug_report` / `verbose_report`, public APIs
  for verbosity-gated user logging that hegel-rust does not expose. The
  closest analog, `tc.note()`, is verbosity-independent and only fires
  on the final failing-test replay.
- `test_reporting.py::test_can_report_when_system_locale_is_ascii` —
  uses Python `monkeypatch.setattr(sys, "stdout", ...)` and `os.pipe()`
  to swap the process stdout for an ASCII-only stream. Both are
  Python-specific facilities with no Rust counterpart.
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
- `test_randomization.py` (in `nocover/`) — both tests rely on the same
  Python-specific global-PRNG integration skipped above in
  `test_random_module.py`. `test_seeds_off_internal_random` reaches into
  `hypothesis.core.threadlocal._hypothesis_global_random` (a private Python
  `random.Random` instance used as Hypothesis's global seed source) and
  drives it via `Random().getstate()` / `setstate()`; hegel-rust seeds come
  from `Settings::seed` / `derandomize`, with no global `Random` singleton
  to introspect or reset. `test_nesting_with_control_passes_health_check`
  uses `st.random_module()` to seed Python's global `random` module inside
  a nested `@given`, plus `HealthCheck.nested_given` suppression — neither
  the `random_module()` strategy nor the nested-given health check exists
  in hegel-rust.
- `test_reusable_values.py` (in `nocover/`) — every test asserts on
  Hypothesis's `SearchStrategy.has_reusable_values` introspection
  attribute (`assert s.has_reusable_values`,
  `assert not s.filter(...).has_reusable_values`,
  `assert not s.map(...).has_reusable_values`,
  `assert not s.flatmap(...).has_reusable_values`,
  `assert not st.lists(...).has_reusable_values`, etc.). hegel-rust
  generators are Rust generic-wrapper types (`Filtered<T, F, G>`,
  `Mapped<...>`, etc.) with no introspectable `.has_reusable_values`
  surface — same strategy-class-structure family as the `.is_empty` /
  `.branches` / `.is_cacheable` / `.label` skips above. The file also
  uses `st.deferred(lambda: ...)` for the recursion test, which has no
  `gs::deferred()` counterpart (same gap as `test_deferred_strategies.py`).
- `test_strategy_state.py` (in `nocover/`) — the entire file is a single
  `HypothesisSpec(RuleBasedStateMachine)` whose design is predicated on
  Python's dynamic typing of strategy objects: `strategies = Bundle("strategy")`
  holds heterogeneously-typed Hypothesis strategy values (integers, booleans,
  floats, text, binary, tuples of strategies, etc.), which are then
  dynamically composed by rules that call `tuples(*spec)`, `source.filter(...)`,
  `source | right`, `source.flatmap(...)`, `sampled_from(values)`, and
  `lists(elements)` on bundle members. hegel-rust's `Variables<T>` is
  strictly monomorphic and `gs::one_of` / `gs::sampled_from` require a
  single static element type, so a bundle of arbitrary generators cannot
  be expressed in Rust's type system. The rule set also draws Python-only
  numeric strategies (`complex_numbers()`, `fractions()`, `decimals()`),
  uses `Random(hashlib.sha384(...).digest())` for deterministic predicate
  seeding, and culminates in `repr_is_good` which asserts `" at 0x" not
  in repr(strat)` — a direct test of Python `__repr__` dunder output on
  strategy objects, with no hegel-rust counterpart.
- `test_modify_inner_test.py` (in `nocover/`) — every test exercises
  Python-specific attribute-access on a `@given`-decorated function:
  `test.hypothesis.inner_test = replacement` swaps the wrapped test body
  in-place (used by shims like pytest-trio's async-to-sync converter).
  The remaining cases pile on more Python-specific machinery:
  `functools.wraps` decorator composition, `pytest.mark.parametrize`
  stacking on top of `@given`, `InvalidArgument` errors raised by
  `@given` for invalid signatures ("Too many positional arguments",
  "given must be called with at least one argument"), and
  `lambda **kw: f(**kw)` kwargs-expansion of the inner test. hegel-rust
  tests are closures passed to `Hegel::new(|tc| {...})` with no inner
  function object, no swappable `inner_test` attribute, no kwargs
  model, and no `InvalidArgument`-at-call-time surface — `#[hegel::test]`
  signature errors are compile-time macro errors.
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
- `test_patching.py` (in `tests/patching/`) — tests
  `hypothesis.extra._patching` (`get_patch_for`, `make_patch`, `FAIL_MSG`,
  `HEADER`, `indent`), a public API that generates Python source code
  patches inserting `@example(...)` decorators into failing test files
  (`@given`/`@example` are Python decorator syntax). Also depends on
  `pytester` (pytest plugin integration) to assert patch-file location
  output, and includes a numpy `UNDEF_NAME` case. No hegel-rust counterpart:
  hegel-rust does not emit Python source patches, is not a pytest plugin,
  and has no `@example`-decorator API.
- `test_phases.py` — every test in the file exercises Hypothesis's `Phase`
  enum / `@settings(phases=...)` public API (phase ordering / deduping,
  `Phase.explicit`-only runs, `Phase.generate` / `Phase.reuse` / `Phase.shrink`
  gating of the generate / database-reuse phases, `settings().phases` default,
  and `InvalidArgument` on non-Phase members). hegel-rust's `Settings`
  builder exposes no `phases` method and no `Phase` type (already noted by
  the `test_core.py::test_non_executed_tests_raise_skipped`,
  `test_explicit_examples.py`, and `test_falsifying_example_output.py`
  skips) — phase gating is not a public API surface here, so none of the
  eight tests are portable.
- `test_reflection.py` — all tests exercise Python-specific reflection utilities:
  `convert_keyword_arguments`/`convert_positional_arguments`/`define_function_signature`
  (Python `inspect.Signature`/`Parameter` manipulation), `function_digest`/`repr_call`/
  `get_pretty_function_description`/`source_exec_as_module`/`proxies`/`is_mock`/
  `is_first_param_referenced_in_function`/`is_identity_function`/`required_args`
  (Hypothesis internal Python reflection helpers), `lambda_sources._function_key`/
  `_normalize_code`/`_clean_source` (Python bytecode and source-code manipulation),
  `LazyStrategy.__repr__` warnings, `unittest.mock` objects, `sys.path`, and
  `functools.partial/wraps`. None of these have Rust counterparts.

- `test_lambda_formatting.py` — every test exercises
  `hypothesis.internal.reflection.get_pretty_function_description` against
  Python `lambda` expressions, verifying the pretty-printer's handling of
  bracket/whitespace stripping, unicode-in-source, nested lambdas,
  unparsable source, trailing comments, decorator argument position, line
  continuations, `eval`-defined callables, module-source mutation across
  `runpy.run_path` calls, and the `lambda_sources` caches
  (`LAMBDA_DESCRIPTION_CACHE`, `LAMBDA_DIGEST_DESCRIPTION_CACHE`,
  `AST_LAMBDAS_CACHE`). The pretty-printer reads Python source with
  `inspect.getsource`, parses it with `ast.parse`, and inspects
  `__code__`/`__globals__`/`__defaults__` on lambda objects — all
  Python-specific facilities with no Rust counterpart. Rust closures have
  no introspectable source, no AST, and no lambda-description cache,
  matching the existing whole-file `test_reflection.py` skip.
- `test_lambda_inlining.py` — every test exercises `hypothesis.vendor.pretty._try_inline_lambda`,
  a Python-specific repr helper that reconstructs lambda source via `ast.parse`; no Rust counterpart.
  Port abandoned due to integration conflict in `src/native/conjecture_runner.rs` (cherry-pick of
  `port/worker-2` onto the supervisor branch failed).

- `nocover/test_deferred_errors.py::test_does_not_recalculate_the_strategy`
  — uses Python's `hypothesis.strategies._internal.core.defines_strategy`
  decorator, which wraps a factory in a `LazyStrategy` that memoises the
  underlying `SearchStrategy` after the first use. Hegel-rust generators
  are eagerly-constructed structs rather than lazy factory wrappers, so
  there is no equivalent laziness/memoisation layer to pin down — the
  behaviour the test describes simply isn't a concept in the Rust API.

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

- `test_seeding.py` (in `tests/pytest/`) — both tests drive the `pytester` plugin
  (`testdir.makepyfile`/`testdir.runpytest`) to spawn pytest subprocesses, parse
  their stdout, and assert on the `--hypothesis-seed=N` pytest CLI flag and the
  seed-instruction printed on `FailedHealthCheck`. Also uses `monkeypatch.delenv`
  and `hypothesis._settings._CI_VARS`. The whole file is Hypothesis's pytest-plugin
  seeding UX, which is Python/pytest-specific with no hegel-rust counterpart.

- `test_checks.py` (in `tests/pytest/`) — the single test drives the `pytester`
  plugin (`testdir.makepyfile`/`testdir.runpytest`) to spawn a pytest subprocess
  and assert that pytest test functions decorated with `@hypothesis.seed`,
  `@hypothesis.example`, `@hypothesis.reproduce_failure`, or `@composite` but
  without `@given` are reported as failed by pytest. This is Hypothesis's
  pytest-plugin `pytest_collection_modifyitems`-style check against misuse of
  Python decorators on pytest test functions; hegel-rust is not a pytest plugin
  and has no decorator-without-`@given` failure path, so the whole concept has
  no counterpart.

- `test_constant_collection_timing.py` (in `tests/pytest/`) — both parametrize rows
  drive the `pytester` plugin (`testdir.makeconftest`/`testdir.makepyfile`/
  `testdir.runpytest`) to spawn a pytest subprocess and parse `--durations=0`
  output (`re.search(r"([\d.]+)s call\s+\S+::test_first", output)`) to assert
  whether the Hypothesis pytest plugin deferred constant collection away from
  `test_first` — toggled with `-p no:hypothesispytest`. Also patches Hypothesis
  internals (`providers._get_local_constants`, `providers._local_constants`,
  `providers._sys_modules_len`) that have no native counterpart. The whole
  concept — pytest-plugin-driven deferred constant collection observed via
  pytest's duration output — is pytest-plugin integration with no hegel-rust
  counterpart.

- `test_doctest.py` (in `tests/pytest/`) — the single test drives the `pytester`
  plugin (`testdir.makepyfile`/`testdir.runpytest`) to spawn a pytest subprocess
  with `--doctest-modules` on a Python file containing doctest syntax (`>>>` in
  triple-quoted docstrings). This is Python doctest integration with pytest; both
  the pytester plugin and Python doctest syntax are Python-specific facilities with
  no hegel-rust counterpart.

- `test_pytest_detection.py` (in `tests/pytest/`) — every test exercises
  Hypothesis's `hypothesis.core.running_under_pytest` module-level flag (set by
  the pytest plugin) and the `pytester` plugin (`testdir.makepyfile`/
  `testdir.runpytest_subprocess`) to assert the hypothesis pytest plugin does
  not import `hypothesis` when pytest loads it. Also uses `sys.modules`
  inspection via a `python` subprocess. hegel-rust is not a pytest plugin and
  has no `running_under_pytest` equivalent — the whole file is Python/pytest
  plugin integration.

- `test_profiles.py` (in `tests/pytest/`) — both tests drive the `pytester`
  plugin (`testdir.makeconftest`/`testdir.makepyfile`/`testdir.runpytest`) to
  spawn a pytest subprocess and assert on the Hypothesis pytest plugin's
  `--hypothesis-profile` CLI flag (`_hypothesis_pytestplugin.LOAD_PROFILE_OPTION`)
  and `--hypothesis-verbosity=verbose` CLI flag output ("hypothesis profile",
  "max_examples=1"). Exercises `settings.register_profile` wired to the pytest
  plugin's profile-loading hook. hegel-rust is not a pytest plugin and has no
  `--hypothesis-profile`/`--hypothesis-verbosity` CLI surface.

- `test_skipping.py` (in `tests/pytest/`) — both tests drive the `pytester`
  plugin (`testdir.makepyfile`/`testdir.runpytest`) to spawn pytest subprocesses
  and assert on how `pytest.skip()` raised inside a `@given`/`@example` test
  body interacts with Hypothesis's shrinking and reporting (no "Falsifying
  example" output; `assert_outcomes(skipped=1)`). Depends on `pytest.skip`,
  `@example` decorator stacking, `-m hypothesis` pytest marker, and
  `--tb=native` pytest CLI — all pytest-plugin integration with no hegel-rust
  counterpart.

- `test_statistics.py` (in `tests/pytest/`) — all tests drive the `pytester`
  plugin (`testdir.makepyfile`/`testdir.runpytest`) to spawn pytest subprocesses
  and assert on the `--hypothesis-show-statistics` pytest CLI flag
  (`_hypothesis_pytestplugin.PRINT_STATISTICS_OPTION`) output
  ("Hypothesis Statistics", "max_examples=100", "< 1% of examples satisfied
  assumptions"). Also exercises xdist (`-n 2`), JUnit XML (`--junit-xml=out.xml`),
  `unittest.TestCase` integration, and `hypothesis.stateful.RuleBasedStateMachine`
  integration — all via Hypothesis's pytest plugin output. hegel-rust is not a
  pytest plugin and has no `--show-statistics` CLI surface.

- `test_collection_warning.py` (in `tests/pytest/`) — the single test drives the
  `pytester` plugin (`pytester.runpytest_subprocess`) to spawn a pytest
  subprocess and assert that the Hypothesis pytest plugin emits a warning
  containing `"Skipping collection of '.hypothesis'"` when pytest tries to
  collect tests from the `.hypothesis` directory. This is Hypothesis's
  pytest-plugin `pytest_collectstart` hook behaviour; hegel-rust is not a
  pytest plugin and has no `.hypothesis` collection surface, so the whole
  concept has no counterpart.

- `test_parametrized_db_keys.py` (in `tests/pytest/`) — the first test drives
  the `pytester` plugin (`testdir.makepyfile`/`testdir.runpytest`) to spawn a
  pytest subprocess and assert on `assert_outcomes(xfailed=3, passed=1)` for a
  script using `@pytest.mark.parametrize` / `@pytest.mark.xfail` to verify that
  `InMemoryExampleDatabase` keys differ per parametrize row. The remaining
  tests (`TestNoDifferingExecutorsHealthCheck`) exercise
  `@pytest.mark.parametrize`, `@pytest.fixture(params=...)`, and
  `HealthCheck.function_scoped_fixture` suppression — the pytest-fixture /
  parametrize-per-test-function identity machinery that Hypothesis's pytest
  plugin hooks into. hegel-rust is not a pytest plugin and has no
  parametrize/fixture surface, so the whole file is pytest-plugin integration
  with no hegel-rust counterpart.

- `test__pytest.py` (in `tests/pytest/`) — the single test drives the
  `pytester` plugin (`testdir.makepyfile`) to spawn a Python subprocess
  running a script that imports pytest's private `_pytest` module and
  verifies Hypothesis does not rely on `_pytest`'s submodules
  (`_pytest.outcomes`) being pre-imported by checking `sys.modules`.
  Python subprocess + `sys.modules` introspection + pytest's private
  module layout — all Python/pytest-specific facilities with no
  hegel-rust counterpart.

- `test_fixtures.py` (in `tests/pytest/`) — every test exercises pytest
  fixture injection into `@given` test functions: `@pytest.fixture`
  (session/module/function scope), positional and keyword mixing of
  fixtures and strategies in `@given` signatures, `Mock`/`create_autospec`
  injection via fixtures, and the `pytester`/`testdir` plugin
  (`testdir.makepyfile`/`testdir.makeconftest`/`testdir.runpytest`) to
  assert on `HealthCheck.function_scoped_fixture`, `settings.register_profile`
  + `--hypothesis-profile`, and `@given`/`@pytest.fixture` decorator
  ordering errors. hegel-rust is not a pytest plugin and has no fixture
  injection surface — the whole file is pytest-plugin integration with
  no hegel-rust counterpart.

- `test_mark.py` (in `tests/pytest/`) — port abandoned after a
  parallel port-loop worker's port could not be cherry-picked onto the
  supervisor branch due to a SKIPPED.md merge conflict; the file drives
  the `testdir`/`pytester` plugin to assert pytest's `-m hypothesis`
  marker filter (via the automatic `@pytest.mark.hypothesis` marker
  Hypothesis's pytest plugin adds to `@given` tests) deselects
  non-hypothesis tests, which is pytest-plugin integration with no
  hegel-rust counterpart.

- `test_capture.py` (in `tests/pytest/`) — every test drives the
  `pytester` plugin (`testdir.makepyfile`/`testdir.runpytest`/
  `testdir.runpytest_subprocess`) to spawn pytest subprocesses and
  assert on pytest-specific behaviour: the `--capture={no,fd}` pytest
  CLI flag's effect on Hypothesis verbose output ("Trying example"
  visibility), `LC_ALL`/`LANG` `monkeypatch` interaction with pytest's
  unicode emission, pytest's `__tracebackhide__` line-number framing
  around `FailedHealthCheck` reporting, and pytest's deprecation
  reporting when a `@composite`-decorated function is collected as a
  test (`assert_outcomes(failed=1)`). Also imports `_CI_VARS` and
  `escape_unicode_characters` from `hypothesis._settings` /
  `hypothesis.internal.compat` and uses `pytest.mark.parametrize` /
  `pytest.mark.xfail`. hegel-rust is not a pytest plugin and has no
  pytest-capture / `--capture` / `__tracebackhide__` /
  `composite`-as-test-collection surface — the whole file is
  pytest-plugin integration with no hegel-rust counterpart.

- `test_junit.py` (in `tests/pytest/`) — both tests drive the
  `pytester` plugin (`testdir.makepyfile`/`testdir.runpytest`) to
  spawn a pytest subprocess with `--junit-xml=out.xml` (and `-n2` for
  the xdist variant), then parse the JUnit XML output with
  `xml.etree.ElementTree` to assert that Hypothesis's pytest plugin
  emits `hypothesis-statistics-...` entries as `<properties>` under
  `<testsuite>` rather than `<testcase>` (xunit2 compatibility), and
  that pytest-xdist's well-known shortfall here doesn't regress.
  Depends on the `pytester` plugin, pytest's `--junit-xml` CLI flag,
  pytest-xdist's `-n` flag, and Hypothesis's pytest-plugin
  statistics-as-junit-properties output. hegel-rust is not a pytest
  plugin and has no JUnit XML / xdist / statistics-properties surface
  — the whole file is pytest-plugin integration with no hegel-rust
  counterpart.

- `test_sideeffect_warnings.py` — all tests exercise Hypothesis's Python-specific
  import-time initialization infrastructure: `_hypothesis_globals.in_initialization`
  (a Python module attribute tracking import phase), `hypothesis.configuration`
  internals (`_first_postinit_what`, `notice_initialization_restarted`,
  `check_sideeffect_during_initialization`), `HypothesisSideeffectWarning` (a
  Python warning type), and `pytest.warns`/`monkeypatch` pytest fixtures. This
  tests Python module-loading side-effect detection during entrypoint loading,
  a concept with no Rust counterpart.

- `test_setup_teardown.py` — every test exercises Hypothesis's public
  `setup_example(self)` / `teardown_example(self, ex)` hook protocol on
  test classes, combined with Python multiple inheritance
  (`class HasSetupAndTeardown(HasSetup, HasTeardown, SomeGivens)`) to mix
  setup/teardown mixins with `@given`-decorated method bodies. The hook
  contract is that Hypothesis calls `self.setup_example()` before, and
  `self.teardown_example(ex)` after, each example of a class-bound
  `@given` method — driven by Python method dispatch on the test
  instance. Hegel-rust exposes a closure-based API
  (`Hegel::new(|tc| ...).run()`) with no class harness and no
  per-example hook surface at any level, so neither the hook API nor
  the inheritance-mixing pattern it relies on has a Rust counterpart.

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

- `test_scrutineer.py` — all tests exercise Hypothesis's "Explain
  phase" / scrutineer (`hypothesis.internal.scrutineer.make_report`),
  which traces Python bytecode via `sys.settrace` / `sys.monitoring`
  to identify which lines of the user's `@given` test function ran
  during failing cases and emits a formatted report. The feature is
  inherently Python-specific: hegel-rust user test code is compiled
  Rust running out-of-process from Python Hypothesis, so there is no
  Python bytecode to trace. The tests additionally depend on
  `pytest.testdir.runpytest_inprocess` (pytest-specific, no Rust
  counterpart) to spawn pytest as a subprocess and inspect its
  stdout, and on Python-specific file-path categorisation
  (local / `site-packages` / stdlib via `sysconfig.get_paths()`) for
  `test_report_sort`.

- `test_filestorage.py` — all tests exercise Hypothesis's `hypothesis.configuration`
  module (`storage_directory`, `set_hypothesis_home_dir`, the
  `HYPOTHESIS_STORAGE_DIRECTORY` environment variable, and the auto-written
  `.gitignore` in `.hypothesis/`), a Python-side facility for configuring where
  Python Hypothesis persists its examples database. Hegel-rust's client has no
  storage-directory configuration surface — persistence is handled server-side
  by Python Hypothesis and is opaque to the Rust client. The two
  `test_writes_gitignore_to_new_storage_dir` / `test_skips_gitignore_for_existing_storage_dir`
  tests additionally drive `subprocess`-launched Python scripts and `git init`.

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

- `nocover/test_filtering.py::test_chained_filters_repr` — asserts
  `repr(base.filter(foo).filter(bar)) == f"{base!r}.filter(foo).filter(bar)"`.
  Python `repr()` on strategies has no Rust counterpart; hegel-rust's
  `Filtered<T, F, G>` wrapper exposes no repr-style introspection surface.

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

- `test_subnormal_floats.py::test_subnormal_validation`,
  `test_subnormal_floats.py::test_allow_subnormal_defaults_correctly` —
  both depend on `floats(allow_subnormal=...)`, a public-API kwarg with
  no counterpart on hegel-rust's `gs::floats()` builder (no
  `.allow_subnormal(bool)` method). The `test_next_float_normal` test
  in the same file is ported natively.

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

- `nocover/test_baseexception.py::test_exception_propagates_fine[KeyboardInterrupt]`,
  `nocover/test_baseexception.py::test_exception_propagates_fine[SystemExit]`,
  `nocover/test_baseexception.py::test_exception_propagates_fine[GeneratorExit]`,
  `nocover/test_baseexception.py::test_exception_propagates_fine_from_strategy[KeyboardInterrupt]`,
  `nocover/test_baseexception.py::test_exception_propagates_fine_from_strategy[SystemExit]`,
  `nocover/test_baseexception.py::test_exception_propagates_fine_from_strategy[GeneratorExit]`,
  `nocover/test_baseexception.py::test_baseexception_no_rerun_no_flaky[KeyboardInterrupt]`,
  `nocover/test_baseexception.py::test_baseexception_in_strategy_no_rerun_no_flaky[KeyboardInterrupt]`,
  `nocover/test_baseexception.py::test_baseexception_in_strategy_no_rerun_no_flaky[SystemExit]`,
  `nocover/test_baseexception.py::test_baseexception_in_strategy_no_rerun_no_flaky[GeneratorExit]`
  — all pin down Python `BaseException`-subclass propagation semantics:
  Hypothesis treats `KeyboardInterrupt`/`SystemExit`/`GeneratorExit`
  differently from `Exception` (no catch/shrink/replay, no `Flaky`
  wrapping). Rust panics are singular — there is no
  `BaseException`/`Exception` split, so these parametrize rows collapse
  onto the `ValueError` cases which are ported in
  `tests/hypothesis/nocover_baseexception.rs`.

- `nocover/test_bad_repr.py::test_just_frosty`,
  `nocover/test_bad_repr.py::test_sampling_snowmen` — both assert the
  Python `repr()` of `st.just(Frosty)` / `st.sampled_from((Frosty, 'hi'))`
  against a literal string, where `Frosty`'s `__repr__` returns `"☃"`.
  hegel-rust generators have no repr surface. Third test
  (`test_sampled_from_bad_repr`) ported in
  `tests/hypothesis/nocover_bad_repr.rs`.

- `nocover/test_baseexception.py::test_explanations` — uses pytest's
  `testdir` fixture plus `runpytest_inprocess` stdout capture to check
  that the stack-trace explanation includes the drawn input when a
  `SystemExit` / `GeneratorExit` propagates out of a `@given` body. Both
  the `BaseException` trigger and the pytest-runtime output surface are
  Python-specific.

- `nocover/test_characters.py::test_can_constrain_characters_to_codec` —
  parametrizes over Python's `encodings.aliases.aliases` dict (100+ codec
  names like `cp1252`, `shift_jis`, `koi8-r`) and asserts the generated
  string encodes via Python's `str.encode(codec)`. Both the codec-list
  source and the verification step are Python-stdlib integrations with
  no Rust counterpart; hegel-rust's `codec` support in `src/native/` is
  limited to `ascii` / `latin-1` / `utf-8`. Rest of the file ported in
  `tests/hypothesis/nocover_characters.rs`.

- `nocover/test_conjecture_engine.py::test_saves_data_while_shrinking`,
  `nocover/test_conjecture_engine.py::test_can_discard` — both
  `monkeypatch.setattr(ConjectureRunner, "generate_new_examples",
  ...)` to seed a specific starting buffer (`[bytes([255] * 10)]` /
  `n` pairs of byte-choices respectively). No public
  seed-a-specific-initial-buffer entry point on the native engine;
  `NativeDatabase::save` only primes database replay (full serialised
  choice sequences), which isn't the same hook. `test_saves_data_while_shrinking`
  additionally depends on `choices_from_bytes` +
  `non_covering_examples(db)`, both tied to Hypothesis's database
  metakey layout. Rest of the file ported in
  `tests/hypothesis/nocover_conjecture_engine.rs`.
- `nocover/test_conjecture_engine.py::test_cached_with_masked_byte_agrees_with_results`
  — uses `runner.cached_test_function([a])`,
  `ConjectureData.for_choices([b], observer=runner.tree.new_observer())`,
  and `runner.test_function(data_b)` to compare cache identity
  (`cached_a is cached_b`) with node-sequence equality. Native
  `TargetedRunner::cached_test_function` returns a fresh
  `CachedTestResult { status }` each call with no `nodes` field and
  no identity semantics, and `CachedTestFunction` takes no pluggable
  observer. Rest of the file ported.
- `nocover/test_conjecture_engine.py::test_node_programs_fail_efficiently`
  — fixates on `shrinker.node_program("XX")` and asserts
  `Shrinker.run_node_program.calls` via `counts_calls`. The
  `node_program` deletion pass is absent from the native shrinker
  (same gap as the block of `node_program` tests already
  individually-skipped in `tests/hypothesis/conjecture_shrinker.rs`),
  and there is no `max_stall` / `fixate_shrink_passes` /
  call-counter surface on `Shrinker`. Rest of the file ported.

- `nocover/test_database_usage.py::test_saves_incremental_steps_in_database`,
  `nocover/test_database_usage.py::test_clears_out_database_as_things_get_boring`,
  `nocover/test_database_usage.py::test_trashes_invalid_examples`
  — assert that `find(strategy, predicate, ...)` accumulates multiple
  distinct entries in the supplied database (or shrinks them back to
  zero across runs as the predicate becomes always-false / always-
  invalid). The native engine doesn't auto-save during generation or
  shrinking — `NativeConjectureRunner::run()` only mutates the database
  through the reuse phase (which deletes invalid entries and replays
  existing ones), and `pareto_front()` is `todo!()`. The public
  `Hegel::new(...).run()` path saves only the final shrunk
  counterexample, never intermediates. So the multi-value accumulation
  these tests pin down isn't observable at either surface; they become
  portable once the native engine grows the auto-save side of
  `pareto_front` / interesting-example saves.
- `nocover/test_database_usage.py::test_respects_max_examples_in_database_usage`
  — counts predicate invocations against `max_examples=10`. Falls
  under the documented `find()` + predicate-call-count skip in
  `porting-tests/references/api-mapping.md`: native re-enters the
  test function for span-mutation attempts, so the predicate-call
  shape Python's `find()` pins down isn't reproducible.
- `nocover/test_database_usage.py::test_does_not_use_database_when_seed_is_forced`
  — uses pytest's `monkeypatch` fixture to set
  `hypothesis.core.global_force_seed` (a Python module-level global) and
  then overrides `database.fetch = None` via dunder-attribute assignment
  to assert `fetch` was not called. Both facilities are Python-specific:
  hegel-rust has no `global_force_seed` equivalent (seeds go through
  `Settings::new().seed(Some(n))`) and no runtime-attribute-assignment
  surface on `NativeDatabase`.
- `nocover/test_database_usage.py::test_ga_database_not_created_when_not_used`
  — constructs `ReadOnlyDatabase(GitHubArtifactDatabase("mock", "mock",
  path=path))`. `GitHubArtifactDatabase` has no Rust counterpart (same
  gap documented for the `test_database_backend.py` `test_ga_*` skips).
  The remaining `test_database_not_created_when_not_used` ports in
  `tests/hypothesis/nocover_database_usage.rs`.

- `nocover/test_emails.py::test_can_restrict_email_domains` —
  `emails(domains=just("mydomain.com"))` has no counterpart; hegel-rust's
  `EmailGenerator` exposes no `domains` parameter. Rest of the file ported
  in `tests/hypothesis/nocover_emails.rs`.

- `nocover/test_flatmap.py::test_flatmap_does_not_reuse_strategies` —
  `find_any(s) is not find_any(s)` is a Python object-identity check
  (the `is not` operator). hegel-rust draws return owned/cloned values,
  so identity-distinctness across draws is structural rather than
  observable. Rest of the file ported in
  `tests/hypothesis/nocover_flatmap.rs`.
- `nocover/test_flatmap.py::test_flatmap_has_original_strategy_repr` —
  asserts `repr(ints) in repr(ints_up)` on a `.flatmap`-composed
  strategy. hegel-rust generators have no repr surface.

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

- `test_slippage.py` — every test pins down Hypothesis's behaviour when
  one shrinking pass "slips" into a second distinct failure, which
  requires the `report_multiple_bugs=True` setting plus the Python
  `ExceptionGroup` Hypothesis raises to surface both failures from a
  single run. hegel-rust's `Settings` exposes no `report_multiple_bugs`
  method (already noted by the
  `test_replay_logic.py::test_does_not_shrink_on_replay_with_multiple_bugs`
  skip), always stops on the first failing panic, and has no
  `ExceptionGroup` / `FlakyFailure` counterpart. Several tests
  additionally depend on the public `target()` scoring API, `Phase`
  phase-control, or the internal `MIN_TEST_CALLS` engine constant —
  none of which hegel-rust exposes — so none of the twelve tests are
  portable.

- `test_escalation.py` — every test exercises Python-specific
  exception/traceback machinery with no Rust counterpart:
  `is_hypothesis_file` resolves traceback filenames via Python module
  `__file__` paths (there is no runtime `__file__` on a Rust crate);
  `errors.MultipleFailures` / `BaseExceptionGroup` are PEP 654 exception
  groups (Rust has no exception-group construct, same gap as the
  whole-file skip of `test_exceptiongroup.py`);
  `errors.ThisIsNotARealAttribute...` tests Python module-level
  `__getattr__` raising `AttributeError` (Rust module items resolve at
  compile time); and `InterestingOrigin.from_exception` traverses Python
  `__context__` chains and `BaseExceptionGroup.exceptions` to classify
  the origin of a caught exception — Rust's panic-payload model has no
  exception chaining, no groups, and `src/native/` has no
  `InterestingOrigin` / escalation counterpart.

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

- `test_unittest.py` — every test exercises Python's `unittest` module
  integration: `test_subTest` builds a `unittest.TestSuite` around a
  `unittest.TestCase` subclass and runs it through `unittest.TextTestRunner`
  while calling `self.subTest(...)` inside a `@given`-decorated method;
  `test_given_on_setUp_fails_health_check` applies `@given` to a
  `unittest.TestCase.setUp` hook; `test_subTest_no_self` spawns a Python
  subprocess to run a `unittest.main()` module. Rust has no `unittest`
  module, no `TestCase` / `setUp` / `subTest` surface, and hegel-rust
  tests are closures passed to `Hegel::new(|tc| ...).run()` with no
  class-based test dispatch or per-test fixture hooks — all Python-specific.

- `test_core.py::test_stops_after_max_examples_if_satisfying`,
  `test_core.py::test_stops_after_ten_times_max_examples_if_not_satisfying` —
  both drive `find(strategy, predicate)` and assert exact / bounded
  call counts on the predicate inside `find()`. hegel-rust has no
  `find()` public API, and `Hegel::new(...).run()` re-enters the test
  function for span-mutation attempts (up to 5 per valid case in
  native), so the predicate-call shape these tests pin down isn't
  reproducible through the public Rust surface.
- `test_core.py::test_is_not_normally_default`,
  `test_core.py::test_settings_are_default_in_given` — both inspect
  `settings.default`, a Python module-level mutable global. hegel-rust
  has no `Settings::default` global; settings are constructed
  per-test via `Settings::new()`.
- `test_core.py::test_pytest_skip_skips_shrinking` — relies on
  `pytest.skip()` inside a `@given` body to abort shrinking;
  hegel-rust has no per-test "skip aborts shrinking" mechanism on its
  public API.
- `test_core.py::test_no_such_example` — uses
  `find(..., database_key=b"...")` and asserts `NoSuchExample`; both
  are `find()`-API surface (same gap as the `find()` skips above).
- `test_core.py::test_validates_strategies_for_test_method` — uses
  `st.lists(st.nothing(), min_size=1)`; hegel-rust has no
  `gs::nothing()` public API (same gap as
  `test_given_error_conditions.py::test_raises_unsatisfiable_if_passed_explicit_nothing`).
- `test_core.py::test_non_executed_tests_raise_skipped` — exercises
  `Phase.target/shrink/explain/explicit/reuse` settings and the
  `unittest.SkipTest` raise-on-non-execution behaviour; hegel-rust
  has no public `Phase`/`phases` setting on `Settings`.

- `test_nothing.py::test_list_of_nothing`,
  `test_nothing.py::test_set_of_nothing`,
  `test_nothing.py::test_validates_min_size`,
  `test_nothing.py::test_no_examples` — each uses `st.nothing()`;
  hegel-rust has no `gs::nothing()` public API (same gap as
  `test_core.py::test_nothing_core` and
  `test_generators.py::test_cannot_witness_nothing`).
- `test_nothing.py::test_function_composition`,
  `test_nothing.py::test_tuples_detect_empty_elements`,
  `test_nothing.py::test_fixed_dictionaries_detect_empty_values`,
  `test_nothing.py::test_empty` — each asserts on `st.nothing()`
  propagating through combinators via `.is_empty` strategy
  introspection; hegel-rust has neither `gs::nothing()` nor an
  `.is_empty` attribute on its typed-wrapper generators. The only
  portable test in the file (`test_resampling`) is ported natively.

- `test_numerics.py::test_fuzz_fractions_bounds`,
  `test_numerics.py::test_fraction_addition_is_well_behaved` — both use
  the `fractions()` strategy (Python `fractions.Fraction`). hegel-rust
  has no counterpart for Python's stdlib `Fraction` type and no
  `gs::fractions()` generator.
- `test_numerics.py::test_fuzz_decimals_bounds`,
  `test_numerics.py::test_all_decimals_can_be_exact_floats`,
  `test_numerics.py::test_decimals_include_nan`,
  `test_numerics.py::test_decimals_include_inf`,
  `test_numerics.py::test_decimals_can_disallow_nan`,
  `test_numerics.py::test_decimals_can_disallow_inf`,
  `test_numerics.py::test_decimals_have_correct_places`,
  `test_numerics.py::test_works_with_few_values`,
  `test_numerics.py::test_issue_725_regression`,
  `test_numerics.py::test_issue_739_regression`,
  `test_numerics.py::test_consistent_decimal_error`,
  `test_numerics.py::test_minimal_nonfinite_decimal_is_inf`,
  `test_numerics.py::test_decimals_warns_for_inexact_numeric_bounds` —
  all use the `decimals()` strategy (Python `decimal.Decimal`).
  hegel-rust has no counterpart for Python's stdlib `Decimal` type and
  no `gs::decimals()` generator.
- `test_numerics.py::test_floats_message` (all four parametrize rows) —
  asserts on the exact `InvalidArgument` message Hypothesis emits for
  infinite bounds combined with `allow_infinity=False`. hegel-rust's
  float generator fills in a default `max_value=f64::MAX` (or
  `min_value=f64::MIN`) when `allow_infinity=False` and one bound is
  left unset, which masks the upstream error with a different "no
  floating-point values between …" message; the exact wording the
  test matches against doesn't appear in hegel-rust's output.

- `test_flakiness.py` — port abandoned: parallel port-loop worker
  produced commits on `port/worker-0` that could not be cherry-picked
  cleanly onto the supervisor branch (conflicting concurrent edits to
  `tests/hypothesis/main.rs` plus an untracked `tests/hypothesis/flakiness.rs`);
  left for human inspection on branch `port/worker-0`.

- `test_precise_shrinking.py` (in `nocover/`) — port abandoned: parallel
  port-loop worker produced commits on `port/worker-0` that could not be
  cherry-picked cleanly onto the supervisor branch (merge conflict in
  `tests/hypothesis/main.rs`); left for human inspection on branch
  `port/worker-0`.

- `test_utils.py` (in `conjecture/`) — port abandoned: parallel port-loop
  worker produced commits on `port/worker-0` that could not be cherry-picked
  cleanly onto the supervisor branch (merge conflicts in `src/lib.rs` and
  `src/native/mod.rs`); left for human inspection on branch `port/worker-0`.

- `test_pareto.py` (in `conjecture/`) — port abandoned: parallel port-loop
  worker produced commits on `port/worker-0` that could not be cherry-picked
  cleanly onto the supervisor branch (merge conflicts in `Cargo.toml`,
  `src/lib.rs`, `src/native/mod.rs`, `src/native/runner.rs`, `src/runner.rs`,
  and `tests/test_native.rs` — worker depended on an unmerged native-backend
  feature); left for human inspection on branch `port/worker-0`.

- `test_validation.py::test_errors_when_given_varargs`,
  `test_validation.py::test_varargs_without_positional_arguments_allowed`,
  `test_validation.py::test_errors_when_given_varargs_and_kwargs_with_positional_arguments`,
  `test_validation.py::test_varargs_and_kwargs_without_positional_arguments_allowed`,
  `test_validation.py::test_bare_given_errors`,
  `test_validation.py::test_errors_on_unwanted_kwargs`,
  `test_validation.py::test_errors_on_too_many_positional_args`,
  `test_validation.py::test_errors_on_any_varargs`,
  `test_validation.py::test_can_put_arguments_in_the_middle`,
  `test_validation.py::test_stuff_keyword`,
  `test_validation.py::test_stuff_positional`,
  `test_validation.py::test_too_many_positional`,
  `test_validation.py::test_given_warns_on_use_of_non_strategies`,
  `test_validation.py::test_given_warns_when_mixing_positional_with_keyword`
  — all exercise Python `@given(*args, **kwargs)` argument-passing
  semantics (varargs, default kwargs, mixed positional/keyword,
  type-as-strategy via `@given(bool)`). `#[hegel::test]` takes generators
  directly, so this validation surface has no Rust counterpart.
- `test_validation.py::test_list_unique_and_unique_by_cannot_both_be_enabled`
  — uses `unique_by=key_fn`; hegel-rust's `VecGenerator::unique` only
  accepts a `bool`, so the `unique`/`unique_by` conflict cannot be
  expressed.
- `test_validation.py::test_recursion_validates_base_case`,
  `test_validation.py::test_recursion_validates_recursive_step` —
  `st.recursive()` has no hegel-rust equivalent (already covered by the
  whole-file skip of `test_recursive.py`).
- `test_validation.py::test_cannot_find_non_strategies` — uses Python
  `find()` and treats `bool` as a type-as-strategy; neither has a Rust
  counterpart.
- `test_validation.py::test_valid_sizes` — passes `min_size="0"` /
  `max_size="10"` (strings); Rust's typed `min_size: usize` rejects them
  at compile time, so there is nothing to assert at runtime.
- `test_validation.py::test_check_type_with_tuple_of_length_two`,
  `test_validation.py::test_check_type_suggests_check_strategy`,
  `test_validation.py::test_check_strategy_might_suggest_sampled_from`
  — exercise Python-only internal helpers
  (`hypothesis.internal.validation.check_type`,
  `hypothesis.strategies._internal.strategies.check_strategy`).
- `test_validation.py::test_warn_on_strings_matching_common_codecs` —
  exercises a Hypothesis warning fired when `st.text('ascii')` is
  called with a codec-like positional alphabet string. hegel-rust's
  `gs::text()` separates `.alphabet()` and `.codec()` into distinct
  methods, so the codec/alphabet ambiguity the warning targets doesn't
  exist.

- `test_control.py::test_cannot_cleanup_with_no_context`,
  `test_control.py::test_cannot_event_with_no_context`,
  `test_control.py::test_cleanup_executes_on_leaving_build_context`,
  `test_control.py::test_can_nest_build_context`,
  `test_control.py::test_does_not_suppress_exceptions`,
  `test_control.py::test_suppresses_exceptions_in_teardown`,
  `test_control.py::test_runs_multiple_cleanup_with_teardown`,
  `test_control.py::test_raises_error_if_cleanup_fails_but_block_does_not`,
  `test_control.py::test_raises_if_current_build_context_out_of_context`,
  `test_control.py::test_current_build_context_is_current` — exercise
  Hypothesis's `BuildContext` context-manager and the `cleanup()` /
  `current_build_context()` public functions; hegel-rust manages test
  context via a thread-local bool flag with no openable/nestable
  context-manager surface, no cleanup-hook registry, and no
  `current_build_context` accessor.
- `test_control.py::test_raises_if_note_out_of_context` — standalone
  `hypothesis.note()` is a free function that checks for an active
  context at call time; in hegel-rust `note` is `TestCase::note`, so
  calling it outside a test context is prevented by the type system
  (no `TestCase` to call it on), leaving nothing to assert at runtime.
- `test_control.py::test_deprecation_warning_if_assume_out_of_context`,
  `test_control.py::test_deprecation_warning_if_reject_out_of_context`
  — standalone `assume()` / `reject()` are free functions in
  Hypothesis; in hegel-rust they are `TestCase::assume` / `TestCase::reject`
  methods, so the out-of-context deprecation path is unreachable.
- `test_control.py::test_prints_all_notes_in_verbose_mode`,
  `test_control.py::test_note_pretty_prints` — both depend on
  `hypothesis.reporting.with_reporter` to redirect note output into a
  list or captured stream during generation; hegel-rust's `tc.note()`
  is verbosity-independent and only prints on the final failing replay
  (same gap as the individually-skipped `test_reporting.py` tests
  above), and there is no public reporter-override API. `test_note_pretty_prints`
  also relies on Python `@dataclass` auto-derived `__repr__` for the
  "pretty-printed" output, which has no Rust counterpart.
- `test_control.py::test_can_convert_non_weakref_types_to_event_strings`
  — exercises the internal `_event_to_string` helper's handling of
  Python weak-reference semantics; hegel-rust has no `event()` public
  API (see `test_cannot_event_with_no_context` above) and no
  weakref-based event cache.

- `test_randoms.py` — every test exercises Python's stdlib `random.Random`
  interface, which `HypothesisRandom` inherits from: distribution methods
  (`betavariate`, `gauss`, `normalvariate`, `lognormvariate`,
  `vonmisesvariate`, `paretovariate`, `weibullvariate`, `binomialvariate`,
  `gammavariate`, `expovariate`, `triangular`, `uniform`), sequence
  helpers (`choice`, `choices`, `sample`, `shuffle`, `randrange`,
  `randint`), bit/byte helpers (`getrandbits`, `randbytes`, `_randbelow`,
  `random` returning a float in `[0, 1)`), state-serialization methods
  (`seed`, `getstate`, `setstate`), Python `copy.copy()` semantics, and
  Hypothesis-specific extensions to that class hierarchy
  (`isinstance(rnd, TrueRandom)`, `note_method_calls=True` to capture the
  method-call log, `ArtificialRandom`/`HypothesisRandom` class
  introspection via `dir()`/`__module__`, internal
  `ConjectureData.for_choices([])` plus `data.states_for_ids` setup).
  hegel-rust's `gs::randoms()` produces `HegelRandom`, which only
  implements the `rand` crate's `TryRng` trait (`next_u32` / `next_u64`
  / `fill_bytes`); none of the Python stdlib `Random` distribution,
  sequence, state, or bit-level methods exist on it, and there is no
  `note_method_calls` / Hypothesis-class-hierarchy surface. The
  rand-crate-shaped equivalent is already exercised by
  `tests/test_randoms.rs`.

- `test_provisional_strategies.py::test_url_fragments_contain_legal_chars`
  — imports the private `_url_fragments_strategy` object and the
  `FRAGMENT_SAFE_CHARACTERS` constant from `hypothesis.provisional`;
  hegel-rust exposes neither a URL-fragment generator nor the
  fragment-safe-characters set as public API.
- `test_provisional_strategies.py::test_invalid_domain_arguments` rows
  with `max_length ∈ {-1, 4.0}` or any `max_element_length` value —
  hegel-rust's `DomainGenerator::max_length` takes `usize` (so negative
  and float values are unrepresentable) and exposes no
  `max_element_length` setter; only `max_length ∈ {0, 3, 256}` invalid
  rows port.
- `test_provisional_strategies.py::test_valid_domains_arguments` rows
  with any `max_element_length` value — same gap; only
  `max_length ∈ {None, 4, 8, 255}` is portable.

- `test_import.py` (in `tests/numpy/`) — numpy-extra integration tests:
  `test_hypothesis_is_not_the_first_to_import_numpy` checks Python's
  `sys.modules` to assert Hypothesis defers numpy import, and
  `test_wildcard_import` exercises `from hypothesis.extra.numpy import *`.
  Both target the numpy integration and use Python-specific facilities
  (`sys.modules`, wildcard import) with no Rust counterpart.

- `test_deprecation.py` (in `tests/numpy/`) — numpy-extra integration
  tests. Every test exercises `hypothesis.extra.numpy`
  (`nps.basic_indices` with an array shape and `min_dims`/`max_dims`
  bounds) and asserts that misusing it emits a
  `HypothesisDeprecationWarning`. hegel-rust has no numpy integration
  or counterpart for numpy basic-index generation.

- `test_argument_validation.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every parametrized case calls a strategy on the
  `xps` namespace built by `hypothesis.extra.array_api.make_strategies_namespace(xp)`
  (`xps.arrays`, `xps.array_shapes`, `xps.from_dtype`, `xps.integer_dtypes`,
  `xps.floating_dtypes`, `xps.complex_dtypes`, `xps.valid_tuple_axes`,
  `xps.broadcastable_shapes`, `xps.mutually_broadcastable_shapes`,
  `xps.indices`) against an Array-API-conforming array module (`mock_xp`,
  `numpy.array_api`, or `array-api-strict`); the standalone test also
  validates `make_strategies_namespace` itself. hegel-rust has no Array
  API integration or counterpart for array/dtype/shape-generation tied
  to an external array-module namespace.

- `test_scalar_dtypes.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every test requires the `xp` / `xps` fixtures (an
  Array-API-conforming array module and the strategies namespace built
  by `hypothesis.extra.array_api.make_strategies_namespace(xp)`), and
  exercises the dtype strategies on that namespace
  (`xps.integer_dtypes`, `xps.unsigned_integer_dtypes`,
  `xps.floating_dtypes`, `xps.real_dtypes`, `xps.complex_dtypes`,
  `xps.scalar_dtypes`, `xps.numeric_dtypes`) against array-module
  dtypes (`xp.bool`, `xp.int8`, …). hegel-rust has no Array API
  integration or counterpart for array-module dtype generation.

- `test_arrays.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every test requires the `xp` / `xps` fixtures (an
  Array-API-conforming array module and the strategies namespace built
  by `hypothesis.extra.array_api.make_strategies_namespace(xp)`), and
  exercises `xps.arrays` (with `xps.array_shapes`, `xps.scalar_dtypes`,
  `xps.from_dtype`, etc.) producing array-module arrays whose dtype,
  shape, and element values are asserted via array-module operations
  (`xp.all`, `xp.any`, `xp.asarray`, `xp.iinfo`, `xp.unique_values`,
  `xp.sum`, `xp.logical_or`, `xp.nonzero`, `xp.isnan`, `xp.astype`,
  `xp.arange`, `xp.sort`, `xp.nan`, `xp.bool`, `xp.int8`, …).
  hegel-rust has no Array API integration or counterpart for
  array/dtype/shape-aware array generation.

- `test_indices.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every test requires the `xp` / `xps` fixtures (an
  Array-API-conforming array module and the strategies namespace built
  by `hypothesis.extra.array_api.make_strategies_namespace(xp)`), and
  exercises `xps.indices` (with `xps.array_shapes`) to generate
  array-module indexers; `test_generate_valid_indices` additionally
  constructs and indexes into array-module arrays via `xp.zeros`,
  `xp.reshape`, `xp.arange` and `array[indexer]`. hegel-rust has no
  Array API integration or counterpart for array-shape-aware indexer
  generation tied to an external array module.

- `test_partial_adoptors.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every test builds a mock array-module via
  `copy(hypothesis.extra.array_api.mock_xp)` with attributes selectively
  removed, then passes it to
  `hypothesis.extra.array_api.make_strategies_namespace(xp, api_version=...)`
  and asserts on the resulting `HypothesisWarning` / `InvalidArgument`
  behaviour of `xps.from_dtype`, `xps.arrays`, `xps.scalar_dtypes`,
  `xps.numeric_dtypes`, `xps.integer_dtypes`, `xps.unsigned_integer_dtypes`,
  `xps.floating_dtypes`, `xps.real_dtypes`, `xps.complex_dtypes` against
  a partially-adopting Array API namespace. hegel-rust has no Array API
  integration or counterpart for strategy-namespace construction over an
  external array module.

- `test_from_dtype.py` (in `tests/array_api/`) — array-api-extra
  integration tests. Every test requires the `xp` / `xps` fixtures (an
  Array-API-conforming array module and the strategies namespace built
  by `hypothesis.extra.array_api.make_strategies_namespace(xp)`), and
  exercises `xps.from_dtype` against array-module dtypes (`xp.float32`,
  `"int8"`, `"uint8"`, …) with reusable-value / castable-builtin /
  bounds-kwargs / subnormal-FTZ assertions routed through
  `find_castable_builtin_for_dtype`, `width_smallest_normals`, and
  `flushes_to_zero` on the array module. hegel-rust has no Array API
  integration or counterpart for dtype-aware scalar generation tied to
  an external array module.

- `test_strategies_namespace.py` (in `tests/array_api/`) — array-api-extra
  integration tests. All three tests exercise `hypothesis.extra.array_api`
  internals (`make_strategies_namespace`, `mock_xp`, `NOMINAL_VERSIONS`,
  `_args_to_xps` `WeakValueDictionary` cache) and rely on pytest's
  `monkeypatch` fixture to swap module-level state. The tests assert on
  `SearchStrategy` instances returned by `xps.complex_dtypes()` on a
  namespace backed by a mock Array API module. hegel-rust has no Array API
  integration or counterpart for strategy-namespace construction or caching
  over an external array module.

- `test_gen_data.py` (in `tests/numpy/`) — numpy-extra integration tests.
  Every test exercises `hypothesis.extra.numpy` (`nps.arrays`,
  `nps.array_shapes`, `nps.broadcastable_shapes`, `nps.from_dtype`,
  `nps.mutually_broadcastable_shapes`, `nps.basic_indices`,
  `nps.integer_array_indices`, `nps.valid_tuple_axes`, etc.) and numpy
  dtypes/arrays (`np.dtype`, `np.ndarray`, `np.zeros`, `np.broadcast_to`).
  hegel-rust has no numpy integration or counterpart for numpy
  array/dtype/shape/index generation.

- `test_gufunc.py` (in `tests/numpy/`) — numpy-extra integration tests
  for generalized ufunc signatures. Every test exercises
  `hypothesis.extra.numpy` (`nps.mutually_broadcastable_shapes`,
  `nps.arrays`) and the internal `_hypothesis_parse_gufunc_signature`
  on numpy gufunc signatures, and asserts on results of `np.matmul`
  and `np.einsum`. hegel-rust has no numpy integration or counterpart
  for gufunc-signature / broadcastable-shape generation.

- `test_narrow_floats.py` (in `tests/numpy/`) — numpy-extra integration
  tests. Every test exercises `hypothesis.extra.numpy` (`arrays`,
  `from_dtype`, `integer_dtypes`) and numpy dtypes (`np.float16`,
  `np.float32`, `np.float64`, `np.dtype(...).itemsize`, `np.isnan`,
  `np.all`). hegel-rust has no numpy integration or counterpart for
  numpy-dtype-aware array/scalar generation.

- `test_fill_values.py` (in `tests/numpy/`) — numpy-extra integration
  tests. Every test exercises `hypothesis.extra.numpy` (`arrays` with
  `object`/`"uint64"`/`float` dtypes and `fill=`/`elements=` strategies)
  to generate numpy arrays, asserting on `set`/`.all()` over array
  contents. hegel-rust has no numpy integration or counterpart for
  numpy array generation with fill-value semantics.

- `test_floor_ceil.py` (in `tests/numpy/`) — numpy-extra integration
  test. The single parametrized test constructs `np.array(eval(value))`
  for large ±2**53/63/64 integer cases and asserts that
  `hypothesis.internal.compat.floor` / `ceil` return exact Python `int`
  (avoiding numpy's float rounding) where `math.floor` / `math.ceil`
  would not. The test's subject is the numpy-specific wrapper in
  `hypothesis.internal.compat`; hegel-rust has no numpy integration and
  Rust's native i128/u64 types have no analogous rounding pitfall to
  guard against.

- `test_randomness.py` (in `tests/numpy/`) — numpy-extra integration
  test. The single test calls `np.random.get_state()`,
  `np.random.bytes(10)`, and `np.testing.assert_array_equal` to assert
  that Hypothesis restores numpy's global PRNG state around a `@given`
  run. The test's subject is the numpy PRNG-seeding hook in Hypothesis;
  hegel-rust has no numpy integration and no global numpy-PRNG state to
  seed/restore.

- `test_from_type.py` (in `tests/numpy/`) — numpy-extra integration
  tests. Every test exercises `st.from_type(...)` against numpy and
  `hypothesis.extra.numpy` types (`np.dtype`, `np.object_`, `np.void`,
  `np.ndarray[typ]`, `NDArray`, `ArrayLike`, `_NestedSequence`,
  `_SupportsArray`) and asserts numpy-specific shape / dtype / coercion
  behaviour (`isinstance(x, np.ndarray)`, `arr.dtype.type == typ`,
  `np.array(arr_like)`, `np.asarray(arr).dtype.kind`). hegel-rust has
  no numpy integration or counterpart for resolving numpy types via
  `from_type`.

- `test_series.py` (in `tests/pandas/`) — pandas-extra integration tests.
  Every test exercises `hypothesis.extra.pandas` (`pdst.series`,
  `pdst.range_indexes`) and pandas/numpy dtypes (`np.dtype("O")`,
  `pd.core.arrays.integer.Int8Dtype`). hegel-rust has no pandas
  integration or counterpart for pandas `Series`/dtype generation.

- `test_indexes.py` (in `tests/pandas/`) — pandas-extra integration tests.
  Every test exercises `hypothesis.extra.pandas` (`pdst.indexes`,
  `pdst.range_indexes`) and pandas/numpy dtypes (`np.dtype("int64")`,
  `np.dtype("int32")`, `pandas.RangeIndex`, `pandas.Index`), and
  `hypothesis.extra.numpy` (`npst.boolean_dtypes`, `npst.integer_dtypes`,
  `npst.floating_dtypes`, `npst.datetime64_dtypes`,
  `npst.timedelta64_dtypes`, `npst.from_dtype`). hegel-rust has no pandas
  or numpy integration or counterpart for pandas `Index`/`RangeIndex`
  generation.

- `test_data_frame.py` (in `tests/pandas/`) — pandas-extra integration tests.
  Every test exercises `hypothesis.extra.pandas` (`pdst.data_frames`,
  `pdst.columns`, `pdst.column`, `pdst.range_indexes`) and pandas/numpy
  types (`np.dtype`, `pd.core.arrays.integer.Int8Dtype`, `np.nan`,
  `hypothesis.extra.numpy` scalar dtypes). hegel-rust has no pandas or
  numpy integration or counterpart for pandas `DataFrame`/dtype generation.

- `test_contracts.py` (in `tests/dpcontracts/`) — dpcontracts-extra
  integration tests. Every test exercises `hypothesis.extra.dpcontracts`
  (`fulfill`), which wraps functions decorated with the third-party
  Python `dpcontracts` library's `@require(...)` precondition decorator
  so Hypothesis can filter inputs that violate the contract. hegel-rust
  has no `dpcontracts` integration or counterpart for design-by-contract
  precondition-aware generation.

- `test_dateutil_timezones.py` (in `tests/datetime/`) — dateutil-extra
  integration tests. Every test exercises `hypothesis.extra.dateutil`
  (`timezones`), the `datetimes(timezones=...)` / `times(timezones=...)`
  keyword argument that yields timezone-aware `datetime.time` /
  `datetime.datetime` values, and the third-party Python `dateutil`
  library's `tz.UTC`, `tz.gettz`, `tz.datetime_exists`, and
  `zoneinfo.get_zonefile_instance` APIs, plus the internal
  `hypothesis.strategies._internal.datetime.datetime_does_not_exist`
  helper. hegel-rust's `gs::datetimes()` / `gs::times()` generators
  produce plain ISO 8601 strings with no `timezones` parameter and no
  tzinfo concept, and hegel-rust has no `dateutil` integration or
  counterpart for zoneinfo-backed timezone-aware datetime generation.

- `test_pytz_timezones.py` (in `tests/datetime/`) — pytz-extra integration
  tests. Every test exercises `hypothesis.extra.pytz.timezones()` (which
  yields `pytz` timezone objects), the `datetimes(timezones=...)` /
  `times(timezones=...)` keyword argument that yields timezone-aware
  `datetime.datetime` / `datetime.time` values, and the third-party Python
  `pytz` library's `pytz.UTC`, `pytz.timezone()`, and `pytz.all_timezones`
  APIs, plus the internal
  `hypothesis.strategies._internal.datetime.datetime_does_not_exist` helper.
  hegel-rust's `gs::datetimes()` / `gs::times()` generators produce plain ISO
  8601 strings with no `timezones` parameter and no tzinfo concept, and
  hegel-rust has no `pytz` integration or counterpart for pytz-backed
  timezone-aware datetime generation.

- `test_zoneinfo_timezones.py` (in `tests/datetime/`) — zoneinfo-backed
  timezone tests. Every test exercises `st.timezones()` (which yields
  `zoneinfo.ZoneInfo` instances), `st.timezone_keys()` (with the
  `allow_prefix` argument), and `st.datetimes(..., timezones=...)` for
  timezone-aware `datetime` values, plus Python's stdlib `zoneinfo`
  module and `InvalidArgument` validation on `.validate()`. hegel-rust's
  `gs::datetimes()` produces plain ISO 8601 strings with no `timezones`
  parameter and no tzinfo concept, and has no `timezones()` /
  `timezone_keys()` generator counterpart.

- `test_given_models.py` (in `tests/django/toystore/`) — django-extra
  integration tests. Every test exercises `hypothesis.extra.django`
  (`from_model`, `register_field_strategy`, `TestCase`,
  `TransactionTestCase`) to construct Django ORM model instances
  (`Company`, `Store`, `Customer`, `ManyNumerics`, `OddFields`, `User`,
  etc.), calls `instance.full_clean()` / `instance.pk` /
  `Model.objects.all()`, and drives Django's test-case transaction
  rollback machinery. hegel-rust has no Django (or Python ORM)
  integration — no `from_model` equivalent, no ORM-aware model/field
  generator, and no Django-settings/transaction harness.

- `test_basic_configuration.py` (in `tests/django/toystore/`) — django-extra
  integration tests. Every test subclasses `hypothesis.extra.django`'s
  `SimpleTestCase` / `TestCase` / `TransactionTestCase`, creates
  `Company` ORM model rows via `Company.objects.create(...)`, relies on
  Django's transaction-rollback test-case machinery to keep the DB a
  blank slate across `@given` replays, and asserts that `@given` raises
  `InvalidArgument` when used on a non-Hypothesis Django `TestCase`
  subclass. hegel-rust has no Django integration, no ORM model / test-
  case harness, and no `hypothesis.extra.django`-style
  `@given`-compatible test-case base class to validate against.

- `test_given_forms.py` (in `tests/django/toystore/`) — django-extra
  integration tests. Every test exercises `hypothesis.extra.django`
  (`from_form`, `from_field`, `register_field_strategy`, `TestCase`)
  against `django.forms` `Form` subclasses (`CustomerForm`,
  `ManyNumericsForm`, `BasicFieldForm`, `EmailFieldForm`,
  `URLFieldForm`, `RegexFieldForm`, `UUIDFieldForm`,
  `ChoiceFieldForm`, `FileFieldsForm`, `StoreForm`,
  `MultipleCompaniesForm`, etc.), asserting `form.is_valid()` and
  drilling into `form.data` / `form.files`. The
  `TestFormsWithModelChoices` class additionally creates `Company`
  ORM model rows via `Company.objects.create(...)` and uses
  `forms.ModelChoiceField` / `forms.ModelMultipleChoiceField` against
  a Django queryset. hegel-rust has no Django integration, no
  `from_form` / `from_field` form-strategy generator, and no
  `register_field_strategy` hook for custom Django form fields.

- `test_redis_exampledatabase.py` (in `tests/redis/`) — redis-extra
  integration tests. Every test exercises
  `hypothesis.extra.redis.RedisExampleDatabase`, which wraps a
  `redis.Redis` / `fakeredis.FakeRedis` client to persist failing examples
  in a Redis server (`save`/`fetch`/`move`/`delete`, pub/sub listener
  notifications via `db._pubsub.get_message`, `expire_after` timedelta,
  `key_prefix`/`listener_channel` configuration). The file imports
  `fakeredis.FakeRedis` for its test server, and the stateful
  `DatabaseComparison` machine drives the Redis-backed database side by
  side with `InMemoryExampleDatabase` to cross-check behaviour.
  hegel-rust has no redis / fakeredis integration and no
  `RedisExampleDatabase` counterpart — the only persistent database
  backend is `Database::Path(...)` filesystem storage.

- `test_attrs.py` (in `tests/attrs/`) — port abandoned: parallel
  port-loop worker produced commits on `port/worker-0` that could not
  be cherry-picked cleanly onto the supervisor branch (post-rebase
  integration failed); left for human inspection on branch
  `port/worker-0`.

- `test_inference.py` (in `tests/attrs/`) — every test exercises
  Hypothesis's `attrs`-library integration: classes built with `@attr.s`
  / `attr.ib(type=..., validator=..., converter=..., default=...)` and
  resolved via `st.builds(cls)` / `st.from_type(cls)` using inference
  from `attr.validators.instance_of` / `optional` / `in_` and
  `attr.Factory(...)` defaults, plus attrs-specific private-attribute
  (`_x`) and `alias=` resolution rules. hegel-rust has no `attrs`
  counterpart and no `attrs`-aware strategy-inference path
  (`#[derive(Generate)]` is the Rust analog but resolves Rust types,
  not Python `attrs` declarations).

- `test_ghostwriter_cli.py` (in `tests/ghostwriter/`) — every test
  invokes the `hypothesis write` CLI via `subprocess.run(...)` and
  compares its stdout against code generated by the
  `hypothesis.extra.ghostwriter` Python library (`fuzz`, `idempotent`,
  `equivalent`, `roundtrip`, `magic`, `binary_operation`). The
  ghostwriter is a Python-specific public-API tool that discovers
  functions via Python module introspection (dotted attribute paths
  like `mycode.MyClass.my_staticmethod`, `importlib.import_module`,
  `__init__.py` layout) and emits Python Hypothesis test source.
  hegel-rust has no ghostwriter CLI / test-scaffold generator
  counterpart.

- `test_ghostwriter.py` (in `tests/ghostwriter/`) — every test drives
  the `hypothesis.extra.ghostwriter` Python library directly
  (`ghostwriter.fuzz`, `magic`, `binary_operation`, `idempotent`,
  `equivalent`, `roundtrip`, `_check_except`, `_check_style`,
  `_valid_syntax_repr`) and asserts on the emitted Python source (parsed
  via `ast`, `exec`'d into a namespace, inspected for `@given(...)`
  clauses and import lines). The ghostwriter inspects Python function
  signatures (type hints, defaults, `attr.s` classes, `re.Pattern`,
  `KeysView`, `ForwardRef`, unittest-style scaffolding, etc.) and emits
  Python Hypothesis test source; hegel-rust has no ghostwriter /
  test-scaffold generator counterpart.

- `test_expected_output.py` (in `tests/ghostwriter/`) — golden-master
  tests for `hypothesis.extra.ghostwriter`: each parametrised case calls
  `ghostwriter.fuzz`, `ghostwriter.magic`, `ghostwriter.idempotent`,
  `ghostwriter.equivalent`, `ghostwriter.binary_operation`, or
  `ghostwriter.roundtrip`, compares the emitted string to a stored
  `.txt` file, then `exec()`s the generated Python source to check for
  `SyntaxError`/`NameError`. The tests also drive numpy ufunc/gufunc
  signatures and use `black` to re-format the output. The entire file
  exercises Python-only facilities (ghostwriter, `exec`, `ast`, `black`,
  `numpy`); hegel-rust has no ghostwriter / test-scaffold generator
  counterpart.

- `test_provider.py` (in `conjecture/`) — every test exercises Hypothesis's
  public backend/provider registration system: the `PrimitiveProvider`
  base class that users subclass to supply custom data generation
  (`PrngProvider`, `TrivialProvider`, `RealizeProvider`, etc.), the
  `with_register_backend(name, cls)` / `AVAILABLE_PROVIDERS` name-based
  registry, the `backend="name"` `@settings` parameter that selects a
  registered provider at runtime, and the associated provider-plugin
  surface (`provider.realize`, `provider.lifetime` = `"test_case"` /
  `"test_function"`, `observe_test_case` / `observe_information_messages`,
  `per_test_case_context_manager` / `on_observation`,
  `BackendCannotProceed`, `FlakyBackendFailure`, `run_conformance_test`,
  `ConjectureData(provider=..., provider_kw=...)`). hegel-rust picks its
  backend at compile time (server vs `feature = "native"`) and exposes no
  pluggable-provider public API: no `backend=` setting (same gap noted in
  the `test_settings.py` skip), no `register_backend` entry point, no
  `PrimitiveProvider` class to subclass, and no provider-lifetime /
  realize / observe / conformance machinery.

- `test_shrinking_interface.py` (in `conjecture/`) — all three tests
  target the Python-specific introspection/formatting surface of the
  `Integer` shrinker rather than its shrinking behaviour.
  `test_includes_name_in_repr_if_set` and
  `test_normally_contains_no_space_for_name` assert on Python `__repr__`
  dunder output (`"Integer('hi there', initial=10, current=10)"` /
  `"Integer(initial=10, current=10)"`); `test_debug_output` uses
  `capture_out` to check that `Integer.shrink(..., debug=True)` prints
  Python-formatted log lines containing `"initial=10"` and
  `"shrinking to 0"` to stdout. hegel-rust's `IntegerShrinker` in
  `src/native/shrinker/value_shrinkers.rs` has no `name=` / `debug=`
  constructor keywords, no `__repr__`-equivalent surface, and no reason
  to emit the exact Python debug-log format.

- `conjecture/test_forced.py::test_forced_many` — exercises
  `cu.many(data, min_size=…, max_size=…, forced=N)` where `forced` sets
  the total collection count. Native `ManyState::new(min_size, max_size)`
  has no forced-count parameter, and `schema::many_more` only forces the
  per-step boolean based on min/max bounds; there is no public entry
  point for constructing a forced-count `ManyState`.
- `conjecture/test_forced.py::test_forced_with_large_magnitude_integers`
  — uses `2**127 + 1` as a bound and forced value, which exceeds
  `i128::MAX`. Native `draw_integer` takes `i128` bounds and cannot
  represent the Python-bignum range this test exercises.
- `conjecture/test_forced.py::test_forced_values` (the
  `@given(choice_types_constraints(use_forced=True))` branch and the
  four `@example("integer", {"shrink_towards":…, "weights":{…}, "forced":…})`
  rows) — requires porting
  `hypothesis.internal.conjecture.provider_conformance.choice_types_constraints`
  / `constraints_strategy` (no native counterpart) and extending
  `draw_integer` with `shrink_towards` / `weights` constraints (native
  `draw_integer(min, max)` accepts neither). The remaining `@example`
  rows (`boolean`, `float`) are ported.

- `conjecture/test_provider_contract.py::test_provider_contract_bytestring`
  — drives a `ConjectureData` via `BytestringProvider` (feeds raw bytes
  to power the draws). No `src/native/` counterpart: native's
  prefix-based constructor `NativeTestCase::for_choices` takes concrete
  `ChoiceValue`s, not a byte string, and there is no pluggable-provider
  surface. The `HypothesisProvider` contract (`draw_*(constraints)` →
  value satisfies `choice_permitted(value, constraints)`) is ported
  in `tests/hypothesis/conjecture_provider_contract.rs`.
- `conjecture/test_provider_contract.py::test_provider_contract[URandomProvider]`
  — the `URandomProvider` parametrize row reads bytes from
  `/dev/urandom` to drive draws. No `src/native/` counterpart
  (no pluggable-provider surface, and no urandom-backed
  `NativeTestCase` constructor). The `HypothesisProvider` row is
  ported.

- `conjecture/test_choice.py::test_compute_max_children_and_all_children_agree`,
  `::test_compute_max_children_unbounded_integer_ranges`,
  `::test_all_children_are_permitted_values`,
  `::test_choice_to_index_injective` — iterate every valid value for a
  `ChoiceKind` via `datatree.all_children`. `compute_max_children` is
  ported but the enumerator is not; no `src/native/` counterpart exists.
  (`test_choice_from_value_injective` is *not* in this cluster — it
  iterates `range(cap)` via `from_index` only, and is ported.)
- `conjecture/test_choice.py::test_cannot_modify_forced_nodes` — asserts
  that `ChoiceNode.copy(with_value=…)` on a forced node raises
  `AssertionError`. The native `ChoiceNode::with_value` propagates
  `was_forced` unchanged rather than panicking. (The non-forced branch of
  `test_copy_choice_node` is ported.)
- `conjecture/test_choice.py::test_choice_node_equality` — asserts
  `node != 42` (cross-type). Rust `PartialEq` is type-restricted at compile
  time, so mixed-type comparison is unrepresentable.
- `conjecture/test_choice.py::test_trivial_nodes`,
  `::test_nontrivial_nodes`, `::test_conservative_nontrivial_nodes` —
  the `.trivial` half of these tests is ported (native
  `ChoiceNode::trivial` is a faithful port of upstream). The
  `minimal(values()) == node.value` shrinking-invariant half is NOT
  ported — it needs a shrinking/generator harness beyond what's built.
  Rows parametrised on `shrink_towards` are dropped (no native field).
  Tests ported in full: `test_forced_nodes_are_trivial`.
- `conjecture/test_choice.py::test_choice_node_is_hashable` — native
  `ChoiceNode` does not implement `std::hash::Hash`.
- `conjecture/test_choice.py::test_choices_size_positive` —
  `choices_size([values])` (byte-width of a choice sequence) has no
  `src/native/` counterpart.
- `conjecture/test_choice.py::test_node_template_count`,
  `::test_node_template_to_overrun`, `::test_node_template_single_node_overruns`,
  `::test_node_template_simplest_is_actually_trivial`,
  `::test_node_template_overrun` — use `ChoiceTemplate("simplest", count=n)`
  as a prefix primitive. `NativeTestCase::for_choices` accepts only concrete
  `ChoiceValue`s; there is no template variant in native.
- `conjecture/test_choice.py::test_choice_indices_are_positive` —
  trivially satisfied by Rust's type system: `ChoiceKind::{to,from}_index`
  returns `BigUint` (unsigned), so the non-negativity assertion is a
  tautology with no observable behaviour to check.
- `conjecture/test_choice.py::test_compute_max_children` /
  `::test_compute_max_children_is_positive` (rows parameterised on
  `smallest_nonzero_magnitude`, `weights`, or `shrink_towards`) — native
  `FloatChoice` has no `smallest_nonzero_magnitude`, and native
  `IntegerChoice` has no `weights` / `shrink_towards`. The remaining
  rows are ported.

- `conjecture/test_shrinker.py::test_can_pass_to_an_indirect_descendant`,
  `::test_can_reorder_spans` — test pass-level behaviour
  (`pass_to_descendant`, `reorder_spans`) that consumes span metadata;
  the native shrinker's passes don't use span structure, so the full
  pipeline won't reach the same minimum.
- `conjecture/test_shrinker.py::test_dependent_block_pairs_is_up_to_shrinking_integers`
  — uses `hypothesis.internal.conjecture.utils.Sampler` to pick bit-widths,
  with no native counterpart.
- `conjecture/test_shrinker.py::test_zig_zags_quickly_with_shrink_towards`,
  `::test_redistribute_numeric_pairs_shrink_towards_explicit_integer`,
  `::test_redistribute_numeric_pairs_shrink_towards_explicit_float`,
  `::test_redistribute_numeric_pairs_shrink_towards_explicit_combined`,
  `::test_redistribute_numeric_pairs_shrink_towards_integer` — each
  uses `data.draw_integer(..., shrink_towards=...)`; the native
  `draw_integer(min, max)` accepts no `shrink_towards` constraint.
- `conjecture/test_shrinker.py::test_can_simultaneously_lower_non_duplicated_nearby_integers`
  — fixates on `lower_integers_together`; the native shrinker has no
  equivalent "simultaneously lower adjacent non-duplicated integers"
  pass, and the full pipeline won't lower them in lock-step.
- `conjecture/test_shrinker.py::test_redistribute_with_forced_node_integer`
  — asserts that `redistribute_numeric_pairs` preserves a
  `forced=10` node; the full native pipeline may lower the non-forced
  side via unrelated passes, which is the opposite of what the test
  checks.
- `conjecture/test_shrinker.py::test_redistribute_numeric_pairs` —
  uses Hypothesis's `@given(ChoiceNode, ChoiceNode, ...)` with
  `ChoiceNode` constructed from `type`, `value`, `constraints`
  dicts. The native `ChoiceNode` shape is a plain struct without the
  dynamic "constraints-dict" surface, and we have no generator for
  random node pairs.
- `conjecture/test_shrinker.py::test_lower_duplicated_characters_across_choices`
  — fixates on `lower_duplicated_characters`; the native shrinker's
  `redistribute_string_pairs` has different factoring and the full
  pipeline doesn't necessarily drive duplicated characters across
  non-adjacent choices to the same minimum.
- `conjecture/test_shrinker.py::test_deletion_and_lowering_fails_to_shrink`,
  `::test_permits_but_ignores_raising_order` — monkey-patch
  `ConjectureRunner.generate_new_examples` / `Shrinker.shrink` to
  control engine first-example and shrink path. No monkey-patching
  entry point in the native engine.
- `conjecture/test_shrinker.py::test_node_programs_are_adaptive`,
  `::test_will_let_fixate_shrink_passes_do_a_full_run_through` — use
  `shrinker.node_program("X" * i)` (adaptive deletion pass) or
  `StopShrinking` / `max_stall` control surface. Neither the adaptive
  node-program pass nor the `max_stall`/`StopShrinking` API exists in
  the native shrinker.
- `conjecture/test_shrinker.py::test_will_terminate_stalled_shrinks` —
  asserts `shrinker.calls <= 1 + 2 * shrinker.max_stall`; native
  `Shrinker` has no `calls` counter or `max_stall` knob.
- `conjecture/test_shrinker.py::test_alternative_shrinking_will_lower_to_alternate_value`
  — calls `shrinker.initial_coarse_reduction()`, a Python-specific
  coarse-grained pre-pass. The asserted final state
  (`shrinker.choices[0] == 0`) depends on the pre-pass discovering an
  alternate interesting origin via stateful test-body scratch, which
  the full `Shrinker::shrink()` pipeline doesn't trigger from the
  initial `(1, b"hello world")`.
- `conjecture/test_shrinker.py::test_silly_shrinker_subclass` —
  subclasses the generic base-class
  `hypothesis.internal.conjecture.shrinking.common.Shrinker` with a
  no-op `run_step`. Hegel's value-shrinker ports (`IntegerShrinker`,
  `OrderingShrinker`) are concrete structs with fixed `run_step`
  implementations and no subclass-pluggable base class.

The `conjecture/test_engine.py::*` entries below are parked awaiting
per-cluster ports onto the `NativeConjectureRunner` wrapper that now
lives in `hegel::__native_test_internals`. The wrapper's individual
attributes are `todo!()` stubs in
`src/native/conjecture_runner.rs`; each cluster's follow-up TODO.yaml
entry names the attributes its port lands. Removing each bullet below
is acceptance for the corresponding follow-up.

- `conjecture/test_engine.py::test_does_not_save_on_interrupt` —
  asserts the runner does not persist to the database when the test
  function raises `KeyboardInterrupt`. Rust has no direct
  `KeyboardInterrupt` analog (panics are distinct from interrupts,
  and the native engine's `run_test_fn` deliberately resumes
  unwinding on anything that isn't a `MarkPanic` / `STOP_TEST_PANIC`
  sentinel, which matches Python's behaviour of propagating
  unexpected exceptions). No Rust counterpart to exercise.
- `conjecture/test_engine.py::test_saves_on_skip_exceptions_to_reraise`
  — asserts the runner saves `pytest.skip()`-style exceptions to the
  database so they short-circuit future runs. Rust tests don't have
  a `pytest.skip`-equivalent propagating through the engine; the
  native runner treats every non-sentinel panic as a hard failure
  rather than distinguishing a skip. No Rust counterpart to exercise.
- `conjecture/test_engine.py::test_recursion_error_is_not_flaky` —
  relies on CPython's `RecursionError` stack-depth tricks and
  `hypothesis.internal.compat.ensure_free_stackframes` to probe a
  specific interaction between Python's recursion limit and the
  engine's flakiness detector. Rust has no equivalent surface: the
  test upstream is itself skipped under PyPy and under coverage
  instrumentation, and the native-engine port has no reason to
  simulate the RecursionError shape (native `run_test_fn` catches
  `MarkPanic` / `STOP_TEST_PANIC` sentinels and resumes unwinding on
  anything else, which is the right behaviour). The other five tests
  in this cluster (`test_detects_flakiness`,
  `test_exit_because_max_iterations`,
  `test_max_iterations_with_all_invalid`,
  `test_max_iterations_with_some_valid`,
  `test_exit_because_shrink_phase_timeout`) are ported in
  `tests/hypothesis/conjecture_engine.rs`.
The `conjecture/test_test_data.py::*` entries below are parked
awaiting follow-up additions to `NativeTestCase` /
`NativeConjectureData` in `src/native/`. The three tests that map to
the existing `NativeTestCase::for_choices` / `weighted` /
`draw_bytes_forced` / `record_span` surface are ported in
`tests/hypothesis/conjecture_test_data.rs`. Each cluster's follow-up
TODO.yaml entry names the native API additions its port lands;
removing each bullet below is acceptance for the corresponding
follow-up.

- `test_crosshair.py` (in `crosshair/`) — entire file exercises Hypothesis's
  `backend="crosshair"` integration with the third-party `crosshair`
  symbolic-execution library (`import crosshair`,
  `from hypothesis_crosshair_provider.crosshair_provider import
  CrossHairPrimitiveProvider`). Tests assert on
  `crosshair.statespace.context_statespace().choices_made` to verify
  the provider doesn't add symbolic path constraints. hegel-rust has
  no alternative-backend plumbing and no crosshair counterpart — this
  is a Python-library integration (same rationale as the numpy /
  pandas / django skips).

- `test_conformance.py` (in `crosshair/`) — entire file tests provider
  conformance of `CrossHairPrimitiveProvider` (from
  `hypothesis_crosshair_provider.crosshair_provider`) using
  `run_conformance_test`. Depends on `crosshair.core` exceptions
  (`IgnoreAttempt`, `UnexploredPath`, `NotDeterministic`) and the
  CrossHair Python symbolic-execution library. hegel-rust has no
  CrossHair integration — same rationale as the `test_crosshair.py`
  skip above.

- `test_grammar.py` (in `tests/lark/`) — every test exercises Hypothesis's
  `hypothesis.extra.lark.from_lark(...)` integration with the third-party
  `lark` parser library (`from lark.lark import Lark`); the strategy
  consumes an EBNF `Lark` grammar object and generates strings that parse
  against it, with keyword arguments (`start=`, `explicit=`, `alphabet=`)
  routed through lark's rule/terminal machinery. hegel-rust has no lark
  integration and no grammar-driven string generator — this is a
  Python-library integration (same rationale as the numpy / pandas /
  django / crosshair skips).

- `test_always_failing.py` (in `tests/snapshots/`) — parallel port-loop
  worker produced a port on `port/worker-1`, but cherry-picking onto the
  supervisor branch failed with conflicts across `Cargo.toml`,
  `src/lib.rs`, `src/native/mod.rs`, `src/native/runner.rs`,
  `src/runner.rs`, and `tests/test_native.rs` (worker branched before
  the native backend feature landed). Skipping pending human review of
  the stashed worker branch.

- `test_explain.py` (in `tests/snapshots/`) — every test exercises
  Hypothesis's `Phase.explain` (via `EXPLAIN_SETTINGS = settings(phases=[Phase.generate, Phase.shrink, Phase.explain], ...)`)
  and snapshot-tests the resulting human-readable "Falsifying example"
  output, including the explain-phase "# The test always failed when
  commented parts were varied together." / "# or any other generated
  value" comments. Both halves are Python-specific public API with no
  hegel-rust counterpart: hegel-rust has no public `Phase`/`phases`
  setting on `Settings` (same gap cited by the `test_core.py::test_non_executed_tests_raise_skipped`
  skip), and the snapshots pin Python `__repr__` formatting of class
  instances (`MyClass(x=0, y=True)`, `Outer(inner=Inner(x=0), ...)`)
  and dict literals (`{'x': 0, 'y': True}`). Unlike `test_shrinking.py`,
  which was portable by asserting on the shrunk value via `minimal()`,
  the whole subject of this file is the explain-phase output format.

- `test_combinators.py::test_sampled_from_enum_flag`,
  `test_combinators.py::test_sampled_from_module_level_enum_flag`
  (in `tests/snapshots/`) — both rely on Python's `enum.Flag` and
  Hypothesis's special-case handling of `sampled_from(EnumFlag)`,
  which generates the power-set of flag combinations via `Flag`
  bitwise OR semantics. `enum.Flag` is a Python-specific facility
  with no Rust analog, and hegel-rust's `gs::sampled_from` has no
  flag-set integration. Snapshots also pin Python `__repr__` of
  enum-flag values (`<locals>.Color.RED`, `Direction.NORTH`). The
  third test in the file (`test_data_draw`) was ported via
  `minimal()` on the shrunk-value claim, same approach as
  `test_shrinking.py`.

- `test_sets.py` (in `nocover/`) — parallel port-loop worker produced a
  port on `port/worker-1`, but cherry-picking onto the supervisor branch
  failed with conflicts across `Cargo.toml`, `src/lib.rs`,
  `src/native/mod.rs`, `src/native/runner.rs`, `src/runner.rs`, and
  `tests/test_native.rs` (worker branched before the native backend
  feature landed). Skipping pending human review of the stashed worker
  branch.

- `test_sharing.py` (in `nocover/`) — every test exercises
  `st.shared(strategy, key=...)`, a public Hypothesis strategy that
  produces the same value across multiple draw sites within a single
  test case (optionally keyed). hegel-rust has no `gs::shared()`
  counterpart — the gap is already documented in the
  `tests/hypothesis/direct_strategies.rs` module docstring alongside
  the other missing public-API strategies. Without a `shared()`
  surface, none of the seven tests (instance-by-default sharing,
  same-key sharing, distinct-instance non-sharing, distinct-key
  non-sharing, key-vs-default non-sharing, shrink-simplification of
  shared lists, shrink-simplification of size-linked shared sums)
  can be expressed.

- `test_database.py` (in `tests/watchdog/`) — every test exercises
  Hypothesis's database filesystem-listener API
  (`db.add_listener(listener)` / `db.remove_listener(listener)`,
  `("save", (k, v))` / `("delete", (k, v))` event tuples,
  `MultiplexedDatabase`, `InMemoryExampleDatabase`, `db.move(k1, k2, v)`)
  layered on top of the third-party Python `watchdog` library for
  cross-platform filesystem inotify/FSEvents/ReadDirectoryChanges. The
  entire file is `pytestmark = pytest.mark.skip(reason="see comment")`
  upstream because the listener tests are flaky against watchdog itself.
  hegel-rust's `Database::Path(...)` exposes no listener / event
  callback / multiplexed-database API and has no watchdog counterpart —
  this is a Python-library integration (same rationale as the
  redis-extra / numpy / pandas / django / crosshair / lark skips).

- `test_build_signature.py` (in `nocover/`) — every test exercises either
  `st.builds(cls)` or `st.from_type(cls)` against custom Python classes
  that override `__signature__` and `__annotations__` via
  `inspect.signature` / `typing.get_type_hints`. hegel-rust has no
  `st.builds()` / `st.from_type()` counterpart and no runtime
  type-annotation resolution.

- `test_from_type_recipe.py` (in `nocover/`) — port was abandoned due to
  a cherry-pick conflict in `src/native/conjecture_runner.rs` between the
  worker branch and the supervisor branch; defer to a human to resolve.
