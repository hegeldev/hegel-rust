# Review brief

Printable summaries of the nondeterminism branch, compiled from the book
(`notes/book/`), the design and decision notes, and the code:

- `brief.typ` — the review brief: problems, the design in brief, detailed
  considerations.
- `worked-example.typ` — one test with nondeterministic generation, followed
  through the engine end to end.

Build with:

```
typst compile brief.typ brief.pdf
typst compile worked-example.typ worked-example.pdf
```
