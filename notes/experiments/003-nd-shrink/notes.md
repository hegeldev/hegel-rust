# 003: flat-timeline shrink in-engine

Question (from `000-plan.md`): do gauntlet + ledger tame drift on real synthetic flaky
tests — final true p, size, executions — with the statistics running in the actual shrinker
rather than the 001 simulation?

## Setup

Engine scaffolding (all `pub(crate)`, gated on `Settings::nd_experiment`, default `Off`):

- **`Resample`**: `serve_replays = false` (002's seam), no tree conclusion recording (so
  outcome flips can't trip the mismatch abort), novel-prefix generation off, pre-shrink
  verify replaced by a 20-run confirmation batch (skip the origin if it never reproduces),
  discovery confirmation (below). Accepts stay single-run — 001's P0 transplanted.
- **`Gauntlet`**: `Resample` plus 001's P3 in `EngineShrinkProbe`: a matching first run
  starts a sequential test on the realized choice sequence — accept at Wilson LCB >=
  max(0.8 x anchor, 0.05), reject at UCB below it or a 30-run cap. The ledger keys on
  serialized realized choices and lives for the whole shrink of one origin, so pass
  repetitions accumulate power on retried rejects. The anchor seeds from the pre-shrink
  confirmation batch and rises to accepted candidates' LCBs.
- **`Baseline`**: today's engine untouched.

Harness (`/experiments/nd-shrink`, driving `explore` through `__bench`): body draws
n in 0..=20 then n atoms in 0..=100 and fails via a hidden per-trial PRNG with probability
p(atoms) — 001's landscapes on real choice sequences (bug atom = value >= 10; L1 rising
needs >= 3 bug atoms so there is room to shrink; L3 constant 0.5; L4 noise-floor 0.9 bug /
0.02 background). 100 seeds x 500-test-case budget per cell.

## Two leaks found on the way (both predicted by `design.md`, now demonstrated live)

1. **Raw `update_interesting` displacement.** With `report_multiple_failures` (the default)
   generation keeps running after the first bug (`last_bug_at * 2` window, self-extending
   while interesting cases keep arriving — on these bodies, to the whole budget), and every
   raw interesting execution with a smaller sort key displaces the origin's entry —
   including p = 0.02 empty-case noise flukes. In the first run of this experiment the
   confirmed discovery (20/20 confirmation fails) was displaced by noise before shrinking
   started in ~80% of L4 trials; the gauntlet then anchored on the fluke and shrunk garbage.
   Fix (design-mandated "gate all acceptance paths on validated accepts"): in ND mode a raw
   interesting run may fill a vacant origin, never displace an occupied one.
2. **Discovery-time confirmation is load-bearing.** First-interesting on L4 is a noise fluke
   roughly 2:1 over genuine bugs (noise fires at 0.02 across the many no-bug cases; bugs at
   0.9 on the rarer bug cases). Today's engine *accidentally* filters those: the pre-shrink
   verify replay fails to reproduce and aborts the run — an underappreciated function of the
   Flaky abort that ND mode removes and must replace. The scaffold now confirms a newly
   discovered origin with a 20-run batch and requires >= 2 failures; an unconfirmed origin
   is removed so generation keeps hunting. P(pass | p = 0.02) ~ 5% — the one L4 gauntlet
   trial that still lost the bug is that false-accept rate showing up on schedule. 005
   should derive the discovery bar from the noise-floor caution rather than a flat 2.

## Results

| landscape | mode | shrunk | aborted | bug kept | len med | final p med (p10-p90) | execs med (p90) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| L1 | Baseline | 31 | 69 | 31/31 | 3 | 0.26 (0.26-0.26) | 1498 (1959) |
| L1 | Resample | 100 | 0 | 100/100 | 3 | 0.26 (0.26-0.26) | 3462 (6728) |
| L1 | Gauntlet | 100 | 0 | 100/100 | 6 | 0.50 (0.26-0.58) | 13556 (31199) |
| L3 | Baseline | 44 | 56 | 44/44 | 1 | 0.50 (0.50-0.50) | 1016 (1051) |
| L3 | Resample | 100 | 0 | 100/100 | 1 | 0.50 (0.50-0.50) | 1529 (1834) |
| L3 | Gauntlet | 100 | 0 | 100/100 | 1 | 0.50 (0.50-0.50) | 1804 (2564) |
| L4 | Baseline | 71 | 29 | 69/71 | 1 | 0.90 (0.90-0.90) | 1033 (1055) |
| L4 | Resample | 100 | 0 | 3/100 | 0 | 0.02 (0.02-0.02) | 1322 (1820) |
| L4 | Gauntlet | 100 | 0 | 99/100 | 1 | 0.90 (0.90-0.90) | 1894 (2752) |

(Execs include ~1000 generation calls per trial — the report-multiple window runs the
budget out on these bodies — so shrink-only ratios are larger than the totals suggest.)

## What we learned

1. **The 001 statistics transfer to the real shrinker.** Gauntlet completes 100% of runs on
   every landscape (baseline aborts 29-69%), never lowers the failure rate (L1 median holds
   at 0.50 vs resample's teleport to 0.26; L4 holds 0.90), and keeps the bug where naive
   resampling loses it 97% of the time. This is the design's core claim validated in-engine.
2. **Naive resampling is not a viable intermediate mode.** It fixes the aborts and then
   destroys the result quality (L4: 3/100 bugs kept, empty counterexamples at p = 0.02). If
   ND mode ships anything, it ships the gauntlet.
3. **Cost concentrates exactly where the sim said**: proving near-threshold rejects on L1
   (13.6k median execs, 31k p90 — the g-rej cost of holding "never lower p" against a
   landscape that pays for every size reduction). L3/L4 pay +18-43% over resample. The
   shrinker's adaptive passes (`BinSearchDown`'s probe-0-first teleports) are defused by the
   same mechanism as in the sim.
4. **Ordering effects the sim couldn't see**: discovery and the post-discovery generation
   window are part of the statistical surface. Confirmation at discovery (with capture) and
   validated-only adoption into the interesting map are prerequisites for the shrink
   statistics to matter at all — they are now demonstrated, not just argued.

Caveats: flat bodies only (no spans, clones, or misalignment — that's 004's territory);
outcome ND only (the hidden PRNG changes failure, never the choice structure); single
origin; gamma fixed at 0.8; anchor never re-seeded during shrink from incumbent re-runs
(the confirmed-dry stopping and pass-repetition budgets from 001 are approximated by the
shrinker's existing fixpoint loop, not reimplemented).
