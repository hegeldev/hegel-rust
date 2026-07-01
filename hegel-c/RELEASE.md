RELEASE_TYPE: minor

This release reworks how test-case handles are released so that **every** handle is freed the same way, and adds the cloning and per-handle concurrency primitives that motivated it.

`hegel_test_case_clone` makes a new handle onto the *same* underlying test case: it draws from the same source, and completion is shared across the family. Each handle has its own lock, so two clones can be driven from different threads at once, whereas using a *single* handle from two threads returns the new `HEGEL_E_CONCURRENT_USE` code. Completion is first-caller-wins: the first `hegel_mark_complete` anywhere in the family records the outcome, and a later call on a *different* handle is a safe no-op (so racing clones don't error), while completing the *same* handle twice returns `HEGEL_E_ALREADY_COMPLETE`.

Handle ownership is now uniform and reference-counted. Every test-case handle — whether it came from `hegel_test_case_from_blob`, `hegel_next_test_case`, or `hegel_test_case_clone` — is owned by the caller and **must** be released with `hegel_test_case_free`. Each handle holds one reference to the shared test case; the underlying data source is released only once the last handle is freed (and, for a run-owned handle, the run has also released its own internal reference). This makes handles easy to wrap in a garbage-collected language: free in the finaliser, uniformly, no matter where the handle came from. Freeing is not completing, though — a run-owned case still needs `hegel_mark_complete` from some handle in its family before the run can advance, so a binding must report each case's outcome (including for escaping exceptions) from its driving loop rather than leaning on the finaliser: freeing the last handle of an uncompleted case leaves `hegel_next_test_case` returning `HEGEL_E_NOT_COMPLETE` until the run is torn down with `hegel_run_free`.

This is a breaking change for callers of `hegel_next_test_case`. Previously a run-owned handle was freed by the run, and calling `hegel_test_case_free` on it returned `HEGEL_E_INVALID_HANDLE`; now the caller owns it and must free it:

```c
/* before — the run freed run-owned handles; freeing one was an error */
hegel_next_test_case(ctx, run, &tc);
/* ... drive the case, hegel_mark_complete(ctx, tc, ...) ... */
/* (must NOT free tc) */

/* after — free every handle you receive */
hegel_next_test_case(ctx, run, &tc);
/* ... drive the case, hegel_mark_complete(ctx, tc, ...) ... */
hegel_test_case_free(ctx, tc);
```

`hegel_test_case_free` accepts every test-case handle, so the same teardown works whether the handle came from a blob, a clone, or the run.

The downstream language bindings (hegel-go, hegel-ocaml, hegel-typescript) need updating for this. Each should free **every** handle from `hegel_test_case_from_blob`, `hegel_next_test_case`, or `hegel_test_case_clone` exactly once (typically from the wrapping object's destructor / finaliser), stop treating run-owned handles as borrowed (not freeing one now leaks), and drop any handling of `HEGEL_E_INVALID_HANDLE` from `hegel_test_case_free` on a run-owned handle as an expected result (it now returns `HEGEL_OK`). A clone is a distinct handle, freed separately from the handle it was cloned from; freeing the same handle twice is still undefined behaviour.
