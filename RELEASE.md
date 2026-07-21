RELEASE_TYPE: patch

This patch adds a `stateful_step_count` setting controlling how many steps a stateful (`#[state_machine]`) test case runs. It defaults to 50, and each case now runs at least one step and at most `stateful_step_count`.

```rust
Settings::new().stateful_step_count(20)
```
