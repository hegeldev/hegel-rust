RELEASE_TYPE: patch

This patch fixes two bugs in the native feature.

1. test case limits were not properly being respected, leading to running up to 5x as many test cases as requested
2. some checks that were supposed to prevent duplicate test cases were not properly being honoured, leading to duplicate tests
