RELEASE_TYPE: minor

This release removes `gs::fixed_dicts()` and its `FixedDictBuilder` /
`FixedDictGenerator` types. They produced CBOR `Value` maps — a leftover
currency from the engine's old serialization layer with no other
consumer. For fixed-shape records, use `#[derive(DefaultGenerator)]` on
a struct, or `gs::tuples!`. The `hegel::ciborium` re-export and the
`ciborium` and `serde` dependencies are gone with it.
