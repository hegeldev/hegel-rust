RELEASE_TYPE: minor

This release removes the 12-component limit on `one_of!`: it now accepts any number of generators. Components stay unboxed, and the result is a `PrintableGenerator` exactly when every component is one.

`one_of!` now builds the same `OneOfGenerator` the vec-based `one_of` function returns, holding a `OneOfCons`/`OneOfLast` chain of its components and dispatching through the new `Alternatives` and `PrintableAlternatives` traits, instead of one of twelve fixed-arity generator types. This breaks code that named those types:

- `OneOf1Generator` through `OneOf12Generator` are gone. To name the type of a `one_of!` result, write the chain out (e.g. `OneOfGenerator<'static, T, OneOfCons<G1, OneOfLast<G2>>>`), or box the components and store a `OneOfGenerator<'static, T>`.
- `OneOfGenerator`'s third type parameter is now the alternatives store rather than the boxed element type: `OneOfGenerator<'a, T, BoxedGenerator<'a, T>>` becomes `OneOfGenerator<'a, T, Vec<BoxedGenerator<'a, T>>>`. The default `OneOfGenerator<'a, T>` form is unchanged.
