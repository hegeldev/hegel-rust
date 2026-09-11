RELEASE_TYPE: patch

This patch raises the limit on the number of choices a single test case may make from 8,192 to 2^20 (1,048,576), and ties it to the `TestCasesTooLarge` health check. A test case that reaches the limit is stopped and discarded, and enough such cases fail `TestCasesTooLarge` (or `LargeInitialTestCase`, when even the smallest natural input does). Suppressing `TestCasesTooLarge` now also removes the limit, so a test case can run indefinitely, which long-running concurrent state machines need. `#[hegel::main]` binaries already suppress that check, so their one test case has no limit.

```rust
#[hegel::test(test_cases = 1, suppress_health_check = [HealthCheck::TestCasesTooLarge])]
fn soak(tc: TestCase) {
    machine(Counter::new()).steps(5_000_000).run_concurrent(tc);
}
```

Adding values to a stateful `Pool` or `ConcurrentPool` no longer slows down as the pool grows: a test case that adds many thousands of values used to take quadratic time in the number of additions.
