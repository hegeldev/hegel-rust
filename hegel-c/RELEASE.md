RELEASE_TYPE: minor

This release replaces the import callback of `hegel_grammar_new` with a grammar builder. libhegel resolves `%import` paths, and the frontend supplies file contents:

```c
/* before */
hegel_grammar_new(ctx, text, start, max_size, max_depth, alphabet,
                  explicit_names, explicit_len, import_callback, import_data, &grammar);

/* after */
hegel_grammar_builder_t *b;
hegel_grammar_builder_new(ctx, text, &b);
for (size_t i = 0;; i++) {
    hegel_string_t path;
    if (hegel_grammar_builder_file(ctx, b, i, &path) == HEGEL_E_NOT_FOUND)
        break;
    /* read `path`; pass NULL if there is no such file */
    hegel_grammar_builder_provide(ctx, b, i, &file_text);
}
hegel_grammar_new(ctx, b, start, max_size, max_depth, alphabet,
                  explicit_names, explicit_len, &grammar);
hegel_grammar_builder_free(ctx, b);
```

The frontend serves one tree of files, with no search path. Missing files fall back to bundled grammars only for absolute imports: `%import .common.WS` now requires a frontend `common.lark`. File contents must be valid UTF-8; invalid bytes are no longer replaced. See `hegel_grammar_builder_t` for path and fallback rules.

`hegel_grammar_import_t` is removed, and the new `HEGEL_E_NOT_FOUND` result code marks the end of a builder's file list.
