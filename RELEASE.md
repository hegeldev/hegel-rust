RELEASE_TYPE: patch

This patch adds support for a `repeat` method on test case, for operations that
you want to run repeatedly until they hit an error. Effectively equivalent to
a `loop` that is better optimised for testing.
