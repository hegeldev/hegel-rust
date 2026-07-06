RELEASE_TYPE: patch

This release replaces the CBOR schema layer between the frontend and the
engine with typed draw calls. This should have no user-facing API impact.

The `#[doc(hidden)]` schema machinery (`Generator::as_basic`,
`BasicGenerator`, the `hegel::ciborium` re-export, `generate_raw`,
`generate_from_schema`, `deserialize_value`) is removed; it was internal
API and not covered by stability guarantees.

Failure databases and reproduce blobs written by earlier versions will
not replay against this release (the database has never been stable
across upgrades).
