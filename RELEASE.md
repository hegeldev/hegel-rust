RELEASE_TYPE: minor

This release removes the CBOR schema layer that generators used to
communicate with the engine. Generators now call typed engine draw
functions directly, which removes a per-draw serialization round-trip and
a large amount of internal machinery. Most user code is unaffected —
generator factory functions, builder methods, and drawn values are
unchanged — but some API surface is gone:

- The `Generator` trait no longer has `as_basic()`; `do_draw` is now its
  only required method. `BasicGenerator` no longer exists. Custom
  generators that only implemented `as_basic` must implement `do_draw`
  instead, composing existing generators or the `TestCase` draw surface.
- `gs::fixed_dicts()` is removed. It produced CBOR `Value` maps, which no
  longer exist as a currency; use `#[derive(DefaultGenerator)]` on a
  struct (or `tuples!`) for fixed-shape records.
- The `hegel::ciborium` re-export and the hidden schema helpers
  (`generate_raw`, `generate_from_schema`, `deserialize_value`) are
  removed, and `hegeltest` no longer depends on `ciborium` at all.

Failure databases and reproduce blobs written by earlier versions will
not replay against this release (the database has never been stable
across upgrades).
