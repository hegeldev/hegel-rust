RELEASE_TYPE: patch

This patch replaces the CBOR schema layer between the frontend and the
engine with typed draw calls. The documented API is unchanged; generation
of regexes, domains, emails, and URLs should get a little faster, since
patterns and alphabets are now prepared once per generator instead of
once per draw.

The `#[doc(hidden)]` schema machinery (`Generator::as_basic`,
`BasicGenerator`, `generate_raw`, `generate_from_schema`,
`deserialize_value`) is removed; it was internal API and not covered by
stability guarantees. `Generator::do_draw` no longer has a default
implementation, so a custom generator that only implemented `as_basic`
must implement `do_draw` instead. `EmailGenerator` and `UrlGenerator`
are no longer unit structs; construct them with `gs::emails()` and
`gs::urls()`.

Failure databases and reproduce blobs written by earlier versions will
not replay against this release (the database has never been stable
across upgrades).
