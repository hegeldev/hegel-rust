RELEASE_TYPE: patch

This patch fixes internal hegel errors being swallowed during test execution. Previously, errors like invalid generator configurations (e.g. `integers().min_value(100).max_value(10)`) or unexpected server responses were caught by the test runner, treated as test failures, and reported as "Property test failed: unknown" — losing the actual error message entirely. Now these errors propagate immediately with the original error message, source location, and backtrace.
