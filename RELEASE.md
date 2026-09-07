RELEASE_TYPE: minor

This release adds named settings profiles. Three ship with Hegel: `development` (what local runs get), `ci` (selected automatically on CI servers), and `antithesis` (selected automatically inside Antithesis). Modify a shipped profile or define your own in a `hegel.toml` at your package or workspace root:

```toml
default = "nightly"   # optional: the default profile for this project

[profiles.ci]
test_cases = 1000

[profiles.nightly]
test_cases = 10000
```

A profile layers over whichever profile the environment selects, so on CI `nightly` resolves as `nightly` → `ci` → `development`. To opt out, pin a parent with `extends`, where `extends = "default"` means the plain base settings. Select a profile with `#[hegel::test(profile = "nightly")]` or `Settings::from_profile`, and set the suite-wide default with the `default` entry in `hegel.toml`, the `HEGEL_DEFAULT_PROFILE` environment variable, or the new `--profile` flag on a `#[hegel::main]` binary. Profiles can also be registered programmatically with `Settings::register_profile`, and the default set with `Settings::set_default_profile`.

The `hegel.toml` is found by searching upward from the test process's working directory; when tests run outside the source tree, set `HEGEL_CONFIG` to the file's path instead, and under debug verbosity each run logs which config file it loaded. See the "Settings profiles" section of the crate documentation for details.

This changes one default: failing tests on CI now print a copy-pasteable `#[hegel::reproduce_failure("…")]` line. The failure database is disabled on CI, so the printed blob is the only way to reproduce a CI failure locally. To restore the old behavior, set `print_blob = false` under `[profiles.ci]` in `hegel.toml`.
