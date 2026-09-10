RELEASE_TYPE: patch

This patch raises the limit on the number of choices a single test case may make from 8,192 to 2^20 (1,048,576), and adds a `max_choices` setting to change or remove it. A test case that reaches the limit is stopped and discarded, and enough such cases fail the `TestCasesTooLarge` or `LargeInitialTestCase` health check; with `max_choices = 0` a test case can run indefinitely, which long-running concurrent state machines need. `#[hegel::main]` binaries have no limit by default, since their one test case is usually meant to run for a long time; pass `max_choices = n` to restore one.

```rust
#[hegel::test(test_cases = 1, max_choices = 0)]
fn soak(tc: TestCase) {
    machine(Counter::new()).steps(5_000_000).run_concurrent(tc);
}
```

Adding values to a stateful `Pool` or `ConcurrentPool` no longer slows down as the pool grows: a test case that adds many thousands of values used to take quadratic time in the number of additions.
