RELEASE_TYPE: patch

This patch improves several internals of the native engine to match
Hypothesis more closely:

- The unicode-normalization shrink pass now also tries NFKD
  (compatibility) decompositions and full case foldings when simplifying
  string characters, so e.g. `①` can shrink directly to `1` and `ß` to
  `s`, matching Hypothesis.
