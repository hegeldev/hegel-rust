RELEASE_TYPE: minor

This release changes ordinary string arguments to borrowed `hegel_string_t` strings passed by value.

Replace a string-pointer call such as `hegel_event(ctx, tc, label)` with:

```c
hegel_string_t label_string = {label, label_len};
hegel_event(ctx, tc, label_string);
```

Release builds and Wasm CI use the current stable Rust compiler, with a required minimum of Rust 1.89. This is because 1.89 introduced an improved Wasm ABI.
