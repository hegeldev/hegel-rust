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
- The shrinker's candidate bookkeeping now matches Hypothesis's
  `cached_test_function`: trying a candidate reports whether the shrink
  target actually *improved* (previously "was interesting", which let
  some passes act on phantom successes), candidates larger than the
  current target are rejected without running the test, and the result
  cache is keyed on the candidate's choice values (the old sort-key-shape
  keying could falsely deduplicate distinct candidates whose constraints
  differed). Shape-probing passes now share that cache and call
  accounting instead of invoking the test function directly, removing
  some duplicate test executions during shrinking.
- The 500-shrink cap is now global across all failure origins in a run
  (matching Hypothesis's engine-level `MAX_SHRINKS`) rather than each
  origin getting its own 500-shrink budget.
- Targeted search (`tc.target()`) now ports the span-realignment retry
  from Hypothesis's optimiser: when perturbing a choice resizes the test
  case, the realised span content is spliced in front of the preserved
  old suffix, so score-gating draws after a resized collection no longer
  stop the hill climber.
