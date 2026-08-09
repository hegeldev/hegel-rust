RELEASE_TYPE: patch

This patch brings various minor improvements to the `#[composite]` macro.

- Adds support for passing parameters by reference without an explicit lifetime, if the lifetime is not used in the return type.
