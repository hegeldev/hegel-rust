RELEASE_TYPE: minor

This patch tightens libhegel's C ABI: it removes the last thread-local state,
replaces the `#define`d integer constants with named enums, and documents
pointer ownership.

For Rust users this is an internal-only change, but it is a significant breaking
change for libhegel users.
