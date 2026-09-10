RELEASE_TYPE: minor

This release replaces `hegel::stateful::run` and `hegel::stateful::run_concurrent` with a builder, `hegel::stateful::Machine`. `Settings::stateful_step_count` is removed. Each state machine chooses its own step count, instead of every stateful test in a run sharing one setting.

```rust
// before
#[hegel::test(stateful_step_count = 200)]
fn test_counter(tc: TestCase) {
    hegel::stateful::run(Counter::new(), tc);
}

// after
use hegel::stateful::machine;

#[hegel::test]
fn test_counter(tc: TestCase) {
    machine(Counter::new()).steps(200).run(tc);
}
```

For concurrent machines, `min_concurrency` and `max_concurrency` replace the concurrency parameters to `run_concurrent`. For example, `machine(m).max_concurrency(5).run_concurrent(tc)` replaces `run_concurrent(m, tc, 1, 5)`. `min_concurrency` and `max_concurrency` both default to 1. Giving a sequential machine concurrency bounds is a compile-time error.
