//! Compile-time tests for hegel's proc macros, driven by `trybuild`.
//!
//! Each `tests/compile/pass/*.rs` fixture must compile cleanly; each
//! `tests/compile/fail/*.rs` fixture must fail to compile with a message
//! matching the committed `*.stderr` snapshot. `trybuild` shares a single
//! `target/` across all fixtures and caches aggressively, so these are
//! much cheaper than the equivalent `TempRustProject` tests that each
//! spawn their own `cargo run`/`cargo test` subprocess.
//!
//! To regenerate stderr snapshots after intentional macro-error changes,
//! run `TRYBUILD=overwrite cargo test --test test_compile`.

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    for entry in std::fs::read_dir("tests/compile/pass").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            t.pass(path);
        }
    }
    for entry in std::fs::read_dir("tests/compile/fail").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            t.compile_fail(path);
        }
    }
}
