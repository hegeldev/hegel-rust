# 018: shrinking the counterexample as a graph

Status: campaign 1 run on 2026-09-14, campaign 2 on 2026-09-16, both against `e08d1000`
(017's engine hook, no engine change).

Question (David, turn 12 of the takeover): 017 validated the graph as a *representation* —
compact where the pool is exponential, replayable through the engine's draw resolution,
able to cover 2^k shapes from O(k) observations under recombination. Its open problem
was the merge criterion, and its unmeasured part was the shrinker. David's priority is
the shrinker: can shrinking be made to work on this representation at all, before any
work on state identity. This experiment builds a prototype graph shrinker outside the
engine and measures what it reaches, at what cost, and what goes wrong.

## Setup

Harness: `experiments/graph-shrink/` (`TRIALS=10 KS=20 R=50 cargo run --release --
results.jsonl`; `python3 summarize.py results.jsonl`). Bodies as 017: `block<k>`,
`shift<k>` for k = 2, 3, 4, 6, 8 and `sum<k>` for k = 2, 3, 4; per trial a fresh
hidden-coin stream, discovery of `T0` by fresh generation, a confirmation batch of 20 or
100 live-set replays of the growing pool with every failing run captured (`poolall`).

**Starting graphs**, three per trial: `t0` (the single discovery run — no confirmation at
all), `exact` (017's exact merge of the captured runs: a trie with suffix sharing, no path
the runs did not take) and `compat` (017's RPNI-style compatible merge, which
recombines and over-generalizes).

**The shrinker.** Greedy passes over four graph edits until a pass accepts nothing and
learns nothing (cap 20 passes, 100k executions):

- *delete* an out-edge (a second edge with the same value as an earlier one at its node
  is dead under first-fit replay and is removed free);
- *contract* a node into one of its successors: every edge into the node is redirected
  to the successor and the node's own edges vanish — the graph form of deleting a span,
  which removes a whole diamond at once;
- *merge* two nodes that occur at the same set of depths, have the same terminal
  standing and are not related by reachability (so the graph stays acyclic): the later
  takes the earlier's identity, edges concatenated — the judged form of 017's offline
  merge;
- *value*: an edge's boolean `true → false`, an integer to 0 then by binary search
  towards it.

The order is (edges, nodes, edge values in breadth-first order by shrink rank); a
candidate must be strictly smaller. A candidate is **judged** by replaying it K = 20 times
through `ExternalReplay` (017's walk with the rejoin rescue), stopping at the first replay
that does not count, so rejections are cheap and acceptances cost K. Three judges:

- **strict** — every replay fails, never leaves the graph (no misfit) and ends on a
  terminal node. A graph that does not itself pass this cannot shrink under it.
- **lenient** — every replay fails; rescue allowed. The pool's judging rule today.
- **learn** — strict, plus: the realized run of a failing replay that left the graph is
  *grafted* into the current graph if the graph cannot walk a run of that shape. The graft
  follows the run while it fits and, where it departs, links to a state at the same depth
  from which the rest of the run is already a path (the rejoin the rescue made, made
  permanent); only where no such state exists are new nodes appended.

Two rules found necessary during development (both recorded in the results as counts).
**Exercise**: an edit other than a deletion is accepted only if some judging replay
actually served an edited edge — the changed value, or one of the edges redirected by a
contraction or a merge (`unexercised` otherwise). Without it the shrinker accepted
untested edits on branches the first-fit walk never takes: a value below the threshold
that a later structural move exposed as a passing path (after which nothing could be
accepted), and — the larger effect — contractions of never-walked grafted branches,
which shortened them; merges then folded the shortened branches into the walked chain,
and the final graph, though it replayed perfectly, claimed 50% more paths than the
failure has, a third of them of the wrong length. Deletion needs no exercise: removing
material no walk uses is exactly right. **Redundancy**: a graft is skipped when the graph
already walks the run's shape (`redundant`) — the rescue's donors are recombinations, and
grafting them re-added material the incumbent already covered, in an endless
learn/delete churn.

Added for campaign 2 (`WARMUP`, default 3 rounds; campaign 1 is `WARMUP=0`): under
`learn`, before the first pass and again whenever a pass would otherwise end the shrink,
the current graph is replayed K times *without* stopping early and every failing run it
did not contain is grafted, repeated while a round grafts something. The reason is under
campaign 1's results.

Also tried and dropped: K = 5 (false accepts in every cell: wrong paths in the final
graph and cold reproduction near 50%), and learn on top of lenient (lenient accepts the
deletion of an arm that the rescue covers, learn grafts the arm back: hundreds of grafts
at k = 2).

Measured per shrink: executions, passes, whether the start passed its own judge
(`start ok`), the final graph's nodes/edges against the **ideal** (the chain of k
diamonds with one bool arm and one int arm each: k + 2 nodes, 2k + 1 edges; for `shift`
2k + 2 nodes, 3k + 1 edges), its paths and how many of the 2^k shapes they cover, the
share of paths that are wrong (the body passes on them, or no run produces them), the
largest size the graph reached during the shrink (`max n/e`), grafts (`learned`),
accepted moves by kind, and 50 cold replays of the result: reproduction rate and the
rate of failures that never left the graph (`clean`).

## Results

Two campaigns. Campaign 1 is the design above as written (`WARMUP=0`; 260 trials, 10 per
body × confirmation size, every trial found its failure). Its `learn` rows showed one
artefact of the judge, described under campaign 1, which campaign 2 removes with a warm-up;
everything else is read from campaign 1 and confirmed by campaign 2.

### Campaign 1: as designed

**Reading the tables.** `start ok` counts trials whose starting graph passed its own judge
(K = 20 replays). `shapes%` is distinct draw-kind sequences among the enumerated paths over
the body's 2^k well-formed shapes, so a value over 100% means paths of shapes the body never
produces. `wrong%` is the share of enumerated paths on which the body passes or which no
run produces. `cold fail% / clean%` is 50 fresh replays of the final graph: the share that
failed, and the share that failed without the rescue.

**From `t0` (one run, no confirmation).** `strict` and `lenient` accept nothing at any k:
a single path has nothing to delete or contract, and every value edit is judged by 20
replays of which the first to diverge and pass rejects it. The result is the discovery run
itself, reproducing at its own rate (block2 53%, block8 8%). `learn` turns the single run
into the ideal graph on every `block` body — block8: 10/17 nodes/edges, all 256 shapes, no
wrong path, 50/50 cold replays failing and none leaving the graph — for a median 474 /
1102 / 2084 / 7702 / 17654 executions at k = 2, 3, 4, 6, 8 (confirm 20; confirm 100 is
the same, `t0` does not see the confirmation). Per-trial the outcome is bimodal: a trial
either reaches the ideal or grafts nothing and ends after one pass (block6: 1/10 and
2/10 trials; block8: 1/10 and 3/10; shift6: 2/10 and 6/10, hence the shift6 medians of
61 grafts and 0). The cause is the judge's early stop: the start is judged by replays
that stop at the first one that does not count, so at high k the first replay diverges,
passes, and the judge stops after one execution having grafted nothing; the pass that
follows rejects every candidate on its first replay in the same way; a pass that neither
accepted nor learned ends the shrink. The bootstrap is starved by the very rule that
makes rejections cheap. Campaign 2 adds a warm-up for this. On `shift`, `learn` reaches
all shapes at k ≤ 4 but above the ideal size (shift4/20: 20/22 against 10/13), and from
k = 6 it builds wrong paths in about half the trials that take off (shift6/20 per trial:
28, 40, 56 and 118 wrong paths in four of eight); at k = 8 the medians are 304–394 paths,
12–34% wrong, cold 51–72%. On `sum` it reaches all shapes with 12–23% wrong paths and
78–86% cold reproduction.

**From `exact` (017's merge of the confirmation's failing runs).** `strict` cannot start
from a 20-run confirmation at k ≥ 4 (start ok 0/10 from block4/20 up): the graph covers
few shapes, a replay of any other shape leaves it, and under `strict` that replay does not
count, so nothing is ever accepted and the start is returned as is (block6/100: 31/54,
46 paths, cold 100/63). Where the start does pass (confirm 100, k ≤ 4) `strict` reaches
the ideal (block4/100: 6/9 in 1434 executions). `lenient` — today's pool rule — shrinks
from any start and reaches the ideal on block6/100 (8/13 in 1445 executions), but it also
accepts the deletion or contraction of an arm the rescue then covers, and the result is a
graph that reproduces worse than it claims: block6/20 9/11, 5 paths, 33% wrong, cold
48/5; block4/100 6/8, 8 paths, 50% wrong, cold 100/26; block8/100 11/16, 40 paths, 33%
wrong, cold 94/13. `learn` reaches the ideal on every `block` cell (block8/100: 78/132 →
10/17 in 21.5k executions, 64 grafts; all ten trials, no stall from this start) and on
`shift` shows the same wrong paths as from `t0` (shift6/100: 74 paths, 20% wrong at the
median; up to 170 in a trial).

**From `compat` (017's recombining merge).** On `block` with 100 confirmations the start
is already the ideal shape and all three judges only shrink values: block8/100 in 744–808
executions, 2 passes, 21 value accepts, cold 100/100 — the cheap path, when the start is
right. With 20 confirmations at k = 8 the start has 2 paths (017: coverage collapses) and
only `learn` recovers (15.5k). On `shift` the start's wrong paths survive: `strict` accepts
nothing and returns them (shift4/100: 164 paths, 53% wrong; shift8/100: 7380 paths, 95%
wrong, cold 50/6), `lenient` deletes down to 33 paths that are 97% wrong, and `learn`
ends at 1827 paths 86% wrong with **99/99 cold reproduction**. On `sum` the start's
12–25% wrong paths persist under every judge.

**The stall, per trial.** `t0`-`learn`, campaign 1: grafts / executions / shapes / wrong
paths / failing cold replays of 50, one entry per trial.

| cell | trials |
| --- | --- |
| block6/20 | 39/8472/64/0/50 27/6994/64/0/50 25/5618/64/0/50 38/8763/64/0/50 50/11491/64/0/50 **0/35/1/0/7** 25/7838/64/0/50 42/8144/64/0/50 31/7567/64/0/50 32/6022/64/0/50 |
| block6/100 | 28/7116/64/0/50 33/7320/64/0/50 **0/15/1/0/9** 38/10895/64/0/50 36/10243/64/0/50 30/7606/64/0/50 32/7064/64/0/50 **0/22/1/0/6** 36/8072/64/0/50 44/8872/64/0/50 |
| block8/20 | 75/14934/256/0/50 59/12114/256/0/50 88/20537/256/0/50 73/16851/256/0/50 89/18044/256/0/50 87/18372/256/0/50 **0/40/1/0/3** 81/23996/256/0/50 88/17861/256/0/50 80/17448/256/0/50 |
| block8/100 | **0/33/1/0/1** 83/17970/256/0/50 77/16880/256/0/50 61/15255/256/0/50 75/17988/256/0/50 83/16545/256/0/50 **0/46/1/0/5** 80/22085/256/0/50 **0/19/1/0/3** 76/17333/256/0/50 |
| shift6/20 | **0/32/1/0/4** 56/20243/64/0/50 101/46947/82/28/45 73/27384/64/0/50 43/16624/64/0/50 50/17321/104/40/50 110/40024/104/56/50 **0/40/1/0/4** 66/15212/182/118/50 74/28311/60/0/49 |
| shift6/100 | **0/40/1/0/4** 57/18150/64/0/50 **0/15/1/0/3** **0/33/1/0/4** **0/47/1/0/4** 97/34142/119/66/44 **0/41/1/0/6** 32/7897/51/3/41 **0/30/1/0/1** 86/28418/98/42/47 |

Full tables (`python3 summarize.py results.jsonl`):

### Starting graphs (medians over trials)

| body | confirm | trials | ideal n/e | poolall | t0 cold fail% / clean% | exact n/e paths wrong% | exact cold fail% / clean% | compat n/e paths wrong% | compat cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 10 | 4/5 | 4 | 49 / 25 | 5/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| block2 | 100 | 10 | 4/5 | 4 | 47 / 27 | 5/7 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| block3 | 20 | 10 | 5/7 | 6 | 38 / 15 | 8/11 6 0% | 100 / 77 | 5/7 8 0% | 100 / 100 |
| block3 | 100 | 10 | 5/7 | 8 | 38 / 13 | 7/11 8 0% | 100 / 100 | 5/7 8 0% | 100 / 100 |
| block4 | 20 | 10 | 6/9 | 8 | 29 / 6 | 12/17 8 0% | 100 / 49 | 6/9 16 0% | 100 / 100 |
| block4 | 100 | 10 | 6/9 | 16 | 26 / 6 | 11/18 16 0% | 100 / 100 | 6/9 16 0% | 100 / 100 |
| block6 | 20 | 10 | 8/13 | 8 | 13 / 0 | 19/24 8 0% | 100 / 13 | 8/13 64 0% | 100 / 100 |
| block6 | 100 | 10 | 8/13 | 47 | 10 / 0 | 34/62 47 0% | 100 / 74 | 8/13 64 0% | 100 / 100 |
| block8 | 20 | 10 | 10/17 | 2 | 10 / 0 | 10/10 2 0% | 7 / 0 | 10/10 2 0% | 13 / 2 |
| block8 | 100 | 10 | 10/17 | 68 | 4 / 0 | 78/132 68 0% | 100 / 27 | 10/17 256 0% | 100 / 100 |
| shift2 | 20 | 10 | 6/7 | 3 | 45 / 28 | 7/8 3 0% | 83 / 76 | 6/8 7 0% | 100 / 100 |
| shift2 | 100 | 10 | 6/7 | 4 | 43 / 27 | 8/10 4 0% | 100 / 100 | 6/8 9 0% | 100 / 100 |
| shift3 | 20 | 10 | 8/10 | 3 | 26 / 14 | 8/10 3 0% | 49 / 40 | 8/9 6 0% | 50 / 44 |
| shift3 | 100 | 10 | 8/10 | 7 | 29 / 12 | 12/17 7 0% | 91 / 91 | 8/14 32 0% | 100 / 100 |
| shift4 | 20 | 10 | 10/13 | 4 | 24 / 7 | 13/16 4 0% | 43 / 28 | 10/14 22 0% | 65 / 49 |
| shift4 | 100 | 10 | 10/13 | 10 | 17 / 9 | 20/26 10 0% | 62 / 62 | 10/20 321 53% | 82 / 77 |
| shift6 | 20 | 10 | 14/19 | 2 | 6 / 2 | 14/14 2 0% | 21 / 5 | 12/13 6 50% | 11 / 5 |
| shift6 | 100 | 10 | 14/19 | 14 | 6 / 0 | 32/41 14 0% | 49 / 21 | 14/31 2952 79% | 75 / 35 |
| shift8 | 20 | 10 | 18/25 | 2 | 3 / 0 | 15/14 2 0% | 4 / 0 | 14/14 2 25% | 5 / 1 |
| shift8 | 100 | 10 | 18/25 | 6 | 2 / 0 | 38/42 6 0% | 16 / 2 | 18/32 7380 95% | 41 / 7 |
| sum2 | 20 | 10 | 4/5 | 4 | 61 / 25 | 4/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| sum2 | 100 | 10 | 4/5 | 4 | 64 / 25 | 5/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| sum3 | 20 | 10 | 5/7 | 6 | 53 / 14 | 7/10 6 0% | 96 / 76 | 5/7 8 6% | 97 / 97 |
| sum3 | 100 | 10 | 5/7 | 6 | 58 / 14 | 7/10 6 0% | 78 / 78 | 5/7 8 25% | 77 / 77 |
| sum4 | 20 | 10 | 6/9 | 9 | 62 / 6 | 12/18 9 0% | 88 / 57 | 6/9 16 12% | 85 / 85 |
| sum4 | 100 | 10 | 6/9 | 13 | 65 / 8 | 12/19 13 0% | 82 / 82 | 6/9 16 16% | 86 / 86 |

### Shrinking from `t0`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 53 / 23 |
| block2 | 20 | lenient | 0/10 | 21 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 46 / 24 |
| block2 | 20 | learn | 0/10 | 474 | 4 | 4/5 | 4/5 | 4 | 100% | 0% | 6/7 | 3 | 0/0/2/6 | 100 / 100 |
| block2 | 100 | strict | 0/10 | 11 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 52 / 26 |
| block2 | 100 | lenient | 0/10 | 18 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 49 / 25 |
| block2 | 100 | learn | 0/10 | 520 | 4 | 4/5 | 4/5 | 4 | 100% | 0% | 7/7 | 3 | 1/0/2/6 | 100 / 100 |
| block3 | 20 | strict | 0/10 | 20 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 40 / 14 |
| block3 | 20 | lenient | 0/10 | 26 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 33 / 12 |
| block3 | 20 | learn | 0/10 | 1102 | 5 | 5/7 | 5/7 | 8 | 100% | 0% | 10/14 | 7 | 2/0/3/12 | 100 / 100 |
| block3 | 100 | strict | 0/10 | 21 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 34 / 14 |
| block3 | 100 | lenient | 0/10 | 26 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 37 / 10 |
| block3 | 100 | learn | 0/10 | 890 | 5 | 5/7 | 5/7 | 8 | 100% | 0% | 10/12 | 6 | 1/0/3/10 | 100 / 100 |
| block4 | 20 | strict | 0/10 | 22 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 26 / 6 |
| block4 | 20 | lenient | 0/10 | 28 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 25 / 5 |
| block4 | 20 | learn | 0/10 | 2084 | 6 | 6/9 | 6/9 | 16 | 100% | 0% | 14/23 | 11 | 4/0/4/17 | 100 / 100 |
| block4 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 24 / 5 |
| block4 | 100 | lenient | 0/10 | 23 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 24 / 4 |
| block4 | 100 | learn | 0/10 | 2496 | 6 | 6/9 | 6/9 | 16 | 100% | 0% | 16/26 | 12 | 5/0/4/26 | 100 / 100 |
| block6 | 20 | strict | 0/10 | 29 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 14 / 0 |
| block6 | 20 | lenient | 0/10 | 31 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 12 / 2 |
| block6 | 20 | learn | 0/10 | 7702 | 8 | 8/13 | 8/13 | 64 | 100% | 0% | 31/54 | 32 | 16/0/6/45 | 100 / 100 |
| block6 | 100 | strict | 0/10 | 32 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 11 / 1 |
| block6 | 100 | lenient | 0/10 | 36 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 13 / 0 |
| block6 | 100 | learn | 0/10 | 7463 | 8 | 8/13 | 8/13 | 64 | 100% | 0% | 31/52 | 32 | 17/0/6/54 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 8 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 7 / 0 |
| block8 | 20 | learn | 0/10 | 17654 | 10 | 10/17 | 10/17 | 256 | 100% | 0% | 57/104 | 80 | 36/0/8/96 | 100 / 100 |
| block8 | 100 | strict | 0/10 | 34 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 7 / 0 |
| block8 | 100 | lenient | 0/10 | 35 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 4 / 0 |
| block8 | 100 | learn | 0/10 | 16712 | 10 | 10/17 | 10/17 | 256 | 100% | 0% | 55/102 | 76 | 35/0/8/101 | 100 / 100 |
| shift2 | 20 | strict | 0/10 | 18 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 48 / 23 |
| shift2 | 20 | lenient | 0/10 | 21 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 42 / 21 |
| shift2 | 20 | learn | 0/10 | 492 | 4 | 7/7 | 6/7 | 4 | 100% | 0% | 10/10 | 3 | 0/1/1/7 | 100 / 100 |
| shift2 | 100 | strict | 0/10 | 16 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 47 / 25 |
| shift2 | 100 | lenient | 0/10 | 21 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 46 / 28 |
| shift2 | 100 | learn | 0/10 | 406 | 3 | 7/7 | 6/7 | 4 | 100% | 0% | 10/10 | 3 | 0/0/1/6 | 100 / 100 |
| shift3 | 20 | strict | 0/10 | 18 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 27 / 12 |
| shift3 | 20 | lenient | 0/10 | 19 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 25 / 12 |
| shift3 | 20 | learn | 0/10 | 1368 | 4 | 12/13 | 8/10 | 8 | 100% | 0% | 18/19 | 6 | 1/1/2/14 | 100 / 100 |
| shift3 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 29 / 14 |
| shift3 | 100 | lenient | 0/10 | 22 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 26 / 9 |
| shift3 | 100 | learn | 0/10 | 2196 | 4 | 13/14 | 8/10 | 8 | 100% | 0% | 20/23 | 8 | 2/1/2/16 | 100 / 100 |
| shift4 | 20 | strict | 0/10 | 26 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 0/0/0/0 | 18 / 5 |
| shift4 | 20 | lenient | 0/10 | 28 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 0/0/0/0 | 15 / 4 |
| shift4 | 20 | learn | 0/10 | 4385 | 6 | 20/22 | 10/13 | 16 | 100% | 0% | 34/42 | 18 | 6/2/3/44 | 99 / 99 |
| shift4 | 100 | strict | 0/10 | 20 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 0/0/0/0 | 18 / 7 |
| shift4 | 100 | lenient | 0/10 | 28 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 0/0/0/0 | 17 / 6 |
| shift4 | 100 | learn | 0/10 | 2200 | 4 | 12/15 | 10/13 | 14 | 88% | 0% | 29/34 | 12 | 2/1/2/12 | 93 / 93 |
| shift6 | 20 | strict | 0/10 | 37 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 0 |
| shift6 | 20 | lenient | 0/10 | 38 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 0 |
| shift6 | 20 | learn | 0/10 | 18782 | 12 | 22/28 | 14/19 | 64 | 100% | 0% | 76/98 | 61 | 24/6/6/84 | 100 / 100 |
| shift6 | 100 | strict | 0/10 | 36 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 9 / 2 |
| shift6 | 100 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 2 |
| shift6 | 100 | learn | 0/10 | 44 | 1 | 12/10 | 14/19 | 1 | 2% | 0% | 12/10 | 0 | 0/0/0/0 | 10 / 3 |
| shift8 | 20 | strict | 0/10 | 48 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | lenient | 0/10 | 50 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | learn | 0/10 | 56976 | 16 | 40/56 | 18/25 | 304 | 105% | 12% | 148/196 | 164 | 60/8/10/158 | 72 / 72 |
| shift8 | 100 | strict | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 0/0/0/0 | 2 / 0 |
| shift8 | 100 | lenient | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 0/0/0/0 | 2 / 0 |
| shift8 | 100 | learn | 0/10 | 52 | 1 | 14/13 | 18/25 | 1 | 0% | 0% | 14/13 | 0 | 0/0/0/0 | 5 / 0 |
| sum2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 58 / 21 |
| sum2 | 20 | lenient | 0/10 | 23 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 64 / 28 |
| sum2 | 20 | learn | 0/10 | 308 | 3 | 4/5 | 4/5 | 4 | 100% | 0% | 6/7 | 3 | 0/0/2/3 | 100 / 100 |
| sum2 | 100 | strict | 0/10 | 14 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 59 / 28 |
| sum2 | 100 | lenient | 0/10 | 20 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 59 / 24 |
| sum2 | 100 | learn | 0/10 | 758 | 3 | 4/6 | 4/5 | 4 | 100% | 0% | 9/10 | 4 | 2/0/2/4 | 100 / 100 |
| sum3 | 20 | strict | 0/10 | 20 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 58 / 16 |
| sum3 | 20 | lenient | 0/10 | 35 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 54 / 11 |
| sum3 | 20 | learn | 0/10 | 1024 | 4 | 9/12 | 5/7 | 8 | 100% | 12% | 12/17 | 8 | 0/0/2/4 | 85 / 85 |
| sum3 | 100 | strict | 0/10 | 25 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 65 / 13 |
| sum3 | 100 | lenient | 0/10 | 40 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 55 / 12 |
| sum3 | 100 | learn | 0/10 | 1711 | 4 | 7/9 | 5/7 | 8 | 100% | 12% | 14/18 | 8 | 0/0/2/6 | 86 / 86 |
| sum4 | 20 | strict | 0/10 | 24 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 62 / 6 |
| sum4 | 20 | lenient | 0/10 | 51 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 60 / 4 |
| sum4 | 20 | learn | 0/10 | 2650 | 4 | 12/19 | 6/9 | 21 | 100% | 23% | 16/24 | 12 | 1/0/2/14 | 82 / 82 |
| sum4 | 100 | strict | 0/10 | 19 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 64 / 7 |
| sum4 | 100 | lenient | 0/10 | 46 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 64 / 8 |
| sum4 | 100 | learn | 0/10 | 4163 | 6 | 11/18 | 6/9 | 22 | 100% | 19% | 18/30 | 14 | 2/0/2/16 | 78 / 78 |

### Shrinking from `exact`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 8/10 | 208 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/1/4 | 100 / 100 |
| block2 | 20 | lenient | 10/10 | 254 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 2/0/1/4 | 100 / 100 |
| block2 | 20 | learn | 8/10 | 225 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/4 | 100 / 100 |
| block2 | 100 | strict | 10/10 | 215 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/5 | 100 / 100 |
| block2 | 100 | lenient | 10/10 | 237 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 2/0/1/5 | 100 / 100 |
| block2 | 100 | learn | 10/10 | 211 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/5 | 100 / 100 |
| block3 | 20 | strict | 1/10 | 656 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 8/11 | 0 | 0/0/2/8 | 100 / 100 |
| block3 | 20 | lenient | 10/10 | 441 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 8/11 | 0 | 3/0/2/6 | 100 / 100 |
| block3 | 20 | learn | 1/10 | 948 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 8/13 | 1 | 2/0/2/10 | 100 / 100 |
| block3 | 100 | strict | 10/10 | 728 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/2/8 | 100 / 100 |
| block3 | 100 | lenient | 10/10 | 350 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/1/5 | 100 / 100 |
| block3 | 100 | learn | 10/10 | 739 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/2/8 | 100 / 100 |
| block4 | 20 | strict | 0/10 | 190 | 1 | 11/16 | 6/9 | 8 | 53% | 0% | 12/17 | 0 | 0/0/0/0 | 100 / 51 |
| block4 | 20 | lenient | 8/10 | 708 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/17 | 0 | 4/0/3/10 | 100 / 100 |
| block4 | 20 | learn | 0/10 | 2122 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/21 | 4 | 4/0/3/20 | 100 / 100 |
| block4 | 100 | strict | 10/10 | 1434 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 2/0/2/18 | 100 / 100 |
| block4 | 100 | lenient | 10/10 | 502 | 2 | 6/8 | 6/9 | 8 | 50% | 50% | 11/18 | 0 | 4/1/1/9 | 100 / 26 |
| block4 | 100 | learn | 10/10 | 1617 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/19 | 1 | 3/0/2/18 | 100 / 100 |
| block6 | 20 | strict | 0/10 | 169 | 1 | 19/24 | 8/13 | 8 | 12% | 0% | 19/24 | 0 | 0/0/0/0 | 100 / 12 |
| block6 | 20 | lenient | 6/10 | 830 | 3 | 9/11 | 8/13 | 5 | 8% | 33% | 19/24 | 0 | 2/2/1/9 | 48 / 5 |
| block6 | 20 | learn | 0/10 | 6030 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 24/44 | 22 | 12/0/5/44 | 100 / 100 |
| block6 | 100 | strict | 0/10 | 2419 | 2 | 31/54 | 8/13 | 46 | 68% | 0% | 34/62 | 0 | 0/0/0/0 | 100 / 63 |
| block6 | 100 | lenient | 10/10 | 1445 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 34/62 | 0 | 9/0/5/16 | 100 / 100 |
| block6 | 100 | learn | 0/10 | 8988 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 34/67 | 18 | 22/0/4/71 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 9 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 12 / 0 |
| block8 | 20 | learn | 0/10 | 17662 | 10 | 10/17 | 10/17 | 256 | 100% | 0% | 58/106 | 74 | 36/0/10/105 | 100 / 100 |
| block8 | 100 | strict | 0/10 | 1860 | 1 | 78/132 | 10/17 | 68 | 26% | 0% | 78/132 | 0 | 0/0/0/0 | 100 / 27 |
| block8 | 100 | lenient | 10/10 | 1944 | 6 | 11/16 | 10/17 | 40 | 16% | 33% | 78/132 | 0 | 9/2/4/21 | 94 / 13 |
| block8 | 100 | learn | 0/10 | 21512 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 79/152 | 64 | 54/0/7/138 | 100 / 100 |
| shift2 | 20 | strict | 4/10 | 111 | 1 | 6/7 | 6/7 | 3 | 75% | 0% | 7/8 | 0 | 0/0/0/0 | 80 / 76 |
| shift2 | 20 | lenient | 4/10 | 237 | 2 | 6/7 | 6/7 | 3 | 75% | 25% | 7/8 | 0 | 0/0/0/0 | 85 / 56 |
| shift2 | 20 | learn | 4/10 | 384 | 2 | 7/8 | 6/7 | 4 | 100% | 0% | 8/10 | 1 | 0/0/0/7 | 100 / 100 |
| shift2 | 100 | strict | 6/10 | 416 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 8/10 | 0 | 0/0/0/8 | 100 / 100 |
| shift2 | 100 | lenient | 6/10 | 547 | 3 | 6/7 | 6/7 | 4 | 88% | 71% | 8/10 | 0 | 2/1/0/4 | 84 / 42 |
| shift2 | 100 | learn | 6/10 | 426 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 8/10 | 0 | 0/0/0/8 | 100 / 100 |
| shift3 | 20 | strict | 0/10 | 68 | 1 | 8/10 | 8/10 | 3 | 38% | 0% | 8/10 | 0 | 0/0/0/0 | 53 / 41 |
| shift3 | 20 | lenient | 1/10 | 84 | 1 | 8/9 | 8/10 | 3 | 38% | 0% | 8/10 | 0 | 0/0/0/0 | 48 / 26 |
| shift3 | 20 | learn | 0/10 | 1040 | 4 | 10/12 | 8/10 | 8 | 100% | 0% | 16/19 | 4 | 0/1/0/16 | 100 / 100 |
| shift3 | 100 | strict | 3/10 | 934 | 3 | 12/14 | 8/10 | 7 | 88% | 0% | 12/17 | 0 | 0/0/0/9 | 87 / 83 |
| shift3 | 100 | lenient | 3/10 | 770 | 2 | 10/10 | 8/10 | 4 | 50% | 50% | 12/17 | 0 | 1/1/0/8 | 86 / 23 |
| shift3 | 100 | learn | 4/10 | 1564 | 4 | 11/13 | 8/10 | 8 | 100% | 0% | 16/20 | 2 | 1/1/1/18 | 100 / 100 |
| shift4 | 20 | strict | 0/10 | 96 | 1 | 13/16 | 10/13 | 4 | 28% | 0% | 13/16 | 0 | 0/0/0/0 | 48 / 24 |
| shift4 | 20 | lenient | 0/10 | 134 | 1 | 13/16 | 10/13 | 4 | 28% | 0% | 13/16 | 0 | 0/0/0/0 | 47 / 26 |
| shift4 | 20 | learn | 0/10 | 2934 | 4 | 20/25 | 10/13 | 16 | 97% | 0% | 28/35 | 10 | 2/1/2/27 | 83 / 83 |
| shift4 | 100 | strict | 2/10 | 426 | 1 | 20/26 | 10/13 | 10 | 62% | 0% | 20/26 | 0 | 0/0/0/0 | 57 / 57 |
| shift4 | 100 | lenient | 2/10 | 532 | 1 | 18/24 | 10/13 | 8 | 50% | 0% | 20/26 | 0 | 0/0/0/0 | 65 / 48 |
| shift4 | 100 | learn | 2/10 | 3384 | 6 | 16/21 | 10/13 | 16 | 100% | 0% | 24/35 | 6 | 5/2/2/33 | 100 / 100 |
| shift6 | 20 | strict | 0/10 | 62 | 1 | 14/14 | 14/19 | 2 | 3% | 0% | 14/14 | 0 | 0/0/0/0 | 10 / 4 |
| shift6 | 20 | lenient | 0/10 | 66 | 1 | 14/14 | 14/19 | 2 | 3% | 0% | 14/14 | 0 | 0/0/0/0 | 18 / 5 |
| shift6 | 20 | learn | 0/10 | 13973 | 10 | 24/35 | 14/19 | 74 | 116% | 13% | 70/92 | 54 | 21/5/4/86 | 87 / 87 |
| shift6 | 100 | strict | 0/10 | 312 | 1 | 32/41 | 14/19 | 14 | 21% | 0% | 32/41 | 0 | 0/0/0/0 | 43 / 21 |
| shift6 | 100 | lenient | 0/10 | 400 | 1 | 30/37 | 14/19 | 11 | 17% | 0% | 32/41 | 0 | 0/0/0/0 | 35 / 14 |
| shift6 | 100 | learn | 0/10 | 19310 | 10 | 27/40 | 14/19 | 74 | 116% | 20% | 62/89 | 36 | 16/6/5/76 | 90 / 90 |
| shift8 | 20 | strict | 0/10 | 62 | 1 | 15/14 | 18/25 | 2 | 1% | 0% | 15/14 | 0 | 0/0/0/0 | 7 / 0 |
| shift8 | 20 | lenient | 0/10 | 64 | 1 | 15/14 | 18/25 | 2 | 1% | 0% | 15/14 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | learn | 0/10 | 63718 | 13 | 28/38 | 18/25 | 535 | 167% | 42% | 138/194 | 136 | 38/8/7/148 | 84 / 84 |
| shift8 | 100 | strict | 0/10 | 230 | 1 | 38/42 | 18/25 | 6 | 3% | 0% | 38/42 | 0 | 0/0/0/0 | 16 / 2 |
| shift8 | 100 | lenient | 0/10 | 262 | 1 | 34/36 | 18/25 | 6 | 3% | 0% | 38/42 | 0 | 0/0/0/0 | 15 / 2 |
| shift8 | 100 | learn | 0/10 | 34742 | 10 | 58/70 | 18/25 | 251 | 95% | 10% | 122/181 | 88 | 15/2/4/50 | 74 / 74 |
| sum2 | 20 | strict | 6/10 | 195 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | lenient | 7/10 | 203 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/6 | 0 | 0/0/0/1 | 100 / 100 |
| sum2 | 20 | learn | 6/10 | 218 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | strict | 8/10 | 202 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | lenient | 8/10 | 246 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | learn | 8/10 | 214 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/1 | 100 / 100 |
| sum3 | 20 | strict | 0/10 | 324 | 2 | 6/9 | 5/7 | 8 | 100% | 6% | 7/10 | 0 | 0/0/1/2 | 85 / 79 |
| sum3 | 20 | lenient | 6/10 | 354 | 2 | 6/6 | 5/7 | 4 | 50% | 0% | 7/10 | 0 | 2/0/0/1 | 80 / 35 |
| sum3 | 20 | learn | 0/10 | 922 | 4 | 6/9 | 5/7 | 8 | 100% | 12% | 7/11 | 1 | 0/0/1/4 | 84 / 84 |
| sum3 | 100 | strict | 3/10 | 218 | 1 | 7/10 | 5/7 | 6 | 75% | 0% | 7/10 | 0 | 0/0/0/0 | 83 / 83 |
| sum3 | 100 | lenient | 3/10 | 449 | 2 | 6/9 | 5/7 | 4 | 56% | 0% | 7/10 | 0 | 0/0/0/2 | 83 / 58 |
| sum3 | 100 | learn | 4/10 | 662 | 2 | 7/10 | 5/7 | 8 | 100% | 0% | 8/12 | 0 | 0/0/0/2 | 85 / 85 |
| sum4 | 20 | strict | 0/10 | 230 | 1 | 12/18 | 6/9 | 9 | 56% | 0% | 12/18 | 0 | 0/0/0/0 | 85 / 53 |
| sum4 | 20 | lenient | 1/10 | 806 | 3 | 7/9 | 6/9 | 4 | 25% | 50% | 12/18 | 0 | 1/1/0/4 | 80 / 15 |
| sum4 | 20 | learn | 0/10 | 2088 | 4 | 12/20 | 6/9 | 16 | 100% | 19% | 13/23 | 4 | 0/0/1/8 | 81 / 81 |
| sum4 | 100 | strict | 3/10 | 605 | 2 | 11/18 | 6/9 | 12 | 78% | 0% | 12/19 | 0 | 0/0/0/1 | 78 / 78 |
| sum4 | 100 | lenient | 3/10 | 940 | 4 | 6/10 | 6/9 | 8 | 53% | 29% | 12/19 | 0 | 2/0/1/4 | 82 / 38 |
| sum4 | 100 | learn | 3/10 | 1928 | 4 | 10/19 | 6/9 | 16 | 100% | 15% | 12/20 | 1 | 0/0/1/4 | 82 / 82 |

### Shrinking from `compat`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 10/10 | 186 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 20 | lenient | 10/10 | 206 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 20 | learn | 10/10 | 180 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 100 | strict | 10/10 | 200 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block2 | 100 | lenient | 10/10 | 220 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block2 | 100 | learn | 10/10 | 202 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block3 | 20 | strict | 10/10 | 283 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 20 | lenient | 10/10 | 307 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 20 | learn | 10/10 | 278 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 100 | strict | 10/10 | 262 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block3 | 100 | lenient | 10/10 | 275 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block3 | 100 | learn | 10/10 | 252 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block4 | 20 | strict | 8/10 | 370 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/9 | 100 / 100 |
| block4 | 20 | lenient | 8/10 | 400 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/9 | 100 / 100 |
| block4 | 20 | learn | 8/10 | 372 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | strict | 10/10 | 366 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | lenient | 10/10 | 404 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | learn | 10/10 | 375 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block6 | 20 | strict | 6/10 | 430 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/10 | 100 / 100 |
| block6 | 20 | lenient | 6/10 | 512 | 2 | 8/12 | 8/13 | 48 | 75% | 0% | 8/13 | 0 | 0/0/0/10 | 87 / 78 |
| block6 | 20 | learn | 6/10 | 538 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/14 | 100 / 100 |
| block6 | 100 | strict | 10/10 | 568 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block6 | 100 | lenient | 10/10 | 624 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block6 | 100 | learn | 10/10 | 561 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 10 / 3 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 12 / 0 |
| block8 | 20 | learn | 0/10 | 15543 | 10 | 10/17 | 10/17 | 256 | 100% | 0% | 52/94 | 65 | 28/0/8/91 | 100 / 100 |
| block8 | 100 | strict | 10/10 | 744 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| block8 | 100 | lenient | 10/10 | 808 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| block8 | 100 | learn | 10/10 | 746 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| shift2 | 20 | strict | 7/10 | 232 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 1/0/0/5 | 100 / 100 |
| shift2 | 20 | lenient | 7/10 | 248 | 2 | 6/6 | 6/7 | 2 | 50% | 0% | 6/8 | 0 | 2/0/0/4 | 100 / 50 |
| shift2 | 20 | learn | 7/10 | 258 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/9 | 0 | 1/0/0/5 | 100 / 100 |
| shift2 | 100 | strict | 7/10 | 258 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 2/0/0/6 | 100 / 100 |
| shift2 | 100 | lenient | 7/10 | 300 | 2 | 6/6 | 6/7 | 2 | 50% | 25% | 6/8 | 0 | 2/0/0/4 | 100 / 53 |
| shift2 | 100 | learn | 7/10 | 246 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 2/0/0/6 | 100 / 100 |
| shift3 | 20 | strict | 3/10 | 62 | 1 | 8/9 | 8/10 | 6 | 50% | 0% | 8/9 | 0 | 0/0/0/0 | 57 / 54 |
| shift3 | 20 | lenient | 3/10 | 78 | 1 | 7/8 | 8/10 | 2 | 25% | 0% | 8/9 | 0 | 0/0/0/0 | 52 / 33 |
| shift3 | 20 | learn | 3/10 | 492 | 3 | 8/10 | 8/10 | 8 | 100% | 0% | 13/16 | 3 | 0/0/0/9 | 100 / 100 |
| shift3 | 100 | strict | 6/10 | 364 | 2 | 8/10 | 8/10 | 8 | 100% | 0% | 8/14 | 0 | 3/0/0/7 | 100 / 100 |
| shift3 | 100 | lenient | 6/10 | 548 | 2 | 8/8 | 8/10 | 2 | 25% | 89% | 8/14 | 0 | 5/0/0/6 | 97 / 11 |
| shift3 | 100 | learn | 6/10 | 432 | 2 | 8/10 | 8/10 | 8 | 100% | 0% | 8/14 | 0 | 3/0/0/9 | 100 / 100 |
| shift4 | 20 | strict | 1/10 | 104 | 1 | 10/13 | 10/13 | 18 | 75% | 0% | 10/14 | 0 | 0/0/0/0 | 58 / 47 |
| shift4 | 20 | lenient | 2/10 | 146 | 1 | 9/11 | 10/13 | 8 | 50% | 0% | 10/14 | 0 | 0/0/0/0 | 63 / 44 |
| shift4 | 20 | learn | 1/10 | 618 | 2 | 12/17 | 10/13 | 16 | 100% | 0% | 14/19 | 2 | 1/0/0/14 | 93 / 93 |
| shift4 | 100 | strict | 4/10 | 338 | 1 | 10/18 | 10/13 | 164 | 200% | 53% | 10/20 | 0 | 0/0/0/0 | 82 / 77 |
| shift4 | 100 | lenient | 4/10 | 1073 | 3 | 9/10 | 10/13 | 4 | 19% | 100% | 10/20 | 0 | 10/1/0/12 | 94 / 0 |
| shift4 | 100 | learn | 4/10 | 588 | 2 | 10/13 | 10/13 | 20 | 125% | 20% | 10/20 | 0 | 8/0/0/12 | 100 / 100 |
| shift6 | 20 | strict | 0/10 | 56 | 1 | 12/13 | 14/19 | 6 | 9% | 50% | 12/13 | 0 | 0/0/0/0 | 15 / 8 |
| shift6 | 20 | lenient | 1/10 | 60 | 1 | 12/13 | 14/19 | 6 | 6% | 50% | 12/13 | 0 | 0/0/0/0 | 14 / 5 |
| shift6 | 20 | learn | 0/10 | 2471 | 4 | 25/34 | 14/19 | 116 | 166% | 46% | 38/48 | 12 | 6/2/0/24 | 85 / 85 |
| shift6 | 100 | strict | 2/10 | 253 | 1 | 14/24 | 14/19 | 384 | 200% | 79% | 14/31 | 0 | 0/0/0/0 | 81 / 41 |
| shift6 | 100 | lenient | 3/10 | 1163 | 3 | 12/15 | 14/19 | 20 | 19% | 100% | 14/31 | 0 | 8/0/0/10 | 78 / 0 |
| shift6 | 100 | learn | 2/10 | 954 | 2 | 16/27 | 14/19 | 376 | 328% | 70% | 18/36 | 3 | 4/0/0/8 | 90 / 90 |
| shift8 | 20 | strict | 0/10 | 62 | 1 | 14/14 | 18/25 | 2 | 1% | 25% | 14/14 | 0 | 0/0/0/0 | 5 / 0 |
| shift8 | 20 | lenient | 0/10 | 64 | 1 | 14/14 | 18/25 | 2 | 1% | 25% | 14/14 | 0 | 0/0/0/0 | 5 / 0 |
| shift8 | 20 | learn | 0/10 | 47027 | 16 | 26/38 | 18/25 | 2415 | 887% | 91% | 140/198 | 118 | 33/26/6/125 | 90 / 90 |
| shift8 | 100 | strict | 0/10 | 182 | 1 | 18/32 | 18/25 | 7380 | 446% | 95% | 18/32 | 0 | 0/0/0/0 | 50 / 6 |
| shift8 | 100 | lenient | 2/10 | 364 | 1 | 16/24 | 18/25 | 33 | 11% | 97% | 18/32 | 0 | 0/0/0/0 | 55 / 0 |
| shift8 | 100 | learn | 0/10 | 10646 | 8 | 22/34 | 18/25 | 1827 | 714% | 86% | 50/80 | 44 | 22/14/2/48 | 99 / 99 |
| sum2 | 20 | strict | 7/10 | 166 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | lenient | 7/10 | 200 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | learn | 7/10 | 162 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | strict | 8/10 | 204 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/1 | 100 / 100 |
| sum2 | 100 | lenient | 8/10 | 221 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | learn | 8/10 | 182 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/1 | 100 / 100 |
| sum3 | 20 | strict | 5/10 | 200 | 2 | 5/7 | 5/7 | 8 | 100% | 12% | 5/7 | 0 | 0/0/0/2 | 85 / 85 |
| sum3 | 20 | lenient | 5/10 | 352 | 2 | 5/6 | 5/7 | 4 | 50% | 25% | 5/7 | 0 | 0/0/0/2 | 85 / 55 |
| sum3 | 20 | learn | 5/10 | 198 | 2 | 5/7 | 5/7 | 8 | 100% | 12% | 5/7 | 0 | 0/0/0/0 | 88 / 88 |
| sum3 | 100 | strict | 3/10 | 104 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 73 / 73 |
| sum3 | 100 | lenient | 3/10 | 123 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 77 / 74 |
| sum3 | 100 | learn | 3/10 | 100 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 77 / 77 |
| sum4 | 20 | strict | 1/10 | 292 | 2 | 6/9 | 6/9 | 16 | 100% | 16% | 6/9 | 0 | 0/0/0/2 | 85 / 85 |
| sum4 | 20 | lenient | 1/10 | 392 | 2 | 6/8 | 6/9 | 8 | 50% | 38% | 6/9 | 0 | 0/0/0/1 | 84 / 19 |
| sum4 | 20 | learn | 2/10 | 308 | 2 | 6/9 | 6/9 | 16 | 100% | 12% | 6/9 | 0 | 0/0/0/0 | 89 / 89 |
| sum4 | 100 | strict | 4/10 | 182 | 1 | 6/9 | 6/9 | 16 | 100% | 19% | 6/9 | 0 | 0/0/0/0 | 82 / 82 |
| sum4 | 100 | lenient | 3/10 | 428 | 2 | 6/8 | 6/9 | 8 | 50% | 66% | 6/9 | 0 | 1/0/0/2 | 86 / 22 |
| sum4 | 100 | learn | 4/10 | 242 | 2 | 6/9 | 6/9 | 16 | 100% | 22% | 6/9 | 0 | 0/0/0/1 | 79 / 79 |

### Campaign 2: with a warm-up

Same bodies, trials and seeds as campaign 1; `WARMUP=3`. Only `learn` cells change (the
warm-up is a `learn` mechanism), and within them:

- **`block`: every cell reaches the ideal in all ten trials, from every start.** No stall
  anywhere. From one run, block8 costs a median 15.8k (confirm 20) / 15.1k (confirm 100)
  executions, with 14–15 grafts made by warm-ups; from `exact`, 15.8k / 20.6k; from
  `compat`, 9.8k with 20 confirmations (the start has 2 paths) and 780 with 100 (the start
  is already the ideal shape).
- **`shift`: the stall is gone at k ≤ 4 and reduced above.** shift4 from one run: all ten
  trials reach ≥ 13 of 16 shapes, eight reach exactly 16, cold ≥ 42/50. shift6: one stall
  in twenty from one run, one from `exact`; shift8: five in twenty from one run, none
  from `exact`. When the warm-up fails to bootstrap on `shift` it is because the rescue's
  positional donors are misaligned there (the int arm is two draws), so a divergent
  replay rarely fails and there is nothing to graft — the same alignment effect that cost
  `shift` its reproduction in 017. Wrong paths grow with k as in campaign 1: shift6 from
  one run ends at a median 70 paths 16% wrong (confirm 20) and 64 paths 0% wrong (confirm
  100), per trial 0–124 wrong paths with one outlier of 1920 in 1984; shift8 at 470 paths
  38% wrong and 343 paths 23%, cold 79/79 and 40/38. From `exact`, shift8 ends at
  626–766 paths, ~60% wrong, cold 90–98; from `compat`, 1806–2504 paths, 82–83% wrong,
  cold 95–96.
- **`sum`: unchanged.** No stalls; all shapes; a median 10–21% wrong paths from one run
  (per trial 0–13 wrong of 16 on sum4), 4–19% from `exact`, 12–25% from `compat`; cold
  78–91.

Full tables (`python3 summarize.py results-warmup.jsonl`):

### Starting graphs (medians over trials)

| body | confirm | trials | ideal n/e | poolall | t0 cold fail% / clean% | exact n/e paths wrong% | exact cold fail% / clean% | compat n/e paths wrong% | compat cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 10 | 4/5 | 4 | 49 / 25 | 5/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| block2 | 100 | 10 | 4/5 | 4 | 47 / 27 | 5/7 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| block3 | 20 | 10 | 5/7 | 6 | 38 / 15 | 8/11 6 0% | 100 / 77 | 5/7 8 0% | 100 / 100 |
| block3 | 100 | 10 | 5/7 | 8 | 38 / 13 | 7/11 8 0% | 100 / 100 | 5/7 8 0% | 100 / 100 |
| block4 | 20 | 10 | 6/9 | 8 | 29 / 6 | 12/17 8 0% | 100 / 49 | 6/9 16 0% | 100 / 100 |
| block4 | 100 | 10 | 6/9 | 16 | 26 / 6 | 11/18 16 0% | 100 / 100 | 6/9 16 0% | 100 / 100 |
| block6 | 20 | 10 | 8/13 | 8 | 13 / 0 | 19/24 8 0% | 100 / 13 | 8/13 64 0% | 100 / 100 |
| block6 | 100 | 10 | 8/13 | 47 | 10 / 0 | 34/62 47 0% | 100 / 74 | 8/13 64 0% | 100 / 100 |
| block8 | 20 | 10 | 10/17 | 2 | 10 / 0 | 10/10 2 0% | 7 / 0 | 10/10 2 0% | 13 / 2 |
| block8 | 100 | 10 | 10/17 | 68 | 4 / 0 | 78/132 68 0% | 100 / 27 | 10/17 256 0% | 100 / 100 |
| shift2 | 20 | 10 | 6/7 | 3 | 45 / 28 | 7/8 3 0% | 83 / 76 | 6/8 7 0% | 100 / 100 |
| shift2 | 100 | 10 | 6/7 | 4 | 43 / 27 | 8/10 4 0% | 100 / 100 | 6/8 9 0% | 100 / 100 |
| shift3 | 20 | 10 | 8/10 | 3 | 26 / 14 | 8/10 3 0% | 49 / 40 | 8/9 6 0% | 50 / 44 |
| shift3 | 100 | 10 | 8/10 | 7 | 29 / 12 | 12/17 7 0% | 91 / 91 | 8/14 32 0% | 100 / 100 |
| shift4 | 20 | 10 | 10/13 | 4 | 24 / 7 | 13/16 4 0% | 43 / 28 | 10/14 22 0% | 65 / 49 |
| shift4 | 100 | 10 | 10/13 | 10 | 17 / 9 | 20/26 10 0% | 62 / 62 | 10/20 321 53% | 82 / 77 |
| shift6 | 20 | 10 | 14/19 | 2 | 6 / 2 | 14/14 2 0% | 21 / 5 | 12/13 6 50% | 11 / 5 |
| shift6 | 100 | 10 | 14/19 | 14 | 6 / 0 | 32/41 14 0% | 49 / 21 | 14/31 2952 79% | 75 / 35 |
| shift8 | 20 | 10 | 18/25 | 2 | 3 / 0 | 15/14 2 0% | 4 / 0 | 14/14 2 25% | 5 / 1 |
| shift8 | 100 | 10 | 18/25 | 6 | 2 / 0 | 38/42 6 0% | 16 / 2 | 18/32 7380 95% | 41 / 7 |
| sum2 | 20 | 10 | 4/5 | 4 | 61 / 25 | 4/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| sum2 | 100 | 10 | 4/5 | 4 | 64 / 25 | 5/6 4 0% | 100 / 100 | 4/5 4 0% | 100 / 100 |
| sum3 | 20 | 10 | 5/7 | 6 | 53 / 14 | 7/10 6 0% | 96 / 76 | 5/7 8 6% | 97 / 97 |
| sum3 | 100 | 10 | 5/7 | 6 | 58 / 14 | 7/10 6 0% | 78 / 78 | 5/7 8 25% | 77 / 77 |
| sum4 | 20 | 10 | 6/9 | 9 | 62 / 6 | 12/18 9 0% | 88 / 57 | 6/9 16 12% | 85 / 85 |
| sum4 | 100 | 10 | 6/9 | 13 | 65 / 8 | 12/19 13 0% | 82 / 82 | 6/9 16 16% | 86 / 86 |

### Shrinking from `t0`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 53 / 23 |
| block2 | 20 | lenient | 0/10 | 21 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 46 / 24 |
| block2 | 20 | learn | 0/10 | 368 | 3 | 4/5 | 4/5 | 4 | 100% | 0% | 6/7 | 3 | 0/0/2/5 | 100 / 100 |
| block2 | 100 | strict | 0/10 | 11 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 52 / 26 |
| block2 | 100 | lenient | 0/10 | 18 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 49 / 25 |
| block2 | 100 | learn | 0/10 | 435 | 3 | 4/5 | 4/5 | 4 | 100% | 0% | 6/7 | 3 | 0/0/2/6 | 100 / 100 |
| block3 | 20 | strict | 0/10 | 20 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 40 / 14 |
| block3 | 20 | lenient | 0/10 | 26 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 33 / 12 |
| block3 | 20 | learn | 0/10 | 680 | 4 | 5/7 | 5/7 | 8 | 100% | 0% | 8/11 | 6 | 0/0/3/10 | 100 / 100 |
| block3 | 100 | strict | 0/10 | 21 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 34 / 14 |
| block3 | 100 | lenient | 0/10 | 26 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 37 / 10 |
| block3 | 100 | learn | 0/10 | 1272 | 4 | 5/7 | 5/7 | 8 | 100% | 0% | 10/14 | 6 | 2/0/3/12 | 100 / 100 |
| block4 | 20 | strict | 0/10 | 22 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 26 / 6 |
| block4 | 20 | lenient | 0/10 | 28 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 25 / 5 |
| block4 | 20 | learn | 0/10 | 2184 | 5 | 6/9 | 6/9 | 16 | 100% | 0% | 14/20 | 10 | 4/0/4/20 | 100 / 100 |
| block4 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 24 / 5 |
| block4 | 100 | lenient | 0/10 | 23 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 24 / 4 |
| block4 | 100 | learn | 0/10 | 2069 | 5 | 6/9 | 6/9 | 16 | 100% | 0% | 12/20 | 10 | 3/0/4/22 | 100 / 100 |
| block6 | 20 | strict | 0/10 | 29 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 14 / 0 |
| block6 | 20 | lenient | 0/10 | 31 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 12 / 2 |
| block6 | 20 | learn | 0/10 | 7326 | 7 | 8/13 | 8/13 | 64 | 100% | 0% | 26/46 | 31 | 14/0/6/48 | 100 / 100 |
| block6 | 100 | strict | 0/10 | 32 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 11 / 1 |
| block6 | 100 | lenient | 0/10 | 36 | 1 | 8/7 | 8/13 | 1 | 2% | 0% | 8/7 | 0 | 0/0/0/0 | 13 / 0 |
| block6 | 100 | learn | 0/10 | 6289 | 7 | 8/13 | 8/13 | 64 | 100% | 0% | 28/46 | 27 | 12/0/6/50 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 8 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 7 / 0 |
| block8 | 20 | learn | 0/10 | 15792 | 9 | 10/17 | 10/17 | 256 | 100% | 0% | 48/90 | 66 | 31/0/8/112 | 100 / 100 |
| block8 | 100 | strict | 0/10 | 34 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 7 / 0 |
| block8 | 100 | lenient | 0/10 | 35 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 0/0/0/0 | 4 / 0 |
| block8 | 100 | learn | 0/10 | 15052 | 9 | 10/17 | 10/17 | 256 | 100% | 0% | 51/94 | 65 | 28/0/8/90 | 100 / 100 |
| shift2 | 20 | strict | 0/10 | 18 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 48 / 23 |
| shift2 | 20 | lenient | 0/10 | 21 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 42 / 21 |
| shift2 | 20 | learn | 0/10 | 475 | 3 | 8/8 | 6/7 | 4 | 100% | 0% | 11/10 | 3 | 0/0/1/8 | 100 / 100 |
| shift2 | 100 | strict | 0/10 | 16 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 47 / 25 |
| shift2 | 100 | lenient | 0/10 | 21 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 0/0/0/0 | 46 / 28 |
| shift2 | 100 | learn | 0/10 | 527 | 3 | 7/7 | 6/7 | 4 | 100% | 0% | 10/10 | 3 | 0/1/1/9 | 100 / 100 |
| shift3 | 20 | strict | 0/10 | 18 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 27 / 12 |
| shift3 | 20 | lenient | 0/10 | 19 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 25 / 12 |
| shift3 | 20 | learn | 0/10 | 1713 | 4 | 12/13 | 8/10 | 8 | 100% | 0% | 19/22 | 8 | 1/1/2/18 | 100 / 100 |
| shift3 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 29 / 14 |
| shift3 | 100 | lenient | 0/10 | 22 | 1 | 6/5 | 8/10 | 1 | 12% | 0% | 6/5 | 0 | 0/0/0/0 | 26 / 9 |
| shift3 | 100 | learn | 0/10 | 1232 | 3 | 12/13 | 8/10 | 8 | 100% | 0% | 19/20 | 7 | 1/1/2/16 | 100 / 100 |
| shift4 | 20 | strict | 0/10 | 26 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 0/0/0/0 | 18 / 5 |
| shift4 | 20 | lenient | 0/10 | 28 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 0/0/0/0 | 15 / 4 |
| shift4 | 20 | learn | 0/10 | 4172 | 5 | 18/22 | 10/13 | 16 | 100% | 0% | 34/41 | 18 | 6/2/2/38 | 100 / 100 |
| shift4 | 100 | strict | 0/10 | 20 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 0/0/0/0 | 18 / 7 |
| shift4 | 100 | lenient | 0/10 | 28 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 0/0/0/0 | 17 / 6 |
| shift4 | 100 | learn | 0/10 | 4492 | 6 | 19/22 | 10/13 | 16 | 100% | 0% | 32/38 | 16 | 6/2/4/37 | 100 / 100 |
| shift6 | 20 | strict | 0/10 | 37 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 0 |
| shift6 | 20 | lenient | 0/10 | 38 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 0 |
| shift6 | 20 | learn | 0/10 | 22264 | 11 | 28/38 | 14/19 | 70 | 109% | 16% | 64/81 | 60 | 25/4/6/79 | 93 / 93 |
| shift6 | 100 | strict | 0/10 | 36 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 9 / 2 |
| shift6 | 100 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 0/0/0/0 | 7 / 2 |
| shift6 | 100 | learn | 0/10 | 25116 | 12 | 38/44 | 14/19 | 64 | 100% | 0% | 72/94 | 63 | 23/4/5/84 | 91 / 91 |
| shift8 | 20 | strict | 0/10 | 48 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | lenient | 0/10 | 50 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | learn | 0/10 | 56580 | 12 | 22/30 | 18/25 | 470 | 151% | 38% | 159/220 | 166 | 52/12/9/192 | 79 / 79 |
| shift8 | 100 | strict | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 0/0/0/0 | 2 / 0 |
| shift8 | 100 | lenient | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 0/0/0/0 | 2 / 0 |
| shift8 | 100 | learn | 0/10 | 14018 | 7 | 20/28 | 18/25 | 343 | 86% | 23% | 116/143 | 46 | 4/2/2/42 | 40 / 38 |
| sum2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 58 / 21 |
| sum2 | 20 | lenient | 0/10 | 23 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 64 / 28 |
| sum2 | 20 | learn | 0/10 | 468 | 3 | 4/5 | 4/5 | 4 | 100% | 0% | 8/9 | 4 | 1/0/2/3 | 100 / 100 |
| sum2 | 100 | strict | 0/10 | 14 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 59 / 28 |
| sum2 | 100 | lenient | 0/10 | 20 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 0/0/0/0 | 59 / 24 |
| sum2 | 100 | learn | 0/10 | 408 | 3 | 4/5 | 4/5 | 4 | 100% | 0% | 7/7 | 3 | 0/0/2/3 | 100 / 100 |
| sum3 | 20 | strict | 0/10 | 20 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 58 / 16 |
| sum3 | 20 | lenient | 0/10 | 35 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 54 / 11 |
| sum3 | 20 | learn | 0/10 | 1156 | 4 | 6/8 | 5/7 | 8 | 100% | 12% | 11/14 | 7 | 1/0/2/4 | 86 / 86 |
| sum3 | 100 | strict | 0/10 | 25 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 65 / 13 |
| sum3 | 100 | lenient | 0/10 | 40 | 1 | 5/4 | 5/7 | 1 | 12% | 0% | 5/4 | 0 | 0/0/0/0 | 55 / 12 |
| sum3 | 100 | learn | 0/10 | 1085 | 3 | 7/10 | 5/7 | 8 | 100% | 10% | 11/16 | 10 | 1/0/2/5 | 86 / 86 |
| sum4 | 20 | strict | 0/10 | 24 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 62 / 6 |
| sum4 | 20 | lenient | 0/10 | 51 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 60 / 4 |
| sum4 | 20 | learn | 0/10 | 2440 | 4 | 16/24 | 6/9 | 20 | 100% | 21% | 20/30 | 16 | 2/0/1/6 | 78 / 78 |
| sum4 | 100 | strict | 0/10 | 19 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 64 / 7 |
| sum4 | 100 | lenient | 0/10 | 46 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 0/0/0/0 | 64 / 8 |
| sum4 | 100 | learn | 0/10 | 3355 | 5 | 14/22 | 6/9 | 24 | 100% | 17% | 20/30 | 14 | 2/0/2/10 | 78 / 78 |

### Shrinking from `exact`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 8/10 | 205 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/1/4 | 100 / 100 |
| block2 | 20 | lenient | 10/10 | 244 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 2/0/1/4 | 100 / 100 |
| block2 | 20 | learn | 8/10 | 256 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/4 | 100 / 100 |
| block2 | 100 | strict | 10/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/5 | 100 / 100 |
| block2 | 100 | lenient | 10/10 | 243 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 2/0/1/5 | 100 / 100 |
| block2 | 100 | learn | 10/10 | 253 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 0/0/1/5 | 100 / 100 |
| block3 | 20 | strict | 1/10 | 644 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 8/11 | 0 | 0/0/2/8 | 100 / 100 |
| block3 | 20 | lenient | 10/10 | 436 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 8/11 | 0 | 3/0/2/6 | 100 / 100 |
| block3 | 20 | learn | 1/10 | 1171 | 4 | 5/7 | 5/7 | 8 | 100% | 0% | 8/13 | 1 | 2/0/2/10 | 100 / 100 |
| block3 | 100 | strict | 10/10 | 742 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/2/8 | 100 / 100 |
| block3 | 100 | lenient | 10/10 | 344 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/1/5 | 100 / 100 |
| block3 | 100 | learn | 10/10 | 770 | 3 | 5/7 | 5/7 | 8 | 100% | 0% | 7/11 | 0 | 2/0/2/8 | 100 / 100 |
| block4 | 20 | strict | 0/10 | 218 | 1 | 11/16 | 6/9 | 8 | 53% | 0% | 12/17 | 0 | 0/0/0/0 | 100 / 55 |
| block4 | 20 | lenient | 8/10 | 686 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/17 | 0 | 4/0/3/10 | 100 / 100 |
| block4 | 20 | learn | 0/10 | 2214 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/21 | 3 | 4/0/3/22 | 100 / 100 |
| block4 | 100 | strict | 9/10 | 1466 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 3/0/2/18 | 100 / 100 |
| block4 | 100 | lenient | 10/10 | 472 | 2 | 6/8 | 6/9 | 8 | 50% | 50% | 11/18 | 0 | 4/1/1/9 | 100 / 26 |
| block4 | 100 | learn | 9/10 | 1804 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/19 | 1 | 4/0/3/21 | 100 / 100 |
| block6 | 20 | strict | 0/10 | 170 | 1 | 19/24 | 8/13 | 8 | 12% | 0% | 19/24 | 0 | 0/0/0/0 | 100 / 14 |
| block6 | 20 | lenient | 6/10 | 816 | 3 | 9/11 | 8/13 | 5 | 8% | 33% | 19/24 | 0 | 2/2/1/10 | 50 / 5 |
| block6 | 20 | learn | 0/10 | 5405 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 24/44 | 23 | 13/0/5/42 | 100 / 100 |
| block6 | 100 | strict | 0/10 | 5536 | 4 | 28/50 | 8/13 | 48 | 72% | 0% | 34/62 | 0 | 0/0/2/2 | 100 / 69 |
| block6 | 100 | lenient | 10/10 | 1428 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 34/62 | 0 | 9/0/5/15 | 100 / 100 |
| block6 | 100 | learn | 0/10 | 9276 | 6 | 8/13 | 8/13 | 64 | 100% | 0% | 34/66 | 18 | 20/0/5/74 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 12 / 0 |
| block8 | 20 | lenient | 0/10 | 59 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 10 / 0 |
| block8 | 20 | learn | 0/10 | 15836 | 9 | 10/17 | 10/17 | 256 | 100% | 0% | 54/94 | 71 | 29/0/8/94 | 100 / 100 |
| block8 | 100 | strict | 0/10 | 1838 | 1 | 78/132 | 10/17 | 68 | 26% | 0% | 78/132 | 0 | 0/0/0/0 | 100 / 26 |
| block8 | 100 | lenient | 10/10 | 1950 | 6 | 11/16 | 10/17 | 40 | 16% | 33% | 78/132 | 0 | 9/2/4/22 | 89 / 12 |
| block8 | 100 | learn | 0/10 | 20577 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 78/153 | 60 | 48/0/7/136 | 100 / 100 |
| shift2 | 20 | strict | 4/10 | 142 | 1 | 6/7 | 6/7 | 3 | 75% | 0% | 7/8 | 0 | 0/0/0/0 | 78 / 74 |
| shift2 | 20 | lenient | 4/10 | 251 | 2 | 6/7 | 6/7 | 3 | 75% | 0% | 7/8 | 0 | 0/0/0/0 | 82 / 67 |
| shift2 | 20 | learn | 4/10 | 422 | 2 | 7/8 | 6/7 | 4 | 100% | 0% | 8/10 | 1 | 0/0/0/7 | 100 / 100 |
| shift2 | 100 | strict | 6/10 | 395 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 8/10 | 0 | 0/0/0/8 | 100 / 100 |
| shift2 | 100 | lenient | 6/10 | 557 | 3 | 6/7 | 6/7 | 4 | 88% | 75% | 8/10 | 0 | 2/1/1/5 | 91 / 48 |
| shift2 | 100 | learn | 6/10 | 456 | 3 | 6/8 | 6/7 | 4 | 100% | 0% | 8/10 | 0 | 0/0/0/8 | 100 / 100 |
| shift3 | 20 | strict | 0/10 | 68 | 1 | 8/10 | 8/10 | 3 | 38% | 0% | 8/10 | 0 | 0/0/0/0 | 40 / 27 |
| shift3 | 20 | lenient | 1/10 | 86 | 1 | 8/10 | 8/10 | 3 | 38% | 0% | 8/10 | 0 | 0/0/0/0 | 42 / 28 |
| shift3 | 20 | learn | 0/10 | 1439 | 4 | 10/12 | 8/10 | 8 | 100% | 0% | 16/20 | 4 | 1/2/1/18 | 100 / 100 |
| shift3 | 100 | strict | 3/10 | 1091 | 4 | 12/15 | 8/10 | 6 | 75% | 0% | 12/17 | 0 | 0/0/0/6 | 80 / 73 |
| shift3 | 100 | lenient | 3/10 | 917 | 4 | 9/11 | 8/10 | 4 | 50% | 42% | 12/17 | 0 | 1/0/1/4 | 76 / 28 |
| shift3 | 100 | learn | 3/10 | 1617 | 4 | 12/14 | 8/10 | 8 | 100% | 0% | 18/23 | 2 | 2/1/1/20 | 100 / 100 |
| shift4 | 20 | strict | 0/10 | 108 | 1 | 13/16 | 10/13 | 4 | 28% | 0% | 13/16 | 0 | 0/0/0/0 | 41 / 25 |
| shift4 | 20 | lenient | 0/10 | 122 | 1 | 13/16 | 10/13 | 4 | 28% | 0% | 13/16 | 0 | 0/0/0/0 | 48 / 26 |
| shift4 | 20 | learn | 0/10 | 5344 | 7 | 19/22 | 10/13 | 16 | 100% | 0% | 28/36 | 12 | 4/2/3/40 | 100 / 100 |
| shift4 | 100 | strict | 2/10 | 393 | 1 | 20/26 | 10/13 | 10 | 62% | 0% | 20/26 | 0 | 0/0/0/0 | 59 / 59 |
| shift4 | 100 | lenient | 2/10 | 952 | 2 | 15/20 | 10/13 | 8 | 50% | 31% | 20/26 | 0 | 0/1/0/8 | 77 / 32 |
| shift4 | 100 | learn | 2/10 | 4198 | 6 | 16/21 | 10/13 | 16 | 100% | 0% | 26/36 | 6 | 5/2/2/36 | 100 / 100 |
| shift6 | 20 | strict | 0/10 | 61 | 1 | 14/14 | 14/19 | 2 | 3% | 0% | 14/14 | 0 | 0/0/0/0 | 15 / 3 |
| shift6 | 20 | lenient | 0/10 | 66 | 1 | 14/14 | 14/19 | 2 | 3% | 0% | 14/14 | 0 | 0/0/0/0 | 15 / 5 |
| shift6 | 20 | learn | 0/10 | 18247 | 10 | 23/30 | 14/19 | 64 | 100% | 0% | 64/88 | 42 | 15/4/4/82 | 99 / 99 |
| shift6 | 100 | strict | 0/10 | 298 | 1 | 32/41 | 14/19 | 14 | 21% | 0% | 32/41 | 0 | 0/0/0/0 | 47 / 17 |
| shift6 | 100 | lenient | 0/10 | 400 | 1 | 31/40 | 14/19 | 11 | 17% | 0% | 32/41 | 0 | 0/0/0/0 | 46 / 22 |
| shift6 | 100 | learn | 0/10 | 24932 | 12 | 27/38 | 14/19 | 81 | 120% | 21% | 63/93 | 50 | 22/6/7/86 | 92 / 92 |
| shift8 | 20 | strict | 0/10 | 62 | 1 | 15/14 | 18/25 | 2 | 1% | 0% | 15/14 | 0 | 0/0/0/0 | 4 / 0 |
| shift8 | 20 | lenient | 0/10 | 65 | 1 | 15/14 | 18/25 | 2 | 1% | 0% | 15/14 | 0 | 0/0/0/0 | 5 / 0 |
| shift8 | 20 | learn | 0/10 | 70420 | 17 | 46/66 | 18/25 | 766 | 217% | 60% | 150/210 | 192 | 60/14/10/159 | 98 / 98 |
| shift8 | 100 | strict | 0/10 | 228 | 1 | 38/42 | 18/25 | 6 | 3% | 0% | 38/42 | 0 | 0/0/0/0 | 12 / 2 |
| shift8 | 100 | lenient | 1/10 | 257 | 1 | 35/39 | 18/25 | 6 | 3% | 0% | 38/42 | 0 | 0/0/0/0 | 11 / 2 |
| shift8 | 100 | learn | 0/10 | 66615 | 19 | 34/41 | 18/25 | 626 | 243% | 58% | 146/203 | 177 | 65/17/10/143 | 90 / 90 |
| sum2 | 20 | strict | 6/10 | 201 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | lenient | 7/10 | 206 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | learn | 6/10 | 238 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | strict | 8/10 | 214 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | lenient | 8/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/1 | 100 / 100 |
| sum2 | 100 | learn | 8/10 | 238 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 0/0/0/1 | 100 / 100 |
| sum3 | 20 | strict | 0/10 | 258 | 2 | 6/8 | 5/7 | 8 | 94% | 6% | 7/10 | 0 | 0/0/0/2 | 86 / 82 |
| sum3 | 20 | lenient | 6/10 | 480 | 3 | 6/6 | 5/7 | 4 | 50% | 25% | 7/10 | 0 | 1/0/0/4 | 90 / 31 |
| sum3 | 20 | learn | 0/10 | 954 | 4 | 5/7 | 5/7 | 8 | 100% | 6% | 7/11 | 1 | 0/0/1/4 | 86 / 86 |
| sum3 | 100 | strict | 3/10 | 182 | 2 | 7/10 | 5/7 | 6 | 75% | 0% | 7/10 | 0 | 0/0/0/0 | 72 / 72 |
| sum3 | 100 | lenient | 4/10 | 240 | 1 | 7/9 | 5/7 | 4 | 56% | 0% | 7/10 | 0 | 0/0/0/0 | 72 / 56 |
| sum3 | 100 | learn | 3/10 | 1300 | 6 | 6/9 | 5/7 | 8 | 100% | 4% | 8/12 | 0 | 0/0/1/6 | 91 / 91 |
| sum4 | 20 | strict | 0/10 | 232 | 1 | 12/18 | 6/9 | 9 | 56% | 0% | 12/18 | 0 | 0/0/0/0 | 87 / 47 |
| sum4 | 20 | lenient | 1/10 | 1220 | 5 | 7/8 | 6/9 | 4 | 22% | 63% | 12/18 | 0 | 2/2/0/5 | 78 / 6 |
| sum4 | 20 | learn | 0/10 | 2544 | 4 | 11/19 | 6/9 | 16 | 100% | 19% | 13/22 | 4 | 0/0/1/10 | 83 / 83 |
| sum4 | 100 | strict | 3/10 | 920 | 2 | 10/16 | 6/9 | 14 | 81% | 6% | 12/19 | 0 | 0/0/0/1 | 77 / 77 |
| sum4 | 100 | lenient | 3/10 | 674 | 2 | 5/8 | 6/9 | 10 | 56% | 100% | 12/19 | 0 | 0/3/0/3 | 84 / 0 |
| sum4 | 100 | learn | 3/10 | 1779 | 4 | 8/12 | 6/9 | 16 | 100% | 18% | 12/20 | 1 | 0/0/2/9 | 82 / 82 |

### Shrinking from `compat`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 10/10 | 198 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 20 | lenient | 10/10 | 207 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 20 | learn | 10/10 | 230 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/4 | 100 / 100 |
| block2 | 100 | strict | 10/10 | 190 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block2 | 100 | lenient | 10/10 | 220 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block2 | 100 | learn | 10/10 | 235 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/5 | 100 / 100 |
| block3 | 20 | strict | 10/10 | 273 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 20 | lenient | 10/10 | 300 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 20 | learn | 10/10 | 320 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/7 | 100 / 100 |
| block3 | 100 | strict | 10/10 | 258 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block3 | 100 | lenient | 10/10 | 280 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block3 | 100 | learn | 10/10 | 292 | 2 | 5/7 | 5/7 | 8 | 100% | 0% | 5/7 | 0 | 0/0/0/6 | 100 / 100 |
| block4 | 20 | strict | 8/10 | 368 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/9 | 100 / 100 |
| block4 | 20 | lenient | 8/10 | 376 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/9 | 100 / 100 |
| block4 | 20 | learn | 8/10 | 414 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | strict | 10/10 | 366 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | lenient | 10/10 | 420 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block4 | 100 | learn | 10/10 | 408 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 0/0/0/10 | 100 / 100 |
| block6 | 20 | strict | 6/10 | 444 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/10 | 100 / 100 |
| block6 | 20 | lenient | 6/10 | 481 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/10 | 100 / 100 |
| block6 | 20 | learn | 6/10 | 604 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/15 | 100 / 100 |
| block6 | 100 | strict | 10/10 | 569 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block6 | 100 | lenient | 10/10 | 618 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block6 | 100 | learn | 10/10 | 602 | 2 | 8/13 | 8/13 | 64 | 100% | 0% | 8/13 | 0 | 0/0/0/16 | 100 / 100 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 11 / 0 |
| block8 | 20 | lenient | 0/10 | 57 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 0/0/0/0 | 11 / 1 |
| block8 | 20 | learn | 0/10 | 9812 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 33/60 | 38 | 18/0/8/60 | 100 / 100 |
| block8 | 100 | strict | 10/10 | 738 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| block8 | 100 | lenient | 10/10 | 812 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| block8 | 100 | learn | 10/10 | 780 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 0/0/0/21 | 100 / 100 |
| shift2 | 20 | strict | 7/10 | 238 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 1/0/0/5 | 100 / 100 |
| shift2 | 20 | lenient | 7/10 | 248 | 2 | 6/6 | 6/7 | 2 | 50% | 0% | 6/8 | 0 | 2/0/0/0 | 96 / 50 |
| shift2 | 20 | learn | 7/10 | 300 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/9 | 0 | 1/0/0/5 | 100 / 100 |
| shift2 | 100 | strict | 7/10 | 256 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 2/0/0/6 | 100 / 100 |
| shift2 | 100 | lenient | 8/10 | 293 | 2 | 6/6 | 6/7 | 2 | 50% | 0% | 6/8 | 0 | 2/0/0/5 | 100 / 52 |
| shift2 | 100 | learn | 7/10 | 294 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/8 | 0 | 2/0/0/6 | 100 / 100 |
| shift3 | 20 | strict | 3/10 | 74 | 1 | 8/9 | 8/10 | 6 | 50% | 0% | 8/9 | 0 | 0/0/0/0 | 52 / 42 |
| shift3 | 20 | lenient | 3/10 | 90 | 1 | 7/7 | 8/10 | 2 | 25% | 0% | 8/9 | 0 | 0/0/0/0 | 47 / 27 |
| shift3 | 20 | learn | 3/10 | 450 | 2 | 9/12 | 8/10 | 8 | 100% | 0% | 12/15 | 2 | 0/0/0/4 | 91 / 91 |
| shift3 | 100 | strict | 6/10 | 370 | 2 | 8/10 | 8/10 | 8 | 100% | 0% | 8/14 | 0 | 3/0/0/7 | 100 / 100 |
| shift3 | 100 | lenient | 6/10 | 545 | 3 | 7/7 | 8/10 | 2 | 25% | 89% | 8/14 | 0 | 5/0/0/7 | 89 / 10 |
| shift3 | 100 | learn | 6/10 | 408 | 2 | 8/10 | 8/10 | 8 | 100% | 0% | 8/14 | 0 | 3/0/0/7 | 100 / 100 |
| shift4 | 20 | strict | 1/10 | 117 | 1 | 10/13 | 10/13 | 18 | 75% | 0% | 10/14 | 0 | 0/0/0/0 | 60 / 53 |
| shift4 | 20 | lenient | 2/10 | 152 | 1 | 9/11 | 10/13 | 8 | 50% | 25% | 10/14 | 0 | 0/0/0/0 | 67 / 43 |
| shift4 | 20 | learn | 1/10 | 1022 | 2 | 10/16 | 10/13 | 16 | 100% | 0% | 14/20 | 3 | 2/0/0/14 | 98 / 98 |
| shift4 | 100 | strict | 4/10 | 338 | 1 | 10/18 | 10/13 | 164 | 200% | 53% | 10/20 | 0 | 0/0/0/0 | 88 / 84 |
| shift4 | 100 | lenient | 4/10 | 936 | 4 | 9/10 | 10/13 | 4 | 25% | 88% | 10/20 | 0 | 10/1/0/12 | 88 / 3 |
| shift4 | 100 | learn | 4/10 | 638 | 2 | 10/13 | 10/13 | 20 | 125% | 20% | 10/20 | 0 | 8/0/0/12 | 100 / 100 |
| shift6 | 20 | strict | 0/10 | 58 | 1 | 12/13 | 14/19 | 6 | 9% | 50% | 12/13 | 0 | 0/0/0/0 | 9 / 4 |
| shift6 | 20 | lenient | 1/10 | 59 | 1 | 12/13 | 14/19 | 6 | 9% | 50% | 12/13 | 0 | 0/0/0/0 | 14 / 4 |
| shift6 | 20 | learn | 0/10 | 8213 | 8 | 22/30 | 14/19 | 208 | 230% | 60% | 52/69 | 30 | 6/4/2/30 | 92 / 92 |
| shift6 | 100 | strict | 2/10 | 260 | 1 | 14/24 | 14/19 | 384 | 200% | 79% | 14/31 | 0 | 0/0/0/0 | 77 / 36 |
| shift6 | 100 | lenient | 4/10 | 1292 | 4 | 12/15 | 14/19 | 13 | 12% | 98% | 14/31 | 0 | 10/2/0/10 | 60 / 4 |
| shift6 | 100 | learn | 2/10 | 972 | 2 | 15/25 | 14/19 | 376 | 319% | 70% | 17/34 | 4 | 4/0/0/8 | 84 / 84 |
| shift8 | 20 | strict | 0/10 | 62 | 1 | 14/14 | 18/25 | 2 | 1% | 25% | 14/14 | 0 | 0/0/0/0 | 6 / 1 |
| shift8 | 20 | lenient | 0/10 | 66 | 1 | 14/14 | 18/25 | 2 | 1% | 25% | 14/14 | 0 | 0/0/0/0 | 6 / 0 |
| shift8 | 20 | learn | 0/10 | 24840 | 10 | 22/34 | 18/25 | 2504 | 631% | 83% | 143/204 | 136 | 28/19/5/104 | 95 / 95 |
| shift8 | 100 | strict | 0/10 | 181 | 1 | 18/32 | 18/25 | 7380 | 446% | 95% | 18/32 | 0 | 0/0/0/0 | 46 / 2 |
| shift8 | 100 | lenient | 2/10 | 394 | 1 | 16/22 | 18/25 | 15 | 6% | 98% | 18/32 | 0 | 0/0/0/0 | 40 / 0 |
| shift8 | 100 | learn | 0/10 | 10446 | 7 | 22/33 | 18/25 | 1806 | 705% | 82% | 50/82 | 61 | 27/15/0/46 | 96 / 96 |
| sum2 | 20 | strict | 7/10 | 178 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | lenient | 7/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 20 | learn | 7/10 | 220 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/2 | 100 / 100 |
| sum2 | 100 | strict | 8/10 | 184 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/1 | 100 / 100 |
| sum2 | 100 | lenient | 8/10 | 222 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/1 | 100 / 100 |
| sum2 | 100 | learn | 8/10 | 222 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 0/0/0/1 | 100 / 100 |
| sum3 | 20 | strict | 5/10 | 200 | 2 | 5/7 | 5/7 | 8 | 100% | 12% | 5/7 | 0 | 0/0/0/1 | 80 / 80 |
| sum3 | 20 | lenient | 5/10 | 411 | 3 | 5/6 | 5/7 | 4 | 50% | 31% | 5/7 | 0 | 0/0/0/2 | 87 / 40 |
| sum3 | 20 | learn | 5/10 | 409 | 2 | 5/7 | 5/7 | 8 | 100% | 12% | 5/7 | 0 | 0/0/0/2 | 89 / 89 |
| sum3 | 100 | strict | 3/10 | 109 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 76 / 76 |
| sum3 | 100 | lenient | 3/10 | 124 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 70 / 62 |
| sum3 | 100 | learn | 3/10 | 142 | 1 | 5/7 | 5/7 | 8 | 100% | 25% | 5/7 | 0 | 0/0/0/0 | 78 / 78 |
| sum4 | 20 | strict | 1/10 | 468 | 2 | 6/9 | 6/9 | 16 | 100% | 12% | 6/9 | 0 | 0/0/0/2 | 88 / 88 |
| sum4 | 20 | lenient | 1/10 | 452 | 2 | 6/8 | 6/9 | 8 | 50% | 19% | 6/9 | 0 | 0/0/0/1 | 86 / 46 |
| sum4 | 20 | learn | 2/10 | 489 | 2 | 6/9 | 6/9 | 16 | 100% | 12% | 6/9 | 0 | 0/0/0/2 | 86 / 86 |
| sum4 | 100 | strict | 4/10 | 212 | 2 | 6/9 | 6/9 | 16 | 100% | 22% | 6/9 | 0 | 0/0/0/0 | 81 / 81 |
| sum4 | 100 | lenient | 3/10 | 644 | 4 | 6/8 | 6/9 | 12 | 75% | 28% | 6/9 | 0 | 0/0/0/4 | 82 / 41 |
| sum4 | 100 | learn | 3/10 | 286 | 2 | 6/9 | 6/9 | 16 | 100% | 22% | 6/9 | 0 | 0/0/0/1 | 81 / 81 |


## Findings

1. **Shrinking over the graph works, and it is what makes one run into the
   counterexample.** On the `block` bodies — the case the graph is for — `learn` reaches
   the ideal graph from every start at every k, in every trial once the start is warmed
   up. From a single discovery run with no confirmation at all, block8 ends at 10 nodes /
   17 edges covering all 256 shapes, 50/50 cold replays failing without leaving the graph,
   for ~15–17k executions. 017 Part A's pipeline at k = 8 stored 1–2 shapes of 256 and
   reproduction broke. The confirmation batch as a phase of its own is no longer needed:
   the warm-up *is* a confirmation batch whose failing runs go into the graph instead of a
   pool, and it keeps going for as long as the shrink does.

2. **Cost.** Executions grow ~1.8× per unit of k from one run (474 → 1102 → 2084 → 7702 →
   17654 at k = 2, 3, 4, 6, 8): with the shapes (2^k), not with the graph (linear). An
   accepted edit costs K = 20 replays; a rejected one costs its first replay, except that
   an edit on an arm the walks did not take costs up to 20 to be rejected as unexercised,
   and the walk takes a given arm with probability ½ per piece. block2 from one run is 474
   executions against the pool's parallel shrinker on `twobranch` (≈ block2) at 4727
   (016, campaign 8) — a tenth, with the caveat that this shrinker has four moves and the
   engine's has many. When the start is already right (`compat` with 100 confirmations on
   `block`), the whole shrink is value shrinking over 17 edges: ~750 executions at k = 8.

3. **Judging by replay does not verify the graph's claims.** First-fit replay walks the
   earliest edge, so a path is walked only when the hidden coin diverges exactly there and
   nothing earlier catches it. Wrong paths — from a merge, from a graft of recombined donor
   material, from a value shrunk against one path's partners that passes with another's —
   are therefore silent: shift8/100 from `compat` under `learn` ends at 1827 paths, 86%
   wrong, and reproduces 99/99 cold. Cold reproduction and the correctness of the graph
   are decoupled. `strict` rejects the *replays* that leave the graph but never removes
   the *paths* they were about; the exercise rule stops the shrinker from accepting edits
   on unwalked material but nothing removes unwalked wrong material that was in the start
   (`compat`'s over-generalisation survives every judge, on `shift` and on `sum`).

4. **Where the wrong paths come from** is 017's two cases, now inside the shrinker. State
   aliasing on `shift`: the graft's rejoin and the merge both identify a state by depth,
   and `shift`'s two arms have different lengths, so a rejoin at the same depth can be at
   a different piece count and the joined path is malformed; the same depth rule keeps
   `learn`'s `shift` graphs above the ideal size (states at different depth sets are never
   merged) and wrong from k = 6. Value coupling on `sum`: 12–25% wrong from `compat` under
   every judge and 12–23% from one run under `learn`; a graph of independent edges cannot
   state a constraint across pieces, so the only correct `sum` graph is the one with every
   piece hot on its own, and nothing steers the shrinker there. `block` has neither problem
   and is perfect.

5. **`lenient` — the pool's rule today — is unsafe on the graph.** It accepts the deletion
   or contraction of an arm the rescue then covers, and the result reproduces worse than it
   claims (block6/20 cold 48/5, block8/100 94/13, with a third of the paths wrong). On the
   graph the rescue must not be part of the judge; it can stay a reproduction fallback.

6. **The start and the candidates need different judges.** Stopping at the first replay
   that does not count is right for a candidate and starves the bootstrap: campaign 1's
   stalls (3/20 block6, 4/20 block8, 8/20 shift6 from one run) are all the start judged
   by one diverging, passing replay. Replaying the incumbent to completion and grafting
   every failing divergent run (campaign 2's warm-up, ~20–60 executions) removes every
   stall on `block` and most on `shift`; the ones left (shift8: 5 of 20 from one run) are
   trials in which no divergent replay of the warm-up failed, because the rescue's
   positional donors are misaligned on `shift` — the graft has to be fed by a rescue that
   understands the state it rejoins at, which is finding 4 again.

**What this says for the design.** (a) The graph with a learning shrinker and no separate
confirmation phase is the direction; on independent pieces it does everything the pool
was built for, at a fraction of the cost and from one run. (b) State identity is now
needed inside the shrinker (graft rejoin, merge), not only for 017's offline merge: the
(kind, open span labels) experiment 017 proposed is the next one, and the engine hook
needs to pass the span stack for it. (c) The stored graph should distinguish walked from
unwalked material — a per-edge census from every replay that traversed it — and the
reported counterexample should be the walked paths, with everything else a hypothesis
(the three-valued stance from turn 11); a wrong path that was never walked is then a
claim the report never made. (d) K = 20 replays per accepted edit is the gauntlet's price
paid from scratch for every candidate; the engine version can credit an edge from every
run that traverses it, which this prototype does not.

**Unmeasured.** The engine's shrinker passes over the graph (four moves here), clone
subgraphs, bodies that are not chains of independent pieces, cost under the execution
cache, shrinking a graph whose start has wrong paths towards a correct one.
