//! Ported from hypothesis-python/tests/cover/test_cache_implementation.py
//!
//! Tests Hypothesis's engine-internal `hypothesis.internal.cache` module
//! (`GenericCache`, `LRUCache`, `LRUReusedCache`). These are ported to
//! their native-mode counterparts under `src/native/cache.rs`, which are
//! currently stubbed with `todo!()` — most tests will fail at runtime
//! until a fixer-task invocation fills in the implementation.
//!
//! Individually-skipped tests:
//!
//! - `test_cache_is_threadsafe_issue_2433_regression` — uses
//!   `st.builds(partial(str))`, a Python-reflection-based strategy
//!   (`from_type` / callable introspection) with no hegel-rust
//!   counterpart. The per-thread caching that test guards is
//!   Hypothesis-specific.

#![cfg(feature = "native")]

use std::collections::{HashMap, HashSet};

use hegel::__native_test_internals::{
    CacheInvalidArgument, CachePinError, CacheScoring, GenericCache, LRUCache, LRUReusedCache,
};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

// -- Cache scoring implementations mirroring the Python subclasses --------

#[derive(Default)]
struct LRUAlternativeScoring {
    tick: i64,
}

impl<K, V> CacheScoring<K, V> for LRUAlternativeScoring {
    fn new_entry(&mut self, _key: &K, _value: &V) -> i64 {
        self.tick += 1;
        self.tick
    }
    fn on_access(&mut self, _key: &K, _value: &V, _score: i64) -> i64 {
        self.tick += 1;
        self.tick
    }
}

struct LFUScoring;

impl<K, V> CacheScoring<K, V> for LFUScoring {
    fn new_entry(&mut self, _key: &K, _value: &V) -> i64 {
        1
    }
    fn on_access(&mut self, _key: &K, _value: &V, score: i64) -> i64 {
        score + 1
    }
}

/// Scoring where the entry's score *is* its value. Only implemented
/// for `V = i64` since that's what the tests use.
struct ValueScored;

impl<K> CacheScoring<K, i64> for ValueScored {
    fn new_entry(&mut self, _key: &K, value: &i64) -> i64 {
        *value
    }
}

struct RandomScoring {
    rng: StdRng,
}

impl RandomScoring {
    fn new() -> Self {
        Self {
            rng: StdRng::seed_from_u64(0),
        }
    }
}

impl<K, V> CacheScoring<K, V> for RandomScoring {
    fn new_entry(&mut self, _key: &K, _value: &V) -> i64 {
        self.rng.next_u64() as i64
    }
    fn on_access(&mut self, _key: &K, _value: &V, _score: i64) -> i64 {
        self.rng.next_u64() as i64
    }
}

/// `new_entry(k, v) = k` — used by `test_still_inserts_if_score_is_worse`.
struct KeyScored;

impl CacheScoring<i64, i64> for KeyScored {
    fn new_entry(&mut self, key: &i64, _value: &i64) -> i64 {
        *key
    }
}

// -- write_pattern composite generator ------------------------------------

fn write_pattern(min_distinct_keys: usize) -> impl Generator<Vec<(i64, i64)>> {
    hegel::compose!(|tc| {
        let keys: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000))
                .unique(true)
                .min_size(std::cmp::max(min_distinct_keys, 1)),
        );
        let values: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).unique(true).min_size(1));
        let writes_gen = gs::vecs(gs::tuples!(
            gs::sampled_from(keys),
            gs::sampled_from(values)
        ))
        .min_size(min_distinct_keys);
        if min_distinct_keys > 0 {
            let mdk = min_distinct_keys;
            tc.draw(writes_gen.filter(move |ls: &Vec<(i64, i64)>| {
                ls.iter().map(|(k, _)| *k).collect::<HashSet<_>>().len() >= mdk
            }))
        } else {
            tc.draw(writes_gen)
        }
    })
}

// -- Common "behaves like a dict with losses" driver ----------------------

trait DictLikeCache {
    fn insert_unwrap(&mut self, k: i64, v: i64);
    fn get_val(&mut self, k: &i64) -> Option<i64>;
    fn len_(&self) -> usize;
    fn check_valid_(&self);
}

impl<S: CacheScoring<i64, i64>> DictLikeCache for GenericCache<i64, i64, S> {
    fn insert_unwrap(&mut self, k: i64, v: i64) {
        self.insert(k, v).unwrap();
    }
    fn get_val(&mut self, k: &i64) -> Option<i64> {
        self.get(k)
    }
    fn len_(&self) -> usize {
        self.len()
    }
    fn check_valid_(&self) {
        self.check_valid()
    }
}

impl DictLikeCache for LRUCache<i64, i64> {
    fn insert_unwrap(&mut self, k: i64, v: i64) {
        self.insert(k, v);
    }
    fn get_val(&mut self, k: &i64) -> Option<i64> {
        self.get(k)
    }
    fn len_(&self) -> usize {
        self.len()
    }
    fn check_valid_(&self) {
        self.check_valid()
    }
}

impl DictLikeCache for LRUReusedCache<i64, i64> {
    fn insert_unwrap(&mut self, k: i64, v: i64) {
        self.insert(k, v).unwrap();
    }
    fn get_val(&mut self, k: &i64) -> Option<i64> {
        self.get(k)
    }
    fn len_(&self) -> usize {
        self.len()
    }
    fn check_valid_(&self) {
        self.check_valid()
    }
}

fn run_dict_like_losses(target: &mut dyn DictLikeCache, writes: &[(i64, i64)], size: usize) {
    let mut model: HashMap<i64, i64> = HashMap::new();

    for &(k, v) in writes {
        if let Some(&mv) = model.get(&k) {
            if let Some(tv) = target.get_val(&k) {
                assert_eq!(mv, tv);
            }
        }
        model.insert(k, v);
        target.insert_unwrap(k, v);
        target.check_valid_();
        assert_eq!(target.get_val(&k), Some(v));
        for (&r, &s) in &model {
            if let Some(tv) = target.get_val(&r) {
                assert_eq!(s, tv);
            }
        }
        assert!(target.len_() <= std::cmp::min(model.len(), size));
    }
}

fn behaves_like_a_dict_with_losses_hegel<C, F>(make: F)
where
    C: DictLikeCache + 'static,
    F: Fn(usize) -> C + Send + Sync + 'static,
{
    // Explicit regression examples carried over from Hypothesis's `@example`
    // decorators on `test_behaves_like_a_dict_with_losses`.
    for (writes, size) in [
        (
            vec![(0_i64, 0_i64), (3, 0), (1, 0), (2, 0), (2, 0), (1, 0)],
            4_usize,
        ),
        (vec![(0, 0)], 1),
        (vec![(1, 0), (2, 0), (0, -1), (1, 0)], 3),
    ] {
        let mut target = make(size);
        run_dict_like_losses(&mut target, &writes, size);
    }

    Hegel::new(move |tc| {
        let writes: Vec<(i64, i64)> = tc.draw(write_pattern(0));
        let size: usize = tc.draw(gs::integers::<usize>().min_value(1).max_value(10));
        let mut target = make(size);
        run_dict_like_losses(&mut target, &writes, size);
    })
    .settings(Settings::new().test_cases(50).database(None))
    .run();
}

#[test]
fn test_behaves_like_a_dict_with_losses_lru() {
    behaves_like_a_dict_with_losses_hegel::<LRUCache<i64, i64>, _>(LRUCache::new);
}

#[test]
fn test_behaves_like_a_dict_with_losses_lfu() {
    behaves_like_a_dict_with_losses_hegel::<GenericCache<i64, i64, LFUScoring>, _>(|sz| {
        GenericCache::new(sz, LFUScoring).unwrap()
    });
}

#[test]
fn test_behaves_like_a_dict_with_losses_lru_reused() {
    behaves_like_a_dict_with_losses_hegel::<LRUReusedCache<i64, i64>, _>(LRUReusedCache::new);
}

#[test]
fn test_behaves_like_a_dict_with_losses_value_scored() {
    behaves_like_a_dict_with_losses_hegel::<GenericCache<i64, i64, ValueScored>, _>(|sz| {
        GenericCache::new(sz, ValueScored).unwrap()
    });
}

#[test]
fn test_behaves_like_a_dict_with_losses_random() {
    behaves_like_a_dict_with_losses_hegel::<GenericCache<i64, i64, RandomScoring>, _>(|sz| {
        GenericCache::new(sz, RandomScoring::new()).unwrap()
    });
}

#[test]
fn test_behaves_like_a_dict_with_losses_lru_alt() {
    behaves_like_a_dict_with_losses_hegel::<GenericCache<i64, i64, LRUAlternativeScoring>, _>(
        |sz| GenericCache::new(sz, LRUAlternativeScoring::default()).unwrap(),
    );
}

// -- test_always_evicts_the_lowest_scoring_value --------------------------
//
// The Python version uses `st.data()` + a Cache subclass whose
// `new_entry` / `on_access` each call back into `data.draw(...)` to
// produce a fresh score. The hooks here have no `tc` in scope, so we
// draw a PRNG seed from `tc` (which Hypothesis can still shrink) and
// let the scoring struct pull fresh scores from the RNG on every
// call — exercising the cache's rebalance-after-on_access path that a
// once-per-key pre-draw would miss.

struct DynamicScoring {
    rng: StdRng,
    scores: HashMap<i64, i64>,
    last_entry: Option<i64>,
    evicted: HashSet<i64>,
}

impl DynamicScoring {
    fn new_score(&mut self, key: i64) -> i64 {
        let s = (self.rng.next_u64() % 1001) as i64;
        self.scores.insert(key, s);
        s
    }
}

impl CacheScoring<i64, i64> for DynamicScoring {
    fn new_entry(&mut self, key: &i64, _value: &i64) -> i64 {
        self.last_entry = Some(*key);
        self.evicted.remove(key);
        assert!(!self.scores.contains_key(key));
        self.new_score(*key)
    }

    fn on_access(&mut self, key: &i64, _value: &i64, _score: i64) -> i64 {
        assert!(self.scores.contains_key(key));
        self.new_score(*key)
    }

    fn on_evict(&mut self, key: &i64, _value: &i64, score: i64) {
        assert_eq!(score, *self.scores.get(key).unwrap());
        self.scores.remove(key);
        if self.scores.len() > 1 {
            let min_other = self
                .scores
                .iter()
                .filter(|(k, _)| Some(**k) != self.last_entry)
                .map(|(_, v)| *v)
                .min()
                .unwrap();
            assert!(score <= min_other);
        }
        self.evicted.insert(*key);
    }
}

#[test]
fn test_always_evicts_the_lowest_scoring_value() {
    Hegel::new(|tc| {
        let writes: Vec<(i64, i64)> = tc.draw(write_pattern(2));
        let n_keys = writes.iter().map(|(k, _)| *k).collect::<HashSet<_>>().len();
        tc.assume(n_keys > 1);
        let size: usize = tc.draw(gs::integers::<usize>().min_value(1).max_value(n_keys - 1));
        let seed: u64 = tc.draw(gs::integers::<u64>());

        let scoring = DynamicScoring {
            rng: StdRng::seed_from_u64(seed),
            scores: HashMap::new(),
            last_entry: None,
            evicted: HashSet::new(),
        };
        let mut target = GenericCache::new(size, scoring).unwrap();
        let mut model: HashMap<i64, i64> = HashMap::new();

        for &(k, v) in &writes {
            target.insert(k, v).unwrap();
            model.insert(k, v);
        }

        assert!(!target.scoring.evicted.is_empty());
        assert_eq!(target.scoring.evicted.len() + target.len(), model.len());
        assert_eq!(target.scoring.scores.len(), target.len());

        let evicted_snapshot: HashSet<i64> = target.scoring.evicted.clone();
        for (&k, &v) in &model {
            match target.get(&k) {
                Some(got) => {
                    assert_eq!(got, v);
                    assert!(!evicted_snapshot.contains(&k));
                }
                None => {
                    assert!(evicted_snapshot.contains(&k));
                }
            }
        }
    })
    .settings(Settings::new().test_cases(50).database(None))
    .run();
}

// -- Plain unit tests -----------------------------------------------------

#[test]
fn test_basic_access() {
    let mut cache = GenericCache::new(2, ValueScored).unwrap();
    cache.insert(1_i64, 0_i64).unwrap();
    cache.insert(1, 0).unwrap();
    cache.insert(0, 1).unwrap();
    cache.insert(2, 0).unwrap();
    assert_eq!(cache.get(&2), Some(0));
    assert_eq!(cache.get(&0), Some(1));
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_can_clear_a_cache() {
    let mut x = GenericCache::<i64, i64, _>::new(1, ValueScored).unwrap();
    x.insert(0, 1).unwrap();
    assert_eq!(x.len(), 1);
    x.clear();
    assert_eq!(x.len(), 0);
}

#[test]
fn test_max_size_must_be_positive() {
    let result: Result<GenericCache<i64, i64, ValueScored>, _> = GenericCache::new(0, ValueScored);
    assert_eq!(result.err(), Some(CacheInvalidArgument));
}

#[test]
fn test_pinning_prevents_eviction() {
    let mut cache = LRUReusedCache::<i64, i64>::new(10);
    cache.pin(20, 1).unwrap();
    for i in 0..20 {
        cache.insert(i, 0).unwrap();
    }
    assert_eq!(cache.get(&20), Some(1));
}

#[test]
fn test_unpinning_allows_eviction() {
    let mut cache = LRUReusedCache::<i64, bool>::new(10);
    cache.pin(20, true).unwrap();
    for i in 0..20_i64 {
        cache.insert(i, false).unwrap();
    }
    assert!(cache.contains_key(&20));

    cache.unpin(&20).unwrap();
    cache.insert(21, false).unwrap();

    assert!(!cache.contains_key(&20));
}

#[test]
fn test_unpins_must_match_pins() {
    let mut cache = LRUReusedCache::<i64, i64>::new(2);
    cache.pin(1, 1).unwrap();
    assert!(cache.is_pinned(&1));
    assert_eq!(cache.get(&1), Some(1));
    cache.pin(1, 2).unwrap();
    assert!(cache.is_pinned(&1));
    assert_eq!(cache.get(&1), Some(2));
    cache.unpin(&1).unwrap();
    assert!(cache.is_pinned(&1));
    assert_eq!(cache.get(&1), Some(2));
    cache.unpin(&1).unwrap();
    assert!(!cache.is_pinned(&1));
}

#[test]
fn test_will_error_instead_of_evicting_pin() {
    let mut cache = LRUReusedCache::<i64, i64>::new(1);
    cache.pin(1, 1).unwrap();
    let err = cache.insert(2, 2).unwrap_err();
    assert_eq!(err, CachePinError::CannotEvictPinnedKey);
    assert!(cache.contains_key(&1));
    assert!(!cache.contains_key(&2));
}

#[test]
fn test_will_error_for_bad_unpin() {
    let mut cache = LRUReusedCache::<i64, i64>::new(1);
    cache.insert(1, 1).unwrap();
    let err = cache.unpin(&1).unwrap_err();
    assert_eq!(err, CachePinError::NotPinned);
}

#[test]
fn test_still_inserts_if_score_is_worse() {
    let mut cache = GenericCache::new(1, KeyScored).unwrap();
    cache.insert(0_i64, 1_i64).unwrap();
    cache.insert(1, 1).unwrap();

    assert!(!cache.contains_key(&0));
    assert!(cache.contains_key(&1));
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_does_insert_if_score_is_better() {
    let mut cache = GenericCache::new(1, ValueScored).unwrap();
    cache.insert(0_i64, 1_i64).unwrap();
    cache.insert(1, 0).unwrap();

    assert!(!cache.contains_key(&0));
    assert!(cache.contains_key(&1));
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_double_pinning_does_not_add_entry() {
    let mut cache = LRUReusedCache::<i64, i64>::new(2);
    cache.pin(0, 0).unwrap();
    cache.pin(0, 1).unwrap();
    cache.insert(1, 1).unwrap();
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_can_add_new_keys_after_unpinning() {
    let mut cache = LRUReusedCache::<i64, i64>::new(1);
    cache.pin(0, 0).unwrap();
    cache.unpin(&0).unwrap();
    cache.insert(1, 1).unwrap();
    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&1));
}

#[test]
fn test_iterates_over_remaining_keys() {
    let mut cache = LRUReusedCache::<i64, &'static str>::new(2);
    for i in 0..3_i64 {
        cache.insert(i, "hi").unwrap();
    }
    let mut ks = cache.keys();
    ks.sort();
    assert_eq!(ks, vec![1, 2]);
}

#[test]
fn test_lru_cache_is_actually_lru() {
    let mut cache = LRUCache::<i64, i64>::new(2);
    cache.insert(1, 1); // [1]
    cache.insert(2, 2); // [1, 2]
    cache.get(&1); //     [2, 1]
    cache.insert(3, 2); // [2, 1, 3] -> drop LRU -> [1, 3]
    let mut ks = cache.keys();
    // Python asserts insertion order `[1, 3]`. `keys()` returns the
    // cache's internal order; sort to compare against the set.
    ks.sort();
    assert_eq!(ks, vec![1, 3]);
}
