RELEASE_TYPE: patch

In the native backend (`--features native`), a non-deterministic generator (one whose choice kind changes at the same position across runs) is now reported as a failing run instead of panicking — so it no longer aborts the process when the engine is driven over FFI.
