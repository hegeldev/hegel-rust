RELEASE_TYPE: minor

This release removes single-test-case mode from the C ABI: the `hegel_mode_t` enum and `hegel_settings_set_mode` are gone, and every run drives the full property-test loop. Frontends that want one test case per invocation should set the test-case budget to 1 with `hegel_settings_set_test_cases` instead. To make that budget useful, a run with a one-case budget now skips the simplest-example probe that opens the generate phase. The single case is randomly generated, at the cost of the `LargeInitialTestCase` health check not running for such runs.

Along with the mode, this release removes the machinery that silently unbounded state machines in single-test-case runs. State machines now always bound their rounds by the `stateful_step_count` setting.
