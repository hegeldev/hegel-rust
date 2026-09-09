RELEASE_TYPE: minor

This release moves the stateful step count from `Settings` to the `run` call. `Settings::stateful_step_count` is removed; `hegel::stateful::run` and `hegel::stateful::run_concurrent` keep running up to 50 steps per test case (now exposed as `hegel::stateful::DEFAULT_STEP_COUNT`), and the new `run_steps` and `run_concurrent_steps` take the step count as an extra parameter, so each state machine chooses its own budget instead of every stateful test in a run sharing one setting:

```rust
// before
#[hegel::test(stateful_step_count = 200)]
fn test_counter(tc: TestCase) {
    hegel::stateful::run(Counter::new(), tc);
}

// after
#[hegel::test]
fn test_counter(tc: TestCase) {
    hegel::stateful::run_steps(Counter::new(), tc, 200);
}
```

A step count below 1 is a usage error, as before.
