use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

use super::{binary, integers, test_case_data, Generator, TestCaseData};

pub struct RandomsGenerator {
    use_true_random: bool,
}

impl RandomsGenerator {
    /// Use true randomness instead of ArtificialRandom randomness.
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

impl Generator<HegelRandom> for RandomsGenerator {
    fn do_draw(&self, data: &TestCaseData) -> HegelRandom {
        if self.use_true_random {
            let seed: u64 = integers().do_draw(data);
            HegelRandom::TrueRandom(Box::new(StdRng::seed_from_u64(seed)))
        } else {
            HegelRandom::ArtificialRandom
        }
    }
}

#[derive(Debug)]
pub enum HegelRandom {
    ArtificialRandom,
    TrueRandom(Box<StdRng>),
}

impl RngCore for HegelRandom {
    fn next_u32(&mut self) -> u32 {
        match self {
            Self::ArtificialRandom => {
                let data = test_case_data().expect(
                    "Can't use random instances from randoms() used outside of a Hegel test",
                );
                integers().do_draw(data)
            }
            Self::TrueRandom(rng) => rng.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match self {
            Self::ArtificialRandom => {
                let data = test_case_data().expect(
                    "Can't use random instances from randoms() used outside of a Hegel test",
                );
                integers().do_draw(data)
            }
            Self::TrueRandom(rng) => rng.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        match self {
            Self::ArtificialRandom => {
                let data = test_case_data().expect(
                    "Can't use random instances from randoms() used outside of a Hegel test",
                );
                let bytes: Vec<u8> = binary()
                    .min_size(dest.len())
                    .max_size(dest.len())
                    .do_draw(data);
                dest.copy_from_slice(&bytes);
            }
            Self::TrueRandom(rng) => rng.fill_bytes(dest),
        }
    }
}

/// Creates a generator for random number generators.
///
/// ```no_run
/// use hegel::generators::randoms;
/// use rand::Rng;
/// use rand::prelude::{IndexedRandom, SliceRandom};
///
/// #[hegel::test]
/// fn my_test() {
///     let mut rng = hegel::draw(&randoms());
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
