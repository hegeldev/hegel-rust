RELEASE_TYPE: patch

This patch adds a shrink pass that deletes whole spans. The existing
deletion passes only try windows of up to eight choices, so a stateful step
whose rule draws and sampled invariant checks together cost more than that
could never be deleted, and shrunk rule sequences kept redundant steps
([#441](https://github.com/hegeldev/hegel-rust/issues/441)).
