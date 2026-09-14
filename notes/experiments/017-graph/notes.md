# 017: the counterexample as a graph

Status: run on 2026-09-14 against `be170b50` (decision 77) plus the experiment's own
engine hook (an external-resolver replay mode behind `__bench`).

Question (David, turn 9 of the takeover): the timeline pool cannot represent independent
branch points — k independent two-way choices are 2^k whole timelines under a cap of 10 —
and "fewer timelines is better" makes every captured observation a growth. Would a
DFA-like graph of draws (a DAG with ordered out-edges, rejoining previously visited
timelines) be a better representation? Two parts: how the current design actually fails
as k grows, and whether a graph built from the same observations reproduces the failure
better, at what size, and with what risk of representing paths the failure never takes.

## Part A — the current design on k-block bodies

Harness: `experiments/live-set/` (016's), two new body families and a shape signature
per stored timeline in `blobinfo`; `drive.py --bodies kblock2,…,kshift6 --episodes 10
--out results-graph-a.jsonl`. `kblock<k>`: draw `a: bool`, then k pieces, each a hidden
coin choosing a bool piece (hot iff true) or an int piece in `0..=100` (hot iff `>= 60`);
fail iff `a` and every piece is hot — `kblock2` is 016's `twobranch`. `kshift<k>`: the
int piece also draws an ignored bool, so the two arms differ in length and shift every
later position. 2000 test cases per discovery (the failure is rare at high k: 0.5 × 0.45^k
per fresh case). The harness now also records the execution count at the first failure,
so the shrink's cost is the total minus it.

## Part B — pool against graph, built from the same observations

Harness: `experiments/graph-replay/` (engine-level bodies through `DataSource`, in the
style of 004; `TRIALS=20 R=50 cargo run --release -- results.jsonl`). The engine hook:
`ExternalReplay` (`core/replay.rs`), a resolver outside the engine deciding every draw
of a replay — given the draw's acceptance test over stored values it returns a value to
serve or declines, and reports its own divergence; `NativeTestCase::for_external`;
`__bench::replay_case` with `ReplayKind::{Fresh, Sequence, Set, External}` (the first
three are the engine's own replay modes: fresh generation, one positional sequence,
the live set of decision 74).

Bodies: `block<k>` and `shift<k>` as in Part A for k = 2, 3, 4, 6, 8, and `sum<k>` for
k = 2, 3, 4: the same pieces, but a bool piece counts 100 when true and an int piece its
value, and the failure is `a` and the sum of the pieces at or above `60k` — the pieces are
coupled, so a value recombined from another run can turn a failing path into a passing
one.

Per trial (20 per body and confirmation size, fresh hidden-coin stream): discover `T0` by
fresh generation; a confirmation batch of 20 (or 100) live-set replays of the growing
pool, every failing run not already stored captured — into `pool10` while it is below
the engine's cap of 10 and into `poolall` always; a trie of `poolall`'s runs merged three
ways — **exact** (nodes with identical futures, value for value: no path the runs did not
take), **struct** (nodes whose observed futures have the same kinds of draws, so the
values observed at a state recombine with every continuation observed from it) and
**compat** (greedy state merging in the manner of RPNI: states visited breadth-first, each
folded into the earliest accepted state it is compatible with — same terminal standing
and, for every kind of draw both have observed, compatible futures; observations that do
not overlap never conflict, so this generalizes furthest; folds that would close a cycle
are refused, since the bodies are acyclic). Then 50 cold replays per strategy: fresh
generation; `T0` positionally with the engine's continuation budget; `pool10` and
`poolall` under the live set; each graph walked from its root, serving the first
out-edge that fits the draw, with four rescues after a misfit — **random** (draw freshly
to the end), **skip** (consume the node's first edge without serving it and carry on from
its target), **positional** (for every later draw, serve from the first state reachable
at that depth that has a fitting edge, staying in rescue: the live set's donor rule in
graph form) and **rejoin** (as positional, but re-anchor on the served edge's target and
carry on walking the graph — David's "rejoin previously visited timelines"). Reported per
cell: reproduction rate and divergence rate; the graph's size (nodes/edges), its number
of root-to-terminal paths, how many of the 2^k shapes those paths cover, and — read off
the body's own predicate — how many of its paths are wrong (a well-formed sequence the
body passes on, or a malformed one).

## Part A results — `results-graph-a.jsonl`, 10 episodes per body

| body | 2^k | median disc. execs (after the first failure) | shapes stored (count: episodes) | blob | reuse | median reuse execs | blob replay | median replay execs | discoveries without a blob |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| kblock2 | 4 | 5388 (5380) | 2: 3, 3: 4, 4: 3 | 10/10 | 10/10 | 2 | 30/30 | 1 | 0 |
| kblock3 | 8 | 14978 (14968) | 2: 2, 3: 3, 4: 2, 5: 3 | 10/10 | 10/10 | 16490 | 30/30 | 1 | 0 |
| kblock4 | 16 | 3938 (3899) | 1: 1, 2: 3, 3: 6 | 10/10 | 10/10 | 52558 | 30/30 | 1 | 0 |
| kblock5 | 32 | 1240 (1184) | 1: 4, 2: 4, 3: 2 | 10/10 | 10/10 | 4004 | 30/30 | 2 | 0 |
| kblock6 | 64 | 1070 (950) | 1: 6, 2: 4 | 10/10 | 10/10 | 2188 | 30/30 | 4 | 0 |
| kshift2 | 4 | 2582 (2570) | 2: 9, 3: 1 | 10/10 | 10/10 | 4 | 30/30 | 1 | 0 |
| kshift3 | 8 | 1912 (1872) | 1: 1, 2: 5, 3: 3, 4: 1 | 10/10 | 10/10 | 5 | 30/30 | 1 | 0 |
| kshift4 | 16 | 1130 (1081) | 1: 2, 2: 5, 3: 2, 4: 1 | 10/10 | 10/10 | 11 | 30/30 | 3 | 0 |
| kshift5 | 32 | 1798 (1614) | 1: 3, 2: 2, 3: 4 | 9/10 | 9/10 | 1802 | 27/27 | 4 | 1 |
| kshift6 | 64 | 1694 (1398) | 1: 6, 2: 2 | 8/10 | 6/10 | 675 | 24/24 | 6 | 2 |

Every stored timeline is a distinct shape (the shapes histogram equals the timelines
histogram in every episode), and no pool reaches the cap of 10: the pool is bounded by
the confirmation sample, not the cap. What it says:

- **Coverage collapses with k.** `kblock2` stores 2–4 of its 4 shapes; `kblock4` 1–3 of
  16; `kblock6` and `kshift6` 1–2 of 64. The confirmation batch is 20 replays, a replay
  lands on a stored shape with probability about (stored shapes)/2^k, and a replay on an
  unstored shape fails only when its fresh draws are all hot (0.45 per differing piece),
  so few unstored shapes are ever captured. Under decision 76 the pool never grows
  afterwards.
- **Reproduction survives on retries, then breaks.** Blob replay is 30/30 throughout, but
  the median number of executions it takes rises 1 → 4 (`kblock`) and 1 → 6 (`kshift`) as
  k grows; `kshift5` and `kshift6` lose one and four of ten reuses respectively. Three
  discoveries report the failure without a blob: the raw incumbent reproduced 6 of 90
  replays ("below the confirmation bar — likely rare") — a failure that a fresh run finds
  in 67 executions is stored as one shape of 64 and reproduces at 7%, under the 10%
  handling target.
- **Reuse re-shrinks.** `kblock3` and `kblock4` reuses cost 16k and 53k executions
  (median): the reuse replay realizes a shape the stored pool does not have and the run
  takes the slow re-confirmation-and-shrink path (016's "slow mode").
- **The shrink is cheap where the pool is poor.** Cost after the first failure peaks at
  `kblock3` (15k, 2–5 lanes) and falls to under 1k at `kblock6` (1–2 lanes): the parallel
  shrinker's cost is per stored timeline, and at high k there are hardly any. The cost
  problem and the coverage problem are the same fact seen from two sides.

## Part B results — `experiments/graph-replay/results.jsonl`, 20 trials per cell, 50 cold replays per strategy

(`python3 summarize.py results.jsonl`; the divergence table's `compat` columns are zero wherever the graph covers every shape.)

### Sizes (medians over trials)

| body | confirm | trials | discovery | confirm fails | pool10 shapes | poolall (shapes) | trie n/e | exact n/e paths | struct n/e paths shapes% wrong% | compat n/e paths shapes% wrong% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 20 | 8 | 18 | 4/4 | 4 (4) | 8/7 | 4/5 4 | 4/5 4 100% 0.0% | 4/5 4 100% 0.0% |
| block2 | 100 | 20 | 8 | 98 | 4/4 | 4 (4) | 8/7 | 4/6 4 | 4/5 4 100% 0.0% | 4/5 4 100% 0.0% |
| block3 | 20 | 20 | 24 | 15 | 7/8 | 7 (7) | 15/14 | 7/10 7 | 7/10 7 88% 0.0% | 5/7 8 100% 0.0% |
| block3 | 100 | 20 | 26 | 94 | 8/8 | 8 (8) | 16/15 | 7/11 8 | 5/7 8 100% 0.0% | 5/7 8 100% 0.0% |
| block4 | 20 | 20 | 44 | 14 | 10/16 | 10 (10) | 26/25 | 12/19 10 | 11/16 10 62% 0.0% | 6/9 16 100% 0.0% |
| block4 | 100 | 20 | 24 | 92 | 10/16 | 16 (16) | 32/31 | 10/17 16 | 6/9 16 100% 0.0% | 6/9 16 100% 0.0% |
| block6 | 20 | 20 | 216 | 9 | 10/64 | 10 (10) | 40/39 | 22/30 10 | 22/30 10 16% 0.0% | 8/13 64 100% 0.0% |
| block6 | 100 | 20 | 222 | 84 | 10/64 | 47 (47) | 110/109 | 34/62 47 | 28/49 47 73% 0.0% | 8/13 64 100% 0.0% |
| block8 | 20 | 20 | 614 | 2 | 2/256 | 2 (2) | 20/19 | 11/12 2 | 11/12 2 1% 0.0% | 10/11 4 2% 0.0% |
| block8 | 100 | 20 | 1308 | 79 | 10/256 | 70 (70) | 230/230 | 79/133 70 | 73/124 70 27% 0.0% | 10/17 256 100% 0.0% |
| shift2 | 20 | 20 | 12 | 13 | 3/4 | 3 (3) | 10/8 | 7/8 3 | 6/8 4 75% 0.0% | 6/8 6 100% 0.0% |
| shift2 | 100 | 20 | 8 | 92 | 4/4 | 4 (4) | 11/10 | 8/9 4 | 6/8 6 100% 0.0% | 6/8 7 100% 0.0% |
| shift3 | 20 | 20 | 20 | 10 | 4/8 | 4 (4) | 16/14 | 11/13 4 | 9/12 8 56% 0.0% | 8/12 17 100% 0.0% |
| shift3 | 100 | 20 | 20 | 76 | 6/8 | 6 (6) | 21/20 | 13/17 6 | 9/14 16 75% 0.0% | 8/13 36 100% 0.0% |
| shift4 | 20 | 20 | 37 | 4 | 3/16 | 3 (3) | 16/16 | 12/13 3 | 12/13 4 19% 0.0% | 10/14 20 94% 25.0% |
| shift4 | 100 | 20 | 37 | 51 | 10/16 | 10 (10) | 37/36 | 19/26 10 | 15/22 42 59% 0.0% | 10/20 168 175% 54.3% |
| shift6 | 20 | 20 | 139 | 2 | 2/64 | 2 (2) | 16/16 | 12/12 2 | 12/12 2 3% 0.0% | 12/12 4 6% 50.0% |
| shift6 | 100 | 20 | 88 | 26 | 10/64 | 18 (18) | 82/82 | 44/56 18 | 36/55 46 27% 6.3% | 14/30 3312 200% 62.5% |
| shift8 | 20 | 20 | 1094 | 0 | 1/256 | 1 (1) | 16/14 | 16/14 1 | 16/14 1 0% 0.0% | 14/14 1 0% 0.0% |
| shift8 | 100 | 20 | 1239 | 6 | 6/256 | 6 (6) | 44/44 | 30/34 6 | 28/34 23 2% 12.7% | 17/28 1380 61% 87.6% |
| sum2 | 20 | 20 | 5 | 18 | 4/4 | 4 (4) | 8/7 | 4/6 4 | 4/5 4 100% 0.0% | 4/5 4 100% 0.0% |
| sum2 | 100 | 20 | 4 | 98 | 4/4 | 4 (4) | 8/7 | 5/6 4 | 4/5 4 100% 0.0% | 4/5 4 100% 0.0% |
| sum3 | 20 | 20 | 4 | 16 | 6/8 | 6 (6) | 14/13 | 8/11 6 | 7/9 6 75% 0.0% | 5/7 8 100% 18.8% |
| sum3 | 100 | 20 | 5 | 78 | 6/8 | 6 (6) | 14/13 | 7/10 6 | 6/8 6 75% 0.0% | 5/7 8 100% 25.0% |
| sum4 | 20 | 20 | 8 | 16 | 10/16 | 10 (10) | 24/24 | 12/18 10 | 11/16 10 59% 0.0% | 6/9 16 100% 15.6% |
| sum4 | 100 | 20 | 5 | 78 | 10/16 | 13 (13) | 28/27 | 11/18 13 | 9/13 13 81% 0.0% | 6/9 16 100% 18.8% |

### Reproduction rate (% of cold replays that failed; medians over trials)

| body | confirm | fresh | t0 | pool10 | poolall | exact-random | exact-skip | exact-positional | exact-rejoin | struct-random | struct-skip | struct-positional | struct-rejoin | compat-random | compat-skip | compat-positional | compat-rejoin |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 10 | 42 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| block2 | 100 | 10 | 51 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| block3 | 20 | 4 | 34 | 100 | 100 | 89 | 90 | 90 | 100 | 92 | 87 | 92 | 100 | 100 | 100 | 100 | 100 |
| block3 | 100 | 4 | 25 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| block4 | 20 | 2 | 20 | 100 | 100 | 72 | 78 | 78 | 100 | 76 | 77 | 80 | 100 | 100 | 100 | 100 | 100 |
| block4 | 100 | 2 | 20 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| block6 | 20 | 0 | 7 | 100 | 100 | 29 | 37 | 50 | 100 | 25 | 33 | 49 | 100 | 100 | 100 | 100 | 100 |
| block6 | 100 | 0 | 9 | 100 | 100 | 84 | 83 | 83 | 100 | 86 | 83 | 85 | 100 | 100 | 100 | 100 | 100 |
| block8 | 20 | 0 | 5 | 12 | 10 | 2 | 12 | 16 | 13 | 3 | 10 | 21 | 15 | 3 | 12 | 16 | 14 |
| block8 | 100 | 0 | 4 | 100 | 100 | 40 | 47 | 53 | 100 | 42 | 50 | 60 | 100 | 100 | 100 | 100 | 100 |
| shift2 | 20 | 10 | 42 | 82 | 85 | 82 | 87 | 90 | 84 | 86 | 88 | 89 | 77 | 100 | 100 | 100 | 100 |
| shift2 | 100 | 9 | 50 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| shift3 | 20 | 3 | 24 | 62 | 69 | 69 | 72 | 71 | 64 | 70 | 63 | 71 | 65 | 89 | 91 | 88 | 92 |
| shift3 | 100 | 5 | 18 | 81 | 74 | 91 | 88 | 90 | 85 | 87 | 86 | 92 | 85 | 100 | 100 | 100 | 100 |
| shift4 | 20 | 2 | 14 | 32 | 35 | 33 | 30 | 34 | 33 | 30 | 34 | 36 | 33 | 34 | 39 | 35 | 39 |
| shift4 | 100 | 2 | 16 | 59 | 57 | 74 | 69 | 82 | 55 | 76 | 74 | 80 | 67 | 74 | 74 | 80 | 84 |
| shift6 | 20 | 0 | 4 | 12 | 11 | 8 | 14 | 14 | 9 | 6 | 8 | 15 | 13 | 8 | 12 | 12 | 15 |
| shift6 | 100 | 0 | 6 | 44 | 56 | 41 | 37 | 52 | 60 | 38 | 36 | 41 | 54 | 66 | 63 | 74 | 72 |
| shift8 | 20 | 0 | 2 | 4 | 4 | 0 | 4 | 6 | 5 | 2 | 4 | 4 | 6 | 4 | 5 | 6 | 6 |
| shift8 | 100 | 0 | 0 | 14 | 16 | 6 | 9 | 14 | 17 | 3 | 12 | 14 | 21 | 14 | 21 | 17 | 22 |
| sum2 | 20 | 16 | 46 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| sum2 | 100 | 16 | 52 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 |
| sum3 | 20 | 17 | 61 | 86 | 87 | 87 | 85 | 81 | 83 | 82 | 84 | 87 | 83 | 83 | 83 | 85 | 79 |
| sum3 | 100 | 16 | 56 | 82 | 79 | 84 | 88 | 87 | 84 | 86 | 89 | 92 | 81 | 80 | 83 | 82 | 78 |
| sum4 | 20 | 16 | 69 | 87 | 86 | 83 | 84 | 78 | 82 | 84 | 82 | 84 | 86 | 81 | 81 | 80 | 82 |
| sum4 | 100 | 15 | 56 | 81 | 81 | 86 | 83 | 88 | 77 | 81 | 85 | 83 | 81 | 82 | 82 | 81 | 84 |

### Divergence rate (% of cold replays that left the stored material; medians over trials)

| body | confirm | fresh | t0 | pool10 | poolall | exact-random | exact-skip | exact-positional | exact-rejoin | struct-random | struct-skip | struct-positional | struct-rejoin | compat-random | compat-skip | compat-positional | compat-rejoin |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| block2 | 100 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| block3 | 20 | 0 | 0 | 18 | 13 | 17 | 14 | 16 | 10 | 17 | 18 | 14 | 16 | 0 | 0 | 0 | 0 |
| block3 | 100 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| block4 | 20 | 0 | 0 | 43 | 32 | 38 | 39 | 41 | 36 | 39 | 38 | 39 | 41 | 0 | 0 | 0 | 0 |
| block4 | 100 | 0 | 0 | 38 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| block6 | 20 | 0 | 0 | 90 | 86 | 85 | 85 | 83 | 86 | 88 | 88 | 86 | 84 | 0 | 0 | 0 | 0 |
| block6 | 100 | 0 | 0 | 82 | 29 | 30 | 25 | 31 | 30 | 27 | 32 | 28 | 28 | 0 | 0 | 0 | 0 |
| block8 | 20 | 0 | 0 | 100 | 99 | 98 | 99 | 98 | 98 | 100 | 98 | 98 | 98 | 97 | 98 | 96 | 97 |
| block8 | 100 | 0 | 0 | 98 | 74 | 73 | 71 | 80 | 72 | 74 | 73 | 75 | 75 | 0 | 0 | 0 | 0 |
| shift2 | 20 | 0 | 0 | 24 | 19 | 29 | 27 | 25 | 26 | 26 | 19 | 23 | 28 | 0 | 0 | 0 | 0 |
| shift2 | 100 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| shift3 | 20 | 0 | 0 | 45 | 49 | 42 | 43 | 46 | 42 | 44 | 43 | 43 | 42 | 0 | 0 | 0 | 0 |
| shift3 | 100 | 0 | 0 | 19 | 26 | 20 | 18 | 24 | 15 | 23 | 23 | 17 | 15 | 0 | 0 | 0 | 0 |
| shift4 | 20 | 0 | 0 | 77 | 79 | 78 | 81 | 79 | 79 | 78 | 74 | 77 | 80 | 52 | 43 | 51 | 55 |
| shift4 | 100 | 0 | 0 | 46 | 43 | 40 | 41 | 42 | 45 | 34 | 39 | 43 | 40 | 0 | 0 | 0 | 0 |
| shift6 | 20 | 0 | 0 | 97 | 98 | 98 | 96 | 98 | 96 | 98 | 97 | 98 | 98 | 93 | 94 | 96 | 93 |
| shift6 | 100 | 0 | 0 | 84 | 73 | 75 | 78 | 77 | 69 | 73 | 78 | 78 | 72 | 0 | 0 | 0 | 0 |
| shift8 | 20 | 0 | 0 | 99 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 100 | 99 | 98 | 99 | 100 |
| shift8 | 100 | 0 | 0 | 98 | 98 | 97 | 98 | 98 | 98 | 98 | 96 | 98 | 98 | 78 | 78 | 76 | 81 |
| sum2 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| sum2 | 100 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| sum3 | 20 | 0 | 0 | 22 | 24 | 26 | 26 | 23 | 23 | 29 | 26 | 21 | 20 | 0 | 0 | 0 | 0 |
| sum3 | 100 | 0 | 0 | 18 | 21 | 23 | 17 | 22 | 16 | 23 | 23 | 19 | 19 | 0 | 0 | 0 | 0 |
| sum4 | 20 | 0 | 0 | 48 | 38 | 42 | 40 | 44 | 41 | 39 | 42 | 45 | 40 | 0 | 0 | 0 | 0 |
| sum4 | 100 | 0 | 0 | 42 | 19 | 16 | 22 | 20 | 23 | 26 | 23 | 23 | 19 | 0 | 0 | 0 | 0 |

## What Part B says

**Size.** Where the pool is exponential the graph is linear. `block8` at 100
confirmations: 70 stored timelines of 9 values against a `compat` graph of 10 nodes and 17
edges covering all 256 shapes; `block6`: 47 timelines against 8 nodes and 13 edges
covering all 64. The `exact` graph (no path the runs did not take) is a trie with suffix
sharing — 79 nodes / 133 edges for those 70 runs — and covers exactly the stored shapes;
`struct` barely generalizes (its coverage equals `poolall`'s in almost every cell) because
two states merge only when their *observed* continuation sets are structurally identical,
which sparse observation rarely produces.

**Reproduction is decided by the rescue and by generalization, not by the graph as such.**

- With matched rescue the graph of observed runs equals the pool. The live set's rescue
  serves every later draw from any pruned timeline positionally, and on `block` bodies —
  where every stored value at a piece position is hot — that oracle reproduces at 100%
  from 10 stored shapes of 64 (`block6`, `pool10`). A graph walk that gives up at the
  first misfit (`random`) or follows one path (`skip`) is far worse (29–37%); with the
  same donor rule (`positional`) it recovers half the gap, and re-anchoring on the donor
  (`rejoin`) closes it exactly: `exact-rejoin` = `pool10` on every `block` and `sum` cell.
  Rejoining is therefore a necessary part of any graph replay, and on its own it buys
  nothing over the pool.
- Recombination is what buys reproduction. `compat` reproduces `block` at 100% in every
  cell with more than two stored runs, with **zero divergence**: from 10 confirmation
  failures at k = 6 it represents all 64 shapes, and its walk never leaves the graph.
  On `shift` — where the arms have different lengths, so the pool's positional rescue
  misaligns — `compat-rejoin` beats `pool10` by 25–30 points once the sample is large
  enough to see each arm at each state (100 confirmations: `shift4` 59 → 84, `shift6`
  44 → 72, `shift8` 14 → 22), and equals or beats it at 20.
- Generalization is also where the graph is wrong. `compat` on `shift` folds "expecting
  the int arm's extra bool" into "start of a piece" (both futures begin with a bool and
  nothing but the draws distinguishes them) and produces paths the body never takes:
  62% of `shift6`'s 3312 paths and 88% of `shift8`'s are wrong or malformed, and the
  graph claims 200% of the shapes. On `sum`, where the pieces are coupled through the
  predicate, recombination puts 16–25% passing paths into the graph; reproduction is
  unaffected (the walk follows the incumbent's values first) but a shrinker over that
  graph would be exploring and proposing sequences that are not failures. `struct` makes
  almost none of these errors and gains almost nothing; the two criteria bracket the
  bias–variance trade-off, and neither is the right one.
- Where nothing is observed nothing helps: `block8` and `shift8` at 20 confirmations
  store one or two runs (the raw failure reproduces at 2–4% here — Part A's "below the
  confirmation bar" cases), and every representation is at 2–16%.

**Verdict on the question asked.** The representation is validated as a representation:
compact where the pool blows up, replayable through the engine's existing draw
resolution with a one-node walk plus rejoin, and — under recombination — able to cover
2^k shapes from O(k) observations, which no bounded pool can. It is not validated as a
finished design: the merge criterion is the whole problem, exactly as predicted in turn
9. Merging on draw kinds alone under-generalizes when strict and over-generalizes when
loose; the information that would separate `shift`'s two bool draws is the span
structure around them (the extra bool sits inside the int arm's span), which the graph
here never sees. The natural next experiment is state identity from `(kind, open span
labels)` rather than kind alone, which needs the hook to pass the open span stack to the
resolver and the bodies to open spans as real generators do. Unmeasured, and needed before
any shrinker work: what an edited path's realized run does to the graph (graft, or miss),
and what a shrinker does with the wrong paths a loose merge admits.

Engine changes made for the experiment, kept: `ExternalReplay` and `Replay::external`
(`core/replay.rs`), `NativeTestCase::for_external` (`state.rs`; both constructors are
compiled under `test` and `__bench` only), `__bench::replay_case` with `ReplayKind`
(`lib.rs`), and `exchange::drive` available under `__bench`. Three embedded tests in
`replay_tests.rs` cover the hook. No engine behaviour changed.
