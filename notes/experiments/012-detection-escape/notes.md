# Experiment 012: detection-escape recheck

The seam plan's second acceptance experiment (`notes/seam-plan.md`, phase 17). A new
frozen crate (`experiments/detection-escape`) cloning `experiments/watermark`'s episode
protocol on the post-seam engine: 200 episodes per cell of {clone-stream, machine} x
p in {0.1, 0.3, 0.9} plus a deterministic control (8 draws, unconditional failure, no
hidden race). Each episode runs discovery (100-case budget, database, blob printing,
statistics), then a database-reuse run, then a blob-reproduction run. The engine's
`__bench` seam dump records each episode's first flip site, and the statistics line
supplies its measurement-replay count.

Bodies are verbatim from `experiments/watermark` so the baselines carry: 009a measured
23/200 never-flip episodes on the clone-stream body at p = 0.9, v1 blob reproduction
180/200 (clone) and 191/200 (machine) at p = 0.9, and >= 98% reuse/blob at p <= 0.3.

Criteria (seam-plan.md): clone p = 0.9 never-flip <= 1/200 (the first check's escape is
(p·s)^4, ~3e-4 at the top of 009a's estimated 0.085-0.13 range for p·s); blob
reproduction >= 199/200 on both bodies at p = 0.9 (the v1 continuation fix carries the
episodes that still escape); p <= 0.3 cells hold 009a's >= 98% reuse/blob rates;
deterministic control: 0 flips and exactly k x origins measurement replays.

## Results (2026-09-04, engine at 5d3aadc3)

| cell | reported | flips by first site | never-flip | blobs v1/v2 | reuse | blob replay | measurement replays min..max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| clone p=0.1 | 200/200 | first-check 200 | 0 | 0/190 | 190/190 | 189/190 | 15..3657920 |
| clone p=0.3 | 200/200 | first-check 200 | 0 | 0/200 | 200/200 | 200/200 | 248680..1907144 |
| clone p=0.9 | 200/200 | first-check 200 | 0 | 0/200 | 200/200 | 200/200 | 240520..2096223 |
| machine p=0.1 | 200/200 | first-check 200 | 0 | 0/193 | 193/193 | 193/193 | 38..3800917 |
| machine p=0.3 | 200/200 | first-check 200 | 0 | 0/200 | 200/200 | 200/200 | 381928..2969107 |
| machine p=0.9 | 200/200 | first-check 200 | 0 | 0/200 | 200/200 | 200/200 | 303566..2662089 |
| det-control | 200/200 | — | 200 | 200/0 | 200/200 | 200/200 | 4..4 |

The blob and reuse denominators at p = 0.1 are the episodes that confirmed and stored
an entry; the other 10 (clone) and 7 (machine) reported caveat-only. Every flip in
every cell happened at the first-interesting check: the hidden schedule almost surely
changes a replay's structure within four exact replays, so the check catches these
bodies at discovery.

## Against the criteria

- **Clone p = 0.9 never-flip <= 1/200: pass at 0/200** (baseline 23/200). Machine
  p = 0.9 is also 0/200.
- **Blob reproduction >= 199/200 on both bodies at p = 0.9: pass at 200/200 and
  200/200** (baselines 180 and 191). All blobs are v2.
- **p <= 0.3 cells hold >= 98% reuse/blob: pass.** Reuse 100% everywhere; blob replay
  189/190 (99.5%) at clone p = 0.1, 100% elsewhere.
- **Deterministic control: pass.** 0 flips, all 200 blobs v1, and exactly 4 measurement
  replays (k = 4 x 1 origin) in every episode — the first check is the whole
  deterministic-run cost.

## The measurement-replay column: a gauntlet cost lottery at high p

The nd cells' replay counts run 240k-3.8M per 100-case episode. A probe rerun of
clone p = 0.9 episode 0 with a body-invocation counter decomposed its 1,198,879
measurement replays: all but one are
`nd_replay_once` calls from the shrink gauntlet's evidence loop, spread over 42,125
distinct candidate timelines at a median of 29 replays each — the `GAUNTLET_CAP` = 30
batch, minus the initial probe execution.

The mechanism: above `RETENTION_HIGH_WATER` = 0.8 the gauntlet's gamma is 1.0, so a
candidate must prove its Wilson lower bound at or above the anchor itself. The anchor
ratchet raises it to each first accept's lower bound, and on a constant-p body it
converges to roughly the bound of an all-fails cap-length batch (~0.88 at p = 0.9).
From there only another all-fails batch can accept — probability 0.9^30 ~= 4% for a
step whose true reproduction rate is the full 0.9 — so nearly every genuine shrink
step rejects at the 30-replay cap and is re-proposed on a later pass. The shrinker
still reaches correct minima (the blob column shows it), but through a 4% acceptance
lottery that costs ~30 replays per attempt.

Cost scales with the shrink surface: 011's L4 cell (same constant p = 0.9, a small
scalar landscape) sat at 1774 median execs with an 11k p90 tail, while these clone
and machine bodies pay 1M+. On microsecond bodies that is seconds; on a 10ms body the
same volume is hours, so in practice the `MAX_SHRINKING_SECONDS` = 300 deadline would
cut the lottery off and the shrink would stall part-way instead. The gauntlet
constants are decision 57's frozen 008/009a calibration, and the interaction of the
anchor ratchet with the high-water gamma predates the seam work (a post-flip
decision-38 requeue paid the same lottery), but the universal first check means every
high-p episode now pays it from discovery, and 012 is the first measurement of its
size. Escalated to DRM rather than fixed here: any fix (capping anchor raises at what
a cap-length batch can prove, scaling the cap with the anchor, or keeping gamma below
1 for acceptance) trades against the no-probability-loss constraint.
