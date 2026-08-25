RELEASE_TYPE: minor

This release redesigns the `derive_generator!` macro for externally defined structs. Previously the macro implemented `DefaultGenerator` for the target type, which Rust's orphan rule rejects whenever that type comes from another crate, so every use the macro was designed for failed to compile. The only invocations that did compile were those in the same crate as the type definition, where `#[derive(DefaultGenerator)]` already works, so the macro was entirely redundant with the derive.

The macro now takes an explicit generator name and generates a standalone public generator struct with `new()` and a builder method per field, and no longer implements `DefaultGenerator`:

```rust
// before
derive_generator!(Person {
    name: String,
    age: u32,
});
let person: Person = tc.draw(gs::default::<Person>());

// after
derive_generator!(PersonGenerator for Person {
    name: String,
    age: u32,
});
let person: Person = tc.draw(PersonGenerator::new());
```

Because the orphan rule makes a `DefaultGenerator` impl impossible for foreign types, `gs::default::<T>()` cannot support types handled by `derive_generator!`; draw from the generated generator directly instead. For types defined in your own crate, keep using `#[derive(DefaultGenerator)]`.

This release also removes hegel's dependency on the `paste` crate, along with the hidden `hegel::paste` re-export.
