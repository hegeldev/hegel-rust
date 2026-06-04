RELEASE_TYPE: patch

The native backend (`--features native`) now implements the `TestCasesTooLarge` and `LargeInitialTestCase` health checks. The former fires when generated inputs routinely overrun the choice buffer; the latter when even the smallest natural input is very large. Both are suppressible via `suppress_health_check`.
