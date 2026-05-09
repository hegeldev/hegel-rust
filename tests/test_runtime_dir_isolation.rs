//! Regression tests: every integration-test binary should be chdir'd into a
//! fresh per-process tempdir, so the hegel library (which creates `.hegel/`
//! in cwd) doesn't leak state across concurrent or successive `cargo test`
//! runs of this repo.
//!
//! The setup lives in `tests/common/mod.rs` behind a `#[ctor::ctor]`; these
//! tests just assert its observable effects.

mod common;

#[cfg(not(feature = "native"))]
use hegel::Hegel;
#[cfg(not(feature = "native"))]
use hegel::generators as gs;

#[test]
fn cwd_is_not_crate_root() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cwd = std::env::current_dir().unwrap();
    assert_ne!(
        cwd, manifest_dir,
        "test binary cwd should have been chdir'd away from the crate root",
    );
}

#[test]
fn cwd_is_a_hegel_rust_test_tempdir() {
    let cwd = std::env::current_dir().unwrap();
    let name = cwd.file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with("hegel-rust-test-"),
        "cwd {cwd:?} should be a tempdir whose name starts with hegel-rust-test-",
    );
}

// The server backend always writes `.hegel/server.*.log` in cwd, so running a
// Hegel test is a load-bearing assertion that the isolation chdir worked.
#[cfg(not(feature = "native"))]
#[test]
fn running_hegel_creates_dot_hegel_in_tempdir_not_crate_root() {
    Hegel::new(|tc| {
        let _: bool = tc.draw(gs::booleans());
    })
    .run();

    let cwd = std::env::current_dir().unwrap();
    assert!(
        cwd.join(".hegel").exists(),
        ".hegel should exist in the test-binary tempdir cwd {cwd:?}",
    );
    let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_ne!(
        cwd, crate_root,
        ".hegel landed in the crate root, which means isolation isn't working",
    );
}

// Native parity: with `Database::Unset` (the non-CI default), a failing
// run must persist its counterexample under `.hegel/examples/` in cwd —
// matching upstream Hypothesis's `.hypothesis/examples/` default. Before
// S7 the default was silently `None` on native, so dev runs lost their
// failing examples between iterations and `.hegel/` never appeared.
#[cfg(feature = "native")]
#[test]
fn running_failing_native_test_creates_dot_hegel_examples() {
    use hegel::Hegel;
    use hegel::generators as gs;

    // Use the failing-property/`Minimal::run`-style catch_unwind dance:
    // the `Hegel::run` call panics with `"Property test failed: ..."`
    // when the body fails, which we expect.
    let _ = std::panic::catch_unwind(|| {
        Hegel::new(|tc: hegel::TestCase| {
            let _: bool = tc.draw(gs::booleans());
            panic!("forced failure to populate the default database");
        })
        .__database_key("dot_hegel_examples_test".to_string())
        .run();
    });

    let cwd = std::env::current_dir().unwrap();
    let db_root = cwd.join(".hegel").join("examples");
    assert!(
        db_root.exists(),
        ".hegel/examples should exist after a failing native run with \
         default settings; cwd = {cwd:?}",
    );
    let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_ne!(
        cwd, crate_root,
        ".hegel landed in the crate root, which means isolation isn't working",
    );
}
