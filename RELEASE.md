RELEASE_TYPE: minor

Hegel now runs entirely in-process. The native Rust engine is the only backend: the Python `hegel-core` server, the Unix-socket protocol, and the automatic `uv` install are gone, so Hegel no longer has any Python dependency and there is nothing extra to install.

The `native` Cargo feature has been removed — it is now always on, so depending on `hegeltest` with `features = ["native"]` is no longer valid (drop the feature). The public generator and `Settings` APIs are unchanged.
