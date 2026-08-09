RELEASE_TYPE: patch

This patch brings various minor improvements to the `#[composite]` macro.

- Adds support for passing parameters by reference without an explicit lifetime, if the lifetime is not used in the return type.
- Stops the items_after_statements clippy lint from firing if constants/ functions are defined in the function body.
- Tries to output generator even if errors are found as to not raise errors elsewhere
