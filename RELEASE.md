RELEASE_TYPE: patch

This patch trims two allocations from every test case of a quiet run: the buffer that captures a nondeterministic run's output is only created when the run's output is not suppressed, and silent test cases share one no-op output sink instead of each allocating their own. Together with the engine changes in this release, a test drawing a single value runs about 15% fewer instructions per test case.
