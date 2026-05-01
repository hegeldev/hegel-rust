//! Generic cache with configurable scoring and eviction.
//!
//! Port of Hypothesis's `hypothesis.internal.cache`
//! (`GenericCache`, `LRUCache`, `LRUReusedCache`). This module provides a
//! dict-like mapping with a bounded `max_size` where each key has an
//! associated score; inserting past capacity evicts the lowest-scoring
//! non-pinned entry.

use std::collections::HashMap;
use std::hash::Hash;

/// Min-heap ordering key for an entry. Unpinned entries sort before
/// pinned ones (matching Hypothesis's `Entry.sort_key`); within each
/// tier, unpinned entries are ordered by `score` and pinned entries are
/// treated as equal.
fn sort_key<K, V>(entry: &CacheEntry<K, V>) -> (u8, i64) {
    if entry.pins == 0 {
        (0, entry.score)
    } else {
        (1, 0)
    }
}

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
    pub fn get(&mut self, key: &K) -> Option<V> {
        let i = *self.keys_to_indices.get(key)?;
        let value = self.data[i].value.clone();
        self.entry_was_accessed(i);
        Some(value)
    }

    /// Insert or overwrite `key` with `value`. May evict the
    /// lowest-scoring unpinned entry if the cache is full. Returns
    /// `Err(CannotEvictPinnedKey)` if all entries are pinned.
    pub fn insert(&mut self, key: K, value: V) -> Result<(), CachePinError> {
        if let Some(&i) = self.keys_to_indices.get(&key) {
            self.data[i].value = value;
            self.entry_was_accessed(i);
            return Ok(());
        }
        let score = self.scoring.new_entry(&key, &value);
        let entry = CacheEntry {
            key: key.clone(),
            value,
            score,
            pins: 0,
        };
        let (i, evicted) = if self.data.len() >= self.max_size {
            if self.data[0].pins > 0 {
                return Err(CachePinError::CannotEvictPinnedKey);
            }
            let old = std::mem::replace(&mut self.data[0], entry);
            self.keys_to_indices.remove(&old.key);
            (0, Some(old))
        } else {
            let idx = self.data.len();
            self.data.push(entry);
            (idx, None)
        };
        self.keys_to_indices.insert(key, i);
        self.balance(i);
        if let Some(ev) = evicted {
            self.scoring.on_evict(&ev.key, &ev.value, ev.score);
        }
        Ok(())
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
        assert_eq!(self.keys_to_indices.len(), self.data.len());
        for (i, e) in self.data.iter().enumerate() {
            assert_eq!(self.keys_to_indices.get(&e.key), Some(&i));
            for j in [i * 2 + 1, i * 2 + 2] {
                if j < self.data.len() {
                    assert!(sort_key(e) <= sort_key(&self.data[j]));
                }
            }
        }
    }

    /// Mark `key` as pinned with the given value. Pinned keys cannot be
    /// evicted until unpinned. Multiple `pin` calls stack; matching
    /// `unpin` calls are required to fully release.
    pub fn pin(&mut self, key: K, value: V) -> Result<(), CachePinError> {
        self.insert(key.clone(), value)?;
        let i = *self.keys_to_indices.get(&key).unwrap();
        self.data[i].pins += 1;
        if self.data[i].pins == 1 {
            self.balance(i);
        }
        Ok(())
    }

    /// Undo one previous `pin(key)` call.
    pub fn unpin(&mut self, key: &K) -> Result<(), CachePinError> {
        let i = match self.keys_to_indices.get(key) {
            Some(&i) if self.data[i].pins > 0 => i,
            _ => return Err(CachePinError::NotPinned),
        };
        self.data[i].pins -= 1;
        if self.data[i].pins == 0 {
            self.balance(i);
        }
        Ok(())
    }

    /// Matches Hypothesis's `GenericCache.is_pinned`: panics if the key
    /// is not in the cache.
    pub fn is_pinned(&self, key: &K) -> bool {
        self.data[self.keys_to_indices[key]].pins > 0
    }

    fn entry_was_accessed(&mut self, i: usize) {
        let entry = &self.data[i];
        let new_score = self
            .scoring
            .on_access(&entry.key, &entry.value, entry.score);
        if new_score != self.data[i].score {
            self.data[i].score = new_score;
            if self.data[i].pins == 0 {
                self.balance(i);
            }
        }
    }

    fn swap(&mut self, i: usize, j: usize) {
        self.data.swap(i, j);
        self.keys_to_indices.insert(self.data[i].key.clone(), i);
        self.keys_to_indices.insert(self.data[j].key.clone(), j);
    }

    fn balance(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / 2;
            if sort_key(&self.data[i]) < sort_key(&self.data[parent]) {
                self.swap(parent, i);
                i = parent;
            } else {
                break;
            }
        }
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            let mut smallest = i;
            if left < self.data.len() && sort_key(&self.data[left]) < sort_key(&self.data[smallest])
            {
                smallest = left;
            }
            if right < self.data.len()
                && sort_key(&self.data[right]) < sort_key(&self.data[smallest])
            {
                smallest = right;
            }
            if smallest == i {
                break;
            }
            self.swap(i, smallest);
            i = smallest;
        }
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

    pub fn insert(&mut self, key: K, value: V) {
        if let Some(pos) = self.cache.iter().position(|(k, _)| k == &key) {
            self.cache.remove(pos);
        }
        self.cache.push_back((key, value));
        while self.cache.len() > self.max_size {
            self.cache.pop_front();
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        let pos = self.cache.iter().position(|(k, _)| k == key)?;
        let entry = self.cache.remove(pos).unwrap();
        let value = entry.1.clone();
        self.cache.push_back(entry);
        Some(value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.cache.iter().any(|(k, _)| k == key)
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn keys(&self) -> Vec<K> {
        self.cache.iter().map(|(k, _)| k.clone()).collect()
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
