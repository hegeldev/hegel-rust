RELEASE_TYPE: minor

This release adds a `reject` method to `TestCase` and `ExplicitTestCase`. It behaves like `assume(false)`, rejecting the current test input, but returns `!` so the compiler knows that code following the call is unreachable.

```rust
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let n: i32 = tc.draw(gs::integers());
    let positive: u32 = match u32::try_from(n) {
        Ok(v) => v,
        Err(_) => tc.reject(),
    };
    // use `positive` here without needing an extra `unreachable!()` branch
}
```
