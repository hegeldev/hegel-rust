RELEASE_TYPE: patch

This adds two new macros to allow more flexible use of hegel.

* `#[hegel::main]` wraps a function as a standalone binary entry point, exposing CLI flags for every Settings option.
* `#[hegel::standalone_function]` rewrites fn(tc: TestCase, args...) into fn(args...) so a property test can be invoked directly.
