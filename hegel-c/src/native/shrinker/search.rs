//! Adaptive search state machines used by the shrink passes.
//!
//! Each search is a plain value driven by a probe/record loop: the caller
//! asks [`probe`](FindInteger::probe) for the next candidate, evaluates the
//! predicate however it likes (typically an `.await` on the shrinker), and
//! feeds the verdict back with [`record`](FindInteger::record) until `probe`
//! returns `None`, at which point [`result`](FindInteger::result) is the
//! answer. Expressing the searches as values rather than
//! higher-order functions keeps the async shrink passes free of
//! callback-held borrows, which the trait solver cannot prove `Send`
//! (rust-lang/rust#110338); a `while let` loop over a search value has no
//! such trouble.
//!
//! ```text
//! let mut search = FindInteger::new();
//! while let Some(k) = search.probe() {
//!     search.record(predicate(k).await?);
//! }
//! let n = search.result();
//! ```

use crate::native::bignum::BigInt;

/// Finds a (hopefully large) integer `n >= 0` such that `f(n)` is true and
/// `f(n+1)` is false, where `f` is the predicate the caller evaluates for
/// each probed value. `f(0)` is assumed to be true and is never probed.
///
/// Used by shrink passes that want to maximise a step size — e.g. "lower
/// both nodes by k" needs the largest k for which the joint replacement
/// is still interesting.
///
/// Probes 1..=4 linearly, then grows exponentially, then binary-searches
/// the final gap. Uses `checked_mul` on the exponential probe and
/// `lo + (hi - lo) / 2` on the binary-search midpoint: a predicate that
/// accepts an unbounded range (e.g. a `lower_integers_together` pass over
/// full-range `i128` nodes) would otherwise walk the probe off the end of
/// `usize`.
pub(crate) struct FindInteger {
    state: FindIntegerState,
}

enum FindIntegerState {
    /// Probing `1..=4` one at a time.
    Linear(usize),
    /// `f(lo)` held; probing `hi`, doubling on success.
    Grow { lo: usize, hi: usize },
    /// `f(lo)` held and `f(hi)` failed; probing midpoints.
    Narrow { lo: usize, hi: usize },
    /// Converged on the answer.
    Done(usize),
}

impl FindInteger {
    pub(crate) fn new() -> Self {
        FindInteger {
            state: FindIntegerState::Linear(1),
        }
    }

    /// The next value to evaluate the predicate at, or `None` once the
    /// search has converged.
    pub(crate) fn probe(&self) -> Option<usize> {
        match self.state {
            FindIntegerState::Linear(i) => Some(i),
            FindIntegerState::Grow { hi, .. } => Some(hi),
            FindIntegerState::Narrow { lo, hi } => Some(lo + (hi - lo) / 2),
            FindIntegerState::Done(_) => None,
        }
    }

    /// Record the predicate's verdict for the value last returned by
    /// [`Self::probe`].
    pub(crate) fn record(&mut self, ok: bool) {
        self.state = match self.state {
            FindIntegerState::Linear(i) if !ok => FindIntegerState::Done(i - 1),
            FindIntegerState::Linear(i) if i < 4 => FindIntegerState::Linear(i + 1),
            FindIntegerState::Linear(_) => FindIntegerState::Grow { lo: 4, hi: 5 },
            FindIntegerState::Grow { hi, .. } if ok => match hi.checked_mul(2) {
                Some(next) => FindIntegerState::Grow { lo: hi, hi: next },
                None => FindIntegerState::Done(hi),
            },
            FindIntegerState::Grow { lo, hi } => Self::narrow_or_done(lo, hi),
            FindIntegerState::Narrow { lo, hi } => {
                let mid = lo + (hi - lo) / 2;
                if ok {
                    Self::narrow_or_done(mid, hi)
                } else {
                    Self::narrow_or_done(lo, mid)
                }
            }
            FindIntegerState::Done(v) => FindIntegerState::Done(v),
        };
    }

    fn narrow_or_done(lo: usize, hi: usize) -> FindIntegerState {
        if lo + 1 < hi {
            FindIntegerState::Narrow { lo, hi }
        } else {
            FindIntegerState::Done(lo)
        }
    }

    /// The largest probed value for which the predicate held (`0` if the
    /// very first probe failed). Only meaningful once [`Self::probe`] has
    /// returned `None`.
    pub(crate) fn result(&self) -> usize {
        match self.state {
            FindIntegerState::Done(v) => v,
            _ => unreachable!("result read before the search converged"),
        }
    }
}

/// Binary search for the smallest value in `[lo, hi]` where the caller's
/// predicate is true. Assumes the predicate holds at `hi` (not probed).
/// Probes `lo` first, so a predicate true at `lo` costs one evaluation.
pub(super) struct BinSearchDown {
    state: BinSearchState<i128>,
}

enum BinSearchState<T> {
    CheckLo { lo: T, hi: T },
    Narrow { lo: T, hi: T },
    Done(T),
}

impl BinSearchDown {
    pub(super) fn new(lo: i128, hi: i128) -> Self {
        BinSearchDown {
            state: BinSearchState::CheckLo { lo, hi },
        }
    }

    /// The next value to evaluate the predicate at, or `None` once the
    /// search has converged.
    pub(super) fn probe(&self) -> Option<i128> {
        match self.state {
            BinSearchState::CheckLo { lo, .. } => Some(lo),
            BinSearchState::Narrow { lo, hi } => Some(lo + (hi - lo) / 2),
            BinSearchState::Done(_) => None,
        }
    }

    /// Record the predicate's verdict for the value last returned by
    /// [`Self::probe`].
    pub(super) fn record(&mut self, ok: bool) {
        self.state = match self.state {
            BinSearchState::CheckLo { lo, .. } if ok => BinSearchState::Done(lo),
            BinSearchState::CheckLo { lo, hi } => Self::narrow_or_done(lo, hi),
            BinSearchState::Narrow { lo, hi } => {
                let mid = lo + (hi - lo) / 2;
                if ok {
                    Self::narrow_or_done(lo, mid)
                } else {
                    Self::narrow_or_done(mid, hi)
                }
            }
            BinSearchState::Done(v) => BinSearchState::Done(v),
        };
    }

    fn narrow_or_done(lo: i128, hi: i128) -> BinSearchState<i128> {
        if lo.checked_add(1).is_some_and(|n| n < hi) {
            BinSearchState::Narrow { lo, hi }
        } else {
            BinSearchState::Done(hi)
        }
    }

    /// The smallest locally-true value found. Only meaningful once
    /// [`Self::probe`] has returned `None`. Test-only: every current pass
    /// consumes the search through the side effects of its probes.
    #[cfg(test)]
    pub(super) fn result(&self) -> i128 {
        match self.state {
            BinSearchState::Done(v) => v,
            _ => unreachable!("result read before the search converged"),
        }
    }
}

/// [`BigInt`] counterpart of [`BinSearchDown`], used by the integer shrink
/// passes which carry values as arbitrary-precision integers. Same
/// contract: assumes the predicate holds at `hi`, converges on the smallest
/// locally-true value in `[lo, hi]`.
pub(super) struct BinSearchDownBig {
    state: BinSearchState<BigInt>,
}

impl BinSearchDownBig {
    pub(super) fn new(lo: BigInt, hi: BigInt) -> Self {
        BinSearchDownBig {
            state: BinSearchState::CheckLo { lo, hi },
        }
    }

    /// The next value to evaluate the predicate at, or `None` once the
    /// search has converged.
    pub(super) fn probe(&self) -> Option<BigInt> {
        match &self.state {
            BinSearchState::CheckLo { lo, .. } => Some(lo.clone()),
            BinSearchState::Narrow { lo, hi } => Some(lo + (hi - lo) / 2),
            BinSearchState::Done(_) => None,
        }
    }

    /// Record the predicate's verdict for the value last returned by
    /// [`Self::probe`].
    pub(super) fn record(&mut self, ok: bool) {
        self.state = match std::mem::replace(&mut self.state, BinSearchState::Done(BigInt::from(0)))
        {
            BinSearchState::CheckLo { lo, .. } if ok => BinSearchState::Done(lo),
            BinSearchState::CheckLo { lo, hi } => Self::narrow_or_done(lo, hi),
            BinSearchState::Narrow { lo, hi } => {
                let mid = &lo + (&hi - &lo) / 2;
                if ok {
                    Self::narrow_or_done(lo, mid)
                } else {
                    Self::narrow_or_done(mid, hi)
                }
            }
            BinSearchState::Done(v) => BinSearchState::Done(v),
        };
    }

    fn narrow_or_done(lo: BigInt, hi: BigInt) -> BinSearchState<BigInt> {
        if &lo + 1 < hi {
            BinSearchState::Narrow { lo, hi }
        } else {
            BinSearchState::Done(hi)
        }
    }

    /// The smallest locally-true value found. Only meaningful once
    /// [`Self::probe`] has returned `None`. Test-only: every current pass
    /// consumes the search through the side effects of its probes.
    #[cfg(test)]
    pub(super) fn result(self) -> BigInt {
        match self.state {
            BinSearchState::Done(v) => v,
            _ => unreachable!("result read before the search converged"),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_search_tests.rs"]
mod tests;
