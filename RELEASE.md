RELEASE_TYPE: patch

Internal change to the native backend (`--features native`): the TooSlow health-check threshold is now passed into the engine rather than read from a constant, so it can be tested deterministically. No user-visible behaviour change.

The native backend (`--features native`) now implements the `TestCasesTooLarge` and `LargeInitialTestCase` health checks. The former fires when generated inputs routinely overrun the choice buffer; the latter when even the smallest natural input is very large. Both are suppressible via `suppress_health_check` and mirror Hypothesis's `data_too_large` / `large_base_example`.
