use std::convert::Infallible;

use rand::Rng;
use rand::SeedableRng;
use rand::rand_core::TryRng;
use rand::rngs::StdRng;

use super::{Generator, TestCase, binary, integers};

/// Generator for random number generators. Created by [`randoms()`].
///
/// By default, produces a [`HegelRandom::ArtificialRandom`] backed by the
/// test case data, which allows Hegel to shrink the randomness. Use
/// [`use_true_random()`](Self::use_true_random) to get a seeded `StdRng` instead.
pub struct RandomsGenerator {
    use_true_random: bool,
}

impl RandomsGenerator {
    /// Set whether to use a seeded `StdRng` instead of test-case-backed randomness.
    ///
    /// True random values are not shrinkable.
    pub fn use_true_random(mut self, use_true_random: bool) -> Self {
        self.use_true_random = use_true_random;
        self
    }
}

impl Generator<HegelRandom> for RandomsGenerator {
    fn do_draw(&self, tc: &TestCase) -> HegelRandom {
        if self.use_true_random {
            let seed: u64 = integers().do_draw(tc);
            HegelRandom::TrueRandom(Box::new(StdRng::seed_from_u64(seed)))
        } else {
            HegelRandom::ArtificialRandom(tc.clone())
        }
    }
}

/// A random number generator produced by [`randoms()`].
///
/// Implements [`Rng`] from the `rand` crate.
#[derive(Debug)]
pub enum HegelRandom {
    /// Backed by test case data. Shrinkable.
    ArtificialRandom(TestCase),
    /// Backed by a seeded `StdRng`. Not shrinkable.
    TrueRandom(Box<StdRng>),
}

impl TryRng for HegelRandom {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(match self {
            Self::ArtificialRandom(tc) => integers().do_draw(tc),
            Self::TrueRandom(rng) => rng.next_u32(),
        })
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(match self {
            Self::ArtificialRandom(tc) => integers().do_draw(tc),
            Self::TrueRandom(rng) => rng.next_u64(),
        })
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        match self {
            Self::ArtificialRandom(tc) => {
                let bytes: Vec<u8> = binary()
                    .min_size(dest.len())
                    .max_size(dest.len())
                    .do_draw(tc);
                dest.copy_from_slice(&bytes);
            }
            Self::TrueRandom(rng) => rng.fill_bytes(dest),
        }
        Ok(())
    }
}

/// Creates a generator for random number generators.
///
/// ```no_run
/// use hegel::generators as gs;
/// use rand::RngExt;
/// use rand::prelude::{IndexedRandom, SliceRandom};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let mut rng = tc.draw(gs::randoms());
///
///     let a: i32 = rng.random_range(1..=100);
///     let b: bool = rng.random();
///     let c = vec![1, 2, 3, 4, 5].choose(&mut rng);
///     vec![1, 2, 3].shuffle(&mut rng);
/// }
/// ```
pub fn randoms() -> RandomsGenerator {
    RandomsGenerator {
        use_true_random: false,
    }
}
