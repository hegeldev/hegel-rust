#![cfg(feature = "rand")]

use hegel::generators::{integers, randoms, vecs};
use rand::prelude::{IndexedRandom, SliceRandom};
use rand::Rng;

#[test]
fn test_randoms_generate() {
    hegel::hegel(|| {
        let _: bool = hegel::draw(&randoms()).random();

        let x: i32 = hegel::draw(&randoms()).random_range(1..=100);
        assert!((1..=100).contains(&x));
    });
}

#[test]
fn test_randoms_shuffle_preserves_elements() {
    hegel::hegel(|| {
        let mut rng = hegel::draw(&randoms());

        let original: Vec<i32> = hegel::draw(&vecs(integers()));
        let mut shuffled = original.clone();
        shuffled.shuffle(&mut rng);

        let mut sorted_original = original.clone();
        sorted_original.sort();
        shuffled.sort();
        assert_eq!(sorted_original, shuffled);
    });
}

#[test]
fn test_randoms_choose() {
    hegel::hegel(|| {
        let mut rng = hegel::draw(&randoms());
        let items: Vec<i32> = hegel::draw(&vecs(integers()).with_min_size(1));
        let picked = items.choose(&mut rng).unwrap();
        assert!(items.contains(picked));
    });
}

#[test]
fn test_randoms_fill() {
    hegel::hegel(|| {
        let mut rng = hegel::draw(&randoms());
        let mut bytes = [0u8; 16];
        rng.fill(&mut bytes);
    });
}

#[test]
fn test_true_random() {
    hegel::hegel(|| {
        let mut rng = hegel::draw(&randoms().use_true_random());
        let x: i32 = rng.random_range(1..=100);
        assert!((1..=100).contains(&x));
    });
}

#[test]
fn test_randoms_composes() {
    hegel::hegel(|| {
        let _ = hegel::draw(&vecs(randoms()));
    });
}

#[test]
fn test_randoms_u64() {
    hegel::hegel(|| {
        let _: u64 = hegel::draw(&randoms()).random();
    });
}

#[test]
fn test_true_randoms_u64() {
    hegel::hegel(|| {
        let _: u64 = hegel::draw(&randoms().use_true_random()).random();
    });
}

#[test]
fn test_true_randoms_fill() {
    hegel::hegel(|| {
        let mut rng = hegel::draw(&randoms().use_true_random());
        let mut bytes = [0u8; 16];
        rng.fill(&mut bytes);
    });
}
