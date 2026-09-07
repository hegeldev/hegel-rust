RELEASE_TYPE: patch

This patch adds two printing combinators for mapped generators, for when a `map` produces a foreign type whose `Debug` output is not pastable Rust (`let kd = 0v3;`) but the drawn input is. `.print_as_call("path::to::function")` prints the mapped expression, and `.print_as_input()` prints the bare input marked with a `// pre-map input` comment. Both require the map's input generator to be printable ([#446](https://github.com/hegeldev/hegel-rust/issues/446)).

```rust
let keys = gs::integers::<u64>()
    .map(KeyData::from_ffi)
    .print_as_call("KeyData::from_ffi");
```

A failing draw from `keys` reports `let key = KeyData::from_ffi(3);`.
