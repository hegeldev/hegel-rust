RELEASE_TYPE: minor

This release changes `#[hegel::composite]` generators and `hegel::compose!` closures to receive the `TestCase` by reference instead of by value:

```rust
# before
#[hegel::composite]
fn sorted_vec(tc: TestCase, min_len: usize) -> Vec<i32> { ... }

# after
#[hegel::composite]
fn sorted_vec(tc: &TestCase, min_len: usize) -> Vec<i32> { ... }
```

Since all `TestCase` methods take `&self`, the body of a composite rarely needs any change beyond the signature; code that moved the owned `TestCase` elsewhere (for example into a spawned thread) should call `tc.clone()` to get an independent handle, which was already the supported way to drive a test case from another thread.

The motivation is that `#[composite]` now expands to a named generator struct (`sorted_vec` above gets a `SortedVecCompositeGenerator`) instead of a function returning an opaque `impl`-typed generator. This makes composite generators much more capable:

- A composite can now recursively draw from itself, directly or through combinators like `one_of!`, which previously failed to compile. Recursive generators for tree-shaped data can now be written as ordinary recursive composites.
- The generator returned by a composite has a nameable type, so it can be stored in structs, returned from functions, and passed as an argument to other composites.
- Composite generators implement `Clone`.

Arguments to a composite (the parameters after the `TestCase`) are now stored on the generated struct and cloned into each draw, so they must implement `Clone`. Most argument types already do; to pass a non-`Clone` generator as an argument, box it first with `.boxed()`.

This release also fixes a bug where an explicit `return` inside a composite or `compose!` body left the generator's span open, unbalancing the span tree the shrinker uses and degrading shrinking for such generators.
