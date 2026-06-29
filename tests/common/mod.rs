pub mod project;
pub mod utils;

#[cfg(not(miri))]
static TEST_CWD: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

#[cfg(not(miri))]
#[ctor::ctor]
fn chdir_to_isolated_tempdir() {
    let tempdir = tempfile::Builder::new()
        .prefix("hegel-rust-test-")
        .tempdir()
        .expect("Failed to create test cwd tempdir");
    std::env::set_current_dir(tempdir.path()).expect("Failed to chdir into test cwd tempdir");
    let _ = TEST_CWD.set(tempdir);
}
