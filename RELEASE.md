RELEASE_TYPE: minor

This release removes `gs::fixed_dicts()` and its `FixedDictBuilder` /
`FixedDictGenerator` types. They produced CBOR `Value` maps — a leftover
currency from the engine's old serialization layer with no other
consumer. For fixed-shape records, use `#[derive(DefaultGenerator)]` on
a struct, or `gs::tuples!`. The `hegel::ciborium` re-export and the
`ciborium` and `serde` dependencies are gone with it.

This release also fixes two long-standing frontend issues:

- Dropping a Hegel handle cached in your own `thread_local!` no longer
  risks aborting the process during thread teardown when destructor
  ordering ran Hegel's internal state down first.
- When the engine reports an error while the runner is pulling the next
  test case, the run now fails with the engine's actual diagnostic
  instead of a misleading "run has not finished yet" internal error.

Finally, `chrono::NaiveDate` generation now draws through the engine's
new bounded date support, so `naive_dates()` shrinks toward 2000-01-01
(clamped into the configured bounds) rather than toward year zero, and
shrinks as a single unit.
