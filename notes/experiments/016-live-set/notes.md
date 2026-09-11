# 016: the counterexample as one test case — reproduction and cost under the live set

Status: six campaigns run on 2026-09-11 against decisions 74/75 as built, each
forcing the correction the next one measured (commits `0849197e` → `b2ff7905`).

Question: with the pool replayed as one test case (the live set, decision 74), the
gauntlet counting only on-timeline evidence, and the multiverse passes shrinking the
set (decision 75), do stored failures still reproduce at the ceiling 007/009a measured
for first-fit — and what does a replay cost, what do the pools look like, and where does
the engine now spend its time?

## Setup

Harness: `experiments/live-set/` (standalone frontend crate + `drive.py`, one process
per run, modelled on 007). Per episode: a fresh temp database; `discover` (Generate +
Shrink, 200 test cases, `print_blob`); `blobinfo` on the blob; `reuse` on the same
database (Reuse + Shrink); three `replay`s of the blob via `reproduce_failure`. The body
counts its executions in a process-global counter. 20 episodes per body, seeds fixed.

    CARGO_TARGET_DIR=/tmp/hegel-exp-target python3 experiments/live-set/drive.py --episodes 20 --out experiments/live-set/results.jsonl

Bodies:

- **racy**: 007's `#[hegel::concurrent_state_machine]` lost-update counter,
  `run_concurrent(m, tc, 2, 4)`. Genuine scheduling nondeterminism.
- **clone**: 007's clone-stream body failing on `x >= 500` every third call. Outcome
  nondeterminism, fixed structure.
- **branch**: David's `ps`/`pt`. Draw `a: bool`; a hidden coin the engine cannot see
  (process-global xorshift, P = 0.5) picks branch s — draw `b: bool`, `x: 0..=100`, fail
  iff `a && b && x >= 60` — or branch t — draw `y, z: 0..=100`, fail iff
  `y >= 60 && z >= 60`; one panic site for both.
- **twobranch**: after `a`, two independent hidden coins each pick a bool piece (hot
  iff true) or an int piece (hot iff `>= 60`); fail iff `a` and both pieces are hot.
  Four failing paths.

Harness note: a bare `Hegel::new(body)` has no database key and the engine persists and
reuses only with one; the harness sets `__database_key`. (007's harness has the same gap
on this branch: its reuse column would read 0/20 if re-run as is.)

## Campaign 1 — `0849197e` (decisions 74 and 75, before the alignment fix)

| body | discovery | median disc. execs | blob | timelines (count: episodes) | median first len | reuse | median reuse execs | blob replay | median replay execs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| racy | 20/20 | 6833 | 20/20 | 1: 9, 3: 2, 4: 1, 5: 2, 6: 2, 7: 4 | 25 | 20/20 | 5 | 60/60 | 1 |
| clone | 20/20 | 2372 | 20/20 | 1: 20 | 2 | 20/20 | 4 | 60/60 | 1 |
| branch | 20/20 | 5306 | 20/20 | 2: 19, 3: 1 | 3 | 20/20 | 4.5 | 60/60 | 1 |
| twobranch | 20/20 | 1411 | 20/20 | 1: 1, 2: 15, 3: 1, 4: 2, 5: 1 | 3 | 20/20 | 941 | 60/60 | 1 |

Caveats printed: every failure was `nondeterministic failure, confirmed` at discovery
(racy printed a `note:` line on 14/20 — the other 6 failed and produced a blob; not
chased) and `reproduced from stored timelines` or `confirmed` at reuse (see below).
No panics outside the intended assertion, no hangs, no missing or undecodable blobs.

### What it says

1. **Reproduction is at the ceiling**: reuse 80/80, blob replay 240/240, against 007's
   20/20 and 60/60 per workload and 009a's ≥ 98% / ≥ 95.5% design points.
2. **A replay costs one execution.** Median blob-replay executions is 1 on every body,
   the branch bodies included: the replay follows whichever branch the test takes.
   Under first-fit a wrong-branch first attempt was a wasted execution per attempt.
3. **The pool covers the branches.** `branch` blobs are `(3, 3)` in 19/20 episodes —
   the shrunk s-path and the shrunk t-path, nothing else: capture at confirmation found
   the other branch, the per-timeline shrinker shrank the incumbent, and the delete pass
   removed everything that stopped serving. `twobranch` has four failing paths and its
   pools hold 2 timelines in 15/20 episodes (1–5 overall): the pool does not reliably
   cover all four, and the uncovered paths reproduce through the rescue tier (random
   fill past the divergence, ~16% per uncovered run at this body's thresholds) and
   retries — which is why reproduction is still 100% while pool coverage is partial.
4. **Reuse cost is bimodal**, and the slow mode is a re-shrink, not a replay problem.
   Sorted reuse executions: racy `[2×7, 3, 4, 5, 5, 6, 6, 13, 21, 1852, 1880, 2343,
   7025, 9548]`; branch `[2×9, 4, 5, 5, 7, 2851, 3917, 4062, 5576, 5709, 6007, 8520]`;
   twobranch `[2, 2, 2, 3, 758 … 1368]`. Fast/slow split: racy 15/5, clone 20/0,
   branch 13/7, twobranch 4/16. The slow episodes are exactly those whose reuse caveat
   reads `confirmed` rather than `reproduced from stored timelines`: the reuse replay
   realized a stored branch other than the incumbent, the reuse path's alignment check
   compared it against the incumbent alone, declared the replay misaligned, and the
   shrink phase ran a confirmation batch and a gauntleted re-shrink of an already shrunk
   example (the standing `replay_aligned` residual of decision 5's table, made visible
   because the live set now follows the test's branch). Fixed in `7210c4f8`: a reuse
   replay that realizes any stored timeline is aligned. Campaign 2 measures the effect.
5. **racy pools** (3–7 entries) hold one or two very long timelines (≈1400–2100
   choices) beside short ones (16–55), and single-timeline racy blobs are either short
   or the raw unshrunk ~1950-choice case: the shrink deadline cuts some racy shrinks
   short, as before. The multiverse passes then run only when the shrink did not time
   out, so those pools were not pruned.

The `note: run with RUST_BACKTRACE=1` "caveat" the 007-style counter picked up on racy
replays (10/60) is std's panic-hook line from a worker thread, not a Hegel caveat; those
replays reproduced.

## Campaign 2 — `7210c4f8` (reuse alignment against any stored timeline)

| body | discovery | blob | timelines (count: episodes) | reuse | median reuse execs | reuse caveats (stored / confirmed) | blob replay | median replay execs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| racy | 20/20 | 20/20 | 1: 6, 2: 1, 3: 3, 4: 5, 5: 4, 6: 1 | 20/20 | 2 | 17 / 3 | 60/60 | 1 |
| clone | 20/20 | 20/20 | 1: 20 | 20/20 | 4 | 20 / 0 | 60/60 | 1 |
| branch | 20/20 | 20/20 | 2: 19, 3: 1 | 20/20 | 2 | 20 / 0 | 60/60 | 1 |
| twobranch | 20/20 | 20/20 | 1: 1, 2: 15, 3: 1, 4: 2, 5: 1 | 20/20 | 854 | 8 / 12 | 60/60 | 1 |

(`racy` printed a `note:` line on 19/20 discoveries this time.)

`branch` reuse is now uniformly the fast mode (20/20 "reproduced from stored
timelines", median 2 executions; was 13/7). `racy` improved 15/5 → 17/3. `twobranch`
stays slow in 12/20: its pools cover two of the four failing paths, so the reuse replay
realizes an uncovered path — through the rescue tier — more often than not, and that is
a genuine misalignment.

### Why the pools cover two of four paths — and a design correction

The delete pass was gauntleted: a set without one component was accepted whenever its
reproduction cleared `gauntlet_threshold(anchor)` = 0.8 × anchor. A four-path set at
100% loses one path's 25% share and lands at ~80%, above the threshold, so the pass
deleted a live branch; a second deletion (to ~60%) was refused. The dropped path then
reproduces only through the rescue tier's random fill. That trades reproduction for
timeline count — the opposite of what "fewer timelines is better" was meant to buy.
Decision 75 now deletes by census instead: 40 replays of the set record which timeline
each failing run followed, and only pool timelines that served none are dropped;
reorder and splice keep the gauntlet. Campaign 3 measures the result.

## Campaign 3 — `dac00371` (deletion by census)

| body | timelines (count: episodes) | reuse | median reuse execs | reuse caveats (stored / confirmed) | blob replay | median replay execs |
| --- | --- | --- | --- | --- | --- | --- |
| racy | 1: 20 | 20/20 | 4.5 | 18 / 2 | 60/60 | 2 |
| clone | 1: 20 | 20/20 | 4 | 20 / 0 | 60/60 | 1 |
| branch | 1: 20 | 20/20 | 3 | 20 / 0 | 60/60 | 1.5 |
| twobranch | 1: 1, 2: 2, 3: 15, 4: 2 | 20/20 | 3.5 | 11 / 9 | 60/60 | 1 |

Not what the census was meant to do: `branch` went from two timelines to one in every
episode and its replay medians rose (blob 1.5, reuse 3). A Debug-traced episode showed
why, and it was not the census. The per-timeline shrinker had adopted `[F, 60, 60]` —
the minimal t-branch example, which fails regardless of `a` — as the incumbent. Its
gauntlet reruns failed whenever the test stayed on it (t-runs), and every s-run was a
bounce: serving `F` at position 0 pruned all the `a = T` pool timelines, the run
diverged at position 1, and the rescue never fails on s. So the census, correctly,
found the pool never serving and dropped it; the counterexample had already lost the s
branch. Two defects behind the adoption:

1. The candidate's evidence was only its on-timeline reruns (100% failing), and the
   bounce budget — meant to bound how much worse than the incumbent a candidate may
   bounce — was exhausted (11 bounces against a budget of 10) without latching a
   verdict, so the shrinker re-proposed the same candidate 66 times; each proposal's
   recruiting run added a fail to the same ledger until it reached 20/20 and was
   accepted. 1910 of the episode's reruns were divergences.
2. The anchor at shrink start was the discovery-time confirmation batch's lower bound
   (0.34–0.58), measured on the lone unshrunk timeline before any pool existed, so even
   a set that reproduces at ~55% cleared 0.8 × anchor.

Fixes (`decision 75`, this commit): the candidate's ledger also carries the set's
evidence and an accept needs both bounds above the threshold, the set's bound raising
the anchor; abandonment latches as a reject; when a pool exists at shrink start the
anchor starts from a 20-replay measurement of the set; failing reruns live on no stored
timeline are captured into the pool. Five `branch` seeds before/after: seed 2001 went
from 66 abandoned candidates, 8167 executions and 1 timeline to 32 abandoned, 2100
executions and `(3, 3)` with anchor 0.91; seeds 2003–2005 likewise `(3, 3)`; seed 2002
stays at 1 timeline because its confirmation batch captured no t-branch run at all, so
the shrink started with nothing to anchor the second branch on (an information limit,
not a rule failure: the set it moved to reproduces at ~P(t) against a 0.6 original).

## Campaign 4 — `b466e146` (set-evidence gauntlet, latched abandonment, set anchor, capture)

| body | median disc. execs | timelines (count: episodes) | reuse | median reuse execs | reuse caveats (stored / confirmed) | blob replay | median replay execs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| racy | 6722 | 1: 18, 2: 2 | 20/20 | 2.5 | 17 / 3 | 60/60 | 1.5 |
| clone | 2388 | 1: 20 | 20/20 | 4 | 20 / 0 | 60/60 | 1 |
| branch | 2013 | 1: 7, 2: 13 | 20/20 | 2 | 20 / 0 | 60/60 | 1 |
| twobranch | 2539 | 4: 20 | 20/20 | 2 | 20 / 0 | 60/60 | 1 |

The first campaign with the whole design in place. `twobranch` stores all four failing
paths in every episode (campaign 1: two paths in 15/20) and its reuse is uniformly the
fast mode (20/20 "reproduced from stored timelines", median 2 executions; campaign 1:
4/20 fast, median 941). `branch` discovery costs 2013 executions instead of 5306 —
the abandoned-candidate churn is gone — and reuse is uniformly fast; 13/20 blobs carry
both branches. The 7/20 with one timeline are the episodes whose confirmation batch
captured no t-branch run, so the shrink began with a single-timeline anchor; the
`b466e146` follow-up (`re-measure the anchor whenever the pool grows`) targets those,
since captures during the shrink now grow the pool. `racy` pools are 1–2 timelines
(campaign 1: 1–7): the census drops schedule-specific captures that never serve again.
Reproduction remains 80/80 and 240/240.

## Campaign 5 — `74b5134c` (anchor re-measured when the pool grows)

Identical to campaign 4 on `branch` (1: 7, 2: 13; discovery 2013; reuse 2) and
`twobranch` (4: 20); `racy` pools 1: 20 with reuse 20/20 stored at a median of 3. The
re-measure changes nothing at this scale — the seven one-timeline `branch` episodes
have no pool to grow until the shrink is already committed to the t-only example.

## Campaign 6 — `e3d8ee1f` (every served timeline shrunk with the per-timeline shrinker)

| body | median disc. execs | timelines (count: episodes) | reuse | median reuse execs | blob replay | median replay execs |
| --- | --- | --- | --- | --- | --- | --- |
| racy | 8029 | 1: 20 | 20/20 | 3.5 | 60/60 | 1 |
| clone | 2388 | 1: 20 | 20/20 | 4 | 60/60 | 1 |
| branch | 4985 | 1: 7, 2: 13 | 20/20 | 2 | 60/60 | 1 |
| twobranch | 49 368 | 4: 20 | 20/20 | 2 | 60/60 | 1 |

The pass did what it should (in the engine test a captured `[T, 77, 91]` shrinks in
place while the shared prefix is kept) at a price nothing here pays for: discovery on
`twobranch` costs 20× campaign 4's executions (2544 → 49 368) and `branch` 2.5×
(2013 → 4985), reproduction and replay cost being unchanged. Each pool timeline gets a
full shrinker run, and a proposal for a branch-specific timeline lands on another branch
with probability 1 − p_branch — three quarters of proposals on `twobranch` — and counts
as a miss, so the passes churn. The recruiting run is a deterministic proposal replay
(the shrinker's ledger keys on its realization), so it cannot simply be made a set replay
with fallbacks: a candidate whose structure changed would look like a bounce. Reverted;
pool timelines stay as captured. The design problem left open is a set-aware recruiting
run that can tell "the test took another branch" from "the proposal misfits".

## Where this leaves the numbers

Against campaign 1 (decisions 74/75 as first built) the final tree (`b2ff7905`: `74b5134c`
plus the revert of `e3d8ee1f`) has: reproduction unchanged at the ceiling (reuse 80/80, blob 240/240); replay
cost one execution; `branch` discovery 5306 → 2013 executions and reuse uniformly the
fast mode (median 2, from a 13/7 split with a slow mode in the thousands); `twobranch`
pools covering all four failing paths in 20/20 episodes (from two in 15/20) and reuse
uniformly fast (median 2, from 941); `racy` reuse 20/20 fast (from 15/5).
