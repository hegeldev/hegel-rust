RELEASE_TYPE: minor

This release adds `gs::uuids()`, a generator for UUID values represented as `u128`. The generator supports restricting to a specific UUID version (1–5) via `.version()` and optionally generating the nil UUID via `.allow_nil(true)`.
