# 008: gauntlet calibration under the shipped rules

Question (from `remediation-plan.md`, findings S1-S6 in `research/critique-asbuilt.md`):
(H1) does the shipped parameterization — bar-seeded anchors, recruit-counted evidence, no
minimum evidence, floor 0.05, flat gamma 0.8 — degenerate to single-run accepts across the
target regime and lose the 001/003 drift protection; (H2) do the remediation rules restore
the 003 numbers at acceptable cost, and at what constants.

**Recommended constants (PRELIMINARY — final values freeze after experiment 009a fixes the
miss-weighting column; see the weighting table at the end):**

- `GAUNTLET_MIN_FAILS = 4` (not the provisional 3 — m3 has no floor passing both floor
  criteria, and leaves 3% bug loss in the target regime)
- `GAUNTLET_FLOOR = 0.05`, now derived: 0.05 < LCB(4/30) = 0.0531, the min-fails
  acceptance boundary at the run cap, so the floor costs zero power at m = 4 (S4)
- `ANCHOR_SEED_RUNS = 20`, both seeding sites (bar extension and accept-time top-up)
- `RETENTION_HIGH_WATER = 0.8` (gamma 1.0 at or above; with 20-run seeding this fires
  exactly on zero-miss-at-20 evidence, LCB 0.839) — the L1 cost letter is missed by 6
  points, flagged for G6 below
- `BOOST_RELIABILITY_FLOOR = 0.30` in extended-20 LCB units (G7's derived value)
- Recruit stays counted; z stays 1.96; recorded operating points below are the
  `gauntlet_matches_the_008_operating_points` fixture inputs

## Harness

`/experiments/shrink-sim` extended (001's policies and tables unchanged; `cargo run
--release` still reproduces the 001 notes). New: an exact model of the shipped rules from
`hegel-c/src/native/nd/mod.rs` @ 9c800e8e — weighted `Evidence` (fails at 1.0, misses at
weight), Wilson z = 1.96 on weighted totals, discovery bar (gate 10 weighted misses / cap
40 physical / accept on the 4th fail), gauntlet accept LCB >= max(gamma·anchor, floor),
reject UCB-below or 30-run physical cap, verdict checked before any rerun, fast-mode
single-run rejects with the miss retained, per-candidate ledger for the whole shrink,
first-accept-per-timeline anchor raises, confirmed-dry stopping. Factors:

- anchor seeding {bar-batch, extended-20, extended-40} — extension applies to both the
  bar batch and the gauntlet accept's ledger top-up
- accept rule {shipped (m=1), min-fails 2/3/4, min-fails-3 + recruit-excluded}
- floor {0.035, 0.05, 0.08, 0.10} × gamma {flat 0.8, high-water 0.7, high-water 0.8}
- miss weight {1.0, 0.2, 0} — the constant a non-failing replay records, standing in for
  the watermark's value on flat / floored / W1-degenerate bodies (the 009a coupling)

Landscapes: 001's L1-L4 and L5 mixture (pin-failing), plus **L4b noise-floor-lo** (bug
p = 0.1, background 0.02 — the decision-16 target regime, where the floor and min-fails
bind), **D1** (006's deterministic core: atom >= 95 -> 1.0, else >= 3 atoms >= 10 -> 0.3;
starts conditioned on one core atom, since the sim's passes cannot discover the core from
below and boost is not modeled), and **D2** (D1 with the flaky region at 0.7 — S6's
displacement arithmetic; D1's 0.3 region never discriminates the gamma levels). Model
simplifications, as in 001/003: outcome-ND only, so the candidate keys the ledger exactly
as realized choices would; mixture candidate evidence draws at the marginal rate; the miss
weight is a constant, not a computed watermark.

Exact-DP module (`src/dp.rs`, the 005A method): P(accept) and E[physical runs] for one
fresh-ledger proposal at true rate q against anchor a, recruit rule and per-run looks
modeled, sharing the simulator's verdict function.

Seeds fixed in code: trial i uses start RNG `i·0xA5A5 ^ 0x5EED`, sim RNG `i ^ 0xF00D`
(001's scheme); the seeding study uses `(base+i)·0x9E37 ^ 0xBA5E`. Outputs are
byte-identical across reruns. N = 500 per headline cell, 200 per factorial cell, 10000 per
seeding cell. Every table below reproduces from `cargo run --release -- <cmd>` in
`/experiments/shrink-sim`; full suite ~40 s CPU (~4 s wall on 18 cores).

## Results

### H1: the shipped policy (`e008-h1`)

DP, P(accept | recruiting run fails), shipped rule:

| anchor | threshold | q=0.02 | q=0.1 | q=0.9 |
| --- | --- | --- | --- | --- |
| <= 0.258 | <= 0.2065 | 1.000 | 1.000 | 1.000 |
| 0.30 | 0.240 | 0.020 | 0.116 | 1.000 |
| 0.51 | 0.408 | 0.0004 | 0.010 | 1.000 |
| 0.839 | 0.671 | 0.000 | 0.000 | 0.922 |

Any anchor <= 0.258 accepts every candidate on its recruiting failure regardless of true
rate — the verdict-before-rerun check on the 1/1 ledger (LCB 0.2065). Bar-batch median
anchors sit inside that zone across the low-to-mid regime: p = 0.1 -> 0.061, 0.3 -> 0.138,
0.5 -> 0.250 (0.9 -> 0.510). Sim, shipped policy, N = 500:

| cell | final p med (p10-p90) | len | bug kept | seed anchor | execs med |
| --- | --- | --- | --- | --- | --- |
| L1 start len 20 (p .95) | 0.34 (0.26-0.42) | 4 | 100% | 0.510 | 1013 |
| L1 start len 6 (p .50) | 0.18 (0.10-0.26) | 2 | 100% | 0.250 | 423 |
| L4 noise-floor | 0.90 (0.90-0.90) | 1 | 100% | 0.510 | 96 |
| L4b noise-floor-lo | 0.10 (0.02-0.10) | 2 | **67%** | 0.061 | 546 |

H1 confirmed with one nuance. In the target regime the degeneracy is total: L4b loses the
bug in 33% of trials to p = 0.02 noise accepts (001's P0 lost 34% on L4), and low-p starts
descend to the P0 value (L1-len6 p10 = 0.10). At high-p starts the LCB(4/4) = 0.51 anchor
ceiling gives partial protection: L1-len20 lands at 0.34, between P0's 0.10 and P3's 0.58.
The plan's "L1 final-p median at the P0 value" holds where the seeded anchor is
degenerate, which is the p <= ~0.5 origin regime.

### Anchor seeding (`e008-seeding`)

Median seeded anchor by true p (w = 1.0), N = 10000 batches:

| p | bar | e20 | e40 | \|bar-e40\| | \|e20-e40\| |
| --- | --- | --- | --- | --- | --- |
| 0.1 | 0.061 | 0.061 | 0.055 | 0.007 | 0.007 |
| 0.3 | 0.138 | 0.145 | 0.181 | 0.043 | 0.035 |
| 0.9 | 0.510 | 0.699 | 0.769 | 0.259 | **0.071** |
| 1.0 | 0.510 | 0.839 | 0.912 | 0.402 | 0.074 |

At p = 0.3 both bar and e20 sit within 0.05 of the 40-run reference. At p = 0.9 nothing
below 40 does: the 0.071 gap is the Wilson interval's width shrinkage between n = 20 and
n = 40 at the same true rate, not seeding noise, so the criterion as written selects only
n = 40. But 40 is structurally excluded. LCB(40/40) = 0.912 > LCB(30/30) = 0.887, so under
gamma = 1 no candidate can match a deterministic incumbent within the 30-run cap, and the
e40 × high-water cells stall completely (D1: 0 accepts, final len 20, 6092 execs; L1
finals at len 12). The anchor's scale must stay inside the gauntlet's cap-reachable range:
**ANCHOR_SEED_RUNS = 20** (LCB(20/20) = 0.839, matchable at exactly 20 <= 30 runs), which
is also the mechanism 001/003 simulated. The p = 0.9 tolerance should be re-based to the
p <= 0.3 target regime, where 20 passes.

### Accept rule (`e008-m`)

s=e20 f=0.05 g=f0.8 w=1, N = 500. On L1/L3/L4/D1 the rules are near-identical — accepts
against informative anchors already carry >= 4 fails, so min-fails costs nothing there
(L1: identical rows, cost 1.00x; the plan's literal criteria pass at every m). The target
regime decides:

| L4b (bug p=0.1) | bug kept | final p med | noise accepts | execs med | vs sh |
| --- | --- | --- | --- | --- | --- |
| sh | 51% | 0.10 (.02-.10) | 0.69 | 941 | 1.00 |
| m2 | 73% | 0.10 (.02-.10) | 0.36 | 1286 | 1.37 |
| m3 | 97% | 0.10 (.10-.10) | 0.03 | 2446 | 2.60 |
| m4 | **100%** | 0.10 (.10-.10) | 0.00 | 2831 | 3.01 |
| m3x | 98% | 0.10 (.10-.10) | 0.02 | 3191 | 3.39 |

The sh row (e20-seeded, 51% kept) is worse than the bar-seeded shipped baseline (67%):
honest 20-run ledgers stop the anchor ratcheting to 0.2065 on 1/1 flukes, so extension
alone keeps the whole shrink in the degenerate regime. Seeding and min-fails only work
landed together, as the fix design's sequencing says.

**GAUNTLET_MIN_FAILS = 4.** By the plan's letter the smallest passing m is 2, because the
plan's criteria are all anchor-protected; by the composed floor criteria below, m3 has no
compliant floor and m4 does. Cost vs the 003-equivalent reference (e20 + shipped rule + flat, L1 execs med
3532): 1.00x on L1/L3/L4/D1 — the 1.5x budget is untouched by the accept rule. The 3x on
L4b is the retention price in a regime where the reference loses the bug half the time.

### Recruit disposition (`e008-recruit`)

DP at the floor: exclusion cuts fresh-proposal false accept 7x (0.059 -> 0.008 conditional
at q = 0.02) — over the plan's 2x trigger — but costs 37% of target-rate power
(0.66 -> 0.42) and is statistically wasteful: the same replay is paid for and its
information discarded. In composition it is dominated by m4 outright: worse L4b retention
(98% vs 100%) at higher cost (3191 vs 2831; L1 4927 vs 3532, +39%), because cross-retry
fail accumulation (accept is checked before the cap) erodes the DP advantage that only
holds for fresh ledgers. **Recruit stays counted** (decision 7's shape); the false-accept
target is met by m = 4 instead. The m-decision does not shift; the >2x DP shift is
recorded here for the phase-12 decision entry.

### Floor (`e008-floor`)

DP at an uninformative anchor (threshold = floor), w = 1.0. Per-shrink false accept =
1-(1-alpha_uncond)^K; measured K (median distinct bugless candidates per L4b shrink) is 13
at m4, 19 at m3.

| m | floor | alpha cond q=.02 | uncond | per-shrink K=13 / K=30 | power q=.1 | power/ceiling |
| --- | --- | --- | --- | --- | --- | --- |
| 3 | 0.035 | 0.107 | 0.0021 | 2.8% / 6.2% | 0.785 | 0.980 |
| 3 | 0.05 | 0.059 | 0.0012 | 1.5% / 3.5% | 0.664 | 0.829 |
| 3 | 0.10 | 0.014 | 0.0003 | 0.4% / 0.9% | 0.317 | 0.396 |
| 4 | 0.035 | 0.020 | 0.0004 | 0.5% / 1.2% | 0.565 | **1.000** |
| 4 | **0.05** | 0.020 | 0.0004 | 0.5% / 1.2% | 0.565 | **1.000** |
| 4 | 0.08 | 0.007 | 0.0001 | 0.2% / 0.4% | 0.349 | 0.618 |

m3 has no floor meeting both criteria (0.035 fails false accept, 0.05+ fails power). At
m4, floors up to 0.05 are free — four fails imply LCB >= LCB(4/30) = 0.0531 at any
n <= 30, so power equals the min-fails-only ceiling exactly — and 0.08 halves power.
**GAUNTLET_FLOOR = 0.05**: the largest value passing both, false accept 0.02 per entered
gauntlet (4.0e-4 per proposal), per-shrink 0.5% at the measured exposure. The sim
concurs: L4b at m4 holds 100% bug retention and 0.00 noise accepts at every floor. The
derivation resolves S4: the floor is the m = 4 acceptance boundary at the cap.

### Retention high-water (`e008-hw`)

r=m4 f=0.05 w=1, N = 500. D1 (flaky 0.3) retains 100% under all three gammas — its flaky
region is priced out by flat 0.8 at e20 anchors, so the 006 reference criterion (>= 27/30)
does not discriminate. D2 (flaky 0.7) is the S6 hazard:

| cell | det finals | final p (p10) | execs med (p90) |
| --- | --- | --- | --- |
| D2 flat 0.8 | 67% | 0.70 | 202 (2607) |
| D2 hw 0.7 | 100% | 1.00 | 181 (290) |
| D2 hw 0.8 | **100%** | 1.00 | 181 (290) |

Cost: L1 execs med 3532 (flat) -> 4445 (hw 0.8, **1.26x**) -> 4997 (hw 0.7, 1.41x); L4
med 201 -> 354 (1.76x, tail-driven). Both hw values miss the <= 20% L1 letter. The
increase concentrates in trials whose incumbent evidence is itself zero-miss at n = 20
(36% of L1 starts, 12% of L4 confirmations), where gamma = 1 refuses to trade reliability
down and L1's final p rises 0.58 -> 0.82 — decision 2's intended behavior rather than
overhead. With 20-run seeding, hw = 0.8 fires exactly on zero-miss-at-20 evidence
(LCB(20/20) = 0.839 is the only reachable anchor >= 0.8; 19/20 gives 0.764): it is the
indistinguishable-from-deterministic detector, and hw = 0.7 adds cost without adding
retention. **RETENTION_HIGH_WATER = 0.8**, with the cost-letter miss put to G6: accept
+26% on near-deterministic landscapes as intended protection, or take "off" and accept
33% displacement of deterministic incumbents by 0.7-rate candidates. L2's det fraction
(82-83% at all gammas, vs 96% shipped) is a search-power artifact, not retention. Its
core is the shortlex minimum, so the shipped policy's naive descent reaches it more
often; D1/D2 are the retention measures.

### Boost floor (`e008-seeding`)

Extended-20 anchor medians: p = 0.1 -> 0.061, 0.3 -> 0.145, **0.5 -> 0.299**, 0.9 ->
0.699, 1.0 -> 0.839. Trigger = anchor < floor, target class = true rate < 0.5. On the
specified population {0.1, 0.3, 0.9}: precision 1.000 at every floor <= 0.40, recall
0.795 / 0.943 / **0.991** / 1.000 at floors 0.15 / 0.25 / 0.30 / 0.45. The >= 90%
precision criterion alone admits up to 0.50; the value matching the trigger intent is the
boundary image — the median e20 anchor of a true-0.5 incumbent, 0.299 ~= LCB(10/20).
**BOOST_RELIABILITY_FLOOR = 0.30** (e20-LCB units): recall 0.991, precision 1.000 on the
spec population; boundary-adjacent sensitivity (population including 0.5/0.7) gives
precision 0.878 at 0.25 and 0.756 at 0.30, so values above 0.30 buy nothing and erode
robustness. Confirms G7's derived value. Decision 28's literal 0.5 over-triggers on
near-boundary incumbents: a true-0.7 incumbent triggers 59% of the time at 0.5, 5% at
0.30.

### Operating points, z disposition (`e008-dp`)

Chosen rule (e20 / m4 / 0.05 / hw 0.8), w = 1.0 — the S5 fixture rows:

| anchor | threshold | q | P(acc\|fail) | P(acc) uncond | E[runs\|fail] |
| --- | --- | --- | --- | --- | --- |
| 0.05 | 0.05 | 0.02 | 0.0198 | 4.0e-4 | 29.9 |
| 0.05 | 0.05 | 0.10 | 0.5650 | 5.7e-2 | 24.2 |
| 0.05 | 0.05 | 0.90 | 1.0000 | 0.90 | 4.3 |
| 0.30 | 0.24 | 0.02 | 1.6e-4 | 3.2e-6 | 22.5 |
| 0.30 | 0.24 | 0.10 | 0.0182 | 1.8e-3 | 27.8 |
| 0.30 | 0.24 | 0.90 | 1.0000 | 0.90 | 4.3 |
| 0.839 | 0.839 | 0.02 | 0.0000 | 0.0 | 3.1 |
| 0.839 | 0.839 | 0.10 | 0.0000 | 0.0 | 3.4 |
| 0.839 | 0.839 | 0.90 | 0.1216 | 0.11 | 28.3 |

Realized per-candidate false accept at the worst case (uninformative anchor) is 4.0e-4
per proposal, under the ~1e-3 design target. The peeking/stop-early anti-conservatism S5
names is contained by the evidence minimum. **z stays 1.96**, and these DP rows become
the specification (the discovery bar's treatment), pinned by
`gauntlet_matches_the_008_operating_points`.

### Drift envelope (`e008-envelope`)

Chosen rule, w = 1.0, N = 500 — the numbers design.md's goal can quote:

| landscape | final p p10/p50/p90 | len med | bug kept | det finals | execs med |
| --- | --- | --- | --- | --- | --- |
| L1 rising | 0.58 / 0.82 / 0.90 | 10 | 100% | — | 4445 |
| L2 det-core | 0.35 / 1.00 / 1.00 | 1 | 100% | 82% | 374 |
| L3 constant | 0.50 / 0.50 / 0.50 | 1 | 100% | — | 366 |
| L4 noise-floor | 0.90 / 0.90 / 0.90 | 1 | 100% | — | 354 |
| L4b noise-floor-lo | 0.10 / 0.10 / 0.10 | 3 | 100% | — | 2831 |
| L5 mixture (eff p) | 0.82 / 0.82 / 0.82 | 1 | 100% | — | 373 |
| D1 det-core 0.3 | 1.00 / 1.00 / 1.00 | 1 | 100% | 100% | 166 |
| D2 det-core 0.7 | 1.00 / 1.00 / 1.00 | 1 | 100% | 100% | 181 |

Weighting columns (same rule; 009a's measured weight distribution picks the column):

| | w=1.0 | w=0.2 | w=0 |
| --- | --- | --- | --- |
| L4b bug kept | 100% | 100% | **89%** |
| D2 det finals | 100% | 100% | **39%** |
| L1 final p med | 0.82 | 0.82 | 0.50 |
| L1 execs med | 4445 | 7100 | 4187 |

w = 0.2 preserves every headline number at +60% L1 cost. w = 0 breaks the target regime
and retention even under m = 4: with no miss dilution, ledger fail counts accumulate
across retries until any candidate reaches four recruit fails, and the accept check runs
before the cap check. The chosen constants are safe under 009a outcomes {shipped
watermark, floored 0.2}. If 009a leaves clone-body weights near zero, min-fails alone
does not hold and the escalation path in the plan (G9/G10, decision-14 territory) is live.

### Factorial and pruning (`e008-factorial`)

540 configs × 7 landscapes at N = 200 (the coarse pass; byte-stable, 2 s wall). Of the 180
w = 1 configs, 48 pass {L1 p50 >= 0.5, L4 >= 99%, L4b >= 99%, D1 det >= 90%}. Every
failure is L1 drift (all 60 bar-seeded configs) or L4b retention (below 99% in 36/36 sh
cells, 24/36 m2, 15/36 m3, 9/36 m3x, 0/36 m4). Pruned as dominated before the N = 500
headline runs: **bar-batch seeding** (fails L1 drift protection in every cell), **sh and
m2** (L4b retention: sh 52-68%, m2 74% at worst), **e40** (1.2-1.9x e20's cost, stalls
shrinking under high-water gamma, and passes nothing e20 fails), **m3x** (dominated by m4
on retention and cost everywhere). The headline tables above cover the
surviving slice plus pruned levels where the contrast is the point.

## What we learned

1. **S1/S2 confirmed and bounded.** The shipped gauntlet is P0 wherever the bar-seeded
   anchor lands at or below 0.258, which is the entire p <= ~0.5 origin regime, and
   partially protected above it by the LCB(4/4) = 0.51 ceiling. The failure lands exactly
   at the decision-16 target: 33% bug loss on L4b, matching 001's P0.
2. **Min-fails is free where it doesn't bind and decisive where it does.** Accepts against
   informative anchors already carry >= 4 fails, so m = 4 costs 1.00x on 003's landscapes;
   all of its cost (3x) and all of its value (51% -> 100% retention) is in the target
   regime.
3. **The floor and min-fails are one mechanism**: 0.05 < LCB(4/30) makes the floor the
   m = 4 acceptance boundary at the cap, so it costs no power and its derivation is on
   record (S4).
4. **Anchor batches and the gauntlet cap must share a scale.** Seeding beyond the cap's
   reachable LCB range (40 runs vs cap 30) makes deterministic incumbents unmatchable and
   stalls shrinking entirely; 20 is both 001/003's simulated mechanism and the largest
   cap-compatible choice.
5. **The high-water gamma is a zero-miss detector, not a tuning dial.** With e20 seeding
   only 0.839 can clear 0.8, so hw = 0.8 grants gamma = 1 exactly to incumbents
   indistinguishable from deterministic; it converts D2's 33% displacement to 0% for +26%
   on L1 (spent raising L1 finals from 0.58 to 0.82).
6. **Recruit exclusion is the wrong tool**: its fresh-ledger DP advantage (7x) does not
   survive ledger retention, and m4 dominates it on every composed measure.
7. **Min-fails does not survive weight-zero evidence** (the 009a coupling): at w = 0 the
   fail-only ledger plus accept-before-cap re-opens a slow noise-acceptance channel that
   min-fails cannot close (L4b 89%, D2 39%).

## Caveats

Same model class as 001/003: outcome-ND only, no structural divergence (the weight column
is a constant stand-in for the watermark), no boost phase, single origin, mixture
candidate evidence at the marginal rate, and D1/D2 starts conditioned on a core atom. The
1.5x cost comparisons are against this harness's shipped-rule cell (e20 seeding,
confirmed-dry), not 003's in-engine totals, which include generation. L4's high-water
median cost (1.76x) exceeds the L1-specific letter and rides the same G6 trade as the
+26%. The in-engine spot check (`experiments/gauntlet-calibration` re-running the 003
cells on the fixed engine) is phase 12's, after the rules land.
