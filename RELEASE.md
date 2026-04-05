RELEASE_TYPE: patch

This release adds more configuration parameters to `generators::text()`:

```rust
gs::text().codec("ascii");
gs::text().min_codepoint(0x20).max_codepoint(0x7E);
gs::text().categories(&["L", "Nd"]);
gs::text().exclude_categories(&["Cc"]);
gs::text().include_characters("@#$");
gs::text().exclude_characters("\n\t");
```

As well as a new `characters()` generator:

```rust
let c: char = tc.draw(gs::characters());
let c: char = tc.draw(gs::characters().codec("ascii"));

// ...similar options to text()
```
