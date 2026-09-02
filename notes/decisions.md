# Decision log

Append-only. Each entry: the decision, rejected alternatives, rationale. "DRM" = David,
"review" = the adversarial design review (`research/critique-*.md`), "agreed" = discussion.

## 2026-09-02

1. **Strictness defaults to quiet.** New setting `nondeterminism_strictness = quiet | warn | error`,
   default quiet — no notice at all when a run flips to ND handling. `error` keeps today's aborts
   for people using them as an accidental-global-state lint. (DRM. Rejected: notice-by-default.)

2. **Shrinking must not lower failure probability.** Raise it when possible (boost before
   shrinking); if shrinking reaches a deterministically-failing region, stay there. Guard against
   small-sample overconfidence. (DRM.)

3. **Unreproduced failures still fail the run**, reporting the observed failure with a caveat
   naming both hypotheses (rare failure vs. environment modification), evidence-weighted wording.
   (Agreed. Rejected: today's abort-with-flaky-error.)

4. **Failure identity stays per-origin** (panic site string). (DRM. Rejected: coalescing origins
   observed from one sequence.)

5. **Representation: timeline pool first.** Semantics are branch-points-with-per-value-suffixes;
   storage is a flat failing incumbent + bounded pool of whole realized timelines per origin.
   The merged trie encoding is deferred: no stable trunk during shrinking (every accept
   invalidates folded branches), anchoring is unreliable, and it costs a new ChoiceValue variant,
   CloneRecord equality, sort-key extension, and recursive serialization. Revisit if the pool
   shows heavy prefix sharing. (Agreed, from review. DRM's earlier preference was tree-shaped
   suffixes; conceded as storage, kept as semantics.)

6. **Data tree**: restore for ND runs if feasible; caching is low priority; disabling is
   acceptable. ND mode never serves cached conclusions. (DRM + review.)

7. **Shrink predicate: charge accepts, not rejects.** Single-run rejects; accepts accumulate
   ledger evidence past an LCB threshold scaled off the incumbent anchor. Rejected candidates
   must be retryable when a pass completes without shrinking (pass repetition), with evidence
   accumulating across retries. (Agreed; DRM added the reject-retry requirement. Rejected:
   per-candidate fixed-N for everything — uniform N-times cost; naive single-run accepts —
   monotone probability drift with no recovery.)

8. **Persist only the representation, never estimates.** ND-ness is carried by the entry/blob
   format itself (self-identifying); nondeterminism-rate estimates are computed afresh each run.
   (DRM, correcting an earlier "persist ND status" answer that had been read as a status flag +
   metadata.) Consequence: every run stands alone; CI with the database disabled is fully served;
   no flag clearing/flapping problem exists.

9. **Detection uses within-run evidence only.** A stored DB entry that stops reproducing is
   staleness (usually a code change), never ND evidence. (Review.)

10. **Capture at confirmation**, not at discovery: the engine marks confirmation replays
    capture-enabled; shrink probes stay cheap; the caveated fallback report shows the shrunk
    incumbent; the sacrificed-first-case trick and NondetStash go away. (Review; DRM directed
    that the current implementation is a bandaid to be replaced, not extended.)

11. **DB hygiene: demote then delete.** Replay budgets derived from the p >= 0.1 target with
    early exit (low-to-high 20s cap, ~1/p expected for real bugs); primary miss -> demote to
    secondary, secondary miss -> delete. No persisted miss counters. (Agreed.)

12. **Workload priorities**: concurrent stateful #1, plain clone-based concurrency #2; external
    randomness and timing dependence should fall out of a good implementation automatically. (DRM.)

13. **Scope**: ignore the parallel-tests branch until it lands; ignore Antithesis (separate
    determinator-based path inside Antithesis). (DRM.)

14. **Per-position anchoring (incl. inside clone streams): deferred, simplest-first.** Start
    with whole-timeline machinery only; instrument where replays fall off stored timelines. DRM
    is suspicious of the "no anchors in clone streams" argument — heavy nondeterminism may need
    *more* alternative tracking, not less — so this is decided by data, not now. (DRM.)

15. **Branch process**: this branch is working state. Commit notes and experiments liberally;
    history cleanliness doesn't matter; the final implementation will be extracted with pruning
    and history rewriting. (DRM.)

16. **Target**: handle tests failing >= 10% of the time they are run; budgets and confidence
    arithmetic are derived from that target rather than fixed small constants (a flat N = 10
    misses a p = 0.1 failure 35% of the time). (DRM set the target; review set the arithmetic.)
