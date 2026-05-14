pub mod project;
pub mod utils;

/// Re-export of the dev-only `#[not_supported_on_native]` attribute proc-macro
/// from `hegeltest-macros`. Pulls it into the test binary's `common` module
/// so test files can `use common::not_supported_on_native;` instead of
/// depending on `hegeltest-macros` by name.
///
/// `allow(unused_imports)` because every test binary includes `mod common;`
/// but only some of them actually use the attribute — without the allow,
/// clippy's `-D warnings` would reject the re-export in those binaries.
#[allow(unused_imports)]
pub use hegel_macros::not_supported_on_native;

// Each cargo test run of this project spawns its own set of integration-test
// binaries, and each one of those binaries normally inherits the crate root as
// its cwd. The hegel library creates `.hegel/` in the process's cwd, so
// without this setup the test binaries would all share (and clobber) one
// `.hegel/` inside the crate root across concurrent and successive `cargo
// test` runs. This ctor chdir's every test binary into its own tempdir before
// main, so each binary's `.hegel/` lands in a fresh, private location.
//
// Note: this is purely a test-harness concern. The library's documented
// behaviour (`.hegel/` in the caller's cwd) is deliberate for real users.
static TEST_CWD: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

#[ctor::ctor]
fn chdir_to_isolated_tempdir() {
    let tempdir = tempfile::Builder::new()
        .prefix("hegel-rust-test-")
        .tempdir()
        .expect("Failed to create test cwd tempdir");
    std::env::set_current_dir(tempdir.path()).expect("Failed to chdir into test cwd tempdir");
    let _ = TEST_CWD.set(tempdir);
}
