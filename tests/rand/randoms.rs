use hegel::TestCase;
use hegel::extras::rand as rand_gs;
use hegel::generators as gs;
use rand::RngExt;
use rand::prelude::{IndexedRandom, SliceRandom};

#[hegel::test]
fn test_randoms_generate(tc: TestCase) {
    let _: bool = tc.draw(rand_gs::randoms()).random();

    let x: i32 = tc.draw(rand_gs::randoms()).random_range(1..=100);
    assert!((1..=100).contains(&x));
}

#[hegel::test]
fn test_randoms_shuffle_preserves_elements(tc: TestCase) {
    let mut rng = tc.draw(rand_gs::randoms());

    let original: Vec<i32> = tc.draw(gs::vecs(gs::integers()));
    let mut shuffled = original.clone();
    shuffled.shuffle(&mut rng);

    let mut sorted_original = original.clone();
    sorted_original.sort();
    shuffled.sort();
    assert_eq!(sorted_original, shuffled);
}

#[hegel::test]
fn test_randoms_choose(tc: TestCase) {
    let mut rng = tc.draw(rand_gs::randoms());
    let items: Vec<i32> = tc.draw(gs::vecs(gs::integers()).min_size(1));
    let picked = items.choose(&mut rng).unwrap();
    assert!(items.contains(picked));
}

// `rng.fill` draws from `gs::binary` under the hood (see
// `src/extras/rand/generators.rs::try_fill_bytes`), which the native
// backend hasn't shipped yet.  Gate to the server backend until then.
#[cfg(not(feature = "native"))]
#[hegel::test]
fn test_randoms_fill(tc: TestCase) {
    let mut rng = tc.draw(rand_gs::randoms());
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
}

#[hegel::test]
fn test_true_random(tc: TestCase) {
    let mut rng = tc.draw(rand_gs::randoms().use_true_random(true));
    let x: i32 = rng.random_range(1..=100);
    assert!((1..=100).contains(&x));
}

#[hegel::test]
fn test_randoms_composes(tc: TestCase) {
    let _ = tc.draw(gs::vecs(rand_gs::randoms()));
}

#[hegel::test]
fn test_randoms_u64(tc: TestCase) {
    let _: u64 = tc.draw(rand_gs::randoms()).random();
}

#[hegel::test]
fn test_true_randoms_u64(tc: TestCase) {
    let _: u64 = tc.draw(rand_gs::randoms().use_true_random(true)).random();
}

#[hegel::test]
fn test_true_randoms_fill(tc: TestCase) {
    let mut rng = tc.draw(rand_gs::randoms().use_true_random(true));
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
}
