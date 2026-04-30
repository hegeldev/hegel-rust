// Embedded tests for src/native/conjecture_utils.rs — exercise the
// compute_sampler_table branches and the Sampler::sample wrapper.

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::native::core::NativeTestCase;

#[test]
fn single_weight_yields_self_table() {
    // n == 1 hits the `scaled == 1.0` branch on initial classification:
    // the only entry is set to alternate=None, alternate_chance=0.
    let table = compute_sampler_table(&[3.0]);
    assert_eq!(table, vec![(0, 0, 0.0)]);
}

#[test]
fn equal_weights_settle_on_diagonal() {
    // All scaled probabilities exactly equal 1, so every entry is classified
    // as `scaled == 1.0` on initialisation and the small/large heaps stay
    // empty. Each entry is its own base with alternate_chance=0.
    let table = compute_sampler_table(&[1.0, 1.0, 1.0]);
    assert_eq!(table, vec![(0, 0, 0.0), (1, 1, 0.0), (2, 2, 0.0)]);
}

#[test]
fn two_unequal_weights_pair_up() {
    // weights = [0.3, 0.7]: scaled = [0.6, 1.4]. Loop pops lo=0, hi=1.
    // After: scaled_probabilities[1] = 1.4 + 0.6 - 1 = 1.0, hits the
    // `== 1.0` branch (alternate_chance=0 for the donor).
    let table = compute_sampler_table(&[0.3, 0.7]);
    // After remap, alternates[0] = 1, alt_chance[0] = 1 - 0.6 = 0.4.
    // Since alternate(1) > base(0), kept as (0, 1, 0.4).
    // Entry 1: alternates=None, alt_chance=0.0 -> (1, 1, 0.0).
    assert_eq!(table.len(), 2);
    assert_eq!(table[0].0, 0);
    assert_eq!(table[0].1, 1);
    assert!((table[0].2 - 0.4).abs() < 1e-12);
    assert_eq!(table[1], (1, 1, 0.0));
}

#[test]
fn donor_recycled_through_small_heap() {
    // weights = [10, 1, 1, 1]: total=13, n=4, scaled = [40/13, 4/13, 4/13, 4/13]
    // ≈ [3.077, 0.308, 0.308, 0.308]. small=[1,2,3], large=[0]. First iter:
    // lo=1, hi=0. After: scaled[0] = 3.077 + 0.308 - 1 = 2.385 > 1, pushed
    // back to large. Second iter: lo=2, hi=0. scaled[0] = 2.385 + 0.308 - 1
    // = 1.692 > 1, pushed back to large. Third iter: lo=3, hi=0.
    // scaled[0] = 1.692 + 0.308 - 1 = 1.0 — hits `== 1.0` branch in loop.
    // Final: small empty, large empty. Validate by checking all entries
    // resolved.
    let table = compute_sampler_table(&[10.0, 1.0, 1.0, 1.0]);
    assert_eq!(table.len(), 4);
    // Probability of returning 0: should be 10/13 — verify analytically.
    let mut p_zero = 0.0;
    for (base, alternate, alt_chance) in &table {
        let p_pick = 1.0 / table.len() as f64;
        if *base == 0 {
            p_zero += p_pick * (1.0 - alt_chance);
        }
        if *alternate == 0 {
            p_zero += p_pick * alt_chance;
        }
    }
    assert!((p_zero - 10.0 / 13.0).abs() < 1e-9, "p_zero={p_zero}");
}

#[test]
fn donor_recycled_through_small_heap_then_large_residual() {
    // Tune weights so a donor ends up `< 1` after a transfer (small
    // re-push branch). weights = [4, 1, 1]: total=6, n=3,
    // scaled = [2.0, 0.5, 0.5]. small=[1,2], large=[0]. Iter: lo=1, hi=0.
    // scaled[0] = 2.0 + 0.5 - 1 = 1.5 > 1, pushed to large. Iter: lo=2,
    // hi=0. scaled[0] = 1.5 + 0.5 - 1 = 1.0 — `== 1.0` branch.
    let table = compute_sampler_table(&[4.0, 1.0, 1.0]);
    assert_eq!(table.len(), 3);
}

#[test]
fn donor_small_branch_in_loop() {
    // Need scaled_probabilities[hi] < 1 *after* the transfer to hit the
    // `small.push(Reverse(hi))` branch inside the loop.
    // weights = [3, 1, 4]: total=8, n=3, scaled = [9/8, 3/8, 12/8] =
    // [1.125, 0.375, 1.5]. small=[1], large=[0, 2]. Iter pops lo=1,
    // hi=0 (smallest in large via Reverse). scaled[0] = 1.125 + 0.375 - 1
    // = 0.5 < 1 — hits the `small.push` branch. Then while-loop ends
    // (small=[0], large=[2]); next iter lo=0, hi=2. scaled[2] = 1.5 + 0.5
    // - 1 = 1.0 — `== 1.0` branch.
    let table = compute_sampler_table(&[3.0, 1.0, 4.0]);
    assert_eq!(table.len(), 3);
}

#[test]
fn residual_large_after_small_exhausted() {
    // Force the `while large` cleanup: with float precision a donor can
    // end the main loop with `> 1.0` left in large but small already
    // empty. weights=[0.4, 0.4, 0.2] is one such case (0.4+0.4+0.2 sums
    // to 1.0000000000000002 in IEEE-754, which propagates through the
    // scaled values).
    let table = compute_sampler_table(&[0.4, 0.4, 0.2]);
    assert_eq!(table.len(), 3);
}

#[test]
fn residual_small_when_large_exhausted_first() {
    // Similar imprecision case for the `while small` cleanup.
    // weights=[0.1, 0.7, 0.2] leaves an entry in `small` after the main
    // loop terminates because large empties first.
    let table = compute_sampler_table(&[0.1, 0.7, 0.2]);
    assert_eq!(table.len(), 3);
}

#[test]
fn sample_distribution_matches_weights() {
    // End-to-end check that Sampler::sample reproduces the input weights.
    let weights = vec![0.5, 1.0, 2.0, 0.5, 1.0];
    let sampler = Sampler::new(&weights);
    let total: f64 = weights.iter().sum();
    let n_iters: u64 = 50_000;

    let mut counts = vec![0u64; weights.len()];
    let mut seed_rng = SmallRng::seed_from_u64(0x5a5a_5a5a);
    for _ in 0..n_iters {
        let inner_seed: u64 = rand::RngExt::random(&mut seed_rng);
        let mut data = NativeTestCase::new_random(SmallRng::seed_from_u64(inner_seed));
        let n = sampler.sample(&mut data, None).ok().unwrap();
        counts[n] += 1;
    }
    for (i, &c) in counts.iter().enumerate() {
        let observed = c as f64 / n_iters as f64;
        let expected = weights[i] / total;
        assert!(
            (observed - expected).abs() < 0.02,
            "i={i} observed={observed} expected={expected}",
        );
    }
}
