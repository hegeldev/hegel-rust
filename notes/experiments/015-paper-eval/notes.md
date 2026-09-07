# Experiment 015: consolidated evaluation on the final engine (paper)

The paper (`notes/paper/`) needs one coherent evaluation measured on the
branch head rather than the historical patchwork (008's spot check at
c68a89eb, 011 at 532be034, 012 at 5d3aadc3, all before decisions 70-72).
This experiment re-runs the two standing in-engine protocols on HEAD and
adds the arms the paper's research questions need. Harness:
`/experiments/paper-eval` (frozen after use), built from
`/experiments/gauntlet-calibration` and `/experiments/detection-escape`
with the decision-70 `FlipSite` fix.

## Part A: landscape suite through the C ABI (`landscapes` subcommand)

011's cells (L1 rising, L3 constant, L4 noise-floor, L4b target-regime,
D2 det-core, D0 det-control) plus one new cell:

- **N0 noise-only**: p = 0.02 for every completed body, no real bug —
  measures the false-confirm rate (a shrunk, blob-carrying report on a
  bugless body) and the caveat-only reporting decision 3 mandates.

Two arms per cell: `quiet` (the shipped default) and `error`
(determinism-as-invariant: every detection aborts, the pre-branch
behaviour for non-concurrent tests). 100 seeds per cell, 500-case budget,
database off, seeds fixed in code. Columns as 011: outcome class
(shrunk / caveat-only / aborted / no-bug), bug kept, final-p and length
percentiles, executions, ND share.

Expectations: quiet matches 011's comparison half modulo decisions 70-72
(the pooled-review bar may reopen a small L4b caveat-only rate; the
backtrack is the rescue); error aborts most runs on every genuinely ND
cell, which is the paper's usefulness baseline.

## Part B: episode suite through the Rust frontend (`episodes` subcommand)

012's protocol (discovery run with database + blob, then a reuse-only
run, then a blob-reproduction run; flip sites from the `__bench` seam
dump) on HEAD, with 012's cells (clone/machine at p in {0.1, 0.3, 0.9},
det-control) plus:

- **clone p = 0.05**: below the decision-16 target — measures the
  per-run confirmation rate the decision-72 attempt cap prices. The
  40-epoch DP row says ~0.42; a 100-case discovery run grants more
  sighting epochs, so the interesting output is the confirmed versus
  caveat-only split, not near-certain confirmation.
- **pass**: the clone body shape with no failure — a passing suite must
  show zero flips and zero measurement replays.

200 episodes per cell (`PAPER_EVAL_EPISODES` overrides). Columns as 012
plus a confirmed/caveat-only split and the measurement-replay median.

Expectations: never-flip 0 everywhere ND, blobs all v2, reuse and blob
reproduction at or near 012's rates; the control pays exactly 4
measurement replays per episode; pass pays zero; the p = 0.9 cost
lottery (decision 67's escalated follow-up) is expected to still be
present.

## Results (commit 9f1eb7c6, Apple M5 Pro)

Raw outputs in `results-*.txt` alongside this file. Some episode files
have expected `reproduce_failure` panic output from blob-replay misses
interleaved on stderr; the data rows are the `|`-prefixed lines.

The first full run crashed in the clone p = 0.05 cell: a stale-index
panic in `lower_and_bump` (`index_passes.rs`) when a mid-loop
`consider()` adopts a realisation shorter than the node index being
walked — the same class of bug `try_shortening_via_increment` already
guarded against. Fixed red-green in 9f1eb7c6 and the whole suite rerun
on the fixed engine. The landscape numbers are identical to the pre-fix
partial run, so the crash path never fired there.

### Part A (100 runs/cell)

Quiet arm: shrunk report in 100/100 on every landscape with a bug except
L4b (98 shrunk + 2 caveat-only); bug kept 98-100/100; N0 confirmed
falsely in 5/100 (against the derived 4.6% per-origin ceiling) and
reported caveat-only in the other 95; no aborts anywhere. Error arm:
aborts in 96-100/100 on every genuinely ND cell (L1 4 survivors, all
degraded to p = 0.26; D2 20 survivors that hit the deterministic core
early); D0 byte-identical across arms. Final-p medians (quiet): L1 0.74,
L3 0.50, L4 0.90, L4b 0.10, D2 1.00.

### Part B (200 episodes/cell)

| body | p | confirmed | caveat | DB reuse | blob | replays p50 (max) |
| --- | --- | --- | --- | --- | --- | --- |
| clone | 0.05 | 74 | 126 | 64/74 | 65/74 | 109 (2.0M) |
| clone | 0.1 | 183 | 17 | 176/183 | 179/183 | 1.71M (6.4M) |
| clone | 0.3 | 197 | 3 | 197/197 | 197/197 | 0.77M (2.9M) |
| clone | 0.9 | 200 | 0 | 200/200 | 200/200 | 0.60M (1.5M) |
| machine | 0.1 | 179 | 21 | 175/179 | 178/179 | 1.73M (4.7M) |
| machine | 0.3 | 199 | 1 | 199/199 | 199/199 | 1.06M (3.0M) |
| machine | 0.9 | 200 | 0 | 200/200 | 200/200 | 0.76M (2.5M) |
| det-control | 1 | 200 | 0 | 200/200 | 200/200 | 4 (4) |
| pass | 0 | 0 | 0 | — | — | 0 (0) |

Every failing episode reported; every flip landed at the first-check;
never-flip 0; all ND entries v2 (det-control all v1). Expectations met
except one: the cost lottery peaks at the design floor, not at p = 0.9.
At p = 0.1 the gauntlet threshold sits at the floor and every candidate
on these bodies fails at exactly p, so no ledger can reject early
(UCB(0/30) = 0.11 > 0.05) and confirmation sweeps drive nearly every
distinct candidate to the 30-run cap: median 1.7M measurement replays
per episode against 0.6-0.8M at p = 0.9. The clone p = 0.05 cell
confirmed 37% of episodes (DP row said ~42% for a 40-epoch run), with
confirmed failures reproducing at 86-88%.
