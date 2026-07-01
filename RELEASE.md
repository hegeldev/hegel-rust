RELEASE_TYPE: patch

This patch makes each `TestCase` clone hold its own reference to the underlying test case, so a clone handed to another thread keeps the test case alive for as long as the thread holds it, rather than aliasing a single engine handle whose lifetime was tied to the run. Draw behaviour is unchanged: clones share one choice sequence, individual draws are still serialised inside the engine, and two clones drawing at the same moment remains non-deterministic (such interleavings cannot be replayed or shrunk well).

It also updates the `hegeltest-c` dependency to the reference-counted engine handles that make this possible.
