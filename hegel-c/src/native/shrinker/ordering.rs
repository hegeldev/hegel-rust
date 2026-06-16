//! Sequence-reordering primitive used by `reorder_spans`.
//!
//! The algorithm walks a permutation of `[0..n)` and tries to make the
//! resulting key-ordered sequence more sorted under the supplied key
//! function, leaving the length and the multiset of keys unchanged.
//! Three phases:
//!
//! 1. Short-circuit: attempt a full sort.
//! 2. `sort_regions`: adaptively grow a sorted-region from each index,
//!    using [`super::find_integer`] for an exponential probe.
//! 3. `sort_regions_with_gaps`: for each index, try sorting the
//!    surrounding region while holding the centre element fixed.
//!
//! The shrink function is generic over an `accept` callback: callers pass
//! a closure that, given a candidate permutation of `[0..n)`, asks the
//! shrinker's `consider` whether the resulting node sequence is still
//! interesting.  The callback returns `true` if the permutation became
//! the new shrink target (and is now reflected in `current`), `false`
//! otherwise.

use super::{ShrinkResult, find_integer_r};
use crate::control::hegel_internal_debug_assert;

/// Run the ordering shrinker over a permutation of `[0..n)`.
///
/// * `keys` is the per-index sort key function; cheaper to compute once
///   up front than re-evaluate it inside every comparator.
/// * `accept(permutation)` returns whether the permutation became the
///   new shrink target.
///
/// The function does not own the permutation it produces: it maintains
/// its own `current` permutation locally and tells the caller, via
/// `accept`, whenever it would like to install a new one.  When `accept`
/// returns `true`, the caller has presumably updated whatever underlying
/// state corresponds to that permutation; the function refreshes its
/// `current` from the new ordering.
pub(super) fn shrink_ordering<T, K, F>(n: usize, mut keys: K, mut accept: F) -> ShrinkResult<()>
where
    T: Ord,
    K: FnMut(usize) -> T,
    F: FnMut(&[usize]) -> ShrinkResult<bool>,
{
    if n <= 1 {
        return Ok(());
    }
    let mut current: Vec<usize> = (0..n).collect();

    // Short-circuit: try a full sort.  If that works there's nothing
    // more to do.
    let sorted_candidate = {
        let mut p = current.clone();
        p.sort_by_key(|&i| keys(i));
        p
    };
    if sorted_candidate != current && accept(&sorted_candidate)? {
        // A full sort is the global optimum under shortlex key
        // ordering, so there's nothing more to do.  No need to update
        // `current` because we return.
        return Ok(());
    }

    // sort_regions: walk from i=0, finding the largest k where sorting
    // current[i..i+k] is accepted; advance by k.
    let mut i = 0;
    while i + 1 < current.len() {
        let snapshot = current.clone();
        let prefix: Vec<usize> = snapshot[..i].to_vec();
        let len = snapshot.len();
        let mut best: Vec<usize> = Vec::new();
        let k = find_integer_r(|k| {
            if i + k > len {
                return Ok(false);
            }
            let mut region: Vec<usize> = snapshot[i..i + k].to_vec();
            region.sort_by_key(|&j| keys(j));
            let mut attempt = prefix.clone();
            attempt.extend_from_slice(&region);
            attempt.extend_from_slice(&snapshot[i + k..]);
            if attempt == snapshot {
                // No actual reordering; treat as a no-op success so the
                // exponential probe keeps growing.
                return Ok(true);
            }
            if accept(&attempt)? {
                best = attempt;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;
        if !best.is_empty() {
            current = best;
        }
        i += k.max(1);
    }

    // sort_regions_with_gaps: holding current[i] fixed, expand the
    // window on each side until sorting the union of the two halves
    // (centre excluded) is no longer accepted.
    let len = current.len();
    if len < 3 {
        return Ok(());
    }
    for i in 1..len - 1 {
        // Skip already-locally-sorted positions
        // (`current[i-1] <= current[i] <= current[i+1]` short-circuit).
        if keys(current[i - 1]) <= keys(current[i]) && keys(current[i]) <= keys(current[i + 1]) {
            continue;
        }
        // Expand right.
        let left = i;
        let mut right = i + 1;
        let snapshot_r = current.clone();
        let i_fixed = i;
        let k_r = find_integer_r(|k| {
            if right + k > snapshot_r.len() {
                return Ok(false);
            }
            try_sort_around(
                &snapshot_r,
                left,
                right + k,
                i_fixed,
                &mut keys,
                &mut accept,
            )
        })?;
        right += k_r;
        // Expand left.  The return value is discarded because `left`
        // is re-initialised at the start of the next iteration.
        let snapshot_l = current.clone();
        find_integer_r(|k| {
            if k > left {
                return Ok(false);
            }
            try_sort_around(
                &snapshot_l,
                left - k,
                right,
                i_fixed,
                &mut keys,
                &mut accept,
            )
        })?;
    }
    Ok(())
}

fn try_sort_around<T, K, F>(
    snapshot: &[usize],
    a: usize,
    b: usize,
    centre: usize,
    keys: &mut K,
    accept: &mut F,
) -> ShrinkResult<bool>
where
    T: Ord,
    K: FnMut(usize) -> T,
    F: FnMut(&[usize]) -> ShrinkResult<bool>,
{
    // `a <= centre < b` allows the boundary case `a == centre`
    // (sort only the right side, with `split = 0`). Callers guarantee
    // these conditions; a debug_assert documents the invariant.
    hegel_internal_debug_assert!(a <= centre && centre < b && b <= snapshot.len());
    let split = centre - a;
    let mut sides: Vec<usize> = snapshot[a..centre].to_vec();
    sides.extend_from_slice(&snapshot[centre + 1..b]);
    sides.sort_by_key(|&j| keys(j));
    let mut attempt: Vec<usize> = snapshot[..a].to_vec();
    attempt.extend_from_slice(&sides[..split]);
    attempt.push(snapshot[centre]);
    attempt.extend_from_slice(&sides[split..]);
    attempt.extend_from_slice(&snapshot[b..]);
    if attempt == snapshot {
        return Ok(true);
    }
    accept(&attempt)
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_ordering_tests.rs"]
mod tests;
