RELEASE_TYPE: patch

This patch adds support for building the raw C ABI as `wasm32-unknown-unknown` for host integrations such as browser TypeScript. The Wasm build uses host-provided entropy and time, disables filesystem failure persistence and concurrent state machines, and is published as a release asset.
