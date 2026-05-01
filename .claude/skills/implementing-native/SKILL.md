---
name: implementing-native
description: "Implement a feature in src/native/ (new module, filling in a todo!() stub, or adding behaviour the test suite expects). Use whenever you're about to add or extend code under src/native/ — before writing the Rust, consult the pbtkit and (if needed) Hypothesis reference implementations."
---

# Implementing a native-engine feature

You are adding or extending behaviour in `src/native/`. Before writing
Rust, go read how the feature works in the upstream Python
implementations. The native engine is a port — its job is to match
pbtkit/Hypothesis semantics, not to reinvent them.

## Port, don't adapt: prefer a vendored Python port over a third-party crate

When a piece of pbtkit/Hypothesis/CPython-stdlib behaviour is needed on
the Rust side and a nearby third-party Rust crate appears to offer "most"
of it — **port the Python module directly into `src/native/` instead**.
Adapting a crate with subtly different semantics is almost always the
wrong call, even when the crate is widely used, well-maintained, and
99% correct.

Concrete precedents in this repo:

- **`src/native/unicodedata.rs`** is a direct port of CPython's
  `unicodedata` module with the UCD tables vendored at a known version.
  An earlier attempt to stand on the `unicode-general-category` crate
  diverged on enough edge cases (private-use blocks, specific codepoints
  Python treats as `Cn` that the crate labels otherwise) that every
  test written against Python semantics kept surfacing off-by-one
  disagreements. The port ended the whack-a-mole.
- **`src/native/bignum.rs`** ports Python's int semantics rather than
  adapting `num-bigint` at the Hypothesis boundary, for the same reason
  (Python's modular arithmetic and shift semantics don't match
  `BigInt`'s signed two's-complement-ish behaviour without per-call
  fix-ups).

The third-party-crate shape has a recognisable tell: you start writing
"translate X" or "normalise Y" helpers at the boundary to paper over
differences. That's the signal to stop and port the module instead.
The translation helpers accrete indefinitely; the ported module is
bounded by the size of the Python source.

### When to port a CPython stdlib module rather than adapt a crate

Prefer a direct port when any of the following hold:

- The Rust crate's behaviour isn't a subset of Python's; the two
  disagree on edge cases rather than the crate being a strict restriction.
- You can see a `translate_X_to_Y`, `python_escape_fixup`,
  `walk_ast_and_rewrite` shim forming at the boundary.
- The Python module is under ~2000 lines of readable stdlib code
  (CPython's `Lib/re/_parser.py`, `Lib/fractions.py`, etc. are in this
  range). Ports at this scale are manageable; they're a one-off cost,
  the translation shims are not.
- Matching Python semantics exactly is the whole point — we are, after
  all, a port of a Python PBT library.

Ports that are generally *not* worth doing directly:

- Parsers for formats Rust already has identically-spec'd crates for
  (JSON, TOML). `serde_json` really does match the spec.
- Anything below the Python level (libc, syscalls, threads). Those
  aren't a semantics problem.

### Rust stdlib is not exempt

The same logic applies to Rust's own stdlib when it offers a
nearly-equivalent function. Stdlib is the most tempting "just reuse
it" target because it's already available — but a mismatch at a single
edge case is a mismatch, and translation shims around stdlib are just
as brittle as shims around third-party crates.

Concrete precedent: `src/native/floats.rs` defines `next_up`/`next_down`
by porting Hypothesis's `hypothesis.internal.floats` rather than
delegating to `f64::next_up`/`f64::next_down`. Rust's stdlib moves
`next_up(-0.0)` to the next subnormal; Hypothesis's contract is
`next_up(-0.0) == 0.0` (and `next_down(0.0) == -0.0`). The tests assert
the Hypothesis contract, so the port preserves it. Add a `///` doc
comment saying *why* stdlib wasn't used, so a future reader doesn't
"simplify" the port back to the stdlib call.

Before reaching for a stdlib function as a shortcut, check its
behaviour on the usual suspects — `±0.0`, NaN, infinities, empty
strings, surrogate codepoints — against the Python counterpart. One
disagreement is enough to justify the port.

### How to do the port

Follow the `unicodedata.rs` shape:

1. **Vendor the source.** Drop the Python file(s) into
   `src/native/<module>/` alongside the Rust port so the diff is
   reviewable without clicking through to CPython. Record the upstream
   URL and commit hash in a module-level doc comment. Ported from a
   snapshot — don't try to track upstream live.
2. **Mirror the Python API.** Function names, public constants, and
   argument order should match the Python module's public API so a
   reader can move between the two files without remapping. Internal
   helpers named with a leading `_` in Python stay `_`-prefixed in
   Rust (lint-allow if needed); private Python modules (`_parser.py`,
   `_constants.py`) become private Rust modules (`parser.rs`,
   `constants.rs`) with the same shape.
3. **Use Rust enums for Python's stringly-typed unions.** Where the
   Python source uses integer constants or string tags (`OP.LITERAL`,
   `AT.BEGINNING`, `"ALL"`) to discriminate variants, define a Rust
   enum. Keep the variant names identical to the Python constants.
4. **Port the tests too.** CPython usually has tests at `Lib/test/test_<module>.py`
   and Hypothesis has tests at `resources/hypothesis/hypothesis-python/
   tests/`. Use them as the behavioural spec. If the tests themselves
   are hard to port (they exercise Python runtime machinery), write
   unit tests against a hand-transcribed oracle derived from running
   the original Python at development time.
5. **Don't port what you don't need.** The bar for inclusion is "the
   rest of the native engine calls it"; the bar for exclusion is "no
   current caller + no plausible near-term caller". Err on the side of
   inclusion — a partial port is its own source of drift later.

   **Exception: the coverage ratchet forbids uncovered code.** `just
   check-coverage` demands 100% line coverage for new native code, and
   `// nocov` needs human permission. If your port's tests don't
   exercise a Python method (`IntervalSet.union` when only `difference`
   / `intersection` are tested), omit that method from the Rust port
   rather than adding it for completeness. Structure the Rust API so a
   later port (e.g. `test_charmap.py`'s `union` cases) can drop the
   method in alongside its siblings. Name the omission and the future
   port that will reintroduce it in the commit message — that's the
   breadcrumb that keeps "partial" from becoming "drifted".
6. **Once the port lands, rip out the boundary shims.** The
   `translate_python_escapes`-style helpers from the old
   third-party-crate path come out in the same commit that swaps the
   dependency. Don't leave them for a follow-up; they rot immediately.

## Where to look, in order

**Ground rule: Hypothesis is the behavioural target.** hegel-rust's
native engine exists to match Hypothesis semantics. pbtkit is a
cleaner, better-factored reference implementation of the same core
ideas, and is usually the easier read — but when the two disagree on
*what the code should do*, Hypothesis wins. pbtkit's role is clarity;
Hypothesis's role is truth.

1. **pbtkit for structure and readability.**
   `resources/pbtkit/src/pbtkit/`. pbtkit is a deliberately-modularised
   subset of Hypothesis designed to be ported; its core (`core.py`,
   `floats.py`, `text.py`, `bytes.py`, `database.py`, `shrinking/*.py`)
   is self-contained, small, and readable. Start here to understand
   *how to factor* a feature in Rust — the module boundaries, the
   data types, the control flow.

2. **Hypothesis for behavioural ground truth.**
   `resources/hypothesis/hypothesis-python/src/hypothesis/internal/`
   (especially `conjecture/`). Always cross-check your understanding
   against the Hypothesis counterpart before writing Rust. When the
   two disagree — on edge cases, constants, shrink orderings, NaN
   handling, range semantics, anything — match Hypothesis. pbtkit
   simplifies in places; the native engine does not get to.

3. **When they conflict, match Hypothesis and say so.** A short doc
   comment naming the Hypothesis file and the pbtkit divergence keeps
   future readers from re-litigating the decision:

   ```rust
   /// Port of Hypothesis `conjecture/floats.py::float_to_lex`.
   /// pbtkit's `floats.py::float_to_lex` simplifies away the
   /// subnormal handling we need here.
   ```

   Cases where Hypothesis wins include (non-exhaustive):
   - The feature doesn't exist in pbtkit at all (pbtkit is a subset —
     e.g. span mutation, some shrink passes, some targeting details).
   - pbtkit simplifies away an edge case (subnormals, surrogates,
     specific shrink-pass orderings).
   - pbtkit's implementation is incomplete or under-tested for the
     case you're handling.
   - Tests (ported from either side) assert Hypothesis-exact behaviour.

## File mapping

| hegel-rust (`src/native/`)           | pbtkit (`resources/pbtkit/src/pbtkit/`)                       | Hypothesis (`resources/hypothesis/hypothesis-python/src/hypothesis/internal/`) |
|--------------------------------------|---------------------------------------------------------------|--------------------------------------------------------------------------------|
| `core/choices.rs`                    | `core.py` + `floats.py` + `bytes.py` + `text.py`              | `conjecture/choice.py`, `conjecture/floats.py`, `conjecture/utils.py`          |
| `core/state.rs`                      | `core.py` (`TestCase`, `_make_choice`)                        | `conjecture/data.py` (`ConjectureData`)                                        |
| `tree.rs`                            | `caching.py` (`CachedTestFunction`)                           | `conjecture/datatree.py`                                                       |
| `shrinker/*.rs`                      | `shrinking/*.py`                                              | `conjecture/shrinker.py` + `conjecture/shrinking/*.py`                         |
| `database.rs`                        | `database.py`                                                 | `database.py` (top-level)                                                      |
| `targeting.rs` (if/when added)       | `targeting.py`                                                | `conjecture/optimiser.py`                                                      |
| `span_mutation.rs` (if/when added)   | `span_mutation.py`                                            | `conjecture/shrinking/*` (no single file)                                      |
| `schema/*`                           | (no counterpart — hegel-rust owns the CBOR schema dispatch)   | —                                                                              |

The mapping in `native-review` is similar; this one is oriented at
implementation (what to read before writing) rather than review (what
to read before simplifying).

## Process

### 1. Read the upstream counterpart end-to-end.

Open the pbtkit file(s) for the feature. Read the function(s) and
their call sites. Note:

- The data types involved and their invariants.
- The control flow (loops, recursion depth, early exits).
- Any Python-specific idioms you'll need to translate (see
  `.claude/skills/porting-tests/references/api-mapping.md`).
- Tests exercising it under `resources/pbtkit/tests/`. Those tests are
  the behavioural spec — they'll land in `tests/pbtkit/` during a
  port, and your implementation must eventually pass them.

If pbtkit isn't sufficient, read the Hypothesis counterpart the same
way. Note any divergence between the two.

### 2. Check what already exists on the Rust side.

Before writing new code, grep `src/native/` for the feature — it may
already exist, be partially there under a different name, or be
stubbed with `todo!()`. In particular:

- Existing functions are often named after the pbtkit function they
  match (e.g. `bin_search_down`, `shrink_sequence`). Follow that
  convention.
- Stubs often live as `todo!("implement X — see pbtkit Y")`. Those
  comments tell you exactly what to port.
- Related helpers (e.g. `sort_key`, `simplest`, `unit` on a choice
  type) usually live alongside each other. Fill the gap where the
  siblings already are, don't start a new module.

### 3. Port the behaviour, don't transliterate the Python.

The goal is Rust that matches pbtkit's semantics, not line-by-line
translation:

- Use Rust types: `Option<T>` for Python `None`-returning paths,
  `Result<T, E>` where pbtkit raises, `&[T]`/`Vec<T>` for Python
  lists, typed enums for string-tagged Python unions.
- Python subclass-override hooks (e.g. `GenericCache` subclasses that
  override `new_entry` / `on_access` / `on_evict`) become a strategy
  trait with default method bodies, and the wrapper type becomes
  generic over it — e.g. `GenericCache<K, V, S: CacheScoring<K, V>>`
  in `src/native/cache.rs`. Expose the scoring instance as a `pub`
  field so tests can inspect per-subclass state after a run. See
  `.claude/skills/porting-tests/references/api-mapping.md` "Python
  subclass-override hooks" for the test-side shape.
- Python class generic over a type parameter that's **monomorphic in
  practice** (always instantiated one way) — put the operational
  pipeline on a specialised `impl` block, not a generic one. Keep
  inspection methods generic so tests can still exercise them for
  arbitrary `T`. Precedent:
  `hypothesis.internal.conjecture.shrinking.Collection` is nominally
  generic over `ElementShrinker` but always uses `Integer` in
  practice, and its `left_is_better` / `current` / `calls` don't need
  element semantics. Port as
  `CollectionShrinker<T: Clone + Eq + Ord + Hash, F>` with
  inspection-only methods in the generic `impl`, and the `run` /
  `run_step` pipeline in a separate `impl<F: FnMut(&[usize]) -> bool>
  CollectionShrinker<usize, F>`. Wrapper types (`BytesShrinker`,
  `StringShrinker`) do the element↔`usize` order-key conversion at
  the call boundary. Avoid the alternatives: don't introduce an
  `ElementShrinker` trait for a single implementor, and don't drop
  the generic struct in favour of `Vec<usize>`-only, which makes the
  inspection methods unreachable for the element types the Python
  tests cover.
- Collapse Python runtime checks that Rust's type system handles at
  compile time. Don't port an `isinstance(x, str)` branch when `x:
  &str`. A Python `self.finished = False` → `self.finished = True`
  lifecycle flag guarded by `assert not self.finished` in other methods
  ports cleanly as a *consuming* finishing method: `fn finish(mut self)
  -> T { ... }` drops `self`, so later calls to the other methods fail
  to compile rather than trip a runtime assert. Same for "used / not
  used" / "opened / closed" / "run / finalized" flags. Precedent:
  `Chooser::finish` in `src/native/choicetree.rs` — `ChoiceTree.py`'s
  `Chooser.finished` flag fell away once `finish` took `self` by value.
- Python aliased-mutable state (tree / DAG nodes where multiple
  parents hold a handle to the same child and mutate it through any
  of them; `defaultdict(Node)` where lookup auto-materialises a node
  other code then keeps a handle to) becomes a `Rc<RefCell<…>>`
  newtype wrapper — `struct Node(Rc<RefCell<NodeInner>>)` with all
  interior state on `NodeInner` and `impl Node` methods borrowing
  through the `RefCell`. `defaultdict(Node)` maps to
  `inner.children.entry(i).or_insert_with(Node::new).clone()`; the
  `clone()` is on the `Rc`, so all callers see the same node.
  Precedent: `src/native/choicetree.rs` (port of
  `conjecture/shrinking/choicetree.py`). Don't reach for alternatives
  that look safer but aren't — `Vec<NodeInner>` with integer handles
  works for a static tree but not when mixed insertion / pruning is
  aliased, and `Arc<Mutex<…>>` is unnecessary overhead when the
  native engine is single-threaded.
- A Python generator function returning `Iterable[T]` (a `def foo(…):
  yield …` whose caller does `for x in foo(…)`) ports to a
  `Box<dyn FnMut(…) -> Vec<T>>` — build the vec in the order the
  Python code would yield and return it eagerly. Hypothesis's
  `LazySequenceCopy.pop(i)` idiom (swap index `i` with the last
  element, pop) ports as `v.swap(i, v.len() - 1); v.pop()`.
- Preserve names where they help a cross-reading reader: a function
  called `_codepoint_key` in pbtkit should be `codepoint_key` (or
  `codepoint_sort_key`) in Rust, not `char_rank`.
- Preserve invariants explicitly: a pbtkit `assert` that guards a
  precondition maps to a Rust `assert!`/`debug_assert!`, or to a
  return-type that makes it unrepresentable. Don't silently drop
  them.

### 4. Add tests alongside the implementation.

Follow `.claude/CLAUDE.md`'s testing conventions: tests under
`tests/`, never inline. If the feature is `pub(crate)` and needs
direct access, the test goes in
`tests/embedded/native/<module>_tests.rs` mirroring the source path.
Otherwise it's an integration test under `tests/pbtkit/` (ported from
the corresponding pbtkit test file, per the `porting-tests` skill).

Coverage: `just check-coverage` enforces 100% line coverage for new
code. Make code testable; don't add `// nocov`.

### 5. Verify against both modes.

```bash
cargo test                                                      # server mode
HEGEL_SERVER_COMMAND=/bin/false cargo test --features native   # native mode
just lint
```

If the feature is `#[cfg(feature = "native")]`-gated, server-mode
tests don't exercise it — make sure the native-mode run does.

### 6. Document the upstream pointer.

Any non-trivial native function gets a `///` doc line naming its
pbtkit counterpart (and Hypothesis counterpart if that was the
authoritative source). This makes the native module self-describing
for future readers and future port reviews:

```rust
/// Shortlex codepoint-ordering key. pbtkit: `text.py::_codepoint_key`.
fn codepoint_key(cp: u32) -> (usize, u32) { ... }
```

When Hypothesis was chosen over pbtkit, say why in the same comment:

```rust
/// Targeted mutation pass. Hypothesis:
/// `conjecture/optimiser.py::Optimiser.run` — pbtkit's
/// `targeting.py::target` simplifies away the hill-climbing step we
/// need here.
```

## Relationship to other skills

- **`porting-tests`**: use when the task is specifically to port a
  Python test file; often spawns native-implementation work when the
  tests exercise features that aren't yet implemented under
  `src/native/`.
- **`native-review`**: use after an implementation lands, to audit it
  for idiom / dead-code / divergence from the upstream reference.
  `implementing-native` is the "write the code" counterpart to
  `native-review`'s "audit the code".
- **`coverage`**: coverage is mandatory for new native code; consult
  it when adding tests for a new native feature.
