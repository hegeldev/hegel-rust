RELEASE_TYPE: minor

This release changes ordinary string arguments to borrowed `hegel_string_t` views passed by value. Views contain `const char *data` and `size_t len`, where the length counts bytes and no NUL terminator is required. String arrays contain views, and borrowed string getters write a view through their output pointer. `hegel_context_last_error` returns a view directly. `{NULL, 0}` represents an absent optional string; a non-NULL pointer with zero length represents an empty string. Owned draw and printer buffers keep their existing free functions. Callback signatures are unchanged by the string-view migration.

Replace a string-pointer call such as `hegel_event(ctx, tc, label)` with:

```c
hegel_string_t label_view = {label, label_len};
hegel_event(ctx, tc, label_view);
```

WebAssembly builds now use the Rust version pinned in `hegel-c/wasm-toolchain` (1.97.1). A by-value view is passed as the address of an eight-byte descriptor in linear memory; a returned view uses a hidden output address. Older Rust toolchains that flatten aggregates are unsupported for Wasm builds. Native builds retain the Rust 1.86 minimum.
