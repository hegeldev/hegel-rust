RELEASE_TYPE: patch

This patch fixes a crash when shrinking a failure in a flaky test. When re-executing the test produced a shorter run than the failure being shrunk, a deletion pass could panic with an index out of bounds. That shrink attempt is now rejected and shrinking continues.
