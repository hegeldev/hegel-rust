RELEASE_TYPE: patch

This patch improves the performance of the native backend by caching the results of span mutations, so a test is no longer re-run for mutated inputs whose outcome is already known. Tests that spend most of their time in the mutation phase should run noticeably faster.
