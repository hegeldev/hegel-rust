RELEASE_TYPE: patch

This release adds an experimental `native` feature flag that swaps the
hegel-core Python server for an in-process Rust engine. When using this
feature, hegel has no Python dependency, and is likely to be significantly
faster.

This should at this point be considered more of a preview than a usable
feature. Some things will work, many things will fail with a `todo!`.
It is likely to contain bugs. Experience reports on things that work
but less well than on the server are extremely welcome.
