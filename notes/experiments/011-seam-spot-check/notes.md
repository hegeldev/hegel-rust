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
