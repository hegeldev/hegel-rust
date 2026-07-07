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
    let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    assert_ne!(xs, ys);
}

#[test]
fn prng_methods_match_inner_small_rng() {
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
    let _ = rng.next_u32();
    let _ = rng.next_u64();
    let mut buf = [0u8; 32];
    rng.fill_bytes(&mut buf);
}

#[cfg(unix)]
#[test]
fn urandom_output_varies() {
    let mut rng = EngineRng::urandom();
    let draws: std::collections::HashSet<u64> = (0..16).map(|_| rng.next_u64()).collect();
    assert!(draws.len() > 1, "urandom returned a constant stream");
}

#[cfg(unix)]
#[test]
fn urandom_spawn_is_another_urandom_reader() {
    let mut rng = EngineRng::urandom();
    let mut child = rng.spawn();
    assert!(matches!(child, EngineRng::Urandom(_)));
    let _ = child.next_u64();
}

/// Pins literal values of the seeded stream so a silent stream change on a
/// `rand` upgrade (SmallRng's algorithm is not stable across releases) is
/// caught here rather than quietly breaking every stored seed. If this test
/// fails after a deliberate `rand` bump, update the values and call out the
/// broken seed reproducibility in the changelog.
#[test]
fn seeded_stream_is_pinned() {
    let mut rng = EngineRng::seeded(42);
    let got: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
    assert_eq!(
        got,
        vec![
            15021278609987233951,
            5881210131331364753,
            18149643915985481100,
            12933668939759105464,
        ]
    );
}
