RELEASE_TYPE: minor

This release loosens the argument types of `sampled_from()` and `one_of()` so callers don't have to pre-materialize a `Vec`.

- `sampled_from()` now accepts anything convertible into `Cow<'_, [T]>`, so `Vec<T>`, `&[T]`, `&Vec<T>`, and `&[T; N]` all work directly. Borrowed slices incur zero allocation up front: the generator keeps a `Cow::Borrowed` and only clones individual elements on draw.
- `one_of()` now accepts any `IntoIterator<Item = BoxedGenerator<'_, T>>` rather than requiring a `Vec`, so callers can pass iterator chains without an intermediate `.collect()`.

Turbofished calls like `sampled_from::<i32>(vec![])` are technically a source-breaking change, since the function now has a second generic parameter, but the non-turbofished call sites (every existing use in practice) continue to work unchanged.
