RELEASE_TYPE: patch

This patch adds concurrent stateful testing: a state machine whose rules run *concurrently* against the system under test, from a number of worker threads drawn within caller-supplied bounds (weighted toward the maximum — concurrency bugs need concurrency).

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

#[hegel::test]
fn test_kv_store(tc: TestCase) {
    let m = KvTest { store: Mutex::new(Default::default()) };
    hegel::stateful::run_concurrent(m, tc, 1, 3); // min/max concurrency levels
}
```

Rules in the same concurrency group may overlap with each other; rules in different groups never do, and invariants run at the join points between rounds while all workers are parked. A failure's trace shows each round's lines grouped by worker — each worker's draws and notes in program order, one worker after another, between markers naming each round's concurrency group. Every worker line is tagged `[worker N +X.XXXms]` with the time offset from the start of the test case, so the wall-clock arrival order across workers stays recoverable; verbose runs additionally stream every line live, in arrival order. The model is shared by reference across the workers, so rules take `&self` and mutable model state needs interior mutability. The new `stateful::ConcurrentPool` is a `Sync` variable pool workers may share; see its docs and `run_concurrent`'s for the lock-poisoning guidance that keeps a rejected rule from inducing fake failures in other workers.

Thread scheduling is nondeterministic, so calling `run_concurrent` with a concurrency bound above one makes the whole run nondeterministic: the first test case to reach the machine is discarded like a failed assumption, and every later case captures its entire trace up front, including draws and notes made before `run_concurrent`. A nondeterministic run reports failures from the discovering execution, and disables replay, shrinking, flakiness complaints, targeting, database persistence, and reproduction blobs. A one-line notice explaining this is printed once per run (suppressed under `Verbosity::Quiet`). We plan to add a more general mechanism for nondeterminism resilience in the future.

Sequential stateful tests now run as the single-group, concurrency-1 special case of the same engine protocol: `Settings::stateful_step_count` still bounds them as before, rules that reject via `assume()` still don't consume the step budget, and the invariants now also run after a rule that rejected (rules are expected to reject before mutating the model). In concurrent machines a rejected rule doesn't consume the rejecting worker's budget for the round. The choice-sequence shape of stateful tests changes, which invalidates previously stored database entries and `#[hegel::reproduce_failure]` blobs for stateful tests: stale database entries are quietly discarded on the next run, while stale blobs now fail with a decode or stale-reproducer error and should be regenerated.
