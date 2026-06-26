//! The random source the native engine draws from.
//!
//! [`EngineRng`] is the single RNG type threaded through every random draw in
//! the native engine (the `biased_*_sample` samplers, the data-tree novel
//! prefix walk, targeting). It is an enum over two sources:
//!
//!   * [`EngineRng::Prng`] — a seeded [`SmallRng`]. This is the default and
//!     drives reproducible, seed-controlled generation.
//!   * [`EngineRng::Urandom`] — reads fresh bytes from `/dev/urandom` on every
//!     draw. Selected by `Backend::Urandom` (and automatically under
//!     Antithesis). It exists so that an external controller of the OS random
//!     source — the Antithesis fuzzer — controls every choice the engine makes,
//!     rather than only the seed of a PRNG that then expands deterministically.
//!     Mirrors Hypothesis's `URandomProvider`, which is the ordinary
//!     `HypothesisProvider` with its `Random` swapped for one reading from
//!     `/dev/urandom`.
//!
//! The `Prng` variant delegates each method to the inner `SmallRng`'s native
//! method (rather than routing everything through `fill_bytes`) so that a given
//! seed produces the exact same stream it did before `EngineRng` existed.

use std::convert::Infallible;

use rand::TryRng;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// A source of randomness reading directly from `/dev/urandom`.
///
/// Stateless: every read opens `/dev/urandom` fresh with userspace buffering
/// disabled and reads exactly the requested number of bytes. Opening fresh per
/// read (rather than holding an open handle or buffering ahead) is deliberate —
/// it ensures an external controller hooking `/dev/urandom` reads (Antithesis)
/// observes each draw as its own read of exactly the size the engine needs.
/// Mirrors Hypothesis's `URandom._urandom`, which opens with `buffering=0`.
#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
pub struct UrandomRng;

#[cfg(unix)]
impl UrandomRng {
    fn read_exact(dst: &mut [u8]) {
        use std::io::Read;
        let mut file = std::fs::File::open("/dev/urandom").expect("failed to open /dev/urandom");
        file.read_exact(dst)
            .expect("failed to read from /dev/urandom");
    }
}

/// The random source the native engine draws from. See the module docs.
#[derive(Debug)]
pub enum EngineRng {
    /// A seeded pseudo-random generator (the default backend).
    Prng(SmallRng),
    /// Fresh OS entropy from `/dev/urandom` on every draw (the urandom backend).
    #[cfg(unix)]
    Urandom(UrandomRng),
}

impl EngineRng {
    /// A PRNG seeded deterministically from `seed`.
    pub fn seeded(seed: u64) -> Self {
        EngineRng::Prng(SmallRng::seed_from_u64(seed))
    }

    /// A PRNG seeded from the operating system's entropy source. Each process
    /// run draws a different stream.
    pub fn from_os() -> Self {
        EngineRng::Prng(SmallRng::from_rng(&mut rand::rng()))
    }

    /// The urandom backend: every draw reads fresh from `/dev/urandom`.
    ///
    /// On platforms without `/dev/urandom` (Windows) there is no urandom
    /// source to read, so this falls back to an OS-seeded PRNG — matching
    /// Hypothesis, which warns and falls back to `backend="hypothesis"` there.
    #[cfg(unix)]
    pub fn urandom() -> Self {
        EngineRng::Urandom(UrandomRng)
    }

    #[cfg(not(unix))]
    pub fn urandom() -> Self {
        eprintln!(
            "warning: the urandom backend reads /dev/urandom, which is not \
             available on this platform; falling back to an OS-seeded PRNG \
             (equivalent to the default backend)."
        );
        Self::from_os()
    }

    /// Derive an independent child RNG for a sub-task (e.g. a single test
    /// case in a batch).
    ///
    /// For [`EngineRng::Prng`] this seeds a fresh `SmallRng` from the parent —
    /// the same `SmallRng::from_rng` derivation the engine used before
    /// `EngineRng` existed, so seeded trajectories are unchanged. For
    /// [`EngineRng::Urandom`] every reader is an equivalent stateless view of
    /// `/dev/urandom`, so the child is simply another urandom reader.
    pub fn spawn(&mut self) -> EngineRng {
        match self {
            EngineRng::Prng(rng) => EngineRng::Prng(SmallRng::from_rng(rng)),
            #[cfg(unix)]
            EngineRng::Urandom(_) => EngineRng::Urandom(UrandomRng),
        }
    }
}

impl TryRng for EngineRng {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok(match self {
            EngineRng::Prng(rng) => rng.next_u32(),
            #[cfg(unix)]
            EngineRng::Urandom(_) => {
                let mut buf = [0u8; 4];
                UrandomRng::read_exact(&mut buf);
                u32::from_le_bytes(buf)
            }
        })
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        Ok(match self {
            EngineRng::Prng(rng) => rng.next_u64(),
            #[cfg(unix)]
            EngineRng::Urandom(_) => {
                let mut buf = [0u8; 8];
                UrandomRng::read_exact(&mut buf);
                u64::from_le_bytes(buf)
            }
        })
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        match self {
            EngineRng::Prng(rng) => rng.fill_bytes(dst),
            #[cfg(unix)]
            EngineRng::Urandom(_) => UrandomRng::read_exact(dst),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/rng_tests.rs"]
mod tests;
