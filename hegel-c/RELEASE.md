RELEASE_TYPE: minor

This release replaces the `int64_t` ids for collections, variable pools, and state machines with opaque caller-owned handles, matching how every other libhegel object works. This is a breaking change to the C ABI.

`hegel_new_collection`, `hegel_new_pool`, and `hegel_new_state_machine` now write a handle (`hegel_collection_t*`, `hegel_pool_t*`, `hegel_state_machine_t*`) through their out-parameter instead of an id, and `hegel_collection_more`, `hegel_collection_reject`, `hegel_pool_add`, `hegel_pool_generate`, and `hegel_state_machine_next_rule` take that handle instead of an id (still alongside the test-case handle, whose stream the draws come from — any clone of the creating test case works, as before). Each handle must be released with its new matching free function — `hegel_collection_free`, `hegel_pool_free`, or `hegel_state_machine_free` — exactly once. The handles are independent of the test case and run they were created under, so the frees are safe in any order, including after `hegel_run_free`. A NULL handle is reported as `HEGEL_E_INVALID_HANDLE`, and the ids' `HEGEL_E_INVALID_ARG` "unknown id" errors are gone.

```c
/* before */
int64_t collection;
hegel_new_collection(ctx, tc, 0, 10, &collection);
hegel_collection_more(ctx, tc, collection, &more);

/* after */
hegel_collection_t *collection;
hegel_new_collection(ctx, tc, 0, 10, &collection);
hegel_collection_more(ctx, tc, collection, &more);
hegel_collection_free(ctx, collection);
```

The threading contract is now per object rather than per family. A collection may be driven by at most one thread at a time: concurrent use reports `HEGEL_E_CONCURRENT_USE`, like a test-case handle. Pools and state machines may be shared between clone handles driven from parallel threads; their operations serialize internally. This also removes a hidden serialization point — collection and pool operations from parallel clones previously contended on one family-wide lock even when touching different objects.

Internal refactoring of the engine's choice-sequence representation. Choice constraints and values are now carried as a single paired type, removing a large class of internal panics on impossible constraint/value combinations; the on-disk choice serialization and reproduce-blob formats are unchanged.

libhegel no longer installs a process-global panic hook. Violated internal invariants of the engine (bugs in hegel itself) no longer panic: during a draw they now report `HEGEL_E_INTERNAL` with the bug-report diagnostic in `hegel_context_last_error`, and during the engine's own exploration (generation, mutation, shrinking) they finish the run with a run-level error read back through `hegel_run_result_error` — exactly where a caught engine panic's message went before. Applications that install their own panic hook no longer have libhegel's hook chained in front of it, and unloading libhegel with `dlclose` no longer leaves a dangling hook behind.

Run-scoped client mistakes no longer abort the process either: an embedding that resumes the engine without concluding the offered test case, and a process launched with `ANTITHESIS_OUTPUT_DIR` pointing at a missing directory, now finish the run with a run-level error naming the mistake, read back through `hegel_run_result_error`.
