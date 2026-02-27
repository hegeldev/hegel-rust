//! Random number generator integration with the `rand` crate.
//!
//! This module is only available with the `rand` feature enabled.
//!
//! # Example
//!
//! ```no_run
//! use hegel::generators::randoms;
//! use rand::Rng;
//! use rand::prelude::{IndexedRandom, SliceRandom};
//!
//! # hegel::hegel(|| {
//! let mut rng = hegel::draw(&randoms());
//!
//! // Use any rand::Rng method
//! let n: i32 = rng.random_range(1..=100);
//! let b: bool = rng.random();
//!
//! // Use rand::seq::SliceRandom
//! let items = vec![1, 2, 3, 4, 5];
//! let picked = items.choose(&mut rng);
//!
//! let mut to_shuffle = vec![1, 2, 3];
//! to_shuffle.shuffle(&mut rng);
//! # });
//! ```

use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

use super::{binary, integers, test_case_data, Generate, TestCaseData};

/// Generator for [`HegelRandom`] instances.
///
/// Created via [`randoms()`].
pub struct RandomsGenerator {
    use_true_random: bool,
}

impl RandomsGenerator {
    /// Use true randomness instead of artificial randomness.
    ///
    /// When enabled, a single seed is generated via Hypothesis, then all
    /// subsequent random operations use a local `StdRng` seeded with that value.
    ///
    /// This is faster (no round-trips per operation) but shrinking only
    /// affects the seed, not individual random values.
    pub fn use_true_random(mut self) -> Self {
        self.use_true_random = true;
        self
    }
}

impl Generate<HegelRandom> for RandomsGenerator {
    fn do_draw(&self, data: &TestCaseData) -> HegelRandom {
        if self.use_true_random {
            let seed: u64 = integers().do_draw(data);
            HegelRandom::True(Box::new(StdRng::seed_from_u64(seed)))
        } else {
            HegelRandom::Artificial
        }
    }
}

/// Creates a generator for random number generators.
///
/// Returns a generator that produces [`HegelRandom`] instances implementing
/// [`rand::RngCore`]. This enables use with the full `rand` ecosystem including
/// `rand::Rng` methods and `rand::seq::SliceRandom`.
///
/// # Modes
///
/// - **Artificial (default)**: Each RNG operation sends a request to Hypothesis,
///   enabling effective shrinking of random values.
///
/// - **True random**: Call [`.use_true_random()`](RandomsGenerator::use_true_random)
///   to generate a seed once, then use a local RNG. Faster, but only the seed shrinks.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::randoms;
/// use rand::Rng;
///
/// # hegel::hegel(|| {
/// let mut rng = hegel::draw(&randoms());
/// let x: f64 = rng.random();
/// let n = rng.random_range(1..=100);
/// # });
/// ```
pub fn randoms() -> RandomsGenerator {
    RandomsGenerator {
        use_true_random: false,
    }
}

/// A random number generator that integrates with Hypothesis.
///
/// Implements [`rand::RngCore`], so it can be used anywhere the `rand` crate
/// expects an RNG. The [`rand::Rng`] trait is automatically available via
/// blanket impl, providing `random()`, `random_range()`, `random_bool()`, etc.
///
/// [`rand::seq::SliceRandom`] also works, providing `choose()`, `shuffle()`,
/// and `choose_multiple()` on slices.
#[derive(Debug)]
pub enum HegelRandom {
    /// Each operation proxies through Hypothesis for shrinking.
    Artificial,
    /// Uses a seeded local RNG.
    True(Box<StdRng>),
}

// Dummy Deserialize impl to satisfy trait bounds when composing with vecs(), etc.
// This is never called at runtime since randoms().schema() returns None,
// causing collection generators to use compositional fallback.
impl<'de> serde::Deserialize<'de> for HegelRandom {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        unreachable!("HegelRandom::deserialize should never be called")
    }
}

impl RngCore for HegelRandom {
    fn next_u32(&mut self) -> u32 {
        match self {
            Self::Artificial => {
                let data = test_case_data();
                integers().do_draw(data)
            }
            Self::True(rng) => rng.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match self {
            Self::Artificial => {
                let data = test_case_data();
                integers().do_draw(data)
            }
            Self::True(rng) => rng.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        match self {
            Self::Artificial => {
                let data = test_case_data();
                let bytes: Vec<u8> = binary()
                    .with_min_size(dest.len())
                    .with_max_size(dest.len())
                    .do_draw(data);
                dest.copy_from_slice(&bytes);
            }
            Self::True(rng) => rng.fill_bytes(dest),
        }
    }
}
