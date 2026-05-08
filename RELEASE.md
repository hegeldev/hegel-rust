RELEASE_TYPE: patch

This patch adds `tc.target(score)` and `tc.target_labelled(score, label)` for targeted property-based testing. Call them inside a test body to feed an observation back to the engine, which uses the score to guide generation toward higher-scoring inputs.

```rust
use hegel::generators as gs;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
    let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
    tc.target((n + m) as f64);
    assert!(n + m < 2000);
}
```

Inside a `#[hegel::test]`, `#[hegel::main]`, or `#[hegel::standalone_function]` body, `tc.target(expr)` is rewritten to `tc.target_labelled(expr, "expr")`, where the label is the source text of `expr`. That way separate targeting expressions get separate labels by default, and the engine optimises each independently. Call `tc.target_labelled(score, "...")` directly when you want to choose the label yourself.
