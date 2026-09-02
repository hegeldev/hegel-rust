# 001: shrink-statistics simulation

Code: `/experiments/shrink-sim`. Pure simulation of the shrink loop's statistical machinery —
no engine, every "execution" is one Bernoulli draw from a landscape's true failure probability.
All the open constants (gauntlet thresholds, gamma, stopping rules, budgets) are properties of
the scheduler logic, so they can be calibrated here in milliseconds per trial and the harness
becomes the regression suite for the real implementation.

## Model

A test case is `Vec<u64>` (atoms 0..=100). An atom >= 50 is a "bug atom"; a candidate
"carries the bug" iff it contains one. Sort key is shortlex (length, then lexicographic).
Starting example: length 20, at least 3 bug atoms, seeded per trial.

Passes (mirroring the real shapes that matter statistically):

- `delete_chunk(k)` for k in 8, 4, 2, 1 — window deletions, retry same index on success.
- `zero_atom` — set each nonzero atom to 0.
- `minimize_atom` — per index: probe 0 first (deliberately reproducing `BinSearchDown`'s
  probe-lo-first teleport hazard), then binary search down.

## Landscapes (candidate -> true p)

- **L1 rising-with-size**: no bug -> 0; else p = clamp(0.1 + 0.08 * (len - 1), 0.1, 0.95).
  Shrinking inherently lowers p; the minimal example is only p = 0.1. Exposes the gamma
  (size vs reliability) tradeoff.
- **L2 deterministic-core**: any atom == 50 -> 1.0; else bug -> 0.35; else 0. `minimize_atom`
  converges bug atoms to exactly 50, so a deterministic region is reachable — tests "shrink
  into determinism and stay".
- **L3 constant**: bug -> 0.5, else 0. Pure outcome noise; the right answer is full
  minimization with final p still 0.5. Measures stalling and wasted cost.
- **L4 noise-floor**: bug -> 0.9; no bug -> 0.02. Background flakiness unrelated to the bug.
  The trap: a bugless candidate fails 2% of the time; a naive accept adopts it and the
  reported example no longer contains the bug at all. Key metric: fraction of trials whose
  final example lost the bug.

## Policies

- **P0 naive**: accept on a single failing run (what deterministic shrinking would do).
- **P1 per-candidate-N**: every candidate judged by fails-within-N (N = 10, early exit) —
  the uniform-cost design rejected in discussion; kept as the cost/quality reference.
- **P2 fixed gauntlet**: single-run rejects; accept requires m = 5 consecutive failures.
- **P3 ledger gauntlet**: single-run rejects with evidence retained (0/1); a first-run failure
  starts a sequential test — keep running until Wilson LCB(candidate) >= gamma * anchor
  (accept), Wilson UCB < gamma * anchor (reject), or run cap 30. Anchor = monotone
  max of validated incumbent LCBs, initialized from up-to-20 confirmation runs of the start
  example. Evidence accumulates across pass repetitions, so retried rejects gain power.
- **P4 = P3 + checkpoint/rollback**: at sweep end, validate the incumbent (top up its ledger
  to >= 10 runs); if its LCB < 0.5 * anchor, roll back to the last validated snapshot and
  poison the accepted candidates since. Measures whether checkpointing adds anything once
  accepts are gauntleted.

All policies: sweeps repeat until S = 3 consecutive sweeps without an accept (uniform, so
stopping power isn't confounded with accept policy), execution cap 200k per trial.

## Metrics

Per trial: final length, final true p, bug retained?, executions, accepts, gauntlet rejects,
rollbacks, sweeps. 200 seeds per (policy, landscape); report median and p10/p90 for the
numeric metrics, rate for bug-retention. Plus a gamma sensitivity slice: P3 on L1 with
gamma in {0.5, 0.8, 1.0}.

## Questions this must answer

1. How much does p drift under P0, per landscape? (Quantifies the problem.)
2. Does the ledger gauntlet hold the never-lower-p constraint, and at what execution cost vs
   P1? (The user's pass-repetition preference vs per-candidate-N, settled empirically.)
3. Does P4's checkpointing improve anything over P3, and how often does it thrash?
4. On L4, what fraction of final examples keep the bug, per policy?
5. On L2, which policies find and keep the deterministic core?
6. On L1, what does gamma buy: final size vs final p across gamma values?

## Results (2026-09-02, first run)

### L1 rising-with-size

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P0 naive | 0.10 (0.10-0.18) | 1 | 100% | 121 (162) | 11.6 | 0.0 | 0.00 | 0 |
| P1 fail-within-10 | 0.10 (0.10-0.10) | 1 | 100% | 483 (596) | 8.4 | 0.0 | 0.00 | 0 |
| P2 gauntlet-m5 | 0.42 (0.42-0.50) | 5 | 100% | 1020 (1358) | 20.4 | 210.7 | 0.00 | 0 |
| P3 ledger g0.8 | 0.58 (0.50-0.66) | 7 | 100% | 2588 (4755) | 10.1 | 162.2 | 0.00 | 0 |
| P4 ledger+ckpt g0.8 | 0.58 (0.50-0.66) | 7 | 100% | 2942 (5214) | 10.8 | 171.4 | 0.17 | 0 |

### L2 deterministic-core

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P0 naive | 1.00 (1.00-1.00) | 1 | 100% | 77 (107) | 10.7 | 0.0 | 0.00 | 0 |
| P1 fail-within-10 | 1.00 (1.00-1.00) | 1 | 100% | 373 (392) | 6.9 | 0.0 | 0.00 | 0 |
| P2 gauntlet-m5 | 1.00 (0.35-1.00) | 1 | 100% | 355 (2048) | 10.7 | 157.9 | 0.00 | 0 |
| P3 ledger g0.8 | 1.00 (1.00-1.00) | 1 | 100% | 106 (167) | 9.9 | 1.1 | 0.00 | 0 |
| P4 ledger+ckpt g0.8 | 1.00 (1.00-1.00) | 1 | 100% | 132 (387) | 11.4 | 2.0 | 0.33 | 0 |

### L3 constant p=0.5

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P0 naive | 0.50 (0.50-0.50) | 1 | 100% | 75 (104) | 10.6 | 0.0 | 0.00 | 0 |
| P1 fail-within-10 | 0.50 (0.50-0.50) | 1 | 100% | 370 (388) | 6.9 | 0.0 | 0.00 | 0 |
| P2 gauntlet-m5 | 0.50 (0.50-0.50) | 3 | 100% | 1238 (1763) | 20.6 | 301.5 | 0.00 | 0 |
| P3 ledger g0.8 | 0.50 (0.50-0.50) | 1 | 100% | 149 (260) | 10.9 | 1.2 | 0.00 | 0 |
| P4 ledger+ckpt g0.8 | 0.50 (0.50-0.50) | 1 | 100% | 205 (320) | 11.7 | 2.0 | 0.28 | 0 |

### L4 noise-floor

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P0 naive | 0.90 (0.02-0.90) | 1 | 66% | 43 (60) | 7.5 | 0.0 | 0.00 | 0 |
| P1 fail-within-10 | 0.02 (0.02-0.02) | 0 | 0% | 109 (319) | 7.3 | 0.0 | 0.00 | 0 |
| P2 gauntlet-m5 | 0.90 (0.90-0.90) | 1 | 100% | 105 (144) | 8.9 | 5.4 | 0.00 | 0 |
| P3 ledger g0.8 | 0.90 (0.90-0.90) | 1 | 100% | 114 (149) | 7.3 | 0.5 | 0.00 | 0 |
| P4 ledger+ckpt g0.8 | 0.90 (0.90-0.90) | 1 | 100% | 119 (153) | 7.3 | 0.5 | 0.00 | 0 |

### gamma sensitivity, L1 rising-with-size

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P3 ledger g0.5 | 0.34 (0.26-0.42) | 4 | 100% | 921 (1423) | 10.6 | 35.6 | 0.00 | 0 |
| P3 ledger g0.8 | 0.58 (0.50-0.66) | 7 | 100% | 2588 (4755) | 10.1 | 162.2 | 0.00 | 0 |
| P3 ledger g1 | 0.74 (0.66-0.90) | 9 | 100% | 4752 (8656) | 17.7 | 292.4 | 0.00 | 0 |

### gamma sensitivity, L4 noise-floor

| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P3 ledger g0.5 | 0.90 (0.90-0.90) | 1 | 100% | 87 (113) | 7.4 | 0.6 | 0.00 | 0 |
| P3 ledger g0.8 | 0.90 (0.90-0.90) | 1 | 100% | 114 (149) | 7.3 | 0.5 | 0.00 | 0 |
| P3 ledger g1 | 0.90 (0.90-0.90) | 1 | 100% | 196 (618) | 7.6 | 14.4 | 0.00 | 0 |

## What we learned

Answering the questions in order:

1. **Drift under naive accepts is real and lands two different ways.** On L4, P0 loses the
   bug entirely in 34% of trials (final example is a bugless case that fails 2% of the time).
   On L1 it descends to the true minimum, which happens to be p = 0.1 — legitimate
   minimization, but nothing enforced a floor.
2. **The ledger gauntlet holds the constraint, and per-candidate-N cannot buy the same result
   at any price.** P1 (fail-within-10) is the striking row: on L4 it loses the bug in **100%
   of trials** and reports the empty test case (p = 0.02). Mechanism: accept-on-any-failure-
   within-N gives a bugless candidate an 18% acceptance chance per proposal (1 - 0.98^10),
   so retrying across sweeps guarantees eventual acceptance, and each noise-accept ratchets
   the sort key down with no recovery. Per-candidate-N without a probability ratchet doesn't
   just cost N times more — it *amplifies* noise acceptance relative to single-run accepts.
   The pass-repetition + charge-accepts preference wins outright: P3 keeps the bug in 100% of
   trials everywhere, at ~1.5-2x naive cost except where the constraint actually binds (L1:
   ~2.6k median executions, ~20x naive, spent proving near-threshold rejects are weak — see
   the g-rej column).
3. **Checkpointing adds nothing once accepts are gauntleted.** P4 tracks P3 exactly on every
   landscape while costing 10-40% more; rollbacks are rare (0.17-0.33/trial) and never change
   the outcome. Caveat before deleting it from the design: the sim's accept gauntlet samples
   the same distribution the checkpoint validates, while the real engine has timeline
   mixtures (pooled candidate rate vs pinned-replay rate can differ), which is the one
   mechanism that could still make post-accept collapse likely. Re-test in experiment 003
   before dropping rollback entirely.
4. **L4 bug retention**: P0 66%, P1 0%, P2/P3/P4 100%.
5. **Everything finds the deterministic core on L2; ledger policies keep it and get cheaper.**
   P3's anchor rises when the core is hit, blocking any later low-p accepts, and its accepts
   are ~1 run each once p = 1 (106 median executions, only ~40% over naive). P2's fixed m = 5
   has a bad tail (p10 = 0.35): a fixed gauntlet is an implicit fixed threshold and
   misclassifies when the true rate sits near it (it also stalls at len 3 on L3 — fixed m is
   the wrong shape; the threshold must come from the incumbent).
6. **Gamma is a clean dial on the size-vs-reliability tradeoff** (L1): g0.5 -> p 0.34/len 4,
   g0.8 -> p 0.58/len 7, g1.0 -> p 0.74/len 9, with cost rising steeply in gamma (near-
   threshold rejects take many runs to prove weak). On L4, where the constraint doesn't bind,
   gamma barely matters (87-196 executions). Note g1.0 against a Wilson-LCB anchor is still
   not literally "never lower than the true starting p" — the anchor is a lower bound, so
   ~0.15-0.2 of slack below true p is built in even at gamma = 1.

Model caveats: candidate-level p here is exact, stationary, and identical for proposal and
replay — no timeline mixtures, no misalignment, no adaptive-search state, no stall guards.
The stopping rule (3 dry sweeps) was held uniform; evidence-based stopping is still to do.

Follow-ups for this harness:

- Evidence-based stopping (replace the fixed 3-dry-sweeps rule; measure missed-reduction
  probability against the ledger bound).
- A mixture landscape (candidate realizes one of several timelines with different p) to
  stress the pooled-vs-pinned gap and give checkpointing a fair chance to matter.
- Anchor-decay variant: the monotone anchor makes L1 stop at len ~7; the user constraint
  says that's correct, but measure what a slowly-decaying anchor buys in size if that stance
  ever softens.

## Follow-up run

Harness additions (all three follow-ups above):

- **Stopping rules**: `FixedDry(k)` for k in 1..3, and `ConfirmedDry` — sweep until one dry
  sweep, then run a confirmation sweep in which every proposal skips the single-run fast
  reject and drives its cumulative ledger evidence to a bound decision (LCB >= threshold
  accept, UCB < threshold or cap-30 reject); stop only if the confirmation sweep accepts
  nothing. New metric: **missed** = an oracle checks, at stop, whether any one-step-reachable
  smaller candidate has true p >= the final acceptance threshold.
- **L5 mixture**: bug candidates have two timelines, p_hi = 0.9 (weight 0.5) and p_lo = 0.05.
  Candidate evaluation always draws fresh (marginal p = 0.475). On accept the incumbent gets
  a *pinned* timeline; post-accept incumbent runs replay the pin with probability 0.8, else
  draw fresh. **eff p** = the pinned-regime failure rate a user replay would see. Pin modes:
  *pin-failing* (capture-at-confirmation: pin drawn proportional to weight x p, so ~95% hi)
  and *pin-random* (pool-fit accident: pin drawn by weight alone, 50% lo).
- **Checkpoint semantics fixed while modeling this**: validation must use *post-accept* runs
  under the pinned regime (the pre-accept gauntlet runs were fresh-generation and are stale
  evidence for the pinned incumbent), and checkpoint evidence must *not raise the anchor*
  (a pinned-hi incumbent's rate would price fresh-generation candidates out and stall the
  shrink). Two rollback rules measured: **v2** rolls back when 10 fresh runs have
  LCB < 0.5 x anchor (rollback on uncertainty); **v3** accumulates 10 runs per checkpoint up
  to 40 and rolls back only when UCB < 0.5 x anchor (rollback on proof), advancing the
  snapshot only when LCB >= the bar.
- **Anchor decay**: anchor *= d at each sweep end, d in {1.0, 0.98, 0.95}.

### Stopping (P3 ledger g0.8)

| landscape | rule | missed | execs med (p90) |
| --- | --- | --- | --- |
| L1 | dry-1 | 10% | 2166 (4078) |
| L1 | dry-3 | 9% | 2588 (4755) |
| L1 | confirmed | 10% | 2428 (4518) |
| L3 | dry-1 | 46% | 116 (232) |
| L3 | dry-3 | 18% | 149 (260) |
| L3 | confirmed | 10% | 245 (327) |
| L4 | dry-1 | 2% | 96 (130) |
| L4 | dry-3 | 0% | 114 (149) |
| L4 | confirmed | 0% | 110 (144) |

### L5 mixture (fd3; final p pooled = 0.48 everywhere, len med 1, bug kept 100%)

| pin mode | policy | eff p med (p10-p90) | execs med | rollbacks |
| --- | --- | --- | --- | --- |
| failing | P0 naive | 0.82 (0.82-0.82) | 78 | - |
| failing | P3 ledger g0.8 | 0.82 (0.82-0.82) | 135 | - |
| failing | P4 v3 | 0.82 (0.82-0.82) | 219 | 0.00 |
| random | P0 naive | 0.14 (0.14-0.82) | 78 | - |
| random | P3 ledger g0.8 | 0.82 (0.14-0.82) | 135 | - |
| random | P4 v2 (LCB rule) | 0.82 (0.82-0.82) | 254 | 2.61 |
| random | P4 v3 (UCB rule) | 0.14 (0.14-0.82) | 222 | 0.01 |

The pin-random eff-p distributions are bimodal (0.14 or 0.82, set by the final accept's coin
flip), so medians near the 50% split are noise; read the p10.

### Checkpoint rule on stable landscapes (P4 vs P3 baseline)

| landscape | variant | final p med | len | missed | execs med | rollbacks |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | P3 (no ckpt) | 0.58 | 7 | 9% | 2588 | - |
| L1 | P4 v2 | 0.66 | 8 | 52% | 5927 | 5.56 |
| L1 | P4 v3 | 0.58 | 7 | 9% | 2516 | 0.01 |
| L3 | P4 v2 | 0.50 | 1 | 32% | 210 | 1.16 |
| L3 | P4 v3 | 0.50 | 1 | 11% | 225 | 0.00 |

### Anchor decay (P3 g0.8, fd3)

| landscape | decay | final p med (p10-p90) | len | missed | execs med |
| --- | --- | --- | --- | --- | --- |
| L1 | 1.0 | 0.58 (0.50-0.66) | 7 | 9% | 2588 |
| L1 | 0.98 | 0.50 (0.42-0.58) | 6 | 51% | 3154 |
| L1 | 0.95 | 0.42 (0.26-0.50) | 5 | 75% | 3690 |
| L4 | any | 0.90 | 1 | 0% | ~115 |

### What the follow-ups settled

1. **Stopping: adopt confirmed-dry.** Cost is at or below dry-3 on L1/L4 and it halves the
   missed rate on L3 (18% -> 10%), where the misses are recoverable value-minimizations that
   fixed dry counts abandon on unlucky single runs. It also terminates with a certificate:
   every remaining smaller candidate was proven below threshold or hit the run cap. The
   residual ~9-10% missed on L1 is cap-30 resolution near the threshold, which no stopping
   rule can fix.
2. **Checkpointing: drop it from the shrink loop.** Both rollback rules lose. Rolling back on
   uncertainty (v2) rescues the mispinned incumbent but fires constantly on stable landscapes
   (L1: 2.3x cost, missed 9% -> 52% from poisoned good candidates). Rolling back on proof
   (v3) is free on stable landscapes but never fires on the hazard: the mispinned effective
   rate (0.135) sits at the 0.5 x anchor bar (anchor LCB ~0.27 for true 0.475), and proving
   UCB below it inside 40-80 fresh runs is marginal by construction — the quantities a
   rollback must separate are within Wilson noise of each other at affordable run counts.
   Post-hoc detection is the wrong tool.
3. **What actually kills the pinning hazard is capture-at-confirmation.** Pin-failing vs
   pin-random is the whole story: ~95% hi pins at source beats any amount of downstream
   statistics. Design consequences: (a) the pool must serve the timeline that produced the
   confirmed failure first (004 must instrument how often first-fit would serve a different
   one); (b) a *final validation* at report time can still detect the residual ~5% mispins
   and annotate the report (reporting concern, not search concern).
4. **Anchor decay: rejected.** It buys 1-2 length units on L1 at +20-40% cost, does nothing
   where the anchor isn't binding, and the 51-75% missed rates show the run stops by dry
   sweeps while the threshold is still falling — the stopping criterion becomes incoherent
   against a moving target. The monotone anchor stands.
5. If any post-accept validation survives elsewhere in the design: roll back only on proof of
   badness (UCB below bar), advance snapshots only on proof of goodness (LCB above bar), hold
   pending otherwise — and never feed pinned-regime evidence into the anchor.
