RELEASE_TYPE: patch

This patch updates the engine, which replaces its data tree with a flat execution cache. Runs are faster and cache memory is bounded. A test that flips between passing and failing on identical generated values now fails the run as flaky instead of being silently masked, and `HealthCheck::FilterTooMuch` fires after a streak of duplicate invalid test cases instead of on exhaustion of the generation space.
