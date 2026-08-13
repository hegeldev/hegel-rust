RELEASE_TYPE: patch

This patch improves shrinking for stateful tests that use variable pools. Previously adding a variable to a pool could be hard to remove if other variables in the pool were used after that point. This should no longer be the case.
