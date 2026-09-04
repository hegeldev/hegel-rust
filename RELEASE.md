RELEASE_TYPE: patch

This patch improves the shrinking of stateful test failures: shrunk rule
sequences no longer keep redundant steps, such as inserts whose effect a
later step overwrites
([#441](https://github.com/hegeldev/hegel-rust/issues/441)). Previously a
step could only be deleted as a short run of individual choices, so machines
with several invariants or draw-heavy rules shrank to sequences padded with
no-op steps.
