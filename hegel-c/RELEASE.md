RELEASE_TYPE: minor

This release reworks how test-case handles are released so that **every** handle is freed the same way, and adds the cloning and per-handle concurrency primitives that motivated it.

`hegel_test_case_clone` makes a new handle onto the *same* underlying test case: it draws from the same source, and marking any handle in the family complete (`hegel_mark_complete`) marks them all. Each handle has its own lock, so two clones can be driven from different threads at once, whereas using a *single* handle from two threads returns the new `HEGEL_E_CONCURRENT_USE` code.

Handle ownership is now uniform and reference-counted. Every test-case handle — whether it came from `hegel_test_case_from_blob`, `hegel_next_test_case`, or `hegel_test_case_clone` — is owned by the caller and **must** be released with `hegel_test_case_free`. Each handle holds one reference to the shared test case; the underlying data source is released only once the last handle is freed (and, for a run-owned handle, the run has also released its own internal reference). This makes handles easy to wrap in a garbage-collected language: free in the finaliser, uniformly, no matter where the handle came from.

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
