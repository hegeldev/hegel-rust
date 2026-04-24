pub mod project;
pub mod utils;

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
