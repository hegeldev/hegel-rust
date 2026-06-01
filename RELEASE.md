RELEASE_TYPE: patch

This patch improves the performance of value generation on the native engine by using a faster hasher for the engine's internal lookup tables, which are keyed only by Hegel's own data and never by adversarial input. This speeds up generation across all generators, most noticeably for tests that make many draws per test case.
