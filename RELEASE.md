RELEASE_TYPE: minor

This release removes the Python `hegel-core` server backend. Hegel now always runs its test engine in-process as a pure Rust implementation — the same engine that was previously available behind the experimental `native` feature.

For most users this requires no changes: `cargo add --dev hegeltest` and writing tests works exactly as before, except Hegel no longer downloads or spawns anything. There is no longer any dependency on Python, `uv`, or the `hegel-core` server, and the `HEGEL_SERVER_COMMAND` environment variable no longer does anything.

The `native` feature flag has been removed, since the native engine is now the only backend. If your `Cargo.toml` enabled it explicitly, drop it:

```toml
# before
hegeltest = { version = "...", features = ["native"] }

# after
hegeltest = "..."
```

This release also fixes two bugs in the engine:

1. Test case limits were not properly being respected, leading to running up to 5x as many test cases as requested.
2. Some checks that were supposed to prevent duplicate test cases were not being honoured, leading to duplicate tests.
