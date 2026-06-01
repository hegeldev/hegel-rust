RELEASE_TYPE: patch

This patch adds **failure blobs** to the native backend.
A failure blob is a base64 string encoding the counterexample's choice sequence. 
With the new `print_blob` setting enabled, a failing `#[hegel::test]` prints it as a
copy-pasteable attribute:

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

The same example can be replayed programmatically with the new
`Settings::reproduce_failure` setting, and the blob is exposed over the C
ABI via `hegel_failure_reproduce_blob` (read it off a failure) and
`hegel_settings_reproduce_failure` (replay it). An undecodable blob panics
with a clear message. A blob that decodes but no longer
reproduces a failure is reported as a failing run rather than silently
passing.

The blob encodes Hegel's internal choice sequence, so it is portable only
between matching Hegel versions.
