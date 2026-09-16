RELEASE_TYPE: patch

This patch fixes a failing test being reported to Antithesis twice. Once the engine had found and shrunk a counterexample, the final replay that re-raises the failure was reported as its own verdict, so `sdk.jsonl` carried the test's assertion twice (three or more times when `report_multiple_failures` found several distinct failures). Inside Antithesis a run now writes its assertion exactly once, whatever its outcome. `#[hegel::reproduce_failure]` still reports the replay it runs, since that replay is the whole test.
