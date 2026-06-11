RELEASE_TYPE: patch

This patch fixes internal hegel errors being swallowed during test execution. Previously, an unexpected error originating inside hegel itself (for example a generated value failing to deserialize as the drawn type) was caught by the test runner, treated as a test failure, and reported as "Property test failed: unknown" — losing the actual error message entirely. Now such errors propagate immediately with the original error message, source location, and backtrace.
