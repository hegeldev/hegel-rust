---
name: native-review
description: "Review and simplify a single src/native/ file against its pbtkit/Hypothesis counterpart. Use when TODO.md has a 'Review & simplify src/native/...' entry, or when manually auditing native-backend code quality."
---

# Review & simplify a src/native/ file

You are reviewing ONE file under `src/native/` to find unidiomatic Rust,
duplication, dead code, or divergences from the pbtkit/Hypothesis reference
implementations. Each review is scoped to one file and produces at most one
commit that touches only that file.

## Process

### 1. Read the target file end-to-end.

Open `src/native/<relpath>` and read the whole file. Note what it claims to
do in its top-of-file comment or module doc.

### 2. Read the counterpart(s).

Find the best-matching file in each upstream reference. **Hypothesis is
the behavioural ground truth**; pbtkit is a cleaner reference
implementation of the same core ideas (easier to read, but defers to
Hypothesis when the two disagree on *what the code should do*).

- **pbtkit** under `/tmp/pbtkit/src/pbtkit/` — read for structure and
  naming vocabulary. Common mappings:
  - `src/native/core/choices.rs` ↔ `core.py` + `floats.py` + `bytes.py` + `text.py` (choice types)
  - `src/native/core/state.rs` ↔ `core.py` (`TestCase` and `_make_choice`)
  - `src/native/tree.rs` ↔ `caching.py` (`CachedTestFunction`)
  - `src/native/shrinker/*` ↔ `shrinking/*.py`
  - `src/native/database.rs` ↔ `database.py`
  - `src/native/schema/*` ↔ no direct counterpart (hegel-rust owns the CBOR schema dispatch)

- **Hypothesis** under `/tmp/hypothesis/hypothesis-python/src/hypothesis/internal/`
  — behavioural source of truth. If the Rust file's semantics match
  pbtkit but diverge from Hypothesis, that's a bug to flag, not
  accept. Not every file has a Hypothesis counterpart; skip when none
  fits.

Read the counterpart and note: what does it cover that the Rust file doesn't?
What does the Rust file do differently or more verbosely? Where do pbtkit
and Hypothesis disagree, and does the Rust code follow Hypothesis?

### 3. Invoke the `simplify` skill.

Use the `simplify` skill (see `/.claude/skills/simplify` in the global
skills) scoped to this one file. Look specifically for:

- **Dead code**: functions, match arms, `#[allow(dead_code)]` attributes
  that hide unused items. Delete them.
- **Duplication**: copy-pasted helpers between files; repeated patterns
  that want a helper function.
- **Unidiomatic Rust**: unnecessary `clone()`, `unwrap()` in production
  code (prefer `expect(msg)` or proper error returns), explicit `.to_vec()`
  where `&[T]` would suffice, `&String` parameters (use `&str`).
- **Over/under-abstraction**: trait objects used where a concrete type
  would be clearer; concrete repetition that would benefit from a small
  helper.
- **Misnamed items**: names that don't match pbtkit's vocabulary (a reader
  who knows pbtkit should recognize the concepts); names that lie about
  what the code does.
- **Missing or stale doc comments on public items**: anything `pub` or
  `pub(crate)` in src/native/ should have a brief `///` that names its
  pbtkit counterpart where relevant.
- **`#[allow(...)]` attributes with no justification comment**: add one
  or remove the attribute.
- **Stale comments**: comments that describe behaviour the code no longer
  has, or that reference types that have been renamed.

### 4. Commit focused improvements.

If you find issues that fit in one commit, fix them and commit. The commit
touches only this file (exception: fixing a rename might ripple into the
file's `use` sites; keep that cross-file churn minimal and still in one
commit).

Example commit message:

```
Simplify src/native/shrinker/deletion.rs

- Remove dead helper `try_replace_raw` (no callers after b4253b9).
- Replace `node.value.clone()` inside the hot loop with a borrow.
- Add doc comment to `bind_deletion` naming its pbtkit counterpart.
```

### 5. Defer larger findings.

If a finding is too large for one commit (substantial refactor, new
abstraction, cross-file API change), append a new `## Pending` item to
`TODO.md` describing it and its acceptance check. Commit the smaller
fixes you can make now.

Example TODO to append:

```
- [ ] Unify shrinker::integers::binary_search_integer_towards_zero and
      shrinker::floats::shrink_floats step 3 — both do a bin_search_down
      over a candidate range; extract a shared helper.
    (verify: both passes still pass their shrink-quality tests; the
     helper has its own embedded test)
```

## Verification

Before committing:

- `just lint` passes (format + clippy + nocov-style).
- `cargo test --features native` still passes for this file's module.
- `cargo test` (server mode) still passes.
- The commit touches only the target file (unless rename rippling required
  a cross-file change, in which case mention the cross-file churn in the
  commit message).

## When there's nothing to fix

If the file reads cleanly and there's nothing to improve, commit a
whitespace-only no-op is **not acceptable**. Instead:

1. Mark the TODO item complete (`- [x]`) and move it to `## Completed`
   with a one-line note: `no changes needed — reviewed against pbtkit/<file>`.
2. Commit the TODO.md update.
3. Move on — the Stop hook will pick the next review TODO.
