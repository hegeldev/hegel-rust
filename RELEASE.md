RELEASE_TYPE: minor

This release adds named settings profiles. A profile is a complete set of settings resolved by name, and three ship with Hegel: `default`, `ci` (selected automatically on CI servers), and `antithesis` (selected automatically inside Antithesis). Modify a shipped profile or define your own in a `hegel.toml` at your package or workspace root:

```toml
[profiles.ci]
test_cases = 1000

[profiles.nightly]
extends = "ci"
test_cases = 10000
```

Select a profile with `#[hegel::test(profile = "nightly")]`, `Settings::from_profile`, the new `--profile` flag on a `#[hegel::main]` binary, or suite-wide with the `HEGEL_DEFAULT_PROFILE` environment variable. Profiles can also be registered programmatically with `Settings::register_profile`. See the "Settings profiles" section of the crate documentation for details.

This changes one default: failing tests on CI now print a copy-pasteable `#[hegel::reproduce_failure("…")]` line. The failure database is disabled on CI, so the printed blob is the only way to reproduce a CI failure locally. To restore the old behavior, set `print_blob = false` under `[profiles.ci]` in `hegel.toml`.
