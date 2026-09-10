RELEASE_TYPE: minor

This release replaces `hegel::stateful::run` and `hegel::stateful::run_concurrent` with a builder, `hegel::stateful::Machine`, which wraps your state machine (`hegel::stateful::machine(m)` is the shorthand) and moves the stateful step count from `Settings` onto it. `Settings::stateful_step_count` is removed; each state machine chooses its own step count instead of every stateful test in a run sharing one setting:

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

`machine(m)` uses the previous default of 50 steps (now `hegel::stateful::DEFAULT_STEP_COUNT`), so `machine(m).run(tc)` is the drop-in replacement for `run(m, tc)`. For concurrent machines, `machine(m).max_concurrency(5).run_concurrent(tc)` replaces `run_concurrent(m, tc, 1, 5)`; `min_concurrency` and `max_concurrency` both default to 1 and are only available when the model is a `ConcurrentStateMachine`, so giving a sequential machine concurrency bounds is a compile error. A step count below 1 is a usage error, as before.
