RELEASE_TYPE: minor

This release adds a native Rust backend for test-case generation and shrinking, available behind the `native` feature flag. When enabled, Hegel no longer requires a Python server process -- all generation, shrinking, and database caching happen in-process. This should significantly improve startup latency and make the library easier to deploy.

The native backend is a port of the core Hypothesis engine and supports the same generation and shrinking semantics. It is activated automatically when the `native` feature is enabled; no code changes are required beyond adding the feature flag.
