RELEASE_TYPE: patch

Internal change to the native backend (`--features native`): the TooSlow health-check threshold is now passed into the engine rather than read from a constant, so it can be tested deterministically. No user-visible behaviour change.

In the native backend (`--features native`), a non-deterministic generator (one whose choice kind changes at the same position across runs) is now reported as a failing run instead of panicking — so it no longer aborts the process when the engine is driven over FFI.
