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
