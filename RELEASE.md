RELEASE_TYPE: patch

Add more configuration options to `generators::text`. You can now write:

```rust
text().codec("ascii")
text().alphabet("abc")
text().min_codepoint(lo).max_codepoint(hi)
text().categories(&["Lu"])
```

This patch also adds `generators::characters`, with similar configuration options. `characters()` is a convenience for `text().min_size(1).max_size(1)`.
