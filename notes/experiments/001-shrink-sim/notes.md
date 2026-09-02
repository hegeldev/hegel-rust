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

## Results

(pending)
