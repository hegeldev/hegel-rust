RELEASE_TYPE: minor

This release changes ordinary string arguments to borrowed `hegel_string_t` strings passed by value. Strings contain `const char *data` and `size_t len`, where the length counts bytes. Inputs need no NUL terminator; zero length accepts NULL data. String arrays contain strings, and borrowed string getters write a string through their output pointer. `hegel_context_last_error` returns a string directly. Output strings have non-NULL data and a trailing NUL, including empty strings. Lengths exclude the terminator. Interior NUL bytes are preserved; use the length to read the complete string. Optional strings use nullable `const hegel_string_t *` pointers: NULL means absent, while a pointer to an empty string means present and empty. Nullable borrowed outputs also return descriptor pointers. Owned draw and printer buffers keep their existing free functions. Callback signatures are unchanged.

Replace a string-pointer call such as `hegel_event(ctx, tc, label)` with:

```c
hegel_string_t label_string = {label, label_len};
hegel_event(ctx, tc, label_string);
```

Release builds and Wasm CI now use the current stable Rust compiler, with a required minimum of Rust 1.89. Local Wasm builds also select stable and enforce this minimum. A by-value string is passed as the address of an eight-byte descriptor in linear memory; a returned string uses a hidden output address. Rust toolchains older than 1.89, which flatten aggregates, are unsupported for Wasm builds. Native builds retain the Rust 1.86 minimum.
