// Shrink-quality tests for scenarios where pairs/groups of integer choices
// can only shrink jointly.  Ranges are chosen so the initial counterexample
// is discoverable by random generation (otherwise we'd be testing the
// generator, not the shrinker).

use crate::common::utils::{Minimal, minimal};
use crate::not_supported_on_native;
use hegel::generators as gs;

// ── 2-integer zigzag: `|m - n| == 1` ──────────────────────────────────────
//
// With the predicate locking m and n one apart, neither can move on its
// own without violating it.  Joint lowering is the only way down.

#[test]
fn test_zigzag_two_ints() {
    let g = gs::tuples!(
        gs::integers::<i64>().min_value(0).max_value(1000),
        gs::integers::<i64>().min_value(0).max_value(1000),
    );
    let (m, n) = minimal(g, |(m, n): &(i64, i64)| (m - n).unsigned_abs() == 1);
    assert_eq!((m, n), (0, 1));
}

#[test]
fn test_zigzag_two_ints_with_lower_bound() {
    // m is floored at 100; the shrunk pair should hug that floor.
    let g = gs::tuples!(
        gs::integers::<i64>().min_value(100).max_value(1000),
        gs::integers::<i64>().min_value(0).max_value(1000),
    );
    let (m, n) = Minimal::new(g, |(m, n): &(i64, i64)| (m - n).unsigned_abs() == 1)
        .test_cases(2000)
        .run();
    assert_eq!(m, 100);
    assert!(n == 99 || n == 101, "got n={n}");
}

// ── Joint lowering across non-integer noise ───────────────────────────────
//
// `lower_integers_together` pairs nodes by integer-index (ignoring other
// kinds in between), so joint lowering should work even when the two
// integers have unrelated choice nodes between them.

#[test]
fn test_joint_shrink_two_ints_through_noise() {
    let g = hegel::compose!(|tc| {
        let a: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        let _s: String = tc.draw(gs::text());
        let _f: f64 = tc.draw(gs::floats::<f64>());
        let _bs: Vec<u8> = tc.draw(gs::vecs(gs::integers::<u8>()));
        let b: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        (a, b)
    });
    let (a, b) = Minimal::new(g, |(a, b): &(i64, i64)| (a - b).unsigned_abs() == 1)
        .test_cases(2000)
        .run();
    assert_eq!((a, b), (0, 1));
}

// ── Five-integer zigzag: chain `|a_{i+1} - a_i| == 1` ─────────────────────
//
// `lower_integers_together` pairs integer-index entries within gap ≤ 3,
// so this five-node chain stresses joint lowering that has to propagate
// across the whole chain.

// Five-integer chained zigzag: the joint motion `lower_common_node_offset`
// powers is exercised at the shrinker level by
// `test_shrinker_efficiency::zigzag_five_ints_chained_converges`. At the
// engine level under the native backend, random generation can't reliably
// discover the initial counterexample (probability ≈ 1e-7 per attempt),
// so we only run this through the server backend for now.
#[not_supported_on_native]
#[test]
fn test_zigzag_five_ints_chained() {
    let g = gs::tuples!(
        gs::integers::<i64>().min_value(0).max_value(100),
        gs::integers::<i64>().min_value(0).max_value(100),
        gs::integers::<i64>().min_value(0).max_value(100),
        gs::integers::<i64>().min_value(0).max_value(100),
        gs::integers::<i64>().min_value(0).max_value(100),
    );
    let (a, b, c, d, e) = Minimal::new(g, |(a, b, c, d, e): &(i64, i64, i64, i64, i64)| {
        (a - b).unsigned_abs() == 1
            && (b - c).unsigned_abs() == 1
            && (c - d).unsigned_abs() == 1
            && (d - e).unsigned_abs() == 1
    })
    .test_cases(5000)
    .run();
    // Smallest sort_key chain anchored at 0: alternating staircase
    // (0, 1, 0, 1, 0) or any rotation that hugs 0.
    assert_eq!(a, 0, "got chain ({a}, {b}, {c}, {d}, {e})");
}

// ── Equal-pair shrinking with mid-range values ────────────────────────────
//
// `shrink_duplicates` lowers same-valued integer groups jointly.

#[test]
fn test_shrink_duplicates_three_copies() {
    let g = gs::tuples!(
        gs::integers::<i64>().min_value(0).max_value(1000),
        gs::integers::<i64>().min_value(0).max_value(1000),
        gs::integers::<i64>().min_value(0).max_value(1000),
    );
    let (a, b, c) = Minimal::new(g, |(a, b, c): &(i64, i64, i64)| {
        *a == *b && *b == *c && *a > 0
    })
    .test_cases(2000)
    .run();
    assert_eq!((a, b, c), (1, 1, 1));
}

// ── try_shortening_via_increment on a float-kind node ──────────────────────
//
// `try_shortening_via_increment` probes powers-of-2 magnitudes
// (1, 2, 4, …, 1024) by constructing `ChoiceValue::Integer(±magnitude)`
// candidates.  Those integer candidates are rejected by `kind.validate`
// against a Float kind, so for a float-typed node the powers-of-2
// fallback is dead code today.
//
// This generator skips an extra draw when `|f| < 100`, so reaching a
// magnitude ≥ 100 (e.g. -128.0, which the powers-of-2 probe *would*
// offer if it issued float candidates) shortens the overall choice
// sequence.

#[test]
fn test_try_shortening_via_increment_float() {
    let g = hegel::compose!(|tc| {
        let f: f64 = tc.draw(gs::floats::<f64>());
        if f.abs() < 100.0 {
            let _: bool = tc.draw(gs::booleans());
        }
        f
    });
    let f = Minimal::new(g, |f: &f64| *f < -86.0).test_cases(2000).run();
    assert!(
        f.abs() >= 100.0,
        "expected |f| >= 100 (short-sequence regime), got f={f}"
    );
}
