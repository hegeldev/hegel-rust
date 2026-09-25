RELEASE_TYPE: patch

This patch adds `hegel_printer_reflow`, which re-lays out a one-line debug representation through a printer's groups and break points. A binding that can only format a value flat — with its language's default debug formatter, such as Rust's `{:?}`, Go's `%#v`, Python's `repr`, JavaScript's `util.inspect` or a Java record's `toString` — passes the result in, and the engine recovers the bracket structure and wraps the value like one printed structurally: `Point { x: 100, y: 200 }` becomes

```text
Point {
    x: 100,
    y: 200 }
```

when it does not fit, and `main.Point{X:100, Y:200}` or `Point(x=100, y=200)` break one field per line aligned past the open delimiter. The grammar is language-agnostic — `(…)`, `[…]` and `{…}` groups with `, ` or `; ` separated items, quoted literals kept whole, and any text glued to an open delimiter kept as the group's prefix — and delimiter text is preserved verbatim, so a representation that fits on one line renders exactly as passed. Text that does not parse as that grammar is emitted verbatim, with newlines honored as hard breaks.

The call takes a `hegel_reflow_options_t` handle (`hegel_reflow_options_new` / `hegel_reflow_options_free`, or NULL for defaults). There are no settable options yet; the handle exists so that options can be added without changing the signature.
