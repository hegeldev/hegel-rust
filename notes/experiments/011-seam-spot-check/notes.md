# Experiment 011: instrumented seam spot check

The seam plan's acceptance experiment (`notes/seam-plan.md`). The frozen
`experiments/gauntlet-calibration` crate, amended 2026-09-04 with a sequential `seam`
subcommand: the engine's `__bench` seam dump (`nd::seam_dump`, the `watermark_dump`
pattern) records every flip's detection site, call index, and interesting map, and every
reject-eviction's evicted incumbent; the harness maps evicted and incumbent values back
to landscape p and decomposes caveat-only outcomes into correct fluke rejections versus
power misses. A D0 deterministic-control cell (core atom fails at p = 1, everything else
passes) and blob-kind counts are added. The 008 `spot`/`one` subcommands are unchanged
and reproduce the 008 output. Baseline half runs in phase 14 on the pre-change engine;
comparison half runs in phase 17 against the acceptance letters in the plan.

## Baseline (2026-09-04, engine at ad3ff0c1 — the dump commit, before any seam-plan fix)

100 seeds per cell, sequential, 500 test-case budget. The spot columns reproduce the 008
table (L1 0.26/0.34/0.82 at 11.4k execs, L4b 49% caveat-only, L4 15%, D2 100/100), so
the amendment did not perturb the engine.

| cell | shrunk | caveat-only | bug kept | final p p10/p50/p90 | execs med (p90) | nd |
| --- | --- | --- | --- | --- | --- | --- |
| L1 rising | 96 | 4 | 96/96 | 0.26 / 0.34 / 0.82 | 11426 (52267) | 91 |
| L3 constant | 100 | 0 | 99/100 | 0.50 / 0.50 / 0.50 | 1680 (2004) | 78 |
| L4 noise-floor | 85 | 15 | 83/85 | 0.90 / 0.90 / 0.90 | 1036 (1508) | 19 |
| L4b noise-floor-lo | 51 | 49 | 50/51 | 0.10 / 0.10 / 0.10 | 2487 (12529) | 51 |
| D2 det-core 0.7 | 100 | 0 | 100/100 | 1.00 / 1.00 / 1.00 | 1047 (1067) | 0 |
| D0 det-control | 100 | 0 | 100/100 | 1.00 / 1.00 / 1.00 | 528 (764) | 0 |

Seam columns (aborted and no-bug were 0 everywhere; D2 and D0 recorded zero flips, zero
evicts, and 100 v1 blobs each):

| cell | flips (sites) | flip calls p10/p50/p90 | incumbent-p at flip p10/p50/p90 | evicts (fluke/target) | caveat-only: fluke-reject / power-miss | blobs v1/v2 |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | 95/100 (verify 69, final 26) | 951/1004/1586 | 0.26/0.26/0.26 | 4 (0/4) | 0 / 4 | 5/91 |
| L3 | 78/100 (verify 56, final 22) | 954/989/1016 | 0.50/0.50/0.50 | 0 | 0 / 0 | 22/78 |
| L4 | 34/100 (verify 29, final 5) | 1003/1003/1010 | 0.02/0.90/0.90 | 15 (15/0) | 15 / 0 | 66/19 |
| L4b | 100/100 (verify 91, final 9) | 13/546/566 | 0.10/0.10/0.10 | 49 (6/43) | 6 / 43 | 0/51 |

## Reading

- **The 49% decomposed** (the gap the seam analysis named): L4b's caveat-only is 6
  correct fluke rejections and 43 power misses — the bar correctly targeting genuine
  p = 0.1 incumbents and rejecting them at its 45%-per-attempt power with no recycle.
  Mechanism 2 is 88% of the loss; the backtrack's many-attempts design aims at exactly
  this. L4's 15% is the opposite: 15/15 correct fluke rejections — the bar working as
  designed on p = 0.02 noise, not seam loss.
- **Mechanism 1 quantified**: L1's incumbent-p at flip is 0.26 at every percentile — by
  the time any detector fires, displacement has already walked the incumbent to the
  minimal-bug floor. The post-flip machinery then holds or slightly improves it (final
  median 0.34). Everything the workflow must recover exists only pre-flip, which is the
  history's case.
- **Flip sites**: the tree's kind-mismatch channel fired zero times across 600 trials —
  on outcome-only ND bodies every flip comes from the shrink verify (majority) or the
  final replay. Consistent with the G20 analysis: for verdict-only nondeterminism the
  tree contributes no detection.
- **Never-flipped runs emit v1 blobs**: L4's 66 deterministic finishes at p = 0.9 (the
  never-flip corner, 009a's 11.5% writ large on this landscape) all persist v1
  exact-choice state; L3's 22 and L1's 5 likewise. The first-interesting check and the
  v1 continuation fix both aim here.
- **D0 control arithmetic**: 528 execs median for a deterministic single-origin run —
  the phase-17 D0 letter (bug-free arithmetic + k x origins + final replay) prices
  against this.

## Comparison (2026-09-04, engine at 532be034 + the phase-16 coverage/docs commit)

Harness amendment, same date: the `seam` subcommand consumes the phase-16
`SeamEvent::Backtrack` (restored-vs-history-best p percentiles and history bytes per
cell) and adds a D2-only core-transition line (core at flip vs core in the shrunk
incumbent). The 008 `spot`/`one` subcommands remain untouched.

100 seeds per cell, sequential, 500 test-case budget:

| cell | shrunk | caveat-only | bug kept | final p p10/p50/p90 | execs med (p90) | nd |
| --- | --- | --- | --- | --- | --- | --- |
| L1 rising | 100 | 0 | 100/100 | 0.26 / 0.74 / 0.82 | 17583 (47296) | 96 |
| L3 constant | 100 | 0 | 100/100 | 0.50 / 0.50 / 0.50 | 2619 (6114) | 100 |
| L4 noise-floor | 100 | 0 | 100/100 | 0.90 / 0.90 / 0.90 | 1774 (11223) | 100 |
| L4b noise-floor-lo | 100 | 0 | 100/100 | 0.10 / 0.10 / 0.10 | 4382 (27923) | 100 |
| D2 det-core 0.7 | 100 | 0 | 99/100 | 0.70 / 1.00 / 1.00 | 1712 (7084) | 80 |
| D0 det-control | 100 | 0 | 100/100 | 1.00 / 1.00 / 1.00 | 708 (1366) | 0 |

Seam columns (aborted, no-bug, and evicts were 0 everywhere; caveat-only 0 everywhere):

| cell | flips (sites) | flip calls p10/p50/p90 | incumbent-p at flip p10/p50/p90 | backtracks: restored-p / best-p (p50) | blobs v1/v2 |
| --- | --- | --- | --- | --- | --- |
| L1 | 96/100 (cache 72, first-check 24) | 3/136/843 | 0.26/0.26/0.82 | 16: 0.34 / 0.26 | 4/96 |
| L3 | 100/100 (first-check 94, cache 6) | 2/3/6 | 0.50/0.50/0.50 | 0 | 0/100 |
| L4 | 100/100 (cache 70, first-check 30) | 2/70/349 | 0.02/0.90/0.90 | 2: 0.90 / 0.02 | 0/100 |
| L4b | 100/100 (first-check 99, cache 1) | 2/11/29 | 0.10/0.10/0.10 | 0 | 0/100 |
| D2 | 80/100 (cache 48, first-check 32) | 2/190/662 | 0.70/0.70/1.00 | 3: 0.70 / 0.70 | 20/80 |
| D0 | 0/100 | — | — | 0 | 100/0 |

D2 core transitions (flip incumbent → shrunk incumbent): kept 37, gained 13, lost 0,
never 30; plus 20 never-flipped deterministic finishes, giving the 68/100 det finals.

### Against the acceptance letters

- **L3, L4, L4b, D0, all-cells: pass.** L4b's caveat-only falls 49% → 0 (the whole
  power-miss channel is gone: the seeded bar plus backtrack recycling leave no
  reject without recourse); L4's 15% → 0 (fluke incumbents flip at the check before
  the bar ever sees them at shrink time); L3 exact; D0 records zero flips, zero
  caveats, and the exec arithmetic below. Aborted and no-bug are 0 everywhere, so the
  duplicate stop does not interfere.
- **L1: letters pass except execs — escalate.** Final-p median 0.74 (letter >= 0.50,
  baseline 0.34), p10 0.26, caveat-only 0 (letter <= 4). Execs median 17583 vs the
  <= 1.5x-baseline letter of 17139 — 1.54x, a 2.6% overshoot (p90 improved: 47296 vs
  52267). The early flip (median call 136 vs 1004) is the mechanism on both sides of
  the trade: displacement stops walking the incumbent down (median final p doubles),
  and the whole shrink runs gauntleted (the exec cost).
- **D2: det finals 68/100 vs the 100/100 letter — escalate.** The decomposition: 30 of
  the 80 flipped trials flip (median call 190) before any core-bearing sighting has
  displaced the incumbent, and after the flip decision 20 freezes displacement while
  the shrinker's value-lowering lattice cannot reach the core shape from a 3-bug-atom
  incumbent; 13 flipped trials do reach it through the gauntlet, and none that held
  the core lost it. The baseline's 100/100 came from a run that never detected the
  0.7 nondeterminism at all (0/100 flips) and displaced freely. The new engine
  reports a confirmed p = 0.7 example with a v2 blob instead of the prettier
  deterministic core in those 30 runs — priced honesty, but a letter miss as written.
- **Model validation**: flipped-at-first-check shares — L3 94/100 against the
  predicted ~94%, L4b 99/100 (~100%), L4 30/100 (~34%). L1 and D2 sit below their
  1 - p^4 numbers (24 and 32) because the cache-mismatch channel preempts the check
  during generation (72 and 48 preemptions) — the combined detected share matches the
  model (D2: 80/100 against 76% + never-flip noise).
- **D0 exec arithmetic**: 708 median vs 528 baseline, with 0 flips and 0 caveats
  exact. The ND machinery's share is k = 4 check replays on the one origin plus the
  final replay; the rest of the +180 is the phase-15 generation trade (exact repeats
  re-execute inside the generation window where the tree served them), which the
  phase-15 passing-body parity guard scopes to failing bodies like this one. No
  post-flip machinery ran (there was no flip), so the letter's identity holds.
