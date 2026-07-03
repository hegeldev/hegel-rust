RELEASE_TYPE: patch

This patch makes cloned `TestCase`s generate from independent choice
streams. Cloning a test case and moving the clone to another thread was
already supported, but concurrent draws across clones interleaved into one
shared sequence: values changed run to run, failures involving concurrent
generation shrank poorly or not at all, and the docs warned the feature was
borderline-internal. Each clone now draws from its own stream, so threads
generating concurrently no longer interfere with each other: the same seed
reproduces the same values on every stream, failures shrink normally
(including the values drawn inside clones), and the shrunk counterexample
replays exactly.

Variable pools and engine-managed collections remain shared across clones;
using one of those from two threads at the same time still makes the
affected draws depend on scheduling order.
