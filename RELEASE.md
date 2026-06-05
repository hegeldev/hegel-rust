RELEASE_TYPE: patch

This patch adds **failure blobs** to the native backend. A failure blob is a 
base64 string encoding the counterexample. With the new `print_blob` setting 
enabled, a failing `#[hegel::test]` prints it as a copy-pasteable attribute:

```text
To reproduce this failure, add the attribute below #[hegel::test]:
    #[hegel::reproduce_failure("AAEC…")]
```

```rust
#[hegel::test]
#[hegel::reproduce_failure("AAEC…")]
fn my_test(tc: hegel::TestCase) {
    let x: i32 = tc.draw(hegel::generators::integers());
    assert!(x < 100);
}
```

The attribute can be stacked to keep track of several failures; only the
first one replays — delete them one by one as you fix the failures. A blob
that decodes but no longer reproduces a failure is reported as a failing run.

Over the C ABI, `hegel_failure_reproduction_blob` reads the blob off a failure
and `hegel_test_case_from_blob` replays it.

We only guarantee the compatibility of the failure blob within a specific Hegel
version.

This patch also fixes internal hegel errors being swallowed during test execution. Previously, an unexpected error originating inside hegel itself (for example an unexpected server response) was caught by the test runner, treated as a test failure, and reported as "Property test failed: unknown" — losing the actual error message entirely. Now such errors propagate immediately with the original error message, source location, and backtrace.
