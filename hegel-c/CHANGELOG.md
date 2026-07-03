# Changelog

## 0.24.0 - 2026-07-03

This release adds primitives for cloning test-case handles, and clears up the semantics of concurrent use of test cases so that a single test-case handle may not be used concurrently, but clones may. In addition, it changes all of the handle types to be caller-owned and freed by the caller.

This is a breaking change for callers of `hegel_next_test_case`. Previously a run-owned handle was freed by the run, and calling `hegel_test_case_free` on it returned `HEGEL_E_INVALID_HANDLE`; now the caller owns it and must free it.
Run results and failures follow the same caller-owned rule, which is also breaking.
