# 009b: composed-rules re-verification

Question (from `remediation-plan.md`, phase 12): do 009a's operating points hold once the
composed rules — `GAUNTLET_MIN_FAILS = 4`, `ANCHOR_SEED_RUNS = 20` at both seeding sites,
the derived `GAUNTLET_FLOOR = 0.05`, gamma 1.0 at anchors ≥ 0.8 — replace the shipped
arithmetic 009a replayed: fluke rejection cost, the confirmed-anchor escalation signal,
gauntlet fluke pricing (which under m = 4 has low-anchor rejects to price at all), the
false-accept rate against 008's DP expectation, and the G10 reproduction rates on the
composed engine. The in-engine half of 009b is the spot check already appended to the 008
notes (`experiments/gauntlet-calibration`). This half recomputes the offline numbers.

## Harness

`/experiments/watermark` extended with a `composed` subcommand — no new harness, and the
009a code path is untouched. The subcommand re-runs the 009a episode protocol unchanged
(same bodies, cells, seeds, 200 episodes per cell) against the engine at this commit,
then replays the composed bar/gauntlet arithmetic (mirrored from `nd/mod.rs` at this
commit) over the re-measured miss weights, 10000 streams per number: the bar batch
extends past its accept to 20 physical runs before the anchor is read, a gauntlet accept
requires four failures and tops its ledger up to 20 runs, threshold
max(gamma·anchor, 0.05) with gamma 1.0 at anchors ≥ 0.8. New alongside the rate-0 fluke
pricing: a false-accept replay at q = 0.02, the rate 008's DP rows price.

The episode re-collection is itself part of the experiment: phase 12 (`c68a89eb`) landed
after 009a's measurements (phase 11, per the plan) but before its harness commit, so the
009a tables reflect the pre-phase-12 engine and the tables below reflect the composed
one. The 009a default subcommand therefore reproduces its notes only on the tree it was
measured on.

Reproduce: `cargo run --release -- composed` in `/experiments/watermark` (tables on
stdout, progress on stderr, ~2 h single-threaded). Stdout is byte-identical across
reruns, verified by diffing two independent full runs.

## Results

009a's number in parentheses where it differs; "(-)" where 009a had no value.

### Episode accounting

| body | p | reported | with blob | miss samples | fail samples | replays vs 009a |
| --- | --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 200/200 | 118 | 117451239 | 12977844 | 1.68x |
| clone | 0.3 | 200/200 | 200 | 99056536 | 42047641 | 1.99x |
| clone | 0.9 | 200/200 | 200 | 6663644 | 56425074 | 4.01x |
| machine | 0.1 | 200/200 | 102 | 125844026 | 13798608 | 1.56x |
| machine | 0.3 | 200/200 | 200 | 133038571 | 55906166 | 2.08x |
| machine | 0.9 | 200/200 | 200 | 10193047 | 86929694 | 6.03x |

Reported and with-blob counts match 009a exactly: both are decided at or before the first
bar accept, and nothing phase 12 changed runs before one (the extension starts at an
accept, the gauntlet and boost after confirmation). The cost shows up past that point:
1.6-2.1x total measurement replays at p ≤ 0.3 and 4.0-6.0x at p = 0.9, fail-heavy because
top-ups against near-deterministic evidence mostly fail — 008's prediction that the cost
concentrates there, on the real engine.

### Miss-weight distribution

| body | p | W50 | mean | p10 | p90 | share 0 | share 1 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 0.400 | 0.476 (0.490) | 0.200 | 1.000 | 0.000 | 0.114 |
| clone | 0.3 | 0.400 (0.444) | 0.492 (0.526) | 0.200 (0.222) | 1.000 | 0.000 | 0.125 |
| clone | 0.9 | 0.400 (0.429) | 0.489 (0.519) | 0.200 (0.250) | 1.000 | 0.000 | 0.124 |
| machine | 0.1 | 0.263 (0.278) | 0.368 (0.377) | 0.150 | 0.778 | 0.000 | 0.048 |
| machine | 0.3 | 0.278 (0.294) | 0.375 (0.389) | 0.150 | 0.778 (0.800) | 0.000 | 0.051 |
| machine | 0.9 | 0.263 (0.333) | 0.370 (0.372) | 0.150 | 0.778 (0.650) | 0.000 | 0.049 |

The G9 picture is unchanged: zero mass at weight 0 everywhere, clone W50 at 0.400,
machine at 0.263-0.278. The composed engine's changed replay mix pulls the means down
0.01-0.04, but the distribution still sits strictly between 008's w = 0.2 and w = 1.0
columns, so decision 57's freeze stands.

### Discovery bar with the extended anchor batch

| body | p | fluke reject med/mean | anchor med | accept share |
| --- | --- | --- | --- | --- |
| clone | 0.1 | 22 / 21.6 (21 / 21.1) | 0.108 (0.106) | 0.56 |
| clone | 0.3 | 21 / 21.0 (20 / 19.7) | 0.231 (0.205) | 1.00 |
| clone | 0.9 | 21 / 21.1 (20 / 19.9) | 0.764 (0.510) | 1.00 |
| machine | 0.1 | 28 / 27.9 (27 / 27.2) | 0.131 (0.130) | 0.57 |
| machine | 0.3 | 27 / 27.4 (26 / 26.4) | 0.269 (0.241) | 1.00 |
| machine | 0.9 | 28 / 27.8 (27 / 27.5) | 0.779 (0.510) | 1.00 |

**Escalation-signal recheck** (target ≤ 0.2 at true p = 0.1): 0.108 clone, 0.131 machine
— pass, essentially 009a's values, because a p = 0.1 accept usually carries more than 20
physical runs already and the extension rarely fires. Decision 57's close survives the
composed rules.

**Fluke rejection cost** (target ≤ 20): clone 21-22, machine 27-28 — both now miss the
letter, clone having sat at 20-21 in 009a. The cost is still 10/mean-weight up to the 37
cap, so the slide is just the lower means (10/0.476 = 21.0) — the body-property
arithmetic decision 57 accepted for the machine cells.

**Extended anchors.** At p = 0.3 the extension raises median anchors modestly (0.205 →
0.231, 0.241 → 0.269); at p = 0.9 it replaces the stopping-rule-pinned 0.510 with
0.764/0.779 — below the 0.8 high-water, so genuinely racy p = 0.9 bodies keep gamma 0.8
and only zero-miss evidence (LCB 0.839) crosses into gamma 1.0, as decision 55 intends.
Accept shares are unchanged (extension happens only after an accept).

### Shrink gauntlet, fluke candidate at the cell's median confirmed anchor

Rate-0 fluke for the pricing columns; q = 0.02 fluke for the false-accept column,
conditional on its recruiting failure, times 0.02 for the per-proposal rate. The 009a
comparison at p = 0.9 spans both the rule change and the higher extended anchor (009a
priced against 0.510).

| body | p | anchor | threshold | reject med | proof share | accept share | false accept per proposal |
| --- | --- | --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 0.108 | 0.087 | 30 (-) | 0.00 (-) | 0.00 (1.00) | 3.7e-4 |
| clone | 0.3 | 0.231 | 0.185 | 30 (-) | 0.00 (-) | 0.00 (1.00) | 4.0e-5 |
| clone | 0.9 | 0.764 | 0.611 | 10 (19) | 1.00 (1.00) | 0.00 (0.00) | 0.0 |
| machine | 0.1 | 0.131 | 0.105 | 30 (-) | 0.00 (-) | 0.00 (1.00) | 4.0e-4 |
| machine | 0.3 | 0.269 | 0.215 | 30 (-) | 0.00 (-) | 0.00 (1.00) | 4.8e-5 |
| machine | 0.9 | 0.779 | 0.623 | 13 (25) | 1.00 (0.96) | 0.00 (0.00) | 0.0 |

**Where a threshold exists to prove against, the targets pass**: at the p = 0.9 anchors,
rejects are 100% proof-rejects at median 10-13 (target ≤ 15 with > 50% proof share),
improved from 009a's 19-25 because the extended anchor raises the threshold.

**The low-anchor rejects m = 4 creates ride the cap**: median 30, proof share 0, in every
p ≤ 0.3 cell. That is arithmetic rather than a defect: under the measured weights a
single-fail ledger's UCB after 29 weighted misses bottoms out near 0.30, above every
threshold reachable from anchors below ~0.38, so no proof-reject exists in exactly the
regime where 008's DP already priced E[runs|fail] at 29.9. The ≤ 15 target was written
against the old rule's informative-anchor rejects and cannot be met at low anchors by a
rule that demands evidence before rejecting. What the 30 replays buy, against 009a's
accept share of 1.00 in the same cells, is that the fluke is not accepted.

**False accept** (008 DP expectation 4.0e-4 per proposal at the floor): 3.7e-4 clone and
4.0e-4 machine at the p = 0.1 anchors (thresholds 0.087/0.105, just above the floor),
4.0-4.8e-5 at p = 0.3, zero at p = 0.9. At or under the DP row everywhere; the measured
weights do not reopen the S1 channel.

### G10 recheck: reproduction of persisted state on the composed engine

| body | p | DB reuse | blob replay |
| --- | --- | --- | --- |
| clone | 0.1 | 118/118 | 118/118 |
| clone | 0.3 | 197/200 (98.5%) | 196/200 (98%) |
| clone | 0.9 | 198/200 (99%) | 180/200 (90%) |
| machine | 0.1 | 101/102 (99%) | 101/102 (99%) |
| machine | 0.3 | 199/200 (99.5%) | 199/200 (99.5%) |
| machine | 0.9 | 198/200 (99%) | 191/200 (95.5%) |

Every rate at p ∈ {0.1, 0.3} is ≥ 98% — the pass line holds and decision 58 stands on the
composed engine. The counts match 009a cell for cell because the misses live in episodes
that never flipped into ND handling (009a's instrumented finding), and a never-flipped
episode executes no phase-12 code, so it is bit-identical across both engines. The
p = 0.9 v1-blob dip (180/200) persists exactly as gate G20 records.

## What we learned

1. **The composed rules keep every correctness property 009a measured, at known cost.**
   Escalation signal quiet (0.108/0.131 vs ≤ 0.2), false accept at or under the DP's
   4.0e-4 per proposal, zero weight-0 mass, G10 ≥ 98% at p ≤ 0.3. Phase 12's constants
   hold against measured weights.
2. **Low-anchor fluke rejection costs the full cap.** m = 4 converts 009a's
   accept-on-recruit (share 1.00) into 30-replay cap-rejects with no proof share:
   weighted misses keep a 1-fail ledger's UCB near 0.30 at the cap, so thresholds below
   that are unprovable. The plan's ≤ 15 reject target passes only at informative anchors
   (10-13 median, 100% proof at p = 0.9) and cannot be met at low anchors by any rule
   that demands evidence before rejecting.
3. **The extension is free where it matters and effective where it binds.** p = 0.1
   accepts are usually past 20 runs already (anchor median unchanged); p = 0.9 anchors
   move 0.510 → 0.764/0.779, buying the sharper gauntlet pricing, while staying under
   the 0.8 high-water on genuinely racy bodies.
4. **In-engine cost of phase 12: 1.6-2.1x measurement replays at p ≤ 0.3, 4-6x at
   p = 0.9.** The increase is fail-heavy top-up work against reliable evidence,
   matching 008's prediction that the cost concentrates where evidence is
   near-deterministic. The two cost letters now formally missed — clone fluke rejection
   21-22 and machine 27-28 against ≤ 20 — are the same 10/mean-weight arithmetic
   decision 57 already accepted.
5. **009a's episode-level results were engine-version-robust.** Everything decided
   before the first bar accept (confirmation counts, never-flip episodes and hence the
   reuse/blob misses) reproduced exactly under the composed engine; only the
   post-confirmation replay mix changed. The weight distribution's small downshift is
   that mix.

## Caveats

009a's caveats carry over: seeded hidden schedules rather than real threads, failure
independent of drawn values, i.i.d. resampling from pooled per-cell weights, the
fresh-ledger fluke framing. New here: the p = 0.9 gauntlet comparison conflates the rule
change with the anchor change; the false-accept shares are measured at 10000 streams
(±0.003 conditional at 2σ), so "at the DP row" means within sampling error; and the 009a
comparison columns pair numbers measured on two engine versions, of which only the ones
decided before a bar accept are strictly comparable episode for episode.
