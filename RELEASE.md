RELEASE_TYPE: patch

This release updates the wire schemas emitted by `optional` and `ip_addresses` to match the cleaned-up server protocol, and bumps our pinned hegel-core to [0.6.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.6.0):

- `optional` now emits `{"type": "constant", "value": null}` for the null branch (instead of `{"type": "null"}`).
- `ip_addresses` now emits `{"type": "ip_address", "version": N}` (instead of `{"type": "ipv4"}` or `{"type": "ipv6"}`).
- The `#[derive(Generate)]` macro emits the new constant-null schema for unit variants of mixed enums.

This is a wire-format change only — there is no change to the public Rust API.
