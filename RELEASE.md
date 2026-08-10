RELEASE_TYPE: patch

This patch adds two environment variables that override settings at runtime, so a whole test suite's behavior can be adjusted without editing source:

- `HEGEL_TEST_CASES` overrides the number of test cases each test runs, taking precedence over values configured in source (including explicit `test_cases` settings). For example, `HEGEL_TEST_CASES=10000 cargo test` runs a deep exploration of every property test.
- `HEGEL_DATABASE` overrides the failure database location: `HEGEL_DATABASE=disabled` turns the database off (the same keyword the `--database` CLI flag uses), and any other non-empty value relocates the database to that path.
