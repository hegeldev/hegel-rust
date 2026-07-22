RELEASE_TYPE: patch

This patch adds concurrent stateful testing: a state machine whose rules run *concurrently* against the system under test, from a drawn number of worker threads.

```rust
use std::sync::Mutex;
use hegel::TestCase;

struct KvTest {
    store: Mutex<std::collections::HashMap<u8, i64>>,
}

#[hegel::concurrent_state_machine]
impl KvTest {
    #[rule(group = "rw")]
    fn put(&self, tc: TestCase) { /* ... */ }

    #[rule(group = "rw")]
    fn get(&self, tc: TestCase) { /* ... */ }

    #[rule(group = "dump")]
    fn dump(&self, tc: TestCase) { /* ... */ }

    #[invariant]
    fn consistent(&self, tc: TestCase) { /* ... */ }
}

#[hegel::test(nondeterministic = true)]
fn test_kv_store(tc: TestCase) {
    let m = KvTest { store: Mutex::new(Default::default()) };
    hegel::stateful::run_concurrent(m, tc, 3); // maximum concurrency level
}
```

Rules in the same concurrency group may overlap with each other; rules in different groups never do, and invariants run at the join points between rounds while all workers are parked. A failure's trace interleaves each worker's draws and notes, tagged with the worker's index, with a marker at every join point naming the round's concurrency group. The model is shared by reference across the workers, so rules take `&self` and mutable model state needs interior mutability. The new `stateful::ConcurrentPool` is a `Sync` variable pool workers may share; see its docs and `run_concurrent`'s for the lock-poisoning guidance that keeps a rejected rule from inducing fake failures in other workers.

Thread scheduling is nondeterministic, so such a test must declare its run nondeterministic with the new `nondeterministic` setting (`#[hegel::test(nondeterministic = true)]`, or `Settings::nondeterministic`). A nondeterministic run reports failures faithfully from the discovering execution — finding that a sequence of concurrent actions can *sometimes* produce a bug is already useful — without replay, shrinking, flakiness complaints, targeting, database persistence, or a reproduce blob, and stops at the first bug, so it reports at most one failure. `#[hegel::reproduce_failure]` is rejected on a test declared nondeterministic.

Sequential stateful tests now run as the single-group, concurrency-1 special case of the same engine protocol. Two behavior changes follow. First, the choice-sequence shape of stateful tests changes, which invalidates previously stored database entries and `#[hegel::reproduce_failure]` blobs for stateful tests: stale database entries are quietly discarded on the next run, while stale blobs now fail with a decode or stale-reproducer error and should be regenerated.
