pub mod exec;
pub mod utils;

#[cfg(not(miri))]
static TEST_CWD: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

#[cfg(not(miri))]
#[ctor::ctor]
fn chdir_to_isolated_tempdir() {
    // Children spawned by `exec::self_test` inherit the parent test binary's
    // already-isolated cwd rather than creating (and, since a ctor tempdir is
    // never dropped, leaking) one of their own.
    if std::env::var_os("HEGEL_TEST_INHERIT_CWD").is_some() {
        return;
    }
    let tempdir = tempfile::Builder::new()
        .prefix("hegel-rust-test-")
        .tempdir()
        .expect("Failed to create test cwd tempdir");
    std::env::set_current_dir(tempdir.path()).expect("Failed to chdir into test cwd tempdir");
    let _ = TEST_CWD.set(tempdir);
}
