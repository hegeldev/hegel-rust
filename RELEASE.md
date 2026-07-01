RELEASE_TYPE: patch

This patch makes every `TestCase` — the one passed to the test body, its children, and any `tc.clone()` — keep the underlying test case alive for as long as it exists. A `TestCase` moved to a thread that outlives the test can therefore no longer touch freed engine state: a draw after the test has finished now fails with a clear panic in that thread instead. Draw behaviour is otherwise unchanged: clones share one choice sequence, individual draws are still serialised inside the engine, and two clones drawing at the same moment remains non-deterministic (such interleavings cannot be replayed or shrunk well).

It also updates the `hegeltest-c` dependency to the reference-counted engine handles that make this possible.
