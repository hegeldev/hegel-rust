//! A tiny in-memory key-value store whose `increment` has a lost-update
//! race — it reads the current value, releases the lock, and writes
//! back the incremented value, so two workers incrementing the same key
//! at once can each write over the other.
//!
//! Run it with:
//!
//! ```text
//! cargo test --example concurrent_kv_store
//! ```

use hegel::TestCase;
use hegel::generators as gs;
use hegel::stateful::{ConcurrentPool, concurrent_pool, run_concurrent};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

struct KvStore {
    map: Mutex<HashMap<u64, i64>>,
}

impl KvStore {
    fn new() -> Self {
        KvStore {
            map: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, i64>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, key: u64) -> Option<i64> {
        self.lock().get(&key).copied()
    }

    fn put(&self, key: u64, value: i64) {
        self.lock().insert(key, value);
    }

    fn put_if_absent(&self, key: u64, value: i64) -> bool {
        let mut map = self.lock();
        if map.contains_key(&key) {
            return false;
        }
        map.insert(key, value);
        true
    }

    fn increment(&self, key: u64) {
        let value = self.get(key).unwrap_or(0);
        std::thread::yield_now(); // Makes it easier to find the race.
        self.put(key, value + 1);
    }

    fn snapshot(&self) -> HashMap<u64, i64> {
        self.lock().clone()
    }
}

struct KvTest {
    store: KvStore,
    keys: ConcurrentPool<u64>,
    increments: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl KvTest {
    #[rule(group = "rw")]
    fn register(&self, tc: TestCase) {
        let key = tc.draw(gs::integers::<u64>().max_value(3));
        if self.store.put_if_absent(key, 0) {
            self.keys.add(&tc, key);
        }
    }

    #[rule(group = "rw")]
    fn increment(&self, tc: TestCase) {
        let key = tc.draw(self.keys.values_reusable());
        self.store.increment(key);
        self.increments.fetch_add(1, Ordering::SeqCst);
    }

    #[rule(group = "rw")]
    fn read(&self, tc: TestCase) {
        let key = tc.draw(self.keys.values_reusable());
        let value = self.store.get(key);
        tc.note(&format!("read {key} -> {value:?}"));
    }

    #[rule(group = "snapshot")]
    fn snapshot(&self, tc: TestCase) {
        let snapshot = self.store.snapshot();
        tc.note(&format!("snapshot holds {} keys", snapshot.len()));
    }

    #[invariant]
    fn no_lost_updates(&self, _: TestCase) {
        let stored: i64 = self.store.snapshot().values().sum();
        let performed = self.increments.load(Ordering::SeqCst);
        assert_eq!(
            stored, performed,
            "increments were lost: the store sums to {stored} after {performed} increments"
        );
    }
}

#[hegel::test(nondeterministic = true)]
fn test_concurrent_kv_store(tc: TestCase) {
    let test = KvTest {
        store: KvStore::new(),
        keys: concurrent_pool(&tc),
        increments: AtomicI64::new(0),
    };
    run_concurrent(test, tc, 1, 4);
}

fn main() {}
