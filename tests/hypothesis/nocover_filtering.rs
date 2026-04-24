//! Ported from resources/hypothesis/hypothesis-python/tests/nocover/test_filtering.py
//!
//! Individually-skipped tests:
//!
//! - `test_chained_filters_repr` — asserts
//!   `repr(base.filter(foo).filter(bar)) == f"{base!r}.filter(foo).filter(bar)"`.
//!   Python `repr()` on strategies has no Rust counterpart; hegel-rust's
//!   `Filtered<T, F, G>` wrapper exposes no repr-style introspection surface.

use crate::common::utils::assert_all_examples;
use hegel::generators::{self as gs, BoxedGenerator, Generator};
use hegel::{Hegel, Settings};
use std::collections::HashSet;

#[test]
fn test_filter_correctly_integers_gt_one() {
    assert_all_examples(
        gs::integers::<i64>().filter(|x: &i64| *x > 1),
        |x: &i64| *x > 1,
    );
}

#[test]
fn test_filter_correctly_nonempty_lists() {
    assert_all_examples(
        gs::vecs(gs::integers::<i64>()).filter(|xs: &Vec<i64>| !xs.is_empty()),
        |xs: &Vec<i64>| !xs.is_empty(),
    );
}

fn run_chained_filters_agree(base: BoxedGenerator<'static, i64>) {
    Hegel::new(move |tc| {
        let forbidden: HashSet<i64> = tc.draw(
            gs::hashsets(gs::integers::<i64>().min_value(1).max_value(20)).max_size(19),
        );

        let mut s: BoxedGenerator<'static, i64> = base.clone();
        for f in &forbidden {
            let f = *f;
            s = s.filter(move |x: &i64| *x != f).boxed();
        }

        let x: i64 = tc.draw(&s);
        assert!((1..=20).contains(&x));
        assert!(!forbidden.contains(&x));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_chained_filters_agree_integers_1_20() {
    run_chained_filters_agree(gs::integers::<i64>().min_value(1).max_value(20).boxed());
}

#[test]
fn test_chained_filters_agree_integers_0_19_mapped() {
    run_chained_filters_agree(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(19)
            .map(|x| x + 1)
            .boxed(),
    );
}

#[test]
fn test_chained_filters_agree_sampled_from_1_20() {
    let values: Vec<i64> = (1..=20).collect();
    run_chained_filters_agree(gs::sampled_from(values).boxed());
}

#[test]
fn test_chained_filters_agree_sampled_from_0_19_mapped() {
    let values: Vec<i64> = (0..20).collect();
    run_chained_filters_agree(gs::sampled_from(values).map(|x| x + 1).boxed());
}
