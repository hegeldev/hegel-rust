# Chronology

The `DRMacIver/nondeterminism` branch is 61 commits on top of `main` at a0185a65, spanning
three working days (2026-09-02 to 09-04) plus a review-fix day (09-05). Dates are git
author dates. The work divides into five eras. Numbering runs continuously across them: phases
1–17 across three plan documents, gates G1–G26, decisions through 67, experiments 001–012.
This chapter is the timeline. Detail lives in the sibling chapters.

| Era | Dates | Commits | Phases | Landmarks |
|---|---|---|---|---|
| Design and experiments | 2026-09-02 | 10 (80af87b2 … a1d1b6d2) | — | 80af87b2 design notes; 46304428 the discovery bar |
| Production plan | 2026-09-02 – 09-03 | 11 (27efbc6b … 9c800e8e) | 1–8 | 097275b3 the ABI break; 7a4fd194 concurrency unified |
| Remediation | 2026-09-03 – 09-04 | 19 (480c3970 … 8d4ca4f8) | 9–13 | 480c3970 the as-built critique; fdc30860 gate G20 |
| Seam plan | 2026-09-04 | 17 (0d639eb3 … 43b2eb35) | 14–17 | 41ed08c6 the tree removed; 43b2eb35 decision 67 |
| Review fixes | 2026-09-05 | 4 (b41de5a6 … 48894dc4) | — | 48894dc4 branch tip |

## Before the branch

The prior art landed over July and August 2026: PR #359 added cloneable test-case handles,
PR #360 gave the clones independent choice streams, and PR #378 built concurrent stateful
testing on top, handled by a sticky run-level flag that disabled shrinking, persistence,
blobs, and the data tree wholesale. Mid-PR, an explicit frontend nondeterminism setting was
added and removed again (5c7456b2, 2026-08-14) in favour of engine self-detection. That was
the project's first reversal, and it predates the branch. [Design history](design-history.md) covers
the regime the branch replaced.

## Design and experiments (2026-09-02)

The opening commit 80af87b2 is pure notes: seven code maps of every determinism-dependent
subsystem, sketch v0, four adversarial design critiques that forced the v0 to v1 reversals,
the decision log, and the experiment plan (`notes/experiments/000-plan.md`).
[Design history](design-history.md) covers the maps, the sketch, and the critiques.

Experiments 001–006 all land the same day, settling in turn the shrink-loop statistics and
the gauntlet (001 in simulation, 003 in the real shrinker), the engine's resampling seam
(002), pool and continuation-budget semantics (004, decision 22), the discovery bar by
exact DP (005A, decision 23), the failure lifecycle end to end (005B, decision 24), and
boost and splicing (006, decision 25). a1d1b6d2 closes the plan with an outcome summary.
[The experiments](experiments.md) covers each in reference form.

## Production plan: phases 1–8 (2026-09-02 – 09-03)

27efbc6b records decision 26 (this branch goes to production grade in place, extraction
later) and maps decisions 1–26 plus the experiment results onto eight phases with gates
G1–G4, closed as decisions 27–30 in 06f0b81a. Phases 1–6 land on 09-02: the statistics
module (00d7e39e), the origin lifecycle (66539c50), detection-driven mode entry replacing
the `NdExperiment` scaffold (beb9dbe5), the replay stack and v2 persistence (b327fd1f),
confirmed-dry stopping and the accounting split (dc6b50cb), and caveated reporting with the
engine-owned final replay and the ABI break (097275b3). On 09-03, phase 7 unifies
concurrent state machines with ND handling, deleting the sacrificed first case and the
blobless regime (7a4fd194), and phase 8 hardens, freezes `experiments/`, and writes
`notes/evaluation.md` (9c800e8e). [The production plan](production.md) covers the phases
and gates.

## Remediation: phases 9–13 (2026-09-03 – 09-04)

The same day phase 8 closed, an adversarial review of the as-built branch produced the
41-finding register and the remediation plan (480c3970). Its verdict was that the
architecture holds but the statistics don't compose. One gate sitting resolved G5–G19
wholesale (ffa83adf, decision 34). Phase 9 fixed the verified defects (27c13cee, 01232f71,
3aa085de). Phase 10 landed the structural fixes: evidence-carrying trust (62bbeda0),
save-then-delete supersession (2390c7ec), and the recursive watermark (bdcc0076).
Phase 11's experiment 009a exposed a latent shrinker crash on its first full run, fixed in
place (d409c04b), and closed gates G9 and G10 (27ca5d39). Phase 12 recalibrated the
constants from experiment 008 (c68a89eb). Phase 13 audited and swept (44091c2d through
7d197fa6), with the gate run recorded green on e79994f4.

Two threads cross the phase order. The phase-12 in-engine spot check (fa657947) surfaced
the deterministic-to-ND seam, recorded as gate G20 (fdc30860). G20 was the only finding
carried open past the era. Experiment 009b (00651a9e, 8d4ca4f8, 09-04) formally closed phase 12's
exit after phase 13's gate run had already been recorded.
[Remediation](remediation.md) covers the register and the fixes.

## Seam plan: phases 14–17 (2026-09-04)

The G20 analysis (0d639eb3) established that the seam is driven by pre-flip single-run
trust, not the data tree. Experiment 010 (de906f01) then measured that the tree buys
nothing a flat cache cannot recover. The seam plan (163026d8) was revised twice under DRM
review before implementation: the backtrack walk became a geometric boundary scan
(cc8a0c1f) and history retention became keep-everything with no eviction (4b997a54).

Phase 14 landed instrumentation, the experiment 011 baseline (6d85e9e9), and independent
fixes including v1 blob retries (ad3ff0c1 through 29e855e2, decision 59). Phase 15 removed
`data_tree.rs` and replaced its live roles with the exec cache, the duplicate stop, and the
kind ledger (41ed08c6, decisions 60–62). Phase 16 added origin history, the
first-interesting check, and backtrack (532be034, closed in 5d3aadc3 as decisions 64–66
together with the 011 comparison). Phase 17 ran experiment 012 and closed G20 as decision
67 with two priced residuals (43b2eb35). [G20 and the seam plan](seam-plan.md) covers the analysis,
the options, and the phases.

## Review fixes (2026-09-05)

A final adversarial review of the whole branch produced four commits: engine ND handling and
shrink scheduling (b41de5a6), blob encoding and decode hardening (67653632), the origin
lifecycle (0f504c52), and frontend, ABI docs, and notes (48894dc4, the branch tip). Some of these
commits reverse choices made in the remediation era: b41de5a6 restores demote-over-delete
for superseded reuse entries per decision 11. [Where the plan stands](../part1/status.md) covers their content
and what remains.

## Cadence

Every era is bracketed by adversarial review: the design notes shipped with four critiques,
the production plan was revised against a 21-finding critique before commit, the
remediation era opened with the as-built register, the seam plan was reviewed before commit
and twice after, and the plans' story ends with four review-fix commits (later work is
recorded in the decision log and [where the plan stands](../part1/status.md)). Gate outcomes are
recorded as numbered decisions in dedicated commits (06f0b81a, ffa83adf, 27ca5d39,
29e855e2, 5d3aadc3, 43b2eb35). Fixes are pinned by tests verified red on the pre-fix tree.
Two commits deliberately land out of phase order: the phase-7 smoke pinned right after
phase 3 (84d37318), and 009b closing phase 12's exit after phase 13's gate run. Experiment
harnesses are frozen rather than deleted, except experiment 010's, which lives on a
separate local branch off main. Extraction of the final implementation, the step that
decision 26 deliberately leaves outside all three plans, remains open at the tip.
