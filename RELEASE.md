RELEASE_TYPE: minor

This release cleans up public-API defects found in a documentation-vs-behavior audit: docs that promised APIs that didn't exist now have the APIs (or accurate docs), and doc examples are compiled as part of the test suite.

Breaking changes:

- The core string-returning date/time generators are renamed to say what they return: `gs::dates()`, `gs::times()`, and `gs::datetimes()` (which produce Python-isoformat `String`s, unlike the typed chrono/jiff generators of the same names) are now `gs::date_strings()`, `gs::time_strings()`, and `gs::datetime_strings()`, returning `DateStringGenerator`/`TimeStringGenerator`/`DateTimeStringGenerator`. The old function and type names remain as `#[deprecated]` aliases pointing at the new names and at `extras::chrono`/`extras::jiff` for typed, boundable values.
- `hegel::extras::jiff::DateGenerator` and `TimeGenerator` are no longer fieldless unit structs; construct them with `dates()`/`times()`.
- Unknown codec names passed to `text().codec(...)` or `characters().codec(...)` are rejected when the builder is called instead of on the first draw, so the error points at the mistake's call site.

Bug fixes:

- `#[hegel::state_machine]` methods can take `&self`, as the stateful docs always promised for invariants: `#[rule]` and `#[invariant]` methods now accept either `&self` or `&mut self` (a `&self` invariant previously failed with a type error inside macro-generated code). By-value `self` receivers get a targeted compile error.
- `#[hegel::test]` now expands to `#[cfg_attr(test, test)]`, so the function body is type-checked even in builds without `--test`. In particular doctests: a broken example inside a `#[hegel::test]` function used to sail through `cargo test --doc` because rustc removed the `#[test]` item before checking it. All doc examples showing `#[hegel::test]` are now compiled, which caught and fixed several: the `one_of!` example's test-function signature, the chrono `naive_dates` example, the async (`#[tokio::test]`) example's signature, and the crate-root `#[derive(DefaultGenerator)]` docs, which described enum builder methods (`default_<VariantName>()` / `<VariantName>(generator)`) that never existed — the real API (snake_case variant methods, closure form for struct variants, positional generators plus `<name>_with` for tuple variants) is now documented with compiled examples.
- The README's quickstart now shows the failure output Hegel really prints (`let vec1 = [0, 0];`, the named-draw replay form) instead of an output format from an older design.

Improvements:

- `hegel::extras::jiff::dates()` and `times()` gained `min_value`/`max_value` builders, matching every other generator in the module (their docs previously pointed at builder methods that didn't exist). Generated `Time`s keep whole-microsecond precision; bounds with sub-microsecond components are honoured by rounding inward, and a non-empty `Time` range containing no whole microsecond is a clean usage error. Date bounds can also widen the default years 1-9999 range down to jiff's minimum of `-9999-01-01`.
- The `.codec(...)` docs on `text()` and `characters()` now state exactly what each supported value does in Rust terms: `"ascii"` is `.max_codepoint(0x7F)`, `"latin-1"`/`"iso-8859-1"` is `.max_codepoint(0xFF)`, and `"utf-8"` is a no-op (every Rust `char` is UTF-8-encodable).
- `gs::uuids()`'s doc spells out that it returns hyphen-formatted `String`s.
