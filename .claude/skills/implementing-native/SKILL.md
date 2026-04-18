---
name: implementing-native
description: "Implement a feature in src/native/ (new module, filling in a todo!() stub, or adding behaviour the test suite expects). Use whenever you're about to add or extend code under src/native/ — before writing the Rust, consult the pbtkit and (if needed) Hypothesis reference implementations."
---

# Implementing a native-engine feature

You are adding or extending behaviour in `src/native/`. Before writing
Rust, go read how the feature works in the upstream Python
implementations. The native engine is a port — its job is to match
pbtkit/Hypothesis semantics, not to reinvent them.

## Where to look, in order

1. **pbtkit first.** `resources/pbtkit/src/pbtkit/`. pbtkit is a
   deliberately-modularised subset of Hypothesis designed to be ported;
   its core (`core.py`, `floats.py`, `text.py`, `bytes.py`,
   `database.py`, `shrinking/*.py`) is self-contained, small, and
   readable. For any feature that exists in both, pbtkit is the
   authoritative reference for hegel-rust's native engine.

2. **Hypothesis second, when pbtkit isn't enough.**
   `resources/hypothesis/hypothesis-python/src/hypothesis/internal/`
   (especially `conjecture/`). Hypothesis takes precedence over pbtkit
   only when:
   - The feature doesn't exist in pbtkit at all (pbtkit is a subset —
     e.g. span mutation, some shrink passes, some targeting details).
   - pbtkit's implementation is known to be incomplete, buggy, or
     under-tested for the case you're handling (check git log / issue
     history before concluding this).
   - You're handling an edge case pbtkit simplifies away.

   When Hypothesis wins, say so in the source: a short doc comment
   naming both the Hypothesis file and why pbtkit's version was
   insufficient keeps future readers from re-litigating the decision.

3. **Both, for cross-checking.** Even when pbtkit is the primary
   reference, read the Hypothesis counterpart too if the feature is
   non-trivial. They occasionally diverge, and noticing the divergence
   early is cheaper than discovering it via a failing port.

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
- Collapse Python runtime checks that Rust's type system handles at
  compile time. Don't port an `isinstance(x, str)` branch when `x:
  &str`.
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
