# 009a: off-ceiling watermark measurement

Question (from `remediation-plan.md`, gates G9/G10, decision 45): what does the
flat-length clone-descending watermark actually record on genuinely racy bodies, off the
ceiling experiment 007 measured — (H1) do miss weights recover from the W1 degeneracy or
stay near zero (PHYS_GATE territory), (H2) what do the discovery bar and shrink gauntlet
operating points become under the measured weights, and which of 008's weighting columns
the distribution selects, (H3) do off-ceiling DB-reuse and blob-replay reproduction hold
the decision 11/31 design points.

## Harness

`/experiments/watermark` (frozen), driving the real engine through the `hegeltest`
frontend, plus the extended `__bench` dump hook: `nd::watermark_dump` now records
(stored, realized, weight, failed) at every measurement replay (`nd_replay_once`), so
failures are sampled alongside misses and the pre-decision-45 weighting (matched scalar
prefix over stored element count) is recomputed offline from the same pairs.

Two bodies, each with a hidden seeded schedule standing in for thread interleaving, so
runs are deterministic and rates injectable. Failure fires at rate `p` per execution,
independent of drawn values, making every timeline's true reproduction rate exactly `p`.
Structural divergence comes from schedule-injected extra draws whose value ranges are
disjoint per role, so a shifted replay cannot pun values:

- **clone**: one clone stream, 8 work draws, a retry draw injected at 0.15/round —
  007's clone workload with the race made structural.
- **machine**: two worker clone streams shaped like `run_concurrent` (per worker-step a
  rule and an argument draw, a contended re-read injected at 0.2/worker-step).

Cells: {clone, machine} × p ∈ {0.1, 0.3, 0.9}, 200 episodes each. An episode is a
discovery run (100 test cases, fresh temp database, `print_blob`) and — when the run
reported a blob, meaning a confirmed origin — a reuse-only run on the same database
(`phases([Reuse])`) plus one `reproduce_failure` blob replay. Discovery-run samples feed
the weight distribution (reservoir-sampled at 400k per cell, exact counts reported), and
reuse/blob samples are drained and discarded. All seeds are fixed in code (engine,
hidden schedule, reservoir, sims: see `seed()` in `src/main.rs`). Stdout is
byte-identical across reruns, verified by diffing two independent full runs.

Bar and gauntlet numbers are an **empirical replay** of `nd::discovery_bar` /
`nd::gauntlet` (arithmetic mirrored from `nd/mod.rs`) over miss weights resampled from
the measured distribution, 10000 streams per number. The `confirm-bar` DP was not
generalised: weighted misses make the state continuous and the measured support has too
many distinct values to grid honestly. Fluke costs replay at fail rate 0, anchors replay
at the cell's true `p`, and the gauntlet replays a fluke candidate whose recruiting
failure is ledgered at weight 1.0 (as `EngineShrinkProbe::run` does) against the cell's
median confirmed anchor.

The first full run crashed the engine in `bind_deletion` ("What we learned", item 6), so
the machine cells were measured after that one guarded fix.

Reproduce: `cargo run --release` in `/experiments/watermark` (tables on stdout, progress
on stderr, ~35 min single-threaded).

## Results

### Episode accounting

| body | p | reported | with blob | miss samples | fail samples |
| --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 200/200 | 118 | 70078821 | 7703821 |
| clone | 0.3 | 200/200 | 200 | 50001846 | 20791094 |
| clone | 0.9 | 200/200 | 200 | 2107573 | 13640847 |
| machine | 0.1 | 200/200 | 102 | 80521227 | 8748613 |
| machine | 0.3 | 200/200 | 200 | 64368071 | 26307194 |
| machine | 0.9 | 200/200 | 200 | 2525889 | 13578435 |

Blobless episodes at p = 0.1 are unconfirmed caveat-only reports, the expected bar power
at the target rate.

### Miss-weight distribution (H1, the G9 numbers)

new = shipped clone-descending watermark, old = pre-decision-45 scalar prefix, both from
the same (stored, realized) pairs:

| body | p | W50 new | mean new | p10 | p90 | share 0 | share 1 | W50 old | mean old | share 0 old |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 0.400 | 0.490 | 0.200 | 1.000 | 0.000 | 0.124 | 0.000 | 0.076 | 0.924 |
| clone | 0.3 | 0.444 | 0.526 | 0.222 | 1.000 | 0.000 | 0.152 | 0.000 | 0.064 | 0.936 |
| clone | 0.9 | 0.429 | 0.519 | 0.250 | 1.000 | 0.000 | 0.139 | 0.000 | 0.032 | 0.968 |
| machine | 0.1 | 0.278 | 0.377 | 0.150 | 0.778 | 0.000 | 0.053 | 0.000 | 0.124 | 0.794 |
| machine | 0.3 | 0.294 | 0.389 | 0.150 | 0.800 | 0.000 | 0.057 | 0.000 | 0.131 | 0.784 |
| machine | 0.9 | 0.333 | 0.372 | 0.150 | 0.650 | 0.000 | 0.034 | 0.000 | 0.077 | 0.873 |

**G9 verdict.** Clone W50 is 0.40-0.44 at every p, inside the ≥ 0.3 no-change zone.
Machine W50 is 0.278/0.294/0.333 — two cells sit just under the 0.3 line, so by the
gate's letter DRM sees these tables. Nothing approaches the < 0.1 PHYS_GATE clause, and
share-0 is exactly zero everywhere, so a physical backstop would be re-deriving constants
to guard a regime the measurement says is empty (the physical CONFIRM_CAP arm at 37
already bounds the worst case). The recommendation is to close G9 as no-change.

**The old weighting was the w = 0 column in practice.** Its share-0 is 0.78-0.97 and its
mean 0.03-0.13: on these bodies the pre-fix bar and gauntlet were running on the
degenerate weights 008's w = 0 row shows breaking min-fails. The fix moves the whole
distribution into the safe band.

**008's weighting column.** Measured means are 0.49-0.53 (clone) and 0.37-0.39
(machine), medians 0.28-0.44, with no mass at zero: the distribution sits strictly
between 008's w = 0.2 and w = 1.0 columns (clone near midway, machine nearer 0.2). Both
bracketing columns preserve every 008 headline number, so the phase-12 constants freeze
as recommended and the w = 0 escalation row is ruled out.

### Discovery bar under measured weights (H2)

Empirical replay, 10000 streams per number. Fluke costs at fail rate 0, anchors at the
cell's true p (accept share in parentheses):

| body | p | fluke reject med/mean new | fluke reject med/mean old | anchor med new (accept) | anchor med old (accept) |
| --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 21 / 21.1 | 37 / 37.0 | 0.106 (0.56) | 0.300 (0.58) |
| clone | 0.3 | 20 / 19.7 | 37 / 37.0 | 0.205 (1.00) | 0.510 (1.00) |
| clone | 0.9 | 20 / 19.9 | 37 / 37.0 | 0.510 (1.00) | 0.510 (1.00) |
| machine | 0.1 | 27 / 27.2 | 37 / 37.0 | 0.130 (0.57) | 0.273 (0.58) |
| machine | 0.3 | 26 / 26.4 | 37 / 37.0 | 0.241 (1.00) | 0.376 (1.00) |
| machine | 0.9 | 27 / 27.5 | 37 / 37.0 | 0.510 (1.00) | 0.510 (1.00) |

**Fluke rejection cost** (target ≤ 20 physical, vs today's 37): clone lands on target
(20-21), machine lands at 26-27 — it misses the target's letter, though it recovers 10
of the 17 replays the degenerate weighting wastes. The cost is 10/mean-weight physical
replays up to the 37 cap, so it tracks the body's divergence profile directly.

**Median confirmed anchor at true p = 0.1** (target ≤ 0.2, the escalation signal): clone
0.106, machine 0.130 — both pass, so weighting alone does describe these clone bodies
and decision-14 territory stays closed. The old weighting put the same anchors at
0.300/0.273, and at p = 0.3 at 0.510, the LCB(4/4) pathology the critique predicted.

### Shrink gauntlet under measured weights (H2)

Fluke candidate against the cell's median confirmed anchor:

| body | p | anchor | reject med new | proof share new | accept share new | reject med old | proof share old | accept share old |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| clone | 0.1 | 0.106 | - | - | 1.00 | - | - | 1.00 |
| clone | 0.3 | 0.205 | - | - | 1.00 | - | - | 1.00 |
| clone | 0.9 | 0.510 | 19 | 1.00 | 0.00 | 30 | 0.00 | 0.00 |
| machine | 0.1 | 0.130 | - | - | 1.00 | - | - | 1.00 |
| machine | 0.3 | 0.241 | - | - | 1.00 | - | - | 1.00 |
| machine | 0.9 | 0.510 | 25 | 0.96 | 0.00 | 30 | 0.00 | 0.00 |

At p ≤ 0.3 anchors there are no rejects to price: the shipped m = 1 rule accepts every
fluke on its recruiting failure (LCB(1/1) = 0.2065 clears the threshold), the S1
degeneracy 008 measured, owned by phase 12's min-fails. Where a real threshold exists
(p = 0.9), the measured weights convert the reject path from 30-always-cap, proof-never
to proof-rejects at 96-100% share, median cost 19 (clone) and 25 (machine) — the > 50%
proof-share target passes and the ≤ 15 cost target misses. 009b re-prices this under the
post-008 rules, where low-anchor rejects exist at all.

### G10: off-ceiling reproduction of persisted state (H3)

Per episode with a blob: one reuse-only run on the discovery database, one blob replay.

| body | p | DB reuse | blob replay |
| --- | --- | --- | --- |
| clone | 0.1 | 118/118 | 118/118 |
| clone | 0.3 | 197/200 (98.5%) | 196/200 (98%) |
| clone | 0.9 | 198/200 (99%) | 180/200 (90%) |
| machine | 0.1 | 101/102 (99%) | 101/102 (99%) |
| machine | 0.3 | 199/200 (99.5%) | 199/200 (99.5%) |
| machine | 0.9 | 198/200 (99%) | 191/200 (95.5%) |

No G10 flag fires: every rate at p ∈ {0.1, 0.3} is ≥ 98%, far above the < 90% (p = 0.3)
and < 60% (p = 0.1) thresholds, so decision 31 stays closed on its own terms.

The one dip is blob replay at p = 0.9 (clone 90%). Instrumenting that cell showed 23/200
episodes never flipped into ND handling — at p = 0.9 the verify replay almost always
reproduces, so a run can end believing the test deterministic — and their blobs are
therefore v1 exact-choice replays, which a racy body breaks (3/23 reproduced, because
any injected race draw misaligns the exact sequence into an overrun before the failure
check). All 177 v2 blobs reproduced. Reuse-phase replays allow continuation past a
divergence, so DB reuse holds 99% on the same episodes while v1 blob replays drop
to 13%.

## What we learned

1. **The W1 degeneracy was real and the fix lands in the safe band.** Pre-fix weights on
   these bodies were effectively 008's w = 0 column (share-0 up to 0.97). Post-fix,
   share-0 is exactly zero and W50 is 0.28-0.44 across every cell.
2. **G9: recommend no-change.** Clone is inside the ≥ 0.3 zone at every p. Machine's
   0.278/0.294 puts two cells formally in the DRM-sees-tables band, but nothing is near
   the < 0.1 PHYS_GATE clause and the CONFIRM_CAP arm already bounds the physical worst
   case at 37.
3. **The escalation signal did not fire.** Median confirmed anchors at true p = 0.1 are
   0.106/0.130 against the ≤ 0.2 target. Evidence weighting alone describes clone
   bodies, and decision-14 machinery stays closed.
4. **Bar economics recover most but not all of the target.** Fluke rejection falls from
   37 to 20-21 (clone) and 26-27 (machine) physical replays. The machine body misses the
   ≤ 20 letter because rejection cost is 10/mean-weight, a body property — tightening it
   further is a bar-constant decision under G9, not a weighting defect.
5. **Gauntlet proof-rejects now work wherever a threshold exists to prove against.** At
   informative anchors the reject path goes from 30-always to 96-100% proof-rejects at
   median 19-25. Low-anchor cells have no rejects at all under the shipped m = 1 rule,
   so the ≤ 15 target is only meaningfully priceable after phase 12's min-fails.
6. **The harness found an off-ceiling shrinker crash on its first full run.**
   `try_replace_with_deletion` indexed `current_nodes` with a stale `idx` after an
   accepted mid-pass candidate — under ND the adopted nodes are the realised run, which
   can be shorter than the probe index — aborting the process from inside
   `hegel_next_test_case`. Fixed with a bounds guard, pinned red-green by
   `bind_deletion_survives_an_adopted_candidate_shorter_than_the_probe_index`. This is
   the one production change phase 11 makes: without it the machine cells abort.
7. **Detection can be escaped at p = 0.9.** 11.5% of clone episodes ended without ever
   flipping ND, and their v1 blobs reproduce at 13% while v2 blobs reproduce at 100%. No
   G10 flag covers p = 0.9, but the failure report's blob quality currently depends on
   whether the run happened to notice its own nondeterminism. This belongs in a risk
   entry or in phase 13's decision-3 audit.

## Caveats

Schedule effects come from a seeded hidden RNG, not real threads — that is what makes
the outputs byte-identical, and 007 already covered real-thread behaviour at ceiling.
Failure is independent of drawn values, so shrinking has no value target and the
incumbents are structure-only. Bar and gauntlet numbers resample miss weights i.i.d.
from the pooled per-cell distribution, ignoring within-episode correlation, and the
gauntlet uses the fresh-ledger fluke framing (as 008's DP does) rather than modelling
cross-retry ledger retention. Miss weights pool every measurement-replay source in the
discovery run (confirmation batches, gauntlet reruns, final replay). Distribution
statistics are computed over a 400k-per-cell reservoir of the full sample counts in the
accounting table. W50 granularity is bounded by flat timeline length, roughly ninths for
the clone body and eighteenths for machine.
