// Embedded tests for src/native/rng.rs — the EngineRng abstraction over a
// seeded PRNG and the /dev/urandom-backed source.

use rand::{Rng, RngExt};

use super::*;

#[test]
fn seeded_is_deterministic() {
    let mut a = EngineRng::seeded(42);
    let mut b = EngineRng::seeded(42);
    let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    assert_eq!(xs, ys);
}

#[test]
fn seeded_different_seeds_differ() {
    let mut a = EngineRng::seeded(1);
    let mut b = EngineRng::seeded(2);
    // Overwhelmingly likely to differ within a handful of draws.
    let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    assert_ne!(xs, ys);
}

#[test]
fn prng_methods_match_inner_small_rng() {
    // The Prng variant must delegate each method to the inner SmallRng's
    // native method, so that a given seed reproduces the exact same stream
    // it did before EngineRng existed.
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    let mut engine = EngineRng::seeded(99);
    let mut inner = SmallRng::seed_from_u64(99);

    assert_eq!(engine.next_u32(), inner.next_u32());
    assert_eq!(engine.next_u64(), inner.next_u64());

    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    engine.fill_bytes(&mut a);
    inner.fill_bytes(&mut b);
    assert_eq!(a, b);
}

#[test]
fn prng_spawn_produces_working_child() {
    let mut parent = EngineRng::seeded(7);
    let mut child = parent.spawn();
    // Child draws without panicking and exposes the RngExt surface.
    let _: u64 = child.random();
    let _: bool = child.random();
}

#[test]
fn prng_spawn_is_deterministic_from_seed() {
    let mut a = EngineRng::seeded(7);
    let mut b = EngineRng::seeded(7);
    let mut ca = a.spawn();
    let mut cb = b.spawn();
    let xs: Vec<u64> = (0..4).map(|_| ca.next_u64()).collect();
    let ys: Vec<u64> = (0..4).map(|_| cb.next_u64()).collect();
    assert_eq!(xs, ys);
}

#[test]
fn from_os_draws_without_panicking() {
    let mut rng = EngineRng::from_os();
    let _: u64 = rng.next_u64();
    let _: u32 = rng.random();
}

#[cfg(unix)]
#[test]
fn urandom_fills_all_widths() {
    let mut rng = EngineRng::urandom();
    // Exercise every method (next_u32, next_u64, fill_bytes) on the urandom
    // arm, which all funnel through UrandomRng::read_exact.
    let _ = rng.next_u32();
    let _ = rng.next_u64();
    let mut buf = [0u8; 32];
    rng.fill_bytes(&mut buf);
}

#[cfg(unix)]
#[test]
fn urandom_output_varies() {
    let mut rng = EngineRng::urandom();
    // /dev/urandom must not return a constant; collect several draws and
    // confirm they are not all identical (all-equal has negligible
    // probability).
    let draws: std::collections::HashSet<u64> = (0..16).map(|_| rng.next_u64()).collect();
    assert!(draws.len() > 1, "urandom returned a constant stream");
}

#[cfg(unix)]
#[test]
fn urandom_spawn_is_another_urandom_reader() {
    let mut rng = EngineRng::urandom();
    let mut child = rng.spawn();
    assert!(matches!(child, EngineRng::Urandom(_)));
    // The child reads from /dev/urandom too.
    let _ = child.next_u64();
}
