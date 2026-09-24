RELEASE_TYPE: minor

This release moves the handling of the `HEGEL_TEST_CASES`, `HEGEL_DATABASE`, `HEGEL_STATISTICS`, `HEGEL_SEED`, `HEGEL_DERANDOMIZE` and `HEGEL_PRINT_BLOB` environment variables from the crate into libhegel, which now applies them when a `Settings` value is created rather than when a test runs. This changes where they sit among the settings layers: they still win over the profile and `hegel.toml`, but a setting written into the test — a `Settings` builder call, a `#[hegel::test]` attribute argument, or a `#[hegel::main]` command-line flag — now takes precedence over them, where previously the variable won. `HEGEL_TEST_CASES=10000 cargo test` still runs every test that does not set `test_cases` itself with 10000 cases; a test declared `#[hegel::test(test_cases = 5)]` now keeps its 5.

A malformed variable is still an error naming the variable; it is now raised when the `Settings` value is created.
