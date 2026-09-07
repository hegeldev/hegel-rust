# Experiment 014: multiplicity control (decision 72)

Derives the constants for the multiplicity mechanisms in
`notes/research/fcr-analysis.md`: the per-origin bar-attempt cap, the
per-origin gauntlet alpha budget, and the pooled-review bar. A frozen
pure-simulation crate (`experiments/fcr-sim`, no engine dependency): exact DP
over the engine's sequential rules on plain counts (decision 71), the 005A
method, plus a seeded Monte Carlo for run-level composition (base seed
0x5eed2026, 100k episodes per cell). All rules mirror
hegel-c/src/native/nd/mod.rs: bar gate 10 / min-fails 4 / cap 40, gauntlet
LCB/UCB at z = 1.96 with cap 30, verdicts checked after every replay.

## The bar and the attempt cap (exact DP + composition)

Plain-count operating points reproduce 005A exactly: alpha = 0.0059 per
q = 0.02 batch (15.2 replays), power 0.454 per p = 0.1 batch. Composed over
attempts:

| attempts F | P(false confirm, q=0.02) | P(confirm, p=0.1) |
| --- | --- | --- |
| 1 | 0.0059 | 0.454 |
| 3 | 0.0175 | 0.837 |
| 5 | 0.0290 | 0.951 |
| 8 | 0.0461 | 0.992 |

`BAR_ATTEMPTS_PER_RUN` = 5 keeps the composition the 005A arithmetic assumed
(2.9% per origin, >95% power at target); with the backtrack's separate 3
batches the per-origin ceiling is F = 8, 4.6%. P(5 straight rejects at
p = 0.1) = 0.049 — the odds a target bug reaches the backtrack with its
sweep budget spent.

Recycling without the cap is unbounded in run length (Monte Carlo, sighting
probability min(1, 10x) per epoch):

| x | epochs | cap | P(confirm) | mean attempts | mean replays |
| --- | --- | --- | --- | --- | --- |
| 0.02 | 40 | none | 0.046 | 7.8 | 119 |
| 0.02 | 40 | 5 | 0.028 | 4.8 | 73 |
| 0.02 | 200 | none | 0.209 | 35.7 | 542 |
| 0.02 | 200 | 5 | 0.029 | 4.9 | 75 |
| 0.05 | 200 | none | 1.000 | 9.8 | 203 |
| 0.05 | 200 | 5 | 0.416 | 4.1 | 84 |
| 0.1 | 200 | none | 1.000 | 2.2 | 51 |
| 0.1 | 200 | 5 | 0.952 | 2.1 | 48 |

A long run confirms an uncapped q = 0.02 fluke 21% of the time; the cap
pins it at the F = 5 arithmetic. The cost lands below target: a p = 0.05
bug drops from certain (given enough run) to 42% per run, recycling
cross-run instead. p ≥ 0.1 loses ≤ 5 points.

**Mixed-timeline origin (the 008 L4b shape).** Bug p = 0.1 and fluke
q = 0.02 sharing one origin, each attempt landing on the fluke timeline
with probability `share` (record_run re-inserts whichever sighting arrives
first after an eviction):

| fluke share | cap | P(confirm on bug) | P(confirm on fluke) | P(unconfirmed) |
| --- | --- | --- | --- | --- |
| 0 | none | 1.000 | 0.000 | 0.000 |
| 0 | 5 | 0.951 | 0.000 | 0.049 |
| 0.5 | none | 0.987 | 0.013 | 0.000 |
| 0.5 | 5 | 0.719 | 0.009 | 0.272 |
| 0.75 | none | 0.956 | 0.037 | 0.007 |
| 0.75 | 5 | 0.450 | 0.018 | 0.533 |

The cap's real power cost concentrates here: attempts burned on fluke
timelines drain the budget the bug needs. This is why the backtrack's
3-attempt budget stays separate (its probes walk history, which is
disproportionately the real bug's pre-flip sightings), and it is the regime
where today's engine already loses ~half of runs to displacement (008 L4b:
49% caveat-only). Priced, with the cross-run recycle and the backtrack as
the mitigations.

## Gauntlet charges (exact DP per mode)

Per-proposal false accept, unconditional. Fast mode conditions on a failing
recruit (which the ledger records); Confirm mode drives every proposal from
empty to a bound verdict (decision 18). At the floor threshold, q = 0.02:

| mode | m=4 | m=5 | m=6 | m=7 | m=8 |
| --- | --- | --- | --- | --- | --- |
| Fast | 4.0e-4 | 5.1e-5 | 5.1e-6 | 4.1e-7 | 2.7e-8 |
| Confirm | 2.9e-3 | 3.0e-4 | 2.5e-5 | 1.7e-6 | 1.0e-7 |

Fast m=4 reproduces 008's 4.0e-4 exactly. The Confirm row is the corrected
exposure 008's composition missed: a confirmation-sweep drive of a bugless
candidate at the floor carries 2.9e-3, seven times the Fast number the
shipped per-shrink arithmetic used. At threshold 0.24 the charges fall to
3.1e-6 / 5.5e-6; at a 0.9 threshold both are exactly zero (unreachable
within the cap — the 012 lottery spends nothing). The flat rule's exposure
without a budget: 1-(1-alpha_4)^K reaches 0.33 (Fast) / 0.94 (Confirm) at
K = 1000.

Affordability of the budget (proposals charged per stage, escalating the
failure minimum when the remainder can't afford the next charge; the table
splits each stage at half the remaining budget):

| budget B | mode | threshold | at m=4 | m=5 | m=6 | m=7 | then m=8, per 10k |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.02 | Fast | 0.05 | 25 | 98 | 499 | 3115 | 0.0003 |
| 0.02 | Fast | 0.24 | 3214 | 22488 | 88874 | 356083 | ~0 |
| 0.02 | Confirm | 0.05 | 3 | 18 | 117 | 854 | 0.0010 |
| 0.02 | Confirm | 0.24 | 1830 | 12308 | 44024 | 165109 | ~0 |

`GAUNTLET_ALPHA_BUDGET` = 0.02 per origin per run: mid-anchor shrinks are
effectively unconstrained, floor-threshold confirmation sweeps escalate
within a handful of driven ledgers, and the total per-origin false-accept
spend is bounded by 0.02 plus an m=8 tail of ≤ 1e-7 per ledger whatever K
the body realizes.

Escalation's power cost on true candidates (Fast mode, threshold from
gamma x anchor; 0.053 is the realistic bar-seeded threshold at p = 0.1,
0.839 the 20-run all-fail seed):

| p | threshold | m=4 | m=5 | m=6 | m=7 |
| --- | --- | --- | --- | --- | --- |
| 0.1 | 0.053 | 0.57 | 0.33 | 0.16 | 0.06 |
| 0.1 | 0.08 | 0.35 | 0.27 | 0.16 | 0.06 |
| 0.3 | 0.24 | 0.46 | 0.42 | 0.40 | 0.37 |
| 0.9 | 0.839 | 0.12 | 0.12 | 0.12 | 0.12 |

(P(accept | recruited); the p = 0.9 row is the 012 lottery — the failure
minimum never binds there, so escalation is free.) The cost concentrates in
floor-threshold shrinks after the budget spends down, which is where
decision 18's certificate weakens: fewer accepts make "the confirmation
sweep accepted nothing" easier to obtain, stopping earlier and missing
recoverable reductions. Accepted as the price of the bound (conservative
under decision 2: refused candidates keep the incumbent).

## The pooled review, any-failure vs the bar

At final replay an unconfirmed origin today confirms on any single failure
across the review's replays (33 for a lone incumbent, 44 with a pool):

| pool n | replays | rate x | OLD confirm | NEW confirm |
| --- | --- | --- | --- | --- |
| 1 | 33 | 0.02 | 0.487 | 0.003 |
| 1 | 33 | 0.1 | 0.969 | 0.440 |
| 1 | 33 | 0.3 | 1.000 | 0.971 |
| 2+ | 44 | 0.02 | 0.589 | 0.003 |
| 2+ | 44 | 0.1 | 0.990 | 0.450 |

Handing the failing run to the standard evidence batch cuts the fluke
confirm rate ~170x. The power cost at p = 0.1 (0.97 → 0.44 before the
backtrack rescue) is real; the failing execution still reaches the report
as a values-carrying unconfirmed caveat, and a rejected batch falls through
to backtrack-then-evict as today.

## Seeded-anchor miscoverage (exact, extension 20)

P(LCB > p | accept) with the accepting batch extended to 20 runs where the
accept lands before 20 (an accept on runs 21-40 seeds its stop-timed LCB —
the majority of target-regime accepts):

| source | p | P(select) | miscoverage | mean anchor |
| --- | --- | --- | --- | --- |
| bar accept | 0.05 | 0.102 | 0.549 | 0.057 |
| bar accept | 0.1 | 0.454 | 0.094 | 0.066 |
| bar accept | 0.3 | 0.971 | 0.018 | 0.155 |
| bar accept | 0.9 | 1.000 | 0.000 | 0.704 |
| gauntlet adopt | 0.1 | 0.035 | 0.337 | 0.097 |
| gauntlet adopt | 0.3 | 0.139 | 0.070 | 0.236 |
| OLD review stop-at-fail | 0.1 | 0.969 | 0.103 | 0.051 |

Miscoverage exceeds the 2.5% nominal near the accept boundary (selection
conditioning) and vanishes by p = 0.3. Mean anchors sit at or below the
true rate everywhere, and an optimistic-high anchor prices candidates too
high — the conservative direction under decision 2, absorbed by the
gamma = 0.8 slack. **No haircut**: the boundary miscoverage is tail spread
around an unbiased-to-low mean, not a shifted estimate.

## Decision hooks

- `BAR_ATTEMPTS_PER_RUN` = 5, backtrack's 3 kept separate: per-origin
  false-confirm ≤ 4.7% at q = 0.02, target-regime power ≥ 95% on pure
  origins, mixed-origin cost priced above.
- `GAUNTLET_ALPHA_BUDGET` = 0.02 per origin per run, exact DP charges per
  (state, threshold, minimum), minima escalating 4 → 8 for new ledgers
  only; bound 0.02 + 1e-7 per ledger beyond.
- The pooled review confirms through the standard evidence batch; the
  any-failure rule retires.
- No anchor haircut; z = 1.96 stands.

Reproduce: `cargo run --release` in `/experiments/fcr-sim` (~1 s; the DP is
exact, the Monte Carlo is seeded).
