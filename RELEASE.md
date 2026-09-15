RELEASE_TYPE: minor

This release adds rule weights to stateful testing. `#[rule(weight = ...)]` hints that a rule should be executed more often than the machine's other rules. It is not a distributional guarantee. `#[rule]` has weight 1. Integer and float literals are both accepted, and the weight must be finite and positive:

```rust
#[hegel::state_machine]
impl Cache {
    #[rule(weight = 5)]
    fn get(&mut self, tc: TestCase) { /* ... */ }

    #[rule]
    fn evict_everything(&mut self, _: TestCase) { /* ... */ }
}
```

`Rule::new` and `ConcurrentRule::new` take the weight as an argument.

Arguments to `#[rule]` on a sequential state machine are now checked, so `#[rule(bad_arg = "...")]` on a `#[hegel::state_machine]` is a compile error.
