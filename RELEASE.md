RELEASE_TYPE: patch

This patch improves several internals of the native engine to match
Hypothesis more closely:

- The unicode-normalization shrink pass now also tries NFKD
  (compatibility) decompositions and full case foldings when simplifying
  string characters, so e.g. `①` can shrink directly to `1` and `ß` to
  `s`, matching Hypothesis.
- Domain-name generation now draws label lengths the way Hypothesis's
  regex-based labels do (skewing heavily towards short labels) instead of
  uniformly over 1..=63, and averages 3 subdomains per domain instead
  of 6. Typical generated domains are much shorter as a result.
