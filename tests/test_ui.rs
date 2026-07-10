//! Compile-diagnostic UI tests: every case in `tests/ui/` is a program that
//! must FAIL to compile, with diagnostics matching its checked-in `.stderr`
//! golden file. These pin the compile-time error messages of the hegel
//! macros (and a couple of deliberate type-level properties, like `TestCase`
//! not being shareable across threads).
//!
//! To (re)generate the goldens after intentionally changing a diagnostic:
//! `TRYBUILD=overwrite cargo test --test test_ui`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
