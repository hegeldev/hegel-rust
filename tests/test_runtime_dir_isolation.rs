//! Regression tests: every integration-test binary should be chdir'd into a
//! fresh per-process tempdir, so the hegel library (which creates `.hegel/`
//! in cwd) doesn't leak state across concurrent or successive `cargo test`
//! runs of this repo.
//!
//! The setup lives in `tests/common/mod.rs` behind a `#[ctor::ctor]`; these
//! tests just assert its observable effects.

mod common;

use hegel::generators as gs;
use hegel::{Hegel, Settings};

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

#[test]
fn running_hegel_creates_dot_hegel_in_tempdir_not_crate_root() {
    // Server backend creates `.hegel/server.log` when its subprocess spawns;
    // native backend only writes under `.hegel/examples/...` when it has an
    // interesting example to persist, which requires a failing body and a
    // database key. Force a save so both backends populate `.hegel/`. Set
    // the database path explicitly because `Settings::new()` defaults it to
    // `Disabled` under CI, which would skip the native save block.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let _: bool = tc.draw(gs::booleans());
            panic!("intentional failure to populate the database");
        })
        .settings(Settings::new().database(Some(".hegel/examples".to_string())))
        .__database_key("runtime_dir_isolation".to_string())
        .run();
    }));
    assert!(result.is_err(), "expected the test body to panic");

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
