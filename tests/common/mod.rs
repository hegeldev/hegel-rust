pub mod project;
pub mod utils;

#[cfg(not(miri))]
static TEST_CWD: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

#[cfg(not(miri))]
#[ctor::ctor]
fn chdir_to_isolated_tempdir() {
    // Reclaim scratch dirs (temp projects, test cwds, …) orphaned in the
    // system temp dir by runs that were killed before `TempDir`'s `Drop`
    // could remove them.
    project::sweep_stale_scratch_dirs(&std::env::temp_dir(), &project::pid_is_live);

    let tempdir = project::scratch_tempdir();
    std::env::set_current_dir(tempdir.path()).expect("Failed to chdir into test cwd tempdir");
    let _ = TEST_CWD.set(tempdir);
}
