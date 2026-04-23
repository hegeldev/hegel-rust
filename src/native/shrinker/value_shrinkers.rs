//! Standalone value shrinkers.
//!
//! Ports Hypothesis's `hypothesis.internal.conjecture.shrinking.Integer` and
//! `Ordering` — per-value shrinkers that take an `(initial, predicate)` pair,
//! track seen values (the `.calls` counter), and attempt to minimise.
//!
//! These live alongside hegel-rust's node-sequence shrinker (which is a port
//! of pbtkit's `Shrinker`): the two architectures coexist.
//!
//! Ported from hypothesis-python/src/hypothesis/internal/conjecture/shrinking/{integer.py,ordering.py,common.py}.
//! See also `junkdrawer.find_integer`.

use std::collections::HashSet;
use std::hash::Hash;

use num_traits::{CheckedSub, One};

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
    pub fn shrink<F>(_initial: &[u8], _predicate: F, _min_size: usize) -> Vec<u8>
    where
        F: FnMut(&[u8]) -> bool,
    {
        todo!("BytesShrinker::shrink — not yet ported")
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
        _initial: &str,
        _predicate: F,
        _intervals: &IntervalSet,
        _min_size: usize,
    ) -> Vec<char>
    where
        F: FnMut(&str) -> bool,
    {
        todo!("StringShrinker::shrink — not yet ported")
    }
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
