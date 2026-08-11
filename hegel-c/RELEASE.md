RELEASE_TYPE: patch

This patch greatly reduces the number of test executions the shrinker needs, typically by around 10x. Shrink passes are no longer re-stepped after a full step stops making progress, and replaying a candidate whose choices run out on an already-explored path is now recognised as an overrun without running the test body, matching Hypothesis. Shrink results are unchanged.
