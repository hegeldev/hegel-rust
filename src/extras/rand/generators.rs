use std::convert::Infallible;

use rand::Rng;
use rand::SeedableRng;
use rand::rand_core::TryRng;
use rand::rngs::StdRng;

use crate::generators::{Generator, PrintableGenerator, TestCase, binary, integers};
use crate::pretty::PrettyPrinter;

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
            HegelRandom {
                source: RandomSource::True(Box::new(StdRng::seed_from_u64(seed))),
            }
        } else {
            HegelRandom {
                source: RandomSource::Artificial(tc.clone(), None),
            }
        }
    }
}

/// Printing a drawn RNG is deferred: at draw time nothing is known about it
/// yet, so a hole is reserved in the document and every value the RNG hands
/// out during the test body is recorded into it. The failing example then
/// shows `HegelRandom { consumed: [v1, v2, …] }` — the random values the
/// test actually consumed. A seeded true-random RNG prints as
/// `HegelRandom { seed: … }` instead, since its values are not drawn
/// through the engine.
impl PrintableGenerator<HegelRandom> for RandomsGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HegelRandom {
        if self.use_true_random {
            let seed: u64 = integers().do_draw(tc);
            printer.text(&format!("HegelRandom {{ seed: {seed} }}"));
            return HegelRandom {
                source: RandomSource::True(Box::new(StdRng::seed_from_u64(seed))),
            };
        }
        printer.begin_group(1, "HegelRandom { consumed: [");
        let slot = printer.clone();
        printer.end_group("] }");
        HegelRandom {
            source: RandomSource::Artificial(
                tc.clone(),
                Some(RngPrintSlot {
                    printer: slot,
                    recorded: 0,
                }),
            ),
        }
    }
}

/// Records the values an artificially-random [`HegelRandom`] hands out into
/// the child region cloned off the printer at draw time.
#[derive(Debug)]
struct RngPrintSlot {
    printer: PrettyPrinter,
    recorded: usize,
}

impl RngPrintSlot {
    fn record(&mut self, value: std::fmt::Arguments<'_>) {
        if self.recorded > 0 {
            self.printer.text(",");
            self.printer.breakable(" ");
        }
        self.recorded += 1;
        self.printer.text(&value.to_string());
    }
}

/// A random number generator produced by [`randoms()`].
///
/// Implements [`Rng`] from the `rand` crate. How it is backed — engine-drawn
/// (shrinkable) data by default, or a seeded `StdRng` after
/// [`use_true_random`](RandomsGenerator::use_true_random) — is internal.
#[derive(Debug)]
pub struct HegelRandom {
    source: RandomSource,
}

/// What a [`HegelRandom`] hands out when asked for randomness.
#[derive(Debug)]
enum RandomSource {
    /// Backed by test case data. Shrinkable. The second field records the
    /// values handed out into the failing-example output; it is `Some` only
    /// when the RNG was drawn by a test case whose output is being reported.
    Artificial(TestCase, Option<RngPrintSlot>),
    /// Backed by a seeded `StdRng`. Not shrinkable.
    True(Box<StdRng>),
}

impl TryRng for HegelRandom {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(match &mut self.source {
            RandomSource::Artificial(tc, slot) => {
                let value: u32 = integers().do_draw(tc);
                if let Some(slot) = slot {
                    slot.record(format_args!("{value}"));
                }
                value
            }
            RandomSource::True(rng) => rng.next_u32(),
        })
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(match &mut self.source {
            RandomSource::Artificial(tc, slot) => {
                let value: u64 = integers().do_draw(tc);
                if let Some(slot) = slot {
                    slot.record(format_args!("{value}"));
                }
                value
            }
            RandomSource::True(rng) => rng.next_u64(),
        })
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        match &mut self.source {
            RandomSource::Artificial(tc, slot) => {
                let bytes: Vec<u8> = binary()
                    .min_size(dest.len())
                    .max_size(dest.len())
                    .do_draw(tc);
                dest.copy_from_slice(&bytes);
                if let Some(slot) = slot {
                    slot.record(format_args!("{bytes:?}"));
                }
            }
            RandomSource::True(rng) => rng.fill_bytes(dest),
        }
        Ok(())
    }
}

/// Creates a generator for random number generators.
///
/// See [`RandomsGenerator`] for builder methods.
///
/// ```no_run
/// use hegel::extras::rand as rand_gs;
/// use rand::RngExt;
/// use rand::prelude::{IndexedRandom, SliceRandom};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let mut rng = tc.draw(rand_gs::randoms());
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
