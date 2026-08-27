RELEASE_TYPE: patch

This patch adds support for `#[derive(DefaultGenerator)]` on tuple structs ([#183](https://github.com/hegeldev/hegel-rust/issues/183)). The generated builder methods are positional, matching the `._0(...)` field builders already used by enum tuple variants:

```rust
#[derive(DefaultGenerator)]
struct Meters(f64);

let g = gs::default::<Meters>()._0(gs::floats().min_value(0.0).max_value(3.0));
```

Deriving on a struct with no fields (unit, empty named, or empty tuple) now produces a clear error in all three cases. Empty named structs previously failed with a confusing "unused lifetime parameter" error from inside the generated code.
