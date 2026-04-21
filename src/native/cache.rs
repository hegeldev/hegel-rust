//! Generic cache with configurable scoring and eviction.
//!
//! Port of Hypothesis's `hypothesis.internal.cache`
//! (`GenericCache`, `LRUCache`, `LRUReusedCache`). This module provides a
//! dict-like mapping with a bounded `max_size` where each key has an
//! associated score; inserting past capacity evicts the lowest-scoring
//! non-pinned entry.
//!
//! Most method bodies are currently stubbed with `todo!()`. A fixer-task
//! invocation will fill them in. The public shape matches what the
//! ported tests in `tests/hypothesis/cache_implementation.rs` need.

use std::collections::HashMap;
use std::hash::Hash;

/// Error returned when constructing a cache with `max_size == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheInvalidArgument;

/// Error returned when a pin or unpin operation is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachePinError {
    /// All keys are pinned; a new insertion cannot evict any of them.
    CannotEvictPinnedKey,
    /// `unpin(k)` called on a key that isn't currently pinned.
    NotPinned,
}

/// Scoring policy for a `GenericCache`.
///
/// Corresponds to the `new_entry`/`on_access`/`on_evict` hooks on
/// Hypothesis's `GenericCache` subclass.
pub trait CacheScoring<K, V> {
    /// Called when a key is inserted for the first time. Returns its
    /// initial score.
    fn new_entry(&mut self, key: &K, value: &V) -> i64;

    /// Called every time an existing key is read or written. Returns
    /// the new score (default: keep the existing score).
    fn on_access(&mut self, _key: &K, _value: &V, score: i64) -> i64 {
        score
    }

    /// Called after a key has been evicted, with its pre-eviction
    /// score. Default: no-op.
    fn on_evict(&mut self, _key: &K, _value: &V, _score: i64) {}
}

/// Heap entry holding (key, value, score, pin-count).
#[derive(Debug, Clone)]
pub struct CacheEntry<K, V> {
    pub key: K,
    pub value: V,
    pub score: i64,
    pub pins: u32,
}

/// Generic cache with bounded size and configurable scoring.
///
/// Backed by a binary heap keyed on `(pins > 0, score)` — unpinned
/// entries sort before pinned ones, so the min-heap root is always
/// the next eviction candidate.
pub struct GenericCache<K, V, S>
where
    K: Hash + Eq + Clone,
    V: Clone,
    S: CacheScoring<K, V>,
{
    pub max_size: usize,
    pub scoring: S,
    pub data: Vec<CacheEntry<K, V>>,
    pub keys_to_indices: HashMap<K, usize>,
}

impl<K, V, S> GenericCache<K, V, S>
where
    K: Hash + Eq + Clone,
    V: Clone,
    S: CacheScoring<K, V>,
{
    pub fn new(max_size: usize, scoring: S) -> Result<Self, CacheInvalidArgument> {
        if max_size == 0 {
            return Err(CacheInvalidArgument);
        }
        Ok(Self {
            max_size,
            scoring,
            data: Vec::new(),
            keys_to_indices: HashMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.keys_to_indices.contains_key(key)
    }

    /// Look up and return the value for `key`, or `None` if missing.
    /// Calls `on_access` as a side-effect when the key exists.
    pub fn get(&mut self, _key: &K) -> Option<V> {
        todo!("GenericCache::get")
    }

    /// Insert or overwrite `key` with `value`. May evict the
    /// lowest-scoring unpinned entry if the cache is full. Returns
    /// `Err(CannotEvictPinnedKey)` if all entries are pinned.
    pub fn insert(&mut self, _key: K, _value: V) -> Result<(), CachePinError> {
        todo!("GenericCache::insert")
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.keys_to_indices.clear();
    }

    /// Returns the current keys (unordered).
    pub fn keys(&self) -> Vec<K> {
        self.keys_to_indices.keys().cloned().collect()
    }

    /// Debug-only invariant check: assert the heap property holds and
    /// `keys_to_indices` is consistent with `data`.
    pub fn check_valid(&self) {
        todo!("GenericCache::check_valid")
    }

    /// Mark `key` as pinned with the given value. Pinned keys cannot be
    /// evicted until unpinned. Multiple `pin` calls stack; matching
    /// `unpin` calls are required to fully release.
    pub fn pin(&mut self, _key: K, _value: V) -> Result<(), CachePinError> {
        todo!("GenericCache::pin")
    }

    /// Undo one previous `pin(key)` call.
    pub fn unpin(&mut self, _key: &K) -> Result<(), CachePinError> {
        todo!("GenericCache::unpin")
    }

    pub fn is_pinned(&self, _key: &K) -> bool {
        todo!("GenericCache::is_pinned")
    }
}

/// Pure LRU cache — a drop-in replacement for `GenericCache` in
/// performance-critical paths.
///
/// No pinning support, no custom scoring — the least-recently-used
/// entry is always the next eviction candidate.
pub struct LRUCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    pub max_size: usize,
    pub cache: std::collections::VecDeque<(K, V)>,
}

impl<K, V> LRUCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "LRUCache max_size must be positive");
        Self {
            max_size,
            cache: std::collections::VecDeque::new(),
        }
    }

    pub fn insert(&mut self, _key: K, _value: V) {
        todo!("LRUCache::insert")
    }

    pub fn get(&mut self, _key: &K) -> Option<V> {
        todo!("LRUCache::get")
    }

    pub fn contains_key(&self, _key: &K) -> bool {
        todo!("LRUCache::contains_key")
    }

    pub fn len(&self) -> usize {
        todo!("LRUCache::len")
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn keys(&self) -> Vec<K> {
        todo!("LRUCache::keys")
    }

    /// No-op — `LRUCache` has no heap invariant to check.
    pub fn check_valid(&self) {}
}

/// Scoring implementation for `LRUReusedCache`: unaccessed entries
/// score below accessed ones; within each tier, more-recently-used
/// sorts larger.
#[derive(Debug, Default)]
pub struct LRUReusedScoring {
    pub tick: u64,
}

impl LRUReusedScoring {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
}

impl<K, V> CacheScoring<K, V> for LRUReusedScoring {
    fn new_entry(&mut self, _key: &K, _value: &V) -> i64 {
        // Tier 1 (freshly inserted, never accessed).
        self.next_tick() as i64
    }

    fn on_access(&mut self, _key: &K, _value: &V, _score: i64) -> i64 {
        // Tier 2 (accessed at least once). Encoded so every tier-2
        // score exceeds every tier-1 score.
        (1_i64 << 40) | (self.next_tick() as i64)
    }
}

/// Scan-resistant LRU cache used by the Hypothesis data cache.
///
/// Thin wrapper around `GenericCache<K, V, LRUReusedScoring>`.
pub struct LRUReusedCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    inner: GenericCache<K, V, LRUReusedScoring>,
}

impl<K, V> LRUReusedCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: GenericCache::new(max_size, LRUReusedScoring::new())
                .expect("LRUReusedCache max_size must be positive"),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<(), CachePinError> {
        self.inner.insert(key, value)
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        self.inner.get(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    pub fn keys(&self) -> Vec<K> {
        self.inner.keys()
    }

    pub fn check_valid(&self) {
        self.inner.check_valid()
    }

    pub fn pin(&mut self, key: K, value: V) -> Result<(), CachePinError> {
        self.inner.pin(key, value)
    }

    pub fn unpin(&mut self, key: &K) -> Result<(), CachePinError> {
        self.inner.unpin(key)
    }

    pub fn is_pinned(&self, key: &K) -> bool {
        self.inner.is_pinned(key)
    }
}
