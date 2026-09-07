RELEASE_TYPE: patch

This patch adds a reproduction pointer to failure output
([#438](https://github.com/hegeldev/hegel-rust/issues/438)). A failing test
now ends with a line naming the database directory the shrunk example was
saved to. When nothing was saved (the database is disabled, as in CI by
default, or the test has no database key) the copy-pasteable
`reproduce_failure` line is printed instead. Previously that line required
enabling the `print_blob` setting.
