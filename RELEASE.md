RELEASE_TYPE: patch

This patch bundles a batch of fixes and improvements, most of them to the native engine (`--features native`).

- Invalid-argument (usage) errors are now reported uniformly. Misconfiguring a generator — `max_value` below `min_value`, a float range that contains no values, an empty `sampled_from`/`one_of`, an unsatisfiable filter — or misusing `tc.target()` (a non-finite score, or the same label twice in one test case) is a mistake in how the test is written, not a property that failed. Previously such errors were caught mid-draw and misreported (and pointlessly shrunk) as a discovered counterexample ("Property test failed: ..."); now the run aborts immediately with the error message, consistently across generators and across the server and native backends.
- The native engine uses a faster hasher (FxHash) for its internal lookup tables, which are keyed only by Hegel's own data and never by adversarial input. This speeds up generation across all generators, most noticeably for tests that make many draws per test case.
- The native engine now iterates targeting labels, shrink origins, and changed-node indices in a deterministic order, so a seeded run with multiple targets or failure origins is reproducible run-to-run.
- A non-deterministic generator on the native backend (one whose choice kind changes at the same position across runs) is now reported as a failing run instead of panicking, so it no longer aborts the process when the engine is driven over FFI.
- The native backend writes example-database values atomically (temp file plus rename), so a process sharing the database directory can't observe a partially-written value.
- The native regex parser now rejects the `\z` anchor (matching CPython's `re`, which only supports `\Z`) and rejects patterns nested beyond a fixed depth with a clear error instead of overflowing the stack on pathologically nested groups.
- Internal change: the native TooSlow health-check threshold is passed into the engine rather than read from a constant, so it can be tested deterministically. No user-visible behaviour change.
