RELEASE_TYPE: minor

This release fixes a large number of bugs found by auditing the native engine against the Hypothesis implementation it is ported from, and adds one new float feature. Generated distributions change noticeably, and a few previously-silent invalid argument combinations now raise errors, so test suites may see different examples after upgrading.

Generation fixes:

- Integer and string draws were dominated by the "interesting constants" pool: for wide integer ranges (e.g. the full `i64` range) *every* draw came from a pool of ~500 constants and the intended heavy-tailed distribution never ran, and for permissive alphabets ~60% of strings were pool constants. The pool probability is now capped.
- The default `gs::floats()` generator returned exactly `f64::MAX` for ~90% of draws (the bounded draw's range-width computation overflowed to infinity). Bounded float draws now use Hypothesis's actual scheme — a lex-ordered draw biased toward simple values, clamped into range — so bounded ranges also regain mass on integers and simple fractions.
- `exclude_min`/`exclude_max` were silently ignored for infinite bounds: `min_value(f64::NEG_INFINITY).exclude_min(true)` (the Hypothesis idiom for "any float except -inf") kept `-inf` generable. Excluding an infinite bound now steps it to ±`f64::MAX`, and an exclusive bound without the corresponding bound now raises `InvalidArgument`, as Hypothesis does.
- Unicode category filters were truncated to the Basic Multilingual Plane: `exclude_categories` could still generate astral members of an excluded category (e.g. plane-15/16 private-use characters for `Co`), and `categories` silently dropped astral members (all emoji for `So`). Category sets now cover the whole codespace.
- `text().codec("ascii").include_characters("é")` silently generated `é`; include characters the codec cannot encode now raise `InvalidArgument`.

New feature: `gs::floats().allow_subnormal(false)` excludes subnormal ("denormalised") values, for testing code that may run with flush-to-zero floating point (e.g. compiled with `-ffast-math`), where subnormal inputs silently become zero. As in Hypothesis, the setting is inferred from the bounds when unset, and contradictory combinations raise `InvalidArgument`. This adds an optional `smallest_nonzero_magnitude` field to the float schema; schemas without it behave exactly as before.

Regex fixes:

- Character-class ranges with escaped endpoints (`[\x00-\x1f]`, `[\--/]`, …) were rejected with "bad character range"; they now parse exactly as CPython does.
- Under `(?i)`, literals with multi-codepoint case mappings could generate strings the pattern does not match (e.g. `(?i)ß` generated `'S'`), and negated classes only excluded the one-step case swap (`(?i)[^İ]` could generate `'I'`). Both now follow CPython's case-equivalence rules.
- Very large character classes (hundreds of thousands of codepoints) no longer take quadratic time to expand.

Engine fixes:

- A failure could be masked by a health check: once a bug was found, continued generation could trip `FilterTooMuch`/`TooSlow`/`TestCasesTooLarge` and report that instead of the bug. Health checks are now disabled from the first failure, as in Hypothesis.
- Failing examples counted against the `test_cases` budget as if they were valid examples; they no longer do.
- Database replay treated a stored example as stale if the test now draws even one more choice than the entry holds, deleting entries that still reproduce; replay now extends past the stored prefix with fresh draws.
- The secondary example corpus (where displaced failing examples are downgraded) was written but never read, and grew without bound. The reuse phase now samples it when the primary corpus comes up short, and the shrink phase drains stale entries, matching Hypothesis.
- A bug with a new panic site discovered *while shrinking another bug* was silently discarded; it is now shrunk and reported alongside the others.
- `phases = [Phase::Reuse]` (without `Phase::Generate`) silently did nothing; phases are now independent, so reuse-only runs replay stored counterexamples.
- A failing test that drew floats from a fraction-only range such as `min_value(0.1).max_value(0.9)` crashed the whole run with a `BigUint` underflow panic as soon as the shrinker touched the float. The float index computation behind this is now exact.
- The choice tree treated forced choices (e.g. a collection's size-boundary continuation decisions) as full-width branch points, so the search space behind any collection never registered as exhausted and "novel" prefixes could silently revisit explored territory.

Shrinking fixes:

- Integer shrinking was anchored at zero rather than the node's `shrink_towards` target: a value between zero and a non-zero target never moved (e.g. a date constrained to years before 2000 never shrank its year up toward the 2000 target). Integer passes now shrink the distance from the target, probing both sides, and pick up Hypothesis's `mask_high_bits` and byte-squeeze moves (predicates like `x & 0xff == 0x77` previously stalled).
- Shrinking two strings with different alphabets could crash debug builds via a failed assertion when a shared character's replacement existed in only one alphabet.
