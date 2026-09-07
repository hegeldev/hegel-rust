# Experiment 013: targeting under ND handling

Derives the constants for `optimise_targets_nd` (decision 68) and quantifies the
failure of the shipped deterministic climber on nondeterministic scores — the
as-built critique's statistics findings on `targeting.rs`, measured. A frozen
pure-simulation crate (`experiments/target-sim`, no engine dependency): candidates
are integer positions on [0, 100], a run of a candidate draws a score from the
landscape's distribution at that position, and both policies spend replays against
that oracle. Base seed 0x5eed2026, trial i seeded base+i, 200 trials per cell.

Landscapes: L-lin (Normal(x, 5)), L-flat (Normal(0, 5) everywhere — progress is
impossible, so it measures false adoption and reference drift), L-heavy (x + 5T,
T Student-t df 2), L-disc (Poisson(5 + x/10), heavy ties), L-gap (Normal(x, 5)
with a +30 jump at x >= 50). A miss_rate variant yields no observation on 30% of
runs.

Policies. OLD models the shipped climber applied to a noisy score: best-ever
single-run score kept as the standing maximum, doubling walks in each direction
accepting on a single strictly-better run, 600-run budget. NEW is the shipped
design: reference = upper-middle median of a 20-run batch, a pool of 16
perturbations successive-halved on mean observed score from 2 replays per round
doubling, the winner adopted only when a fresh HOLDOUT batch has a Wilson
LCB(beats/HOLDOUT) above 0.5 (a beat strictly exceeds the reference; ties and
unobserved runs count against), and on adoption the reference re-estimated on
another fresh batch, monotone max.

## The adoption gate (exact binomial, no simulation)

| k | min beats m_k | q=0.45 | q=0.50 | q=0.55 | q=0.60 | q=0.75 | q=0.90 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 10 | 9 | 0.0045 | 0.0107 | 0.0233 | 0.0464 | 0.2440 | 0.7361 |
| 20 | 15 | 0.0064 | 0.0207 | 0.0553 | 0.1256 | 0.6172 | 0.9887 |
| 30 | 21 | 0.0050 | 0.0214 | 0.0694 | 0.1763 | 0.8034 | 0.9995 |

## OLD: the curse, measured

| landscape | miss | x_final | true mean | curse bias | frozen | steps | runs used |
| --- | --- | --- | --- | --- | --- | --- | --- |
| L-lin | 0.0 | 18.2 | 18.2 | +8.4 | 0.92 | 2.8 | 10.9 |
| L-flat | 0.0 | 1.2 | 0.0 | +8.1 | 0.00 | 1.7 | 9.7 |
| L-heavy | 0.0 | 9.2 | 9.2 | +35.8 | 0.94 | 2.3 | 10.2 |
| L-disc | 0.0 | 0.6 | 5.1 | +3.0 | 1.00 | 1.4 | 9.2 |
| L-gap | 0.0 | 18.2 | 23.3 | +8.4 | 0.92 | 2.8 | 10.9 |
| L-lin | 0.3 | 6.0 | 6.0 | +6.4 | 0.97 | 2.2 | 10.3 |

The climb dies after ~10 of its 600 budgeted runs: one noisy draw inflates the
standing maximum ~1.7 sd above truth (+36 raw on heavy tails, against a score
scale of 5), after which no honest single run can beat it and the walk ends. Frozen-with-gradient-remaining is
92-100% everywhere gradient exists. This is the shrink loop's pre-008 pathology in
its purest form — a max of noisy draws treated as an estimate.

## NEW: the sweeps

HOLDOUT sweep (RACES 4, miss 0):

| landscape | H | x_final | progress | adopts/trial | ref drift | replays/trial | replays/adopt |
| --- | --- | --- | --- | --- | --- | --- | --- |
| L-lin | 10 | 100.0 | 100.0 | 2.06 | — | 790 | 384 |
| L-lin | 20 | 99.9 | 99.9 | 2.34 | — | 952 | 407 |
| L-lin | 30 | 99.9 | 99.9 | 2.38 | — | 1105 | 463 |
| L-flat | 10 | 4.0 | 0.0 | 0.32 | +1.53 | 779 | — |
| L-flat | 20 | 6.5 | 0.0 | 0.51 | +1.30 | 965 | — |
| L-flat | 30 | 8.6 | 0.0 | 0.53 | +1.14 | 1130 | — |
| L-disc | 10 | 80.2 | 8.0 | 1.50 | — | 1289 | 857 |
| L-disc | 20 | 98.1 | 9.8 | 2.00 | — | 1183 | 592 |
| L-disc | 30 | 98.6 | 9.9 | 2.00 | — | 1273 | 636 |

(L-heavy and L-gap track L-lin at every H; full tables in
`experiments/target-sim/results.txt`.)

RACES sweep (H 20, miss 0): 2, 4, and 8 races all reach the maximum on every
gradient landscape; 2 costs ~540 replays/trial, 4 ~950, 8 ~1780, and the L-flat
false-adopt count doubles from 0.33 to 0.67 per trial between 2 and 8.

miss_rate 0.3 (H 20, RACES 4): L-lin still reaches 99.5 at ~20% more replays, and
L-flat false adoption drops to zero with reference drift -0.11 — an unobserved run
counts against the beat quota, so missingness only tightens the gate. Zero dead
labels in any cell.

## What was chosen

- `TARGET_ND_HOLDOUT = 20` (= `ANCHOR_SEED_RUNS`). The 10-run gate needs 9/10
  beats and stalls on tie-heavy scores (L-disc progress 8.0 vs 9.8, x_final 80 vs
  98); 30 buys nothing over 20 anywhere for ~15% more replays.
- `TARGET_ND_RACES = 4`. Full progress everywhere with headroom over the
  2-race minimum; 8 doubles cost and flat-landscape false adoption for no
  progress.
- The sign-test denominator counts unobserved runs. Deliberate: under
  nondeterministic no-shows the gate tightens rather than loosens.
- Reference honesty: measured drift on L-flat is +1.3 (~0.26 sd) against the old
  policy's +8.1 curse. The residual comes from adoption conditioning (a false
  adopt precedes the fresh re-estimate); the fresh-batch rule keeps it bounded
  and the monotone max never compounds it.

False adoption on a flat landscape runs ~4% per race against the gate's 2.1%,
because the reference is itself an estimate. The cost of a false adopt is a
lateral move and one holdout batch — no bug is lost and no anchor moves — so the
composed rate was accepted as priced.
