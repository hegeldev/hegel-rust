RELEASE_TYPE: minor

This release makes the native backend count span mutations towards the `test_cases` budget. Previously each generated example could spawn several extra mutated test cases that ran *on top of* `test_cases`, so a test executed its body several times more often than the configured number of examples. Span mutations are now generated examples like any other and share the same budget, matching Hypothesis.

As a result a run executes roughly `test_cases` times rather than a multiple of it. Tests that were implicitly relying on those extra executions to find a counterexample may need a larger `test_cases` to keep finding it.
