RELEASE_TYPE: patch

This patch improves shrinking for stateful tests that use variable pools. Previously adding a variable to a pool could be hard to remove if other variables in the pool were used after that point. This should no longer be the case.

This patch also fixes shrinking sometimes stopping just short of the simplest failing example. When the randomized escape passes burned through the shrinker's stall budget without progress, the exhausted budget silently discarded every later candidate, so the deterministic passes' final round could no longer normalize the result it was handed. Each scheduling round now starts with a fresh stall budget, so the deterministic passes always get a real final pass over the shrunk example.
