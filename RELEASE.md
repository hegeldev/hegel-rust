RELEASE_TYPE: patch

This fixes caching of test-cases during shrinking, which was happening only for some
test executions. This should significantly speed up shrinking in some cases.
