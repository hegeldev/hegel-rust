//! End-to-end settings profiles: `hegel.toml` discovery and deltas, the
//! `--profile` flag on `#[hegel::main]` binaries, `HEGEL_DEFAULT_PROFILE`
//! selection, and programmatic registration feeding
//! `#[hegel::test(profile = "...")]`.
//!
//! The observable is `print_blob`: it has no CLI flag of its own, so whether
//! a failing run prints the reproducer line shows which profile was in
//! effect. `fixture_main_failing` has no compiled-in settings (selection
//! tests); `fixture_main_profile` compiles in `print_blob = true`
//! (replacement tests).

mod common;

use common::exec::{Cmd, fixture, self_test};
use hegel::TestCase;
use hegel::generators as gs;

const MAIN_FAILING: &str = env!("CARGO_BIN_EXE_fixture_main_failing");
const MAIN_PROFILE: &str = env!("CARGO_BIN_EXE_fixture_main_profile");

const REPRODUCER_MARKER: &str = "To reproduce this failure";

const BLOBBY_TOML: &str = "[profiles.blobby]\nprint_blob = true\n";

/// A failing fixture whose profile selection is fully under the test's
/// control: ambient `HEGEL_DEFAULT_PROFILE` and Antithesis detection are
/// masked, so only the env this test sets picks the profile.
fn failing() -> Cmd {
    fixture(MAIN_FAILING)
        .env_remove("HEGEL_DEFAULT_PROFILE")
        .env_remove("ANTITHESIS_OUTPUT_DIR")
}

fn assert_no_marker(out: common::exec::RunOutput) {
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        !combined.contains(REPRODUCER_MARKER),
        "no reproducer line expected:\n{combined}"
    );
}

#[test]
fn hegel_toml_profile_selected_via_profile_flag() {
    failing()
        .with_file("hegel.toml", BLOBBY_TOML)
        .args(&["--profile", "blobby"])
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[test]
fn hegel_toml_profile_selected_via_default_profile_env() {
    failing()
        .with_file("hegel.toml", BLOBBY_TOML)
        .env("HEGEL_DEFAULT_PROFILE", "blobby")
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[test]
fn default_profile_does_not_print_the_reproducer_line() {
    let out = failing()
        .env("HEGEL_DEFAULT_PROFILE", "default")
        .expect_failure("got nonneg")
        .run();
    assert_no_marker(out);
}

#[test]
fn bogus_default_profile_env_fails_the_run() {
    failing()
        .env("HEGEL_DEFAULT_PROFILE", "nope")
        .expect_failure("unknown settings profile \"nope\"")
        .run();
}

#[test]
fn hegel_toml_delta_applies_on_top_of_the_shipped_ci_profile() {
    let out = failing()
        .with_file("hegel.toml", "[profiles.ci]\nprint_blob = false\n")
        .env("CI", "true")
        .expect_failure("got nonneg")
        .run();
    assert_no_marker(out);
}

#[test]
fn malformed_hegel_toml_fails_with_file_and_line() {
    failing()
        .with_file("hegel.toml", "[profiles.broken]\nwat = true\n")
        .expect_failure(r"hegel\.toml:2")
        .run();
}

#[test]
fn hegel_toml_is_discovered_from_a_subdirectory() {
    failing()
        .with_file("hegel.toml", BLOBBY_TOML)
        .in_subdir("nested/deeper")
        .env("HEGEL_DEFAULT_PROFILE", "blobby")
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[test]
fn profile_flag_replaces_the_compiled_in_settings() {
    let out = fixture(MAIN_PROFILE)
        .args(&["--profile", "default"])
        .expect_failure("got nonneg")
        .run();
    assert_no_marker(out);
}

#[test]
fn compiled_in_settings_apply_on_top_of_the_selected_profile() {
    fixture(MAIN_PROFILE)
        .env("HEGEL_DEFAULT_PROFILE", "default")
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[ctor::ctor]
fn register_test_profile() {
    hegel::Settings::register_profile(
        "test_profiles_registered",
        hegel::Settings::from_profile("default").print_blob(true),
    );
}

#[hegel::test(profile = "test_profiles_registered")]
#[ignore = "fixture: run via exec::self_test"]
fn registered_profile_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

#[test]
fn registered_profile_drives_the_test_attribute() {
    self_test("registered_profile_fixture")
        .env("HEGEL_DEFAULT_PROFILE", "default")
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[hegel::test(profile = "default", test_cases = 5)]
fn shipped_profile_in_the_test_attribute(tc: TestCase) {
    tc.draw(gs::booleans());
}
