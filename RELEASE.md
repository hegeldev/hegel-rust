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
- The pre-shrink one_of-branch lowering now ports the span-splice repair
  from Hypothesis's `try_lower_node_as_alternative`: after each random
  repair probe, the probe's realised branch span is spliced in front of
  the original suffix, so branch lowering succeeds for test cases whose
  post-branch draws must keep their old values.
- Float shrinking ports the remaining `Float.run_step` moves from
  Hypothesis: integer-valued floats now delegate to the full integer
  move set (so e.g. a float constrained by its low byte can still
  collapse via high-bit masking), floats above 2^53 shrink on the
  float grid where stepping a position by one is exactly `next_down`,
  and the precision-dropping ladder runs least-precise-first.
- The zig-zag escape (lowering a common offset across linked integers)
  now fires inside the value-minimization passes after each successful
  node shrink, as in Hypothesis, instead of waiting for its own
  scheduled pass — which the shrink budget could exhaust first — and it
  uses the full integer move set so complete collapse is always probed.
- `from_regex` with an `alphabet` now reports an `InvalidArgument` error
  at build time when the pattern cannot produce any string from the
  alphabet (a literal, charset, or every branch alternative outside it),
  matching Hypothesis's `IncompatibleWithAlphabet`; previously such
  patterns rejected every generated example. Branch alternatives that
  are incompatible with the alphabet are excluded from generation
  instead of being drawn and rejected.
- Generated regex matches are now filtered through a full match of the
  pattern, like Hypothesis's `.filter(regex.search)`, so mid-pattern
  anchors and the `\b` / `\B` word-boundary assertions are enforced
  instead of silently ignored.
- String and bytes values now shrink with the full move set of
  Hypothesis's `Collection` shrinker: an all-simplest probe at the
  current length (collapsing values whose elements are linked in one
  call), adaptive chunked deletion (O(log n) calls for a deletable run
  instead of O(n)), full-sort and gap-preserving reordering, joint
  minimization of duplicated elements, and per-element shrinking with
  the integer move set rather than a plain binary search.
- Shrink passes are no longer re-run against an unchanged shrink target
  (each pass is deterministic, so a re-run is pure waste — probe-based
  passes were re-executing every probe up to 20 times), mirroring how
  Hypothesis's per-pass choice trees exhaust and only reset when the
  target changes. The shrinker's stall guard now matches Hypothesis as
  well: a 200-call budget active from the first call that ends the
  whole shrink when exhausted, rather than a 500-call budget that only
  armed after the first successful shrink.
