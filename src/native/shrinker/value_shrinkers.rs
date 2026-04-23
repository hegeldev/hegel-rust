//! Standalone value shrinkers.
//!
//! Ports Hypothesis's `hypothesis.internal.conjecture.shrinking.{Integer,
//! Ordering, Collection, Bytes, String}` — per-value shrinkers that take an
//! `(initial, predicate)` pair, track seen values (the `.calls` counter), and
//! attempt to minimise.
//!
//! These live alongside hegel-rust's node-sequence shrinker (which is a port
//! of pbtkit's `Shrinker`): the two architectures coexist.
//!
//! Ported from hypothesis-python/src/hypothesis/internal/conjecture/shrinking/{integer.py,ordering.py,collection.py,bytes.py,string.py,common.py}.
//! See also `junkdrawer.find_integer`.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use num_traits::{CheckedSub, One, ToPrimitive};

use crate::native::bignum::BigUint;
use crate::native::intervalsets::IntervalSet;

/// Finds a (hopefully large) integer such that `f(n)` is true and `f(n+1)` is
/// false. `f(0)` is assumed to be true and is not checked.
///
/// Port of `junkdrawer.find_integer`.
fn find_integer(mut f: impl FnMut(usize) -> bool) -> usize {
    // Linear scan over small numbers first: it's wasteful to probe 2 if the
    // answer is 0.
    for i in 1..5 {
        if !f(i) {
            return i - 1;
        }
    }
    // f(4) is True. Exponential probe.
    let mut lo = 4;
    let mut hi = 5;
    while f(hi) {
        lo = hi;
        hi *= 2;
    }
    // Binary search.
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if f(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Standalone shrinker for non-negative integers.
///
/// Port of `hypothesis.internal.conjecture.shrinking.Integer`. Construct with
/// an initial value and a predicate; `run()` minimises while keeping the
/// predicate true. `calls()` reports the number of distinct values considered,
/// matching Python's `Shrinker.calls` property.
pub struct IntegerShrinker<F: FnMut(&BigUint) -> bool> {
    current: BigUint,
    predicate: F,
    seen: HashSet<BigUint>,
}

impl<F: FnMut(&BigUint) -> bool> IntegerShrinker<F> {
    pub fn new(initial: BigUint, predicate: F) -> Self {
        let mut seen = HashSet::new();
        seen.insert(initial.clone());
        IntegerShrinker {
            current: initial,
            predicate,
            seen,
        }
    }

    pub fn calls(&self) -> usize {
        self.seen.len()
    }

    pub fn current(&self) -> &BigUint {
        &self.current
    }

    fn consider(&mut self, value: BigUint) -> bool {
        if value == self.current {
            return true;
        }
        if self.seen.contains(&value) {
            return false;
        }
        self.seen.insert(value.clone());
        if (self.predicate)(&value) && value < self.current {
            self.current = value;
            true
        } else {
            false
        }
    }

    pub fn run(&mut self) {
        if self.short_circuit() {
            return;
        }
        self.run_step();
    }

    fn short_circuit(&mut self) -> bool {
        for i in 0u32..2 {
            if self.consider(BigUint::from(i)) {
                return true;
            }
        }
        self.mask_high_bits();
        let size = self.current.bits() as usize;
        if size > 8 {
            let shifted = &self.current >> (size - 8);
            self.consider(shifted);
            let masked = &self.current & BigUint::from(0xFFu32);
            self.consider(masked);
        }
        self.current == BigUint::from(2u32)
    }

    fn run_step(&mut self) {
        self.shift_right();
        self.shrink_by_multiples(BigUint::from(2u32));
        self.shrink_by_multiples(BigUint::from(1u32));
    }

    fn shift_right(&mut self) {
        let base = self.current.clone();
        let size = base.bits() as usize;
        find_integer(|k| k <= size && self.consider(&base >> k));
    }

    fn mask_high_bits(&mut self) {
        let base = self.current.clone();
        let n = base.bits() as usize;
        find_integer(|k| {
            if k >= n {
                return false;
            }
            let mask = (BigUint::one() << (n - k)) - BigUint::one();
            self.consider(&mask & &base)
        });
    }

    fn shrink_by_multiples(&mut self, k: BigUint) {
        let base = self.current.clone();
        find_integer(|n| {
            base.checked_sub(&(BigUint::from(n) * &k))
                .is_some_and(|attempt| self.consider(attempt))
        });
    }
}

/// Standalone shrinker that tries to make a sequence more sorted.
///
/// Port of `hypothesis.internal.conjecture.shrinking.Ordering`. Does not
/// change length or contents — only reorders elements. Generic over the
/// element type `T`; Python's `key=` defaults to identity and is represented
/// here by the `Ord` bound on `T`.
pub struct OrderingShrinker<T: Ord + Clone + Hash + Eq, F: FnMut(&[T]) -> bool> {
    current: Vec<T>,
    predicate: F,
    seen: HashSet<Vec<T>>,
    full: bool,
    changes: usize,
}

impl<T: Ord + Clone + Hash + Eq, F: FnMut(&[T]) -> bool> OrderingShrinker<T, F> {
    pub fn new(initial: Vec<T>, predicate: F) -> Self {
        let mut seen = HashSet::new();
        seen.insert(initial.clone());
        OrderingShrinker {
            current: initial,
            predicate,
            seen,
            full: false,
            changes: 0,
        }
    }

    /// Set the `full` flag — when true, `run()` iterates `run_step` until no
    /// more improvements are found, matching Python's `Shrinker(full=True)`.
    pub fn full(mut self, full: bool) -> Self {
        self.full = full;
        self
    }

    pub fn calls(&self) -> usize {
        self.seen.len()
    }

    pub fn current(&self) -> &[T] {
        &self.current
    }

    fn consider(&mut self, value: Vec<T>) -> bool {
        if value == self.current {
            return true;
        }
        if self.seen.contains(&value) {
            return false;
        }
        self.seen.insert(value.clone());
        // left_is_better: lexicographic < on key(x); key is identity here.
        if (self.predicate)(&value) && value < self.current {
            self.current = value;
            self.changes += 1;
            true
        } else {
            false
        }
    }

    pub fn run(&mut self) {
        if self.short_circuit() {
            return;
        }
        if self.full {
            let mut prev = usize::MAX;
            while self.changes != prev {
                prev = self.changes;
                self.run_step();
            }
        } else {
            self.run_step();
        }
    }

    fn short_circuit(&mut self) -> bool {
        let mut sorted = self.current.clone();
        sorted.sort();
        self.consider(sorted)
    }

    fn run_step(&mut self) {
        self.sort_regions();
        self.sort_regions_with_gaps();
    }

    fn sort_regions(&mut self) {
        let mut i = 0;
        while i + 1 < self.current.len() {
            let k = find_integer(|k| {
                let cur = &self.current;
                if i + k > cur.len() {
                    return false;
                }
                let mut attempt: Vec<T> = cur[..i].to_vec();
                let mut middle: Vec<T> = cur[i..i + k].to_vec();
                middle.sort();
                attempt.extend(middle);
                attempt.extend_from_slice(&cur[i + k..]);
                self.consider(attempt)
            });
            // Avoid infinite loop when find_integer returns 0.
            i += k.max(1);
        }
    }

    fn sort_regions_with_gaps(&mut self) {
        let mut i = 1;
        while i + 1 < self.current.len() {
            if self.current[i - 1] <= self.current[i] && self.current[i] <= self.current[i + 1] {
                i += 1;
                continue;
            }
            let left = i;
            let mut right = i + 1;

            let grow_right = find_integer(|k| {
                let cur = self.current.clone();
                if right + k > cur.len() {
                    return false;
                }
                self.consider(build_gap_sort(&cur, left, right + k, i))
            });
            right += grow_right;

            find_integer(|k| {
                let cur = self.current.clone();
                if k > left {
                    return false;
                }
                self.consider(build_gap_sort(&cur, left - k, right, i))
            });
            i += 1;
        }
    }
}

/// Standalone shrinker for ordered collections.
///
/// Port of `hypothesis.internal.conjecture.shrinking.Collection`. Holds a
/// value and a `min_size`; `left_is_better` compares two candidate sequences
/// by length first, then by lexicographic ordering of their elements.
///
/// The `run()` body is a stub — the full shrink pipeline (try all-zero,
/// delete-each, reorder via `Ordering`, minimise duplicates, minimise each
/// element) is not yet ported. Callers that only need `left_is_better` (for
/// comparing candidates) don't need `run()`.
pub struct CollectionShrinker<T, F>
where
    T: Clone + Eq + Ord + Hash,
    F: FnMut(&[T]) -> bool,
{
    current: Vec<T>,
    #[allow(dead_code)]
    predicate: F,
    #[allow(dead_code)]
    min_size: usize,
    seen: HashSet<Vec<T>>,
}

impl<T, F> CollectionShrinker<T, F>
where
    T: Clone + Eq + Ord + Hash,
    F: FnMut(&[T]) -> bool,
{
    pub fn new(initial: Vec<T>, predicate: F, min_size: usize) -> Self {
        let mut seen = HashSet::new();
        seen.insert(initial.clone());
        CollectionShrinker {
            current: initial,
            predicate,
            min_size,
            seen,
        }
    }

    pub fn current(&self) -> &[T] {
        &self.current
    }

    pub fn calls(&self) -> usize {
        self.seen.len()
    }

    /// Compare two candidates under the collection ordering: shorter is
    /// better; otherwise compare element-wise. Matches Python's
    /// `Collection.left_is_better`.
    pub fn left_is_better(&self, left: &[T], right: &[T]) -> bool {
        if left.len() < right.len() {
            return true;
        }
        for (v1, v2) in left.iter().zip(right.iter()) {
            if v1 == v2 {
                continue;
            }
            return v1 < v2;
        }
        false
    }

    pub fn run(&mut self) {
        todo!("CollectionShrinker::run — full Collection shrink pipeline not yet ported")
    }
}

/// Standalone shrinker for byte sequences.
///
/// Port of `hypothesis.internal.conjecture.shrinking.Bytes`, which wraps
/// `Collection` with `ElementShrinker=Integer`. The `shrink` entry point
/// matches the Python class method.
pub struct BytesShrinker;

impl BytesShrinker {
    pub fn shrink<F>(initial: &[u8], mut predicate: F, min_size: usize) -> Vec<u8>
    where
        F: FnMut(&[u8]) -> bool,
    {
        let orders: Vec<usize> = initial.iter().map(|&b| b as usize).collect();
        let final_orders = run_collection_in_order_space(
            orders,
            |cand: &[usize]| {
                let bytes: Vec<u8> = cand.iter().map(|&o| o as u8).collect();
                predicate(&bytes)
            },
            min_size,
        );
        final_orders.into_iter().map(|o| o as u8).collect()
    }
}

/// Standalone shrinker for strings over a codepoint `IntervalSet`.
///
/// Port of `hypothesis.internal.conjecture.shrinking.String`, which wraps
/// `Collection` with `ElementShrinker=Integer` and the interval set's
/// `char_in_shrink_order` / `index_from_char_in_shrink_order` as
/// `from_order` / `to_order`.
pub struct StringShrinker;

impl StringShrinker {
    pub fn shrink<F>(
        initial: &str,
        mut predicate: F,
        intervals: &IntervalSet,
        min_size: usize,
    ) -> Vec<char>
    where
        F: FnMut(&str) -> bool,
    {
        let orders: Vec<usize> = initial
            .chars()
            .map(|c| intervals.index_from_char_in_shrink_order(c))
            .collect();
        let final_orders = run_collection_in_order_space(
            orders,
            |cand: &[usize]| {
                let s: String = cand
                    .iter()
                    .map(|&o| intervals.char_in_shrink_order(o))
                    .collect();
                predicate(&s)
            },
            min_size,
        );
        final_orders
            .into_iter()
            .map(|o| intervals.char_in_shrink_order(o))
            .collect()
    }
}

/// Runs `Collection.run` in shrink-order space.
///
/// Port of `hypothesis.internal.conjecture.shrinking.Collection.run_step`.
/// Operates on `Vec<usize>` where each element is an element's position in
/// the caller's shrink ordering; callers (Bytes, String) convert between
/// their element type and the order key via `from_order` / `to_order`.
/// `left_is_better` is length-then-lex on the order keys, matching
/// Collection's definition when `to_order` is the ordering function.
fn run_collection_in_order_space<F>(
    initial: Vec<usize>,
    predicate: F,
    min_size: usize,
) -> Vec<usize>
where
    F: FnMut(&[usize]) -> bool,
{
    let mut inner = CollectionInOrderSpace::new(initial, predicate);
    inner.run(min_size);
    inner.current
}

/// Inner state for `run_collection_in_order_space`. Separated so the shrink
/// sub-passes (Ordering, per-element Integer) can take a `&mut self` closure
/// against the same `current` / `seen`.
struct CollectionInOrderSpace<F>
where
    F: FnMut(&[usize]) -> bool,
{
    current: Vec<usize>,
    predicate: F,
    seen: HashSet<Vec<usize>>,
}

impl<F: FnMut(&[usize]) -> bool> CollectionInOrderSpace<F> {
    fn new(initial: Vec<usize>, predicate: F) -> Self {
        let mut seen = HashSet::new();
        seen.insert(initial.clone());
        CollectionInOrderSpace {
            current: initial,
            predicate,
            seen,
        }
    }

    fn consider(&mut self, value: Vec<usize>) -> bool {
        if value == self.current {
            return true;
        }
        if !self.seen.insert(value.clone()) {
            return false;
        }
        if !collection_left_is_better(&value, &self.current) {
            return false;
        }
        if (self.predicate)(&value) {
            self.current = value;
            true
        } else {
            false
        }
    }

    fn run(&mut self, min_size: usize) {
        // short_circuit: try [from_order(0)] * min_size, i.e. [0; min_size]
        // in order space.
        let zeros = vec![0usize; min_size];
        if self.consider(zeros) {
            return;
        }
        self.run_step();
    }

    fn run_step(&mut self) {
        // 1. Try all-zero at the current length.
        let all_zero = vec![0usize; self.current.len()];
        self.consider(all_zero);

        // 2. Try deleting each element from the back.
        let n = self.current.len();
        for i in (0..n).rev() {
            if i >= self.current.len() {
                continue;
            }
            let mut candidate = self.current.clone();
            candidate.remove(i);
            self.consider(candidate);
        }

        // 3. Reorder via OrderingShrinker. Ordering's Ord on usize matches
        // Collection's by-order element comparison, since we're already in
        // order space.
        let current_copy = self.current.clone();
        {
            let mut ordering =
                OrderingShrinker::new(current_copy, |v: &[usize]| self.consider(v.to_vec()));
            ordering.run();
        }

        // 4. Minimise each set of duplicated elements together. Snapshot the
        // duplicates first — Python iterates a set built before the loop.
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for &v in &self.current {
            *counts.entry(v).or_insert(0) += 1;
        }
        let duplicates: Vec<usize> = counts
            .into_iter()
            .filter_map(|(v, c)| if c > 1 { Some(v) } else { None })
            .collect();
        for dup in duplicates {
            let initial_val = BigUint::from(dup as u64);
            let mut shrinker = IntegerShrinker::new(initial_val, |bu: &BigUint| {
                let new_val = match bu.to_u64() {
                    Some(v) if v <= usize::MAX as u64 => v as usize,
                    _ => return false,
                };
                let candidate: Vec<usize> = self
                    .current
                    .iter()
                    .map(|&x| if x == dup { new_val } else { x })
                    .collect();
                self.consider(candidate)
            });
            shrinker.run();
        }

        // 5. Minimise each element in turn. Python captures i and val at
        // enumerate time, so we snapshot before iterating.
        let initial_vals: Vec<usize> = self.current.clone();
        for (i, &val) in initial_vals.iter().enumerate() {
            let initial_val = BigUint::from(val as u64);
            let mut shrinker = IntegerShrinker::new(initial_val, |bu: &BigUint| {
                let new_val = match bu.to_u64() {
                    Some(v) if v <= usize::MAX as u64 => v as usize,
                    _ => return false,
                };
                if i >= self.current.len() {
                    return false;
                }
                let mut candidate = self.current.clone();
                candidate[i] = new_val;
                self.consider(candidate)
            });
            shrinker.run();
        }
    }
}

/// Port of `Collection.left_is_better`: shorter wins, otherwise lexicographic
/// over order keys. We only get here with `left.len() <= right.len()` in
/// practice (the pipeline never extends).
fn collection_left_is_better(left: &[usize], right: &[usize]) -> bool {
    if left.len() < right.len() {
        return true;
    }
    for (a, b) in left.iter().zip(right.iter()) {
        if a == b {
            continue;
        }
        return a < b;
    }
    false
}

/// Builds a "gap sort" attempt: sort `current[a..b]` excluding index `i`, then
/// splice the sorted values back around `current[i]` preserving its position.
/// Caller must ensure `a <= i < b <= current.len()`.
fn build_gap_sort<T: Ord + Clone>(current: &[T], a: usize, b: usize, i: usize) -> Vec<T> {
    let split = i - a;
    let mut values: Vec<T> = current[a..i].to_vec();
    values.extend_from_slice(&current[i + 1..b]);
    values.sort();
    let mut attempt: Vec<T> = current[..a].to_vec();
    attempt.extend_from_slice(&values[..split]);
    attempt.push(current[i].clone());
    attempt.extend_from_slice(&values[split..]);
    attempt.extend_from_slice(&current[b..]);
    attempt
}
