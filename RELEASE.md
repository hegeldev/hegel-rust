RELEASE_TYPE: patch

This patch maps `TestCase::clone` onto the libhegel C API's new `hegel_test_case_clone`, so cloned test cases each get their own handle rather than serializing on a shared lock. Concurrent generation across clones is still non-deterministic; making it robust is future work.
