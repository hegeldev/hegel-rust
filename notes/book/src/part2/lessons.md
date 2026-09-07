# False starts and lessons

Each entry gives what was tried, what showed it wrong, and what replaced it, ordered roughly
by when the replacement landed. The narrative around these reversals lives in
[the chronology](chronology.md) and the era chapters.

## Before the branch

**A declared nondeterminism setting.** PR #378 briefly carried an explicit frontend
`nondeterminism` setting, removed mid-PR (commit 5c7456b2, 2026-08-14) in favour of engine
self-detection. The branch kept that ruling: `nd_active` is set by detection and declaration,
never by a user flag, and the user surface is `nondeterminism_strictness` (decision 1).

**Wholesale surrender.** The pre-branch concurrent regime handled declared nondeterminism by
disabling everything: one sticky flag switched off shrinking, targeting, span mutation,
persistence, blobs, and the verify pass (and with it the Flaky check — detection disabled by
the thing it would detect). The first case creating a concurrent machine was sacrificed as
INVALID so later cases could be stamped before running. Reporting came from a single-slot
`NondetStash` that collapsed distinct bugs. Capture-at-confirmation (decision 10) dissolved
the constraint that forced the sacrifice, and phase 7 (7a4fd194) deleted the regime:
concurrent failures confirm, shrink, persist, and report v2 blobs like any other ND failure.

## Killed at design review

Sketch v0 lasted one day. The four adversarial critiques (see
[design history](design-history.md)) forced eight reversals before any code existed.

**The merged ND-node artifact.** v0 represented nondeterminism as branch points in the choice
sequence, per-value suffixes folded into one artifact. The representation critique showed it
has no stable trunk, since every accepted shrink invalidates all folded branches, and that no
comparison site (`consider`, `update_interesting`, reconciliation) ever sees such a node.
Replaced by the timeline pool (decision 5): a flat failing incumbent plus a bounded pool of
whole realized timelines per origin. The trie encoding stayed deferred until experiment 004
measured prefix sharing anticorrelated with pool need and closed it (decision 22).

**Misfit-anchored widening.** v0 triggered branching where a recorded value or kind no longer
fits a draw. The critique showed punning misfits to a unit value is a deliberate shrinking
mechanism widening would break, and that the dominant divergence class (same kinds, different
structure) is invisible to both triggers. Replaced by structural comparison signals, which
matured into the verbatim watermark weighting divergence as evidence (decisions 22, 45).

**Database-reuse divergence as a flip site.** v0 flipped a run when a stored entry replayed
differently. The lifecycle critique pointed out that between-run divergence overwhelmingly
means a code change, so this mis-stamps deterministic tests routinely. Replaced by within-run
evidence only (decision 9).

**Persisted ND status and rate estimates.** Inert in CI (the database is disabled by default),
and the clear-on-zero-divergence heuristic flaps on passing-but-still-ND tests. Replaced by
persisting only the representation: v2 entries are self-identifying and every run stands alone
(decision 8).

**"No stamping needed".** v0 claimed unified reporting removes the need to mark cases before
they start. The claim was false: emit, backtrace, and diagnostic decisions are all made before
the body runs, so an unreproduced failure would print a values-less report. Replaced by
capture-at-confirmation (decision 10) — the engine stamps replays capture-enabled. The stamp
was later renamed `hegel_test_case_should_capture` with no shim, so the changed contract broke
at compile time (decision 50).

**Raw-streak ratchet classes.** v0 classified incumbents deterministic / high-p / low-p from
consecutive-failure streaks. The statistics critique showed "never seen passing" from
stop-at-first-failure samples has near-zero power — a p = 0.7 bug enters shrinking classified
deterministic 70% of the time, after which essentially all true reductions are rejected.
Replaced by Wilson lower confidence bounds over one evidence ledger.

**Rollback to the previous checkpoint.** Re-baselining lets failure probability decay
geometrically: halvings compound across many checkpoints while each individual checkpoint
reads fine. Replaced by the monotone anchor. Experiment 001's follow-up then dropped
checkpointing entirely (rollback-on-uncertainty fires constantly on stable landscapes at 2.3x
cost, rollback-on-proof never fires — decision 17) and rejected anchor decay, which makes
stopping incoherent against a falling threshold (decision 19).

**Unguarded single-run accepts.** `consider()` accepts on one interesting run with no
un-accept, each noise accept consumes irreversible budget, and one lucky failing run in
`BinSearchDown`'s low-probe state teleports the incumbent. Replaced by charging accepts, not
rejects (decision 7): rejects stay single-run and retryable, and every accept pays the
gauntlet. The uniform-cost alternative, per-candidate fixed N, lost the bug on 100% of
experiment 001's noise-floor trials: accept-on-any-failure-within-N amplifies noise acceptance.

Boost by Optimiser hill-climbing also died (three verified contradictions; experiment 006A's
holdout-gated successive-halving race replaced it), as did outcome distributions in the tree.
The statistics critique's lasting product was methodological: build a pure-simulation
calibration harness before touching the engine — the origin of the numbered experiment series.

## Measured out in the experiment era

**Fixed dry sweeps.** The first 001 run stopped after three dry sweeps. The follow-up replaced
that with confirmed-dry (decision 18): after one dry sweep, a confirmation sweep where every
proposal skips the fast reject and drives its ledger evidence to a bound decision. Same cost
as three dry sweeps, half the recoverable missed-reduction rate, and stopping carries a
certificate rather than a count. The final review later turned the stall guard off during
confirmation sweeps, because the certificate holds only if every candidate executes (b41de5a6).

**The 2-in-20 confirmation scaffold.** Experiment 003's placeholder bar was explicitly
provisional (decision 21). Experiment 005A's exact DP priced it at a 6% false accept per fluke
(26.6% run-level) and replaced it with the gate-then-extend discovery bar (decision 23).

## Constants that did not survive contact

**The gauntlet degenerated back to single-run accepts.** The mechanism introduced to kill
unguarded accepts shipped with constants that reproduced them: a fresh ledger's single failure
has Wilson LCB 0.2065, and any anchor at or below 0.258 — the whole target regime, on
bar-seeded anchors — accepted every candidate on its recruiting failure. Experiment 008
measured 33% bug loss at the p = 0.1 target (experiment 001's naive-policy arithmetic
resurfacing). The fix is composed: `GAUNTLET_MIN_FAILS = 4`, `ANCHOR_SEED_RUNS = 20` at both
seeding sites, and a floor derived rather than chosen (0.05 < LCB(4/30) = 0.0531, so it costs
no power). Neither piece works alone: extension without min-fails retains less than the
shipped rule (decision 54). Each constant had been derived in an isolated experiment, the
composition from bar to anchor to gauntlet was never measured, and the shipped anchor seeding
had never been simulated at all. The one composed safeguard that survived, the monotone
anchor, is why this was a recalibration and not a redesign.

**The boost floor was in the wrong units.** Decision 28's 0.5 was written for the old
estimator; against honest 20-run anchors it triggered on 59% of true-0.7 incumbents.
Decision 56 re-derived it as 0.30 — LCB(10/20), the estimator's image of "true rate
below 0.5". Changing the estimator silently re-prices every threshold written against it.

**A transcription error shipped as a budget.** `REPRODUCE_SPLICES` shipped as 6 because
decision 25's "(~6 replays/miss)", a cost per rescue, was read as the attempt cap. The 65-100%
rescue rates had been measured at a 10-splice cap. Corrected to 10 (decision 52).

**The watermark zeroed out on the flagship workload.** `verbatim_weight` was element-granular
over top-level values, and an entire clone stream is one element compared by whole-record
equality, so any intra-stream divergence zeroed the miss weight and evidence degenerated to a
fail counter on concurrent machines. The weighting had been derived on flat scalar bodies, and
experiment 007's ceiling reproduction rates never exercised misses. Decision 45 replaced it
with flat-length weighting and recursive clone descent. Experiment 009a then measured the old
weighting putting 78-97% of misses at exactly zero — in practice, experiment 008's
known-broken w = 0 column — where the new estimator left no mass at zero (decision 57).

## The data tree

Sketch v0 wanted the tree restored for ND runs if feasible (decision 6). The representation
critique cut that to detection and novel-prefix generation only, and decision 29 then kept it
disabled under ND. The G20 analysis absolved it of the seam (the cliff was single-run trust,
not caching), but the question of what the tree buys had been asked, and experiment 010
answered it on production main. Recording alone cost 40-80% wall overhead on passing bodies,
and novel prefix and exhaustion bought nothing measurable on large spaces. Serving's one real
win, 6.5x on non-stateful shrinking, inverted on the stateful workload, and serve counts
matched exact-repeat counts, so a flat cache recovers it. Phase 15 (41ed08c6) deleted
`hegel-c/src/native/data_tree.rs` and replaced its live roles with the execution cache, the
duplicate stop, and the kind ledger (decision 60). The replacement detects verdict flips the
tree always silently overwrote, and experiment 011's baseline showed the tree's own detection
channel, the kind mismatch, firing zero times in 600 trials. "Restore if feasible" ended in
removal.

The removal carried two nested reworks. The planned unconditional duplicate stop ended a
32-way `one_of` before reaching every alternative (late in coupon collection a duplicate
streak is near-certain), so it was scoped to run only while no valid case exists
(decision 61). And the planned reuse-comparison flip channel was dropped for the within-run
kind ledger, on decision 9's ground (decision 62).

## The quiet flip as the loss mechanism

The mode split the project was built around (run deterministic until evidence arrives, then
flip) became the dominant loss mechanism, not through any ND algorithm but through pre-flip
deterministic rules acting irreversibly on single observations. Phase 12's in-engine spot
check (fa657947) found the shrink mechanics holding wherever a confirmed origin entered
shrinking, and every headline miss in the lazy entry: pre-flip displacement had walked the
incumbent toward the minimal-bug floor before any detector fired, and a flip at shrink-verify
met a bar with one attempt and no re-hunt budget (49% of target-regime trials reported
caveat-only). Experiment 009a added the third loss mechanism: never-flipped runs persisted
v1 blobs reproducing at 13%. The resolution (decisions 64-67;
see [the seam plan](seam-plan.md)) does not detect earlier so much as make pre-flip actions
reversible: the first-interesting check spends four replays before anything consumes an
origin, the history keeps what displacement destroyed, and the backtrack gives the bar several
attempts instead of one. Experiment 011 measured caveat-only falling to zero, and experiment
012 measured never-flip episodes at 0/200 against a 23/200 baseline at p = 0.9.

Two seam-plan mechanisms were themselves reworked at review before landing.

**The backtrack walk.** Planned as a linear newest-first walk over the history. Review showed
it burns the scan budget on the degraded tail and re-runs the exact failure it exists to fix —
the bar admits a degraded recent entry and the anchor ratifies it (cc8a0c1f). Replaced by a
geometric boundary scan with binary refinement, biased old under uncertainty; decision 2 makes
the old bias safe (decision 66).

**History eviction.** Planned as a bounded ring with evict-oldest. Review showed evict-oldest
deletes the reproduction boundary exactly when shrinking went nondeterministic early, and the
memory the bound defended against had left with the tree (4b997a54). Replaced by
keep-everything with dedup, moving the scan's termination argument from the history bound to
the replay caps (decision 65).

**v1 blob replay** belongs to the same family. One bare `for_choices` shot with no
continuation reproduced never-flipped failures at 13%, where the reuse path, which allows
continuation, held 99% on the same episodes: the fragility was in alignment, not example
quality. Replaced by `V1_BLOB_REPLAYS = 4` budgeted attempts under the standard continuation
budget (decision 59).

## Rules that keep the statistics honest

One failure shape recurred across eras: an estimate contaminated by the selection process that
produced it. Each instance got the same class of fix — move the accounting to a validated
event.

**Validated accepts.** Raw interesting runs are selection, not evidence. Experiment 003 showed
flukes displacing a 20/20-confirmed discovery in ~80% of noise-floor trials (decision 20:
never displace an occupied origin). Experiment 005B's run-triggered confirmation hook let
span-mutation executions fill origins unconfirmed, confirming flukes in 26 of 30 pure-noise
runs (decision 24: confirmation gates admission on every path, enforced by the discovery
sweep). The as-built critique's S7 showed gauntlet accepts among never-adoptable candidates
raising the anchor and pricing real reductions out (decision 36: only the shrinker's adoption
consumes an accept).

**Anchor bias.** The anchor estimates one thing — the incumbent's reproduction rate under the
engine's own pinned-replay procedure (decision 46). Post-accept evidence never feeds it, or a
pinned incumbent prices fresh-generation candidates out and stalls the shrink (decision 19).
Seeding batches extend to 20 physical runs past their accept, so anchors estimate the rate
rather than the stopping rule (decision 54): a four-straight-fail bar batch would otherwise
seed 0.51 regardless of the true rate.

**The in-batch witness.** The first-check seed slot deposits a miss's evidence into the
origin's next evidence batch so observations are not paid for twice. As shipped, a seeded
quota could satisfy the whole bar with no reproducing replay in the batch itself. The final
review closed it: a bar accept requires an in-batch reproducing replay as witness (b41de5a6).

## Persistence ordering

**Save-then-delete.** Supersession originally demoted superseded same-run saves, so the
secondary corpus grew without bound while an ND failure stayed live: every gauntlet accept
persisted and demoted, and under ND every run re-shrinks and re-deposits (the as-built P2).
Decision 44 (2390c7ec) writes the new incumbent before deleting what it supersedes, so the
primary key carries the most recent validated example at every instant and Ctrl-C loses
nothing. Superseded same-run saves are deleted outright, since they never ended a run as
anyone's best example, and the secondary corpus is capped at 50 per key. The pattern recurred
twice more: the pre-shrink drain had deleted v2 entries it never replayed, a zero-strike
deletion violating decision 11 (decision 40 scoped it to v1 under deterministic handling), and
the final review found supersession of a reused run-start entry deleting where decision 11
says demote, plus byte-identical entries shared between origins dying with one origin's
supersession (b41de5a6).

## What review caught that tests had not

The suite verified, throughout, that the code did what its author believed. Two of the worst
defects were pinned by passing tests: the gauntlet's single-run accept had a unit test
asserting the degenerate case as intended, and the values-less caveat-only report was pinned
as designed while neither the decision log nor the design acknowledged the loss (as-built S1,
R2). The DP fixtures likewise passed while the composed pipeline degenerated, because
composition was not a property any single fixture stated.

The as-built round (nine subsystem reviewers plus four design-foundations auditors, every
finding adversarially verified, six killed in verification) found the composition failure, the
watermark degeneracy, the persistence bugs, and thirteen documentation-drift findings. The
final branch review (b41de5a6 through 48894dc4) caught cross-feature interactions the suite
had no test shaped to see. A flip during a successful deterministic final replay kept its
pre-flip verdict, and a dry pooled review rejected without consulting the history. The fast
reject could overrule a conclusively accepted ledger with one dry replay, and the caveat-only
fallback ignored `report_multiple_failures`. One interaction had been created by an earlier
hardening fix: the phase-9 decode inflation bound rejected the encoder's own output on large
payloads (67653632).

Experiments were the other amplifier. Experiment 009a's first full run crashed the shrinker on
a latent stale-index bug in `bind_deletion` that only an adopted-shorter-realization sequence
reaches. Experiment 012, run to close a detection corner, surfaced the gauntlet cost lottery
above the retention high-water — around a million measurement replays per episode on constant
p = 0.9 bodies, escalated as a follow-up outside G20's accounting (decision 67). Neither was
caught by the suite; both needed a harness running the machinery at a scale the tests do not
reach.
