RELEASE_TYPE: patch

This patch adds a `hegel_test_case_clone` function to the libhegel C API and gives every test-case handle its own per-instance lock, laying the groundwork for concurrent data generation.

A cloned handle is a view onto the *same* underlying test case: it draws from the same source, and marking any handle in the family complete (`hegel_mark_complete`) marks them all. Each handle carries its own lock, so two clones can be driven from different threads at once, whereas using a *single* handle from two threads now returns the new `HEGEL_E_CONCURRENT_USE` code. Only the root handle may be freed — doing so releases every clone with it — and freeing a clone directly returns the new `HEGEL_E_NOT_ROOT` code.

hegel-rust's `TestCase` now maps `clone` onto this API, so cloned test cases no longer serialize on a shared lock. Concurrent generation across clones is still non-deterministic; making it robust is future work.
