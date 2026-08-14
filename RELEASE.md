RELEASE_TYPE: minor

This release changes `one_of!` to expand to arity-specific generator types
(`OneOf1Generator` through `OneOf12Generator`, mirroring the `tuples!`
design) instead of boxing every alternative into a `OneOfGenerator`. The
component generators keep their concrete types, so the macro's result is a
nameable type that can be stored in a struct field or returned from a
function, and building it no longer allocates per alternative. The drawn
choice sequence (a ONE_OF span around one index draw) is unchanged, so
saved failures replay identically.

Two things can break. Code that annotated the macro's result as
`OneOfGenerator` must now name the arity-specific type (or box the
alternatives explicitly and call `one_of()`):

```rust
// before
let g: gs::OneOfGenerator<i64> = hegel::one_of!(gs::integers(), gs::just(7));

// after
let g: gs::OneOf2Generator<gs::IntegerGenerator<i64>, gs::JustGenerator<i64>, i64> =
    hegel::one_of!(gs::integers(), gs::just(7));
```

And `one_of!` now supports at most 12 alternatives; a longer list is a
compile error pointing at the vec-based `one_of()`, which remains the way
to choose among a runtime-sized (or very large) collection of boxed
generators. `one_of()` itself now accepts any iterable of generators of
one type — `OneOfGenerator` is generic over the stored generator type,
defaulting to `BoxedGenerator`, so most existing uses keep compiling
unchanged. The exception is calls that spelled out the type arguments:
`one_of` gained a type parameter, so e.g. `one_of::<i64, _>(gens)` must
drop the turbofish (plain `one_of(gens)` infers everything).

For parity, the arity-specific tuple generator types (`Tuple0Generator`
through `Tuple12Generator`) are now exported as well, so `tuples!` results
can be named the same way, and `tuples!` reports the same clear compile
error as `one_of!` when given more than 12 generators.
