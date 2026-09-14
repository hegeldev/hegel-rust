RELEASE_TYPE: patch

This patch raises the limit on the number of choices a single test case may make from 8,192 to 2^20 (1,048,576), and adds `hegel_settings_set_unbounded_choices` to remove it. Suppressing the `TestCasesTooLarge` health check removes it too. An unbounded test case's draws never fail with `HEGEL_E_STOP_TEST` for running out of room. The `LargeInitialTestCase` health check now measures against the configured limit rather than the fixed buffer size.

This patch also makes `hegel_pool_add` take constant amortised time. Handing out a fresh identifier used to scan every identifier the test case had already handed out, which made a test case that adds many thousands of values to pools quadratically slow.
