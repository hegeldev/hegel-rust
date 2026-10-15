RELEASE_TYPE: minor

This release replaces the import callback of `hegel_grammar_new` with a grammar builder. libhegel now works out which file each `%import` needs, and the frontend only reads files. Previously the frontend had to answer Lark-style lookups and report back the path it found each grammar at.

```c
/* before */
hegel_grammar_new(ctx, text, start, max_size, max_depth, alphabet,
                  explicit_names, explicit_len, import_callback, import_data, &grammar);

/* after */
hegel_grammar_builder_t *b;
hegel_grammar_builder_new(ctx, text, text_len, &b);
for (size_t i = 0;; i++) {
    const char *path;
    size_t path_len;
    if (hegel_grammar_builder_file(ctx, b, i, &path, &path_len) == HEGEL_E_NOT_FOUND)
        break;
    /* read `path`; pass NULL, 0 if there is no such file */
    hegel_grammar_builder_provide(ctx, b, i, file_text, file_text_len);
}
hegel_grammar_new(ctx, b, start, max_size, max_depth, alphabet,
                  explicit_names, explicit_len, &grammar);
hegel_grammar_builder_free(ctx, b);
```

A file's path is slash-separated, relative and ends in `.lark`. `%import a.b.X` needs `a/b.lark`, and a relative `%import .c.X` in `x/y.lark` needs `x/c.lark`. The grammar text sits at the top level. Every path component is a Lark name, so a path joined under a root directory always stays inside it. Each path is asked for once. `hegel_grammar_new` only reads the builder, so one builder can compile grammars for several start rules. A file's text must now be valid UTF-8; previously invalid bytes were replaced.

Imports follow Lark's rules for absolute and relative imports, without a search path: the frontend serves one tree of files. When the frontend has no file for an absolute import, the bundled grammar at that path (`common.lark`, `lark.lark`, `python.lark` or `unicode.lark`) stands in. Nothing stands in for a relative import's file, so `%import .common.WS` now fails unless the frontend has `common.lark`; previously the bundled grammar was used.

`hegel_grammar_import_t` is removed, and the new `HEGEL_E_NOT_FOUND` result code marks the end of a builder's file list.
