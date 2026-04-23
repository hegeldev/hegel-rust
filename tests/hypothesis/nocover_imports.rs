//! Ported from hypothesis-python/tests/nocover/test_imports.py
//!
//! The original checks `from hypothesis import *` and
//! `from hypothesis.strategies import *` both work. The Rust analog is
//! glob-importing from `hegel` and `hegel::generators`; this test
//! exercises both glob imports (scoped inside the function so that the
//! `hegel::test` proc macro doesn't shadow the built-in `#[test]`).

#[test]
fn test_can_glob_import_from_hegel() {
    use hegel::generators::*;
    use hegel::*;

    Hegel::new(|tc| {
        let xs: Vec<i32> = tc.draw(vecs(integers::<i32>()));
        let _ = xs.iter().map(|&x| x as i64).sum::<i64>() > 1;
    })
    .settings(
        Settings::new()
            .test_cases(10)
            .verbosity(Verbosity::Quiet)
            .database(None),
    )
    .run();
}
