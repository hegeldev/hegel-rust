RELEASE_TYPE: patch

This patch adds three environment variables that override settings for a single run, so a `#[hegel::test]` can be re-run with a chosen seed or made to print its reproducer without editing the test (libtest owns the command line, so the `#[hegel::main]` flags were never available to ordinary test targets):

- `HEGEL_SEED` overrides `seed`: an integer fixes the seed, and `none` clears a compiled-in one, the same vocabulary as the `--seed` flag.
- `HEGEL_DERANDOMIZE` overrides `derandomize`, and `HEGEL_PRINT_BLOB` overrides `print_blob`, each taking `true`, `1` or `yes`, or `false`, `0` or `no`.

Like the existing `HEGEL_TEST_CASES` and `HEGEL_DATABASE`, each wins over values configured in source, including explicit attribute settings; an empty variable is ignored and a malformed one fails the run with a message naming the variable. The overrides do not change how the settings combine: a fixed seed still takes precedence over `derandomize` wherever either came from. ([#492](https://github.com/hegeldev/hegel-rust/issues/492))
