//! Sequence-reordering primitive used by `reorder_spans`.
//!
//! The algorithm walks a permutation of `[0..n)` and tries to make the
//! resulting key-ordered sequence more sorted under the supplied key
//! function, leaving the length and the multiset of keys unchanged.
//! Three phases:
//!
//! 1. Short-circuit: attempt a full sort.
//! 2. `sort_regions`: adaptively grow a sorted-region from each index,
//!    using [`FindInteger`] for an exponential probe.
//! 3. `sort_regions_with_gaps`: for each index, try sorting the
//!    surrounding region while holding the centre element fixed.
//!
//! The shrink function is generic over an `accept` judge: callers pass
//! a [`PermutationJudge`] that, given a candidate permutation of `[0..n)`,
//! asks the shrinker's `consider` whether the resulting node sequence is
//! still interesting.  The judge returns `true` if the permutation became
//! the new shrink target (and is now reflected in `current`), `false`
//! otherwise.

use std::future::Future;
use std::pin::Pin;

use super::ShrinkResult;
use super::search::FindInteger;
use crate::control::hegel_internal_debug_assert;

/// The boxed future a [`PermutationJudge`] resolves to: whether the
/// permutation became the new shrink target.
pub(super) type JudgeFuture<'s> = Pin<Box<dyn Future<Output = ShrinkResult<bool>> + Send + 's>>;

/// Decides whether a candidate permutation becomes the new shrink target.
///
/// Like [`ShrinkProbe`](super::ShrinkProbe), the method hand-desugars
/// `async fn` into a boxed future so the judge may borrow itself while
/// suspended — the real judge runs the shrinker's `consider`, which hands
/// the test case to the driver. Synchronous judges (the shape the ordering
/// unit tests use) get this for free through the blanket [`FnMut`] impl.
pub(super) trait PermutationJudge {
    fn accept<'s>(&'s mut self, permutation: &'s [usize]) -> JudgeFuture<'s>;
}

impl<F> PermutationJudge for F
where
    F: FnMut(&[usize]) -> ShrinkResult<bool>,
{
    fn accept<'s>(&'s mut self, permutation: &'s [usize]) -> JudgeFuture<'s> {
        Box::pin(std::future::ready(self(permutation)))
    }
}

/// Run the ordering shrinker over a permutation of `[0..n)`.
///
/// * `keys` is the per-index sort key function; cheaper to compute once
///   up front than re-evaluate it inside every comparator.
/// * `accept.accept(permutation)` returns whether the permutation became
///   the new shrink target.
///
/// The function does not own the permutation it produces: it maintains
/// its own `current` permutation locally and tells the caller, via
/// `accept`, whenever it would like to install a new one.  When `accept`
/// returns `true`, the caller has presumably updated whatever underlying
/// state corresponds to that permutation; the function refreshes its
/// `current` from the new ordering.
pub(super) async fn shrink_ordering<T, K, F>(
    n: usize,
    mut keys: K,
    mut accept: F,
) -> ShrinkResult<()>
where
    T: Ord,
    K: FnMut(usize) -> T,
    F: PermutationJudge,
{
    if n <= 1 {
        return Ok(());
    }
    let mut current: Vec<usize> = (0..n).collect();

    let sorted_candidate = {
        let mut p = current.clone();
        p.sort_by_key(|&i| keys(i));
        p
    };
    if sorted_candidate != current && accept.accept(&sorted_candidate).await? {
        return Ok(());
    }

    let mut i = 0;
    while i + 1 < current.len() {
        let snapshot = current.clone();
        let prefix: Vec<usize> = snapshot[..i].to_vec();
        let len = snapshot.len();
        let mut best: Vec<usize> = Vec::new();
        let mut search = FindInteger::new();
        while let Some(k) = search.probe() {
            let ok = if i + k > len {
                false
            } else {
                let mut region: Vec<usize> = snapshot[i..i + k].to_vec();
                region.sort_by_key(|&j| keys(j));
                let mut attempt = prefix.clone();
                attempt.extend_from_slice(&region);
                attempt.extend_from_slice(&snapshot[i + k..]);
                if attempt == snapshot {
                    true
                } else if accept.accept(&attempt).await? {
                    best = attempt;
                    true
                } else {
                    false
                }
            };
            search.record(ok);
        }
        let k = search.result();
        if !best.is_empty() {
            current = best;
        }
        i += k.max(1);
    }

    let len = current.len();
    if len < 3 {
        return Ok(());
    }
    for i in 1..len - 1 {
        if keys(current[i - 1]) <= keys(current[i]) && keys(current[i]) <= keys(current[i + 1]) {
            continue;
        }
        let left = i;
        let mut right = i + 1;
        let snapshot_r = current.clone();
        let i_fixed = i;
        let mut search = FindInteger::new();
        while let Some(k) = search.probe() {
            let ok = if right + k > snapshot_r.len() {
                false
            } else {
                try_sort_around(
                    &snapshot_r,
                    left,
                    right + k,
                    i_fixed,
                    &mut keys,
                    &mut accept,
                )
                .await?
            };
            search.record(ok);
        }
        right += search.result();
        let snapshot_l = current.clone();
        let mut search = FindInteger::new();
        while let Some(k) = search.probe() {
            let ok = if k > left {
                false
            } else {
                try_sort_around(
                    &snapshot_l,
                    left - k,
                    right,
                    i_fixed,
                    &mut keys,
                    &mut accept,
                )
                .await?
            };
            search.record(ok);
        }
    }
    Ok(())
}

async fn try_sort_around<T, K, F>(
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
    F: PermutationJudge,
{
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
    accept.accept(&attempt).await
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_ordering_tests.rs"]
mod tests;
