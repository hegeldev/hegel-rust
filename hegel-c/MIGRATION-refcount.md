# Migrating to uniform, reference-counted test-case handles

This release of libhegel (`hegeltest-c`) changes the rule for releasing
`hegel_test_case_t*` handles. **Every** handle is now owned by the caller and
must be released with `hegel_test_case_free`, regardless of where it came from.
Previously the rule depended on the handle's origin, which made handles awkward
to wrap in a garbage-collected language.

This is a **breaking** change for the downstream language bindings
(hegel-go, hegel-ocaml, hegel-typescript). Each must be updated. The
`bump-libhegel` dispatch fired by a libhegel release notifies those repos; the
checklist below is what each of them needs to do. **No edits to those repos are
made from hegel-rust** — this is a heads-up + checklist only.

## What changed

| Handle source | Before (last released libhegel) | After |
|---|---|---|
| `hegel_test_case_from_blob` | caller frees with `hegel_test_case_free` | unchanged — caller frees |
| `hegel_next_test_case` (run-owned) | freed by the run; `hegel_test_case_free` returned `HEGEL_E_INVALID_HANDLE` | **caller frees** with `hegel_test_case_free` |
| `hegel_test_case_clone` | n/a — `hegel_test_case_clone` is new in this release | caller frees with `hegel_test_case_free` |

The underlying test case is reference-counted: it stays alive until its last
handle is freed (and, for a run-owned family, until the run releases its own
internal reference). So freeing a run-owned handle is always safe and never
disturbs the run, and a clone keeps working after the handle it was cloned from
is freed.

## Checklist for each downstream wrapper

1. **Free every handle.** Wherever the wrapper obtains a `hegel_test_case_t*` —
   from `hegel_test_case_from_blob`, `hegel_next_test_case`, or
   `hegel_test_case_clone` — arrange for `hegel_test_case_free` to be called on
   it exactly once (e.g. in the wrapper object's destructor / finaliser /
   `Drop` / `close`).

2. **Stop treating run-owned handles as borrowed.** Remove any code that
   deliberately does *not* free a handle from `hegel_next_test_case` on the
   assumption that the run owns it. The run no longer frees the caller's handle;
   not freeing it now leaks the handle (and, eventually, its data source).

3. **Remove special-casing of free results.** Any code that treated
   `HEGEL_E_INVALID_HANDLE` from `hegel_test_case_free` on a run-owned handle as
   expected should be removed — that path now returns `HEGEL_OK`.

4. **Free exactly once.** As before, freeing the same handle twice is undefined
   behaviour. A clone is a distinct handle and is freed separately from the
   handle it was cloned from.

5. **(Optional) Adopt the GC-friendly pattern.** Because freeing is now uniform
   and order-independent, the wrapper can hold a handle in a GC-managed object
   and free it from the finaliser without tracking whether it was run-owned,
   from a blob, or a clone.
