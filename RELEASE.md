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
- Value enumeration over finite generators is gone: `filter` on a
  `sampled_from` no longer enumerates the surviving values to draw one
  directly, and sets over small sampled alphabets no longer draw without
  replacement. Both now use plain rejection sampling, so a filter that
  rejects most of a small value set — or a set that must contain most of
  its alphabet — can trip the `FilterTooMuch` health check where it
  previously succeeded, and an unsatisfiable filter surfaces as
  `FilterTooMuch` rather than a dedicated "Unsatisfiable filter" error.
  This logic would otherwise need reimplementing in every Hegel binding;
  if it proves necessary in practice it will come back as engine
  support.

Failure databases and reproduce blobs written by earlier versions will
not replay against this release (the database has never been stable
across upgrades).
