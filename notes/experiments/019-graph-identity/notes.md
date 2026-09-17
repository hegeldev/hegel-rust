# 019: state identity from the span structure

Status: campaign run on 2026-09-17 against this commit (one engine change: the
`ExternalReplay` hook passes each draw's structural address).

Question (David, turn 15 of the takeover): 018 showed that shrinking over the graph
works where states are identified correctly (`block`, where depth is exact) and produces
malformed paths and never-ideal graphs where it is not (`shift`, whose arms have unequal
lengths, so depth aliases "piece 3 via two bools" with "piece 2 via an int and its
ignored bool"). The proposal at the end of 018 was to identify states by the test's span
structure instead of by depth. This experiment does that in 018's harness and asks: do the
`shift` cells reach ideal graphs with no wrong paths; does `block` keep working; and what
happens on bodies whose structure depends on a drawn value or on a hidden coin, which no
structural identity can settle on its own.

## Setup

Harness: `experiments/graph-identity/` (`TRIALS=10 KS=20 R=50 cargo run --release --
results.jsonl`; `python3 summarize.py results.jsonl`). The shrinker, judges (strict,
lenient, learn), warm-up, exercise and redundancy rules, caps and metrics are 018's; what
changes is what a state *is*.

**The address of a draw.** The engine wraps every draw in a span labelled by its kind
(`HEGEL_LABEL_INTEGER`, `HEGEL_LABEL_BOOLEAN`, …), inside whatever spans the test has
opened. `ExternalReplay::resolve` now receives the draw's **frames**: the open spans,
outermost first, each as `(label, ordinal)` where the ordinal is the number of earlier
siblings under the same parent with the same label. `NativeTestCase::open_span_frames`
computes it; the hook is the only caller, so the production draw path is untouched. The
bodies open spans as real generators would: a `PIECE` span (label 1001) around each loop
iteration and, in `shift`, an `ARM` span (1002) around the int and its ignored bool. So in
`shift` the draws of piece 2 have the addresses `P2/b0` (bool arm), `P2/A0/i0` and
`P2/A0/b0` (int arm) — written here as label/ordinal per frame, `b`, `i`, `P`, `A` for the
four labels.

**The identity of a state.** A state is where a run is *between* two draws. It is
identified by the prefix of the next draw's address through its first frame that was not
open at the previous draw — the first span the run enters after the last one it left.
Before the first draw that is `b0`; after `a` and before piece 0 it is `P0`, whichever arm
follows; inside `shift`'s int arm, between the int and the ignored bool, it is
`P0/A0/b0`; after the last draw it is `END`, the empty identity. Two runs are at the same
state iff their identities are equal. In 018's terms this replaces depth everywhere depth
was used:

- the **walk's rejoin** re-anchors at the first node of the reported identity with a
  fitting edge, not the first node at the same depth;
- the **graft** follows a run while the graph has its edges *to nodes of the right
  identity*, links where it departs to a node of the identity the run reaches from which
  the rest of the run is already a path, and otherwise appends nodes carrying their
  identities;
- the **merge move** pairs nodes of equal identity (not equal depth sets).

Two consequences of identity being observable at every draw:

- **The walk checks itself.** After serving an edge, the walk knows which node it claims
  to be on; at the next draw the engine reports the address, hence the identity the test
  is actually at. If they differ the graph's claim was wrong at that point — a
  **misjoin** — and it is a divergence like a misfit is. 018's finding 2 was that wrong
  paths are silent to replay judging; with identity, a structurally wrong path is not.
  (A wrong *value* on a right structure still is: `sum` is unchanged by this and is not in
  the campaign.)
- **Tied edges.** In the graph a node's edge is `(address, value) → target`. When the
  test's structure after a draw is decided by something other than the draw — a hidden
  coin choosing to loop again or to stop — one `(address, value)` legitimately leads to
  several states. The graph allows it: several edges with the same address and value and
  different targets, and the walk, having served the value, keeps all of their targets
  pending and settles on the one whose identity the next draw reports. Settling a tie is
  not a divergence; failing to (no pending target has the reported identity) is the
  misjoin above. Every `END` is one node, so "the run may stop here or go on" is a tie
  between `END` and the continuation.

**Bodies.** `block<k>` and `shift<k>` as in 018 (with the spans above), `sum` dropped.
Two new bodies probe what structural identity cannot settle:

- `list<k>`: `a`, then `n = int(0, k)` and *n* pieces (block arms); fails iff `a`, `n ≥ 1`
  and every piece hot. The structure depends on a drawn value: states `P1` in a run with
  n = 2 and in one with n = 3 have the same identity but different futures (one ends, one
  draws a third piece). The ideal graph fixes n = 1: 4 nodes / 4 edges, 2 shapes.
- `loop<k>`: `a`, then pieces while a hidden coin continues (p = 0.75, at most *k*),
  then a tail bool `z`; fails iff `a`, `z` and every piece hot. The structure depends on a
  hidden coin *after* a draw: from `Pj`, the edge just served leads to `Pj+1` or to the
  tail state `b1`. Only ties can represent it. The ideal graph has k + 3 nodes and 4k + 1
  edges (root to `P0` or `b1`; each `Pj` with both arms to `Pj+1` and to `b1`; `b1` to
  `END`) and covers 2^(k+1) − 1 shapes.

**Starts**, three per trial: `t0` (the discovery run alone), `exact` (017's exact merge of
the captured runs, identity included in the signature) and `ident` — every node of the
runs' trie merged with every other of the same identity, i.e. the structural automaton of
the observed runs, which replaces 018's `compat` (the RPNI-style merge whose wrong joins
identity is meant to supersede). Discovery is a fresh run with its addresses recorded; the
confirmation batch (20 or 100 runs) replays the trie of the runs captured so far through
the identity walk rather than through the engine's live set, since the live set has no
addresses to give.

**Two rules found necessary during development**, both consequences of ties, and both
recorded in the results as counts (campaign 1 below ran without them):

- **Exercise by settlement.** 018's exercise rule accepted a non-deletion edit only if a
  judging replay *served* an edited edge. With ties, serving is not enough: a contraction
  that redirects `P5 --97--> P6` to `P7` leaves the value 97 served on every replay (the
  tie's other alternatives, `P6` and the tail, absorb every actual continuation) while the
  redirected alternative is never the one the next draw's identity picks. It is a dead
  claim the walk can never falsify — a wrong path invisible to replay, in the new
  representation. An edited edge now counts as exercised only when a replay **settled** on
  it: the next draw's identity picked its target from the tie, or the run ended on it as a
  terminal target.
- **Foreign runs are not grafted.** Under `learn`, a failing replay of a *rejected
  candidate* that left the graph is grafted into the incumbent. When the candidate's edit
  was what made the replay diverge — the deleted or changed value is exactly where the
  walk drew at random — the realized run is one the *incumbent* would never produce: at
  that draw the incumbent's first-fit walk serves a different value. 018's redundancy rule
  (skip the graft when the graph walks the run's shape) does not catch it on `list`, where
  a run with another `n` has another structure; the graft then adds a branch first-fit never
  takes, the delete pass removes it, the next rejected candidate re-adds it — a churn that
  ran `list8` to the pass cap in a quarter of its trials. A run is now classified against
  the incumbent's walk as **whole** (redundant), **foreign** (the walk would have served a
  different value somewhere — not grafted) or a **gap** (the walk has no edge for a draw,
  no tie target of the reported identity, or does not end on `END` — grafted).

**Metrics** as 018, plus: `shapes%` is now over well-formed paths only, as a share of the
body's number of structures (2^k; 2 for `list`; 2^(k+1) − 1 for `loop`); `wrong%` counts
paths the body passes on or that no run produces, judged on addresses as well as values;
`misjoined` is the number of judging replays in which the walk caught a misjoin; `foreign`
the runs not grafted for being foreign; the cold column adds the share of cold replays that
were misjoined. `paths` counts a tie's
alternatives separately.

## Results

Two campaigns, 220 trials each (10 per body × confirmation size; every trial found its
failure; `TRIALS=10 KS=20 R=50`, K = 20, warm-up 3). **Campaign 1** ran with 018's
exercise rule (served, not settled) and without the foreign rule; its tables are below as
the record of what those two rules were introduced for (its raw rows were not kept — the
harness truncates its output file and the second campaign overwrote them). **Campaign 2**
is the design as described in the setup; `results.jsonl` holds its rows.

**`shift` is fixed.** From every start at every k, `learn` reaches the ideal graph with no
wrong path in every trial but one: shift8 from `t0` ends at 18 nodes / 25 edges, all 256
shapes, 100% clean cold reproduction, in ~9k executions (campaign 2 medians 9.1k from the
discovery run, 9.2k from `exact`, 2.4k from `ident`); 018 ended shift8 above ideal with
23–60% wrong paths and 99% "reproduction". The one exception is 018's remaining stall
(1 of 60 shift8 `learn` cells, from `t0` at confirm = 100): the single run's three warm-up
rounds produced no failing divergent replay, the shrink ended after one pass with the
discovery run alone, and cold reproduction is 2%.

**The `ident` start is the confirmation batch done right.** With 100 confirmations the
structural automaton of the ~66 captured runs is already the ideal graph at k = 8 (block8
10/17, shift8 18/25, all 256 shapes, 100% clean, 0% wrong, from 66–68 runs); `strict` then
shrinks values in ~800 executions. 018's `compat` from the same runs was 86% wrong. With 20
confirmations the runs are too few (2–3 at k = 8) and every start is far from ideal;
`learn` brings them there regardless.

**Misjoins are seen.** Cold replays of every correct graph report 0% misjoined. `lenient`
— which deletes structure the rescue then papers over — ends on `shift` with 75–100%
wrong paths, and 71–100% of its cold replays are misjoined: the identity check reports the
graph's wrongness at the point where 018's replay judging saw a 99% reproduction rate.
Judging replays of candidates are misjoined constantly (tens to thousands per shrink):
that is contractions and merges being caught.

**`list`** (structure from a drawn value). Every graph is correct (0% wrong, 100% clean) in
both campaigns. Campaign 1 reached the ideal n = 1 graph in 7–9 of 10 trials per cell but
ran 1–3 of 10 to the pass cap in the churn the foreign rule was introduced for. Campaign
2 has no churn (2–3 passes, ~200 executions) but *n does not shrink*: in 5–6 of 10 trials
the graph keeps the discovered n = 2 or 3 (5/6, 6/8, 7/10 nodes/edges). Finding 5 says
why.

**`loop`** (structure from a hidden coin after a draw). loop4 reaches the ideal 7/17
graph — 31 shapes, ties and all — in every trial of both campaigns, at 9–15k executions.
loop8 does not converge: every cell hits the pass cap at 42–48k executions and 4–7 of 10
trials end 1–4 edges above the ideal 11/33 (a rare tie alternative whose value never got
shrunk: `P5/i0=78` and `P5/i0=89` both to the tail), with 100% clean reproduction in all
but one trial (84%, on the largest leftover graph, 17/44). In
campaign 1 loop8 also had wrong paths in 1–4 trials per cell — the `P5 → P7` contraction
that the settlement rule stops; campaign 2 has none.

### Campaign 1: exercise by serving, no foreign rule

#### Starting graphs (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | trials | ideal n/e | poolall | t0 n/e paths shapes% wrong% | t0 cold | exact n/e paths shapes% wrong% | exact cold | ident n/e paths shapes% wrong% | ident cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 10 | 4/5 | 4 | 4/3 1 25% 0% | 49 / 25 / 0 | 5/6 4 100% 0% | 100 / 100 / 0 | 4/5 4 100% 0% | 100 / 100 / 0 | 
| block2 | 100 | 10 | 4/5 | 4 | 4/3 1 25% 0% | 47 / 27 / 0 | 5/7 4 100% 0% | 100 / 100 / 0 | 4/5 4 100% 0% | 100 / 100 / 0 | 
| block4 | 20 | 10 | 6/9 | 8 | 6/5 1 6% 0% | 29 / 6 / 0 | 12/17 8 53% 0% | 100 / 49 / 0 | 6/9 16 100% 0% | 100 / 100 / 0 | 
| block4 | 100 | 10 | 6/9 | 16 | 6/5 1 6% 0% | 26 / 6 / 0 | 11/18 16 100% 0% | 100 / 100 / 0 | 6/9 16 100% 0% | 100 / 100 / 0 | 
| block8 | 20 | 10 | 10/17 | 2 | 10/9 1 0% 0% | 10 / 0 / 0 | 10/10 2 1% 0% | 7 / 0 / 0 | 10/10 2 1% 0% | 13 / 2 / 0 | 
| block8 | 100 | 10 | 10/17 | 68 | 10/9 1 0% 0% | 4 / 0 / 0 | 78/132 68 26% 0% | 100 / 27 / 0 | 10/17 256 100% 0% | 100 / 100 / 0 | 
| list4 | 20 | 10 | 4/4 | 3 | 5/4 1 50% 0% | 47 / 25 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 5/6 4 200% 0% | 100 / 100 / 0 | 
| list4 | 100 | 10 | 4/4 | 3 | 4/4 1 50% 0% | 61 / 35 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 
| list8 | 20 | 10 | 4/4 | 3 | 5/4 1 50% 0% | 54 / 30 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 
| list8 | 100 | 10 | 4/4 | 3 | 4/4 1 50% 0% | 57 / 36 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 
| loop4 | 20 | 10 | 7/17 | 5 | 4/3 1 3% 0% | 47 / 10 / 61 | 7/10 5 16% 0% | 65 / 46 / 27 | 6/10 6 19% 0% | 64 / 51 / 24 | 
| loop4 | 100 | 10 | 7/17 | 24 | 3/2 1 3% 0% | 44 / 24 / 69 | 14/33 24 73% 0% | 100 / 86 / 4 | 7/17 31 100% 0% | 100 / 100 / 0 | 
| loop8 | 20 | 10 | 11/33 | 6 | 3/2 1 0% 0% | 36 / 18 / 74 | 6/10 6 1% 0% | 63 / 45 / 37 | 6/10 7 1% 0% | 60 / 45 / 41 | 
| loop8 | 100 | 10 | 11/33 | 17 | 3/2 1 0% 0% | 38 / 25 / 73 | 16/28 17 3% 0% | 86 / 62 / 32 | 8/20 45 8% 0% | 84 / 72 / 20 | 
| shift2 | 20 | 10 | 6/7 | 4 | 5/4 1 25% 0% | 53 / 28 / 0 | 7/8 4 100% 0% | 100 / 100 / 0 | 6/7 4 100% 0% | 100 / 100 / 0 | 
| shift2 | 100 | 10 | 6/7 | 4 | 5/4 1 25% 0% | 47 / 27 / 0 | 7/9 4 100% 0% | 100 / 100 / 0 | 6/7 4 100% 0% | 100 / 100 / 0 | 
| shift4 | 20 | 10 | 10/13 | 10 | 8/6 1 6% 0% | 23 / 7 / 0 | 19/26 10 66% 0% | 100 / 64 / 0 | 10/13 16 100% 0% | 100 / 100 / 0 | 
| shift4 | 100 | 10 | 10/13 | 16 | 7/6 1 6% 0% | 26 / 9 / 0 | 16/23 16 100% 0% | 100 / 100 / 0 | 10/13 16 100% 0% | 100 / 100 / 0 | 
| shift6 | 20 | 10 | 14/19 | 10 | 10/10 1 2% 0% | 14 / 2 / 0 | 34/42 10 16% 0% | 100 / 16 / 0 | 14/19 64 100% 0% | 100 / 100 / 0 | 
| shift6 | 100 | 10 | 14/19 | 47 | 10/10 1 2% 0% | 13 / 0 / 0 | 52/79 47 73% 0% | 100 / 73 / 0 | 14/19 64 100% 0% | 100 / 100 / 0 | 
| shift8 | 20 | 10 | 18/25 | 3 | 14/12 1 0% 0% | 7 / 0 / 0 | 25/26 3 1% 0% | 32 / 0 / 0 | 16/20 24 9% 0% | 26 / 10 / 0 | 
| shift8 | 100 | 10 | 18/25 | 66 | 12/11 1 0% 0% | 6 / 0 / 0 | 120/172 66 26% 0% | 100 / 29 / 0 | 18/25 256 100% 0% | 100 / 100 / 0 | 

#### Shrinking from `t0`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 2 | 0/0/0/0 | 53 / 23 / 0 |
| block2 | 20 | lenient | 0/10 | 23 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 4 | 0/0/0/0 | 46 / 24 / 0 |
| block2 | 20 | learn | 0/10 | 234 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 2 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | strict | 0/10 | 11 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 3 | 0/0/0/0 | 52 / 26 / 0 |
| block2 | 100 | lenient | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 3 | 0/0/0/0 | 48 / 25 / 0 |
| block2 | 100 | learn | 0/10 | 262 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 2 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 0/10 | 22 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 3 | 0/0/0/0 | 26 / 6 / 0 |
| block4 | 20 | lenient | 0/10 | 28 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 4 | 0/0/0/0 | 25 / 5 / 0 |
| block4 | 20 | learn | 0/10 | 433 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 4 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 3 | 0/0/0/0 | 24 / 5 / 0 |
| block4 | 100 | lenient | 0/10 | 26 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 4 | 0/0/0/0 | 24 / 4 / 0 |
| block4 | 100 | learn | 0/10 | 1035 | 3 | 6/9 | 6/9 | 16 | 100% | 0% | 9/14 | 6 | 18 | 2/0/1/15 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 8 / 0 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 5 | 0/0/0/0 | 7 / 0 / 0 |
| block8 | 20 | learn | 0/10 | 10102 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 36/66 | 46 | 148 | 22/0/5/76 | 100 / 100 / 0 |
| block8 | 100 | strict | 0/10 | 34 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 7 / 0 / 0 |
| block8 | 100 | lenient | 0/10 | 35 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 4 / 0 / 0 |
| block8 | 100 | learn | 0/10 | 7696 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 30/56 | 38 | 132 | 18/0/6/53 | 100 / 100 / 0 |
| list4 | 20 | strict | 0/10 | 16 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 53 / 26 / 0 |
| list4 | 20 | lenient | 0/10 | 24 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 6 | 0/0/0/0 | 50 / 21 / 0 |
| list4 | 20 | learn | 0/10 | 770 | 5 | 4/4 | 4/4 | 2 | 100% | 0% | 8/10 | 6 | 17 | 2/0/0/4 | 100 / 100 / 0 |
| list4 | 100 | strict | 0/10 | 16 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 3 | 0/0/0/0 | 59 / 37 / 0 |
| list4 | 100 | lenient | 1/10 | 22 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 5 | 0/0/0/0 | 64 / 43 / 0 |
| list4 | 100 | learn | 0/10 | 374 | 4 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 4 | 12 | 2/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | strict | 0/10 | 14 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 47 / 25 / 0 |
| list8 | 20 | lenient | 0/10 | 16 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 48 / 27 / 0 |
| list8 | 20 | learn | 0/10 | 1285 | 7 | 4/4 | 4/4 | 2 | 100% | 0% | 8/11 | 16 | 30 | 9/0/0/5 | 100 / 100 / 0 |
| list8 | 100 | strict | 0/10 | 15 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 3 | 0/0/0/0 | 61 / 41 / 0 |
| list8 | 100 | lenient | 0/10 | 18 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 4 | 0/0/0/0 | 65 / 35 / 0 |
| list8 | 100 | learn | 0/10 | 763 | 6 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 8 | 24 | 4/0/0/4 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 8 | 1 | 4/3 | 7/17 | 1 | 3% | 0% | 4/3 | 0 | 6 | 0/0/0/0 | 45 / 6 / 54 |
| loop4 | 20 | lenient | 0/10 | 10 | 1 | 4/3 | 7/17 | 1 | 3% | 0% | 4/3 | 0 | 8 | 0/0/0/0 | 49 / 9 / 58 |
| loop4 | 20 | learn | 0/10 | 14598 | 18 | 7/17 | 7/17 | 31 | 100% | 0% | 9/26 | 68 | 1062 | 52/0/2/24 | 100 / 100 / 0 |
| loop4 | 100 | strict | 0/10 | 5 | 1 | 3/2 | 7/17 | 1 | 3% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 40 / 16 / 69 |
| loop4 | 100 | lenient | 0/10 | 8 | 1 | 3/2 | 7/17 | 1 | 3% | 0% | 3/2 | 0 | 8 | 0/0/0/0 | 40 / 20 / 70 |
| loop4 | 100 | learn | 0/10 | 14782 | 18 | 7/17 | 7/17 | 31 | 100% | 0% | 11/32 | 66 | 956 | 48/0/2/26 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 44 / 25 / 63 |
| loop8 | 20 | lenient | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 6 | 0/0/0/0 | 42 / 16 / 73 |
| loop8 | 20 | learn | 0/10 | 49673 | 20 | 11/36 | 11/33 | 599 | 100% | 0% | 29/91 | 302 | 2276 | 213/6/4/80 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 5 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 4 | 0/0/0/0 | 44 / 23 / 75 |
| loop8 | 100 | lenient | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 39 / 21 / 74 |
| loop8 | 100 | learn | 0/10 | 53316 | 20 | 11/36 | 11/33 | 608 | 100% | 0% | 30/86 | 298 | 2342 | 222/6/6/92 | 100 / 100 / 0 |
| shift2 | 20 | strict | 0/10 | 18 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 54 / 24 / 0 |
| shift2 | 20 | lenient | 0/10 | 22 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 46 / 21 / 0 |
| shift2 | 20 | learn | 0/10 | 276 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 2 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift2 | 100 | strict | 0/10 | 16 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 54 / 25 / 0 |
| shift2 | 100 | lenient | 0/10 | 20 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 51 / 27 / 0 |
| shift2 | 100 | learn | 0/10 | 288 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 2 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 0/10 | 26 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 4 | 0/0/0/0 | 24 / 5 / 0 |
| shift4 | 20 | lenient | 0/10 | 28 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 4 | 0/0/0/0 | 21 / 4 / 0 |
| shift4 | 20 | learn | 0/10 | 1030 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 14/20 | 7 | 52 | 2/0/2/15 | 100 / 100 / 0 |
| shift4 | 100 | strict | 0/10 | 20 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 4 | 0/0/0/0 | 26 / 7 / 0 |
| shift4 | 100 | lenient | 0/10 | 26 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 4 | 0/0/0/0 | 23 / 6 / 0 |
| shift4 | 100 | learn | 0/10 | 972 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 14/22 | 8 | 42 | 3/0/2/15 | 100 / 100 / 0 |
| shift6 | 20 | strict | 0/10 | 37 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 0 / 0 |
| shift6 | 20 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 0 / 0 |
| shift6 | 20 | learn | 0/10 | 2836 | 4 | 14/19 | 14/19 | 64 | 100% | 0% | 26/38 | 14 | 102 | 6/0/5/25 | 100 / 100 / 0 |
| shift6 | 100 | strict | 0/10 | 36 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 2 / 0 |
| shift6 | 100 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 16 / 3 / 0 |
| shift6 | 100 | learn | 0/10 | 2264 | 4 | 14/19 | 14/19 | 64 | 100% | 0% | 22/34 | 12 | 92 | 6/0/4/25 | 100 / 100 / 0 |
| shift8 | 20 | strict | 0/10 | 48 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 8 | 0/0/0/0 | 9 / 0 / 0 |
| shift8 | 20 | lenient | 0/10 | 52 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 8 | 0/0/0/0 | 8 / 0 / 0 |
| shift8 | 20 | learn | 0/10 | 9707 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 55/86 | 42 | 244 | 24/0/8/66 | 100 / 100 / 0 |
| shift8 | 100 | strict | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 6 | 0/0/0/0 | 6 / 0 / 0 |
| shift8 | 100 | lenient | 0/10 | 37 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 6 | 0/0/0/0 | 6 / 0 / 0 |
| shift8 | 100 | learn | 0/10 | 8352 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 56/86 | 41 | 227 | 24/0/9/54 | 100 / 100 / 0 |

#### Shrinking from `exact`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 8/10 | 212 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 8 | 0/0/1/4 | 100 / 100 / 0 |
| block2 | 20 | lenient | 10/10 | 246 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 13 | 2/0/1/4 | 100 / 100 / 0 |
| block2 | 20 | learn | 8/10 | 254 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/4 | 100 / 100 / 0 |
| block2 | 100 | strict | 10/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/5 | 100 / 100 / 0 |
| block2 | 100 | lenient | 10/10 | 260 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 16 | 2/0/1/5 | 100 / 100 / 0 |
| block2 | 100 | learn | 10/10 | 249 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 0/10 | 222 | 1 | 10/13 | 6/9 | 8 | 53% | 0% | 12/17 | 0 | 12 | 0/0/0/0 | 100 / 51 / 0 |
| block4 | 20 | lenient | 8/10 | 698 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/17 | 0 | 48 | 4/0/3/9 | 100 / 100 / 0 |
| block4 | 20 | learn | 0/10 | 2070 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/21 | 4 | 38 | 4/0/3/20 | 100 / 100 / 0 |
| block4 | 100 | strict | 9/10 | 1492 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 31 | 3/0/2/18 | 100 / 100 / 0 |
| block4 | 100 | lenient | 10/10 | 612 | 3 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 33 | 4/0/2/10 | 100 / 100 / 0 |
| block4 | 100 | learn | 10/10 | 1649 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/19 | 0 | 34 | 3/0/2/20 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 5 | 0/0/0/0 | 9 / 0 / 0 |
| block8 | 20 | lenient | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 9 | 0/0/0/0 | 10 / 0 / 0 |
| block8 | 20 | learn | 0/10 | 9422 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 32/62 | 42 | 151 | 20/0/6/68 | 100 / 100 / 0 |
| block8 | 100 | strict | 0/10 | 1873 | 1 | 78/132 | 10/17 | 68 | 26% | 0% | 78/132 | 0 | 18 | 0/0/0/0 | 100 / 18 / 0 |
| block8 | 100 | lenient | 10/10 | 1822 | 6 | 10/17 | 10/17 | 152 | 38% | 42% | 78/132 | 0 | 898 | 9/2/6/21 | 100 / 33 / 56 |
| block8 | 100 | learn | 0/10 | 21182 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 78/154 | 64 | 214 | 54/0/6/126 | 100 / 100 / 0 |
| list4 | 20 | strict | 7/10 | 136 | 2 | 5/6 | 4/4 | 4 | 175% | 0% | 5/6 | 0 | 8 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 20 | lenient | 9/10 | 368 | 3 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 14 | 1/1/0/3 | 100 / 100 / 0 |
| list4 | 20 | learn | 7/10 | 1065 | 9 | 4/4 | 4/4 | 2 | 100% | 0% | 6/9 | 10 | 35 | 7/0/0/4 | 100 / 100 / 0 |
| list4 | 100 | strict | 10/10 | 150 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | lenient | 10/10 | 268 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 11 | 0/0/0/2 | 100 / 100 / 0 |
| list4 | 100 | learn | 10/10 | 918 | 5 | 4/4 | 4/4 | 2 | 100% | 0% | 8/9 | 6 | 19 | 4/0/0/3 | 100 / 100 / 0 |
| list8 | 20 | strict | 8/10 | 155 | 2 | 5/6 | 4/4 | 3 | 150% | 0% | 5/6 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | lenient | 9/10 | 262 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 12 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 20 | learn | 8/10 | 1004 | 5 | 4/4 | 4/4 | 2 | 100% | 0% | 8/11 | 6 | 21 | 6/0/0/6 | 100 / 100 / 0 |
| list8 | 100 | strict | 10/10 | 176 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | lenient | 10/10 | 269 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 18 | 0/0/0/3 | 100 / 100 / 0 |
| list8 | 100 | learn | 10/10 | 508 | 4 | 4/4 | 4/4 | 2 | 100% | 0% | 6/8 | 2 | 12 | 2/0/0/5 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 66 | 1 | 7/10 | 7/17 | 5 | 16% | 0% | 7/10 | 0 | 30 | 0/0/0/0 | 66 / 48 / 24 |
| loop4 | 20 | lenient | 0/10 | 96 | 1 | 7/10 | 7/17 | 5 | 16% | 0% | 7/10 | 0 | 42 | 0/0/0/0 | 65 / 41 / 33 |
| loop4 | 20 | learn | 0/10 | 12868 | 14 | 7/17 | 7/17 | 31 | 100% | 0% | 10/28 | 58 | 848 | 46/0/2/26 | 100 / 100 / 0 |
| loop4 | 100 | strict | 1/10 | 3836 | 5 | 10/26 | 7/17 | 24 | 71% | 0% | 14/33 | 0 | 664 | 3/0/2/2 | 100 / 84 / 12 |
| loop4 | 100 | lenient | 10/10 | 921 | 3 | 7/10 | 7/17 | 4 | 13% | 0% | 14/33 | 0 | 698 | 7/0/1/7 | 92 / 10 / 83 |
| loop4 | 100 | learn | 1/10 | 13234 | 13 | 7/17 | 7/17 | 31 | 100% | 0% | 14/40 | 52 | 754 | 45/0/2/27 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 94 | 1 | 6/10 | 11/33 | 6 | 1% | 0% | 6/10 | 0 | 38 | 0/0/0/0 | 66 / 45 / 37 |
| loop8 | 20 | lenient | 0/10 | 134 | 1 | 6/10 | 11/33 | 6 | 1% | 0% | 6/10 | 0 | 58 | 0/0/0/0 | 60 / 45 / 35 |
| loop8 | 20 | learn | 0/10 | 52456 | 20 | 11/35 | 11/33 | 552 | 100% | 0% | 24/87 | 307 | 2264 | 208/8/4/95 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 456 | 1 | 16/28 | 11/33 | 17 | 3% | 0% | 16/28 | 0 | 156 | 0/0/0/0 | 88 / 66 / 28 |
| loop8 | 100 | lenient | 2/10 | 3176 | 5 | 10/19 | 11/33 | 14 | 2% | 3% | 16/28 | 0 | 1492 | 2/1/0/2 | 87 / 53 / 45 |
| loop8 | 100 | learn | 0/10 | 56448 | 20 | 11/35 | 11/33 | 580 | 100% | 0% | 30/100 | 344 | 2386 | 244/10/6/99 | 100 / 100 / 0 |
| shift2 | 20 | strict | 9/10 | 227 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/8 | 0 | 15 | 0/0/1/5 | 100 / 100 / 0 |
| shift2 | 20 | lenient | 10/10 | 279 | 2 | 4/5 | 6/7 | 4 | 25% | 75% | 7/8 | 0 | 162 | 2/2/1/4 | 100 / 28 / 72 |
| shift2 | 20 | learn | 9/10 | 278 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/5 | 100 / 100 / 0 |
| shift2 | 100 | strict | 10/10 | 240 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/6 | 100 / 100 / 0 |
| shift2 | 100 | lenient | 10/10 | 278 | 2 | 4/5 | 6/7 | 4 | 25% | 75% | 7/9 | 0 | 168 | 2/2/1/5 | 100 / 25 / 75 |
| shift2 | 100 | learn | 10/10 | 276 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 0/10 | 350 | 1 | 14/18 | 10/13 | 10 | 66% | 0% | 19/26 | 0 | 14 | 0/0/0/0 | 100 / 64 / 0 |
| shift4 | 20 | lenient | 8/10 | 672 | 3 | 6/9 | 10/13 | 14 | 6% | 94% | 19/26 | 0 | 550 | 4/4/2/8 | 100 / 6 / 94 |
| shift4 | 20 | learn | 0/10 | 1922 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 19/28 | 3 | 56 | 5/0/4/18 | 100 / 100 / 0 |
| shift4 | 100 | strict | 9/10 | 1642 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 16/23 | 0 | 58 | 4/0/3/20 | 100 / 100 / 0 |
| shift4 | 100 | lenient | 10/10 | 745 | 3 | 6/9 | 10/13 | 16 | 6% | 94% | 16/23 | 0 | 588 | 5/4/2/10 | 100 / 4 / 96 |
| shift4 | 100 | learn | 10/10 | 1844 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 16/23 | 0 | 66 | 4/0/3/20 | 100 / 100 / 0 |
| shift6 | 20 | strict | 0/10 | 275 | 1 | 34/42 | 14/19 | 10 | 16% | 0% | 34/42 | 0 | 14 | 0/0/0/0 | 100 / 18 / 0 |
| shift6 | 20 | lenient | 6/10 | 1200 | 4 | 8/13 | 14/19 | 32 | 2% | 98% | 34/42 | 0 | 1048 | 4/6/4/9 | 100 / 2 / 97 |
| shift6 | 20 | learn | 0/10 | 5184 | 5 | 14/19 | 14/19 | 64 | 100% | 0% | 36/56 | 18 | 146 | 15/0/7/42 | 100 / 100 / 0 |
| shift6 | 100 | strict | 0/10 | 3434 | 2 | 51/74 | 14/19 | 50 | 75% | 0% | 52/79 | 0 | 60 | 0/0/0/0 | 100 / 74 / 0 |
| shift6 | 100 | lenient | 10/10 | 1534 | 6 | 8/13 | 14/19 | 64 | 2% | 98% | 52/79 | 0 | 1284 | 9/6/5/14 | 100 / 2 / 98 |
| shift6 | 100 | learn | 0/10 | 8250 | 5 | 14/19 | 14/19 | 64 | 100% | 0% | 52/82 | 16 | 168 | 24/0/8/55 | 100 / 100 / 0 |
| shift8 | 20 | strict | 0/10 | 128 | 1 | 25/26 | 18/25 | 3 | 1% | 0% | 25/26 | 0 | 12 | 0/0/0/0 | 36 / 0 / 0 |
| shift8 | 20 | lenient | 2/10 | 173 | 1 | 22/22 | 18/25 | 3 | 1% | 0% | 25/26 | 0 | 14 | 0/0/0/0 | 26 / 0 / 0 |
| shift8 | 20 | learn | 0/10 | 10047 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 60/94 | 39 | 242 | 23/0/10/74 | 100 / 100 / 0 |
| shift8 | 100 | strict | 0/10 | 2148 | 1 | 120/172 | 18/25 | 66 | 26% | 0% | 120/172 | 0 | 20 | 0/0/0/0 | 100 / 19 / 0 |
| shift8 | 100 | lenient | 10/10 | 2017 | 6 | 10/17 | 18/25 | 160 | 0% | 100% | 120/172 | 0 | 1788 | 9/12/6/18 | 100 / 0 / 100 |
| shift8 | 100 | learn | 0/10 | 17840 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 120/192 | 52 | 298 | 50/0/12/86 | 100 / 100 / 0 |

#### Shrinking from `ident`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 10/10 | 188 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 20 | lenient | 10/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 10 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 20 | learn | 10/10 | 232 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 100 | strict | 10/10 | 194 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | lenient | 10/10 | 218 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 12 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | learn | 10/10 | 232 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 8/10 | 354 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/9 | 100 / 100 / 0 |
| block4 | 20 | lenient | 8/10 | 410 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 16 | 0/0/0/9 | 100 / 100 / 0 |
| block4 | 20 | learn | 8/10 | 400 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | strict | 10/10 | 371 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | lenient | 10/10 | 409 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 16 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | learn | 10/10 | 401 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 5 | 0/0/0/0 | 11 / 1 / 0 |
| block8 | 20 | lenient | 0/10 | 58 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 7 | 0/0/0/0 | 9 / 0 / 0 |
| block8 | 20 | learn | 0/10 | 6794 | 6 | 10/17 | 10/17 | 256 | 100% | 0% | 28/52 | 37 | 113 | 17/0/5/53 | 100 / 100 / 0 |
| block8 | 100 | strict | 10/10 | 744 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 18 | 0/0/0/21 | 100 / 100 / 0 |
| block8 | 100 | lenient | 10/10 | 808 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 34 | 0/0/0/21 | 100 / 100 / 0 |
| block8 | 100 | learn | 10/10 | 780 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 18 | 0/0/0/21 | 100 / 100 / 0 |
| list4 | 20 | strict | 9/10 | 128 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 20 | lenient | 9/10 | 351 | 3 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 18 | 1/1/0/3 | 100 / 100 / 0 |
| list4 | 20 | learn | 9/10 | 665 | 5 | 4/4 | 4/4 | 2 | 100% | 0% | 8/10 | 4 | 18 | 3/0/0/4 | 100 / 100 / 0 |
| list4 | 100 | strict | 10/10 | 153 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | lenient | 10/10 | 267 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 14 | 0/0/0/2 | 100 / 100 / 0 |
| list4 | 100 | learn | 10/10 | 384 | 3 | 4/4 | 4/4 | 2 | 100% | 0% | 6/8 | 2 | 10 | 1/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | strict | 9/10 | 150 | 2 | 5/6 | 4/4 | 3 | 150% | 0% | 5/6 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | lenient | 9/10 | 264 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 13 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 20 | learn | 9/10 | 726 | 6 | 4/4 | 4/4 | 2 | 100% | 0% | 8/10 | 6 | 19 | 4/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | strict | 10/10 | 166 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | lenient | 10/10 | 258 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 12 | 0/0/0/3 | 100 / 100 / 0 |
| list8 | 100 | learn | 10/10 | 924 | 5 | 4/4 | 4/4 | 2 | 100% | 0% | 7/8 | 6 | 22 | 5/0/0/5 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 64 | 1 | 6/10 | 7/17 | 6 | 19% | 0% | 6/10 | 0 | 30 | 0/0/0/0 | 75 / 53 / 25 |
| loop4 | 20 | lenient | 1/10 | 116 | 1 | 6/10 | 7/17 | 6 | 19% | 0% | 6/10 | 0 | 40 | 0/0/0/0 | 66 / 47 / 25 |
| loop4 | 20 | learn | 0/10 | 15104 | 16 | 7/17 | 7/17 | 31 | 100% | 0% | 9/24 | 54 | 920 | 47/0/1/22 | 100 / 100 / 0 |
| loop4 | 100 | strict | 10/10 | 1436 | 4 | 7/16 | 7/17 | 30 | 94% | 0% | 7/17 | 0 | 270 | 2/0/0/4 | 100 / 88 / 12 |
| loop4 | 100 | lenient | 10/10 | 820 | 3 | 7/9 | 7/17 | 4 | 13% | 0% | 7/17 | 0 | 614 | 8/0/0/8 | 88 / 16 / 83 |
| loop4 | 100 | learn | 10/10 | 11300 | 14 | 7/17 | 7/17 | 31 | 100% | 0% | 9/23 | 38 | 803 | 38/0/0/22 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 90 | 1 | 6/10 | 11/33 | 7 | 1% | 0% | 6/10 | 0 | 40 | 0/0/0/0 | 64 / 51 / 31 |
| loop8 | 20 | lenient | 0/10 | 130 | 1 | 6/10 | 11/33 | 7 | 1% | 0% | 6/10 | 0 | 58 | 0/0/0/0 | 66 / 54 / 27 |
| loop8 | 20 | learn | 0/10 | 50048 | 20 | 11/34 | 11/33 | 623 | 100% | 0% | 24/81 | 280 | 2212 | 214/6/4/87 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 326 | 1 | 8/20 | 11/33 | 45 | 8% | 0% | 8/20 | 0 | 102 | 0/0/0/0 | 88 / 74 / 19 |
| loop8 | 100 | lenient | 1/10 | 916 | 2 | 8/18 | 11/33 | 25 | 5% | 0% | 8/20 | 0 | 349 | 0/0/0/1 | 80 / 60 / 32 |
| loop8 | 100 | learn | 0/10 | 46553 | 20 | 11/35 | 11/33 | 640 | 100% | 0% | 20/54 | 234 | 2186 | 190/6/4/77 | 100 / 100 / 0 |
| shift2 | 20 | strict | 10/10 | 212 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/5 | 100 / 100 / 0 |
| shift2 | 20 | lenient | 10/10 | 249 | 2 | 4/5 | 6/7 | 4 | 25% | 75% | 6/7 | 0 | 156 | 0/2/0/4 | 100 / 29 / 71 |
| shift2 | 20 | learn | 10/10 | 260 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/5 | 100 / 100 / 0 |
| shift2 | 100 | strict | 10/10 | 230 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift2 | 100 | lenient | 10/10 | 260 | 2 | 4/5 | 6/7 | 4 | 25% | 75% | 6/7 | 0 | 166 | 0/2/0/5 | 100 / 27 / 73 |
| shift2 | 100 | learn | 10/10 | 276 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 8/10 | 422 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 20 | lenient | 8/10 | 452 | 2 | 6/9 | 10/13 | 16 | 6% | 94% | 10/13 | 0 | 376 | 0/4/0/8 | 100 / 9 / 91 |
| shift4 | 20 | learn | 8/10 | 464 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 100 | strict | 10/10 | 419 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 100 | lenient | 10/10 | 486 | 2 | 6/9 | 10/13 | 16 | 6% | 94% | 10/13 | 0 | 392 | 0/4/0/10 | 100 / 7 / 93 |
| shift4 | 100 | learn | 10/10 | 470 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift6 | 20 | strict | 6/10 | 498 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/11 | 100 / 100 / 0 |
| shift6 | 20 | lenient | 6/10 | 610 | 2 | 8/13 | 14/19 | 64 | 2% | 98% | 14/19 | 0 | 521 | 0/6/0/9 | 100 / 2 / 97 |
| shift6 | 20 | learn | 6/10 | 670 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift6 | 100 | strict | 10/10 | 594 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift6 | 100 | lenient | 10/10 | 702 | 2 | 8/13 | 14/19 | 64 | 2% | 98% | 14/19 | 0 | 612 | 0/6/0/14 | 100 / 0 / 100 |
| shift6 | 100 | learn | 10/10 | 649 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift8 | 20 | strict | 2/10 | 95 | 1 | 16/20 | 18/25 | 24 | 9% | 0% | 16/20 | 0 | 16 | 0/0/0/0 | 33 / 7 / 0 |
| shift8 | 20 | lenient | 2/10 | 116 | 1 | 16/18 | 18/25 | 24 | 5% | 0% | 16/20 | 0 | 19 | 0/0/0/0 | 31 / 7 / 0 |
| shift8 | 20 | learn | 2/10 | 1670 | 3 | 18/25 | 18/25 | 256 | 100% | 0% | 27/37 | 7 | 80 | 3/0/2/30 | 100 / 100 / 0 |
| shift8 | 100 | strict | 10/10 | 811 | 2 | 18/25 | 18/25 | 256 | 100% | 0% | 18/25 | 0 | 50 | 0/0/0/21 | 100 / 100 / 0 |
| shift8 | 100 | lenient | 10/10 | 938 | 2 | 10/17 | 18/25 | 256 | 0% | 100% | 18/25 | 0 | 824 | 0/8/0/18 | 100 / 0 / 100 |
| shift8 | 100 | learn | 10/10 | 864 | 2 | 18/25 | 18/25 | 256 | 100% | 0% | 18/25 | 0 | 50 | 0/0/0/21 | 100 / 100 / 0 |

### Campaign 2: as described

#### Starting graphs (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | trials | ideal n/e | poolall | t0 n/e paths shapes% wrong% | t0 cold | exact n/e paths shapes% wrong% | exact cold | ident n/e paths shapes% wrong% | ident cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | 10 | 4/5 | 4 | 4/3 1 25% 0% | 49 / 25 / 0 | 5/6 4 100% 0% | 100 / 100 / 0 | 4/5 4 100% 0% | 100 / 100 / 0 | 
| block2 | 100 | 10 | 4/5 | 4 | 4/3 1 25% 0% | 47 / 27 / 0 | 5/7 4 100% 0% | 100 / 100 / 0 | 4/5 4 100% 0% | 100 / 100 / 0 | 
| block4 | 20 | 10 | 6/9 | 8 | 6/5 1 6% 0% | 29 / 6 / 0 | 12/17 8 53% 0% | 100 / 49 / 0 | 6/9 16 100% 0% | 100 / 100 / 0 | 
| block4 | 100 | 10 | 6/9 | 16 | 6/5 1 6% 0% | 26 / 6 / 0 | 11/18 16 100% 0% | 100 / 100 / 0 | 6/9 16 100% 0% | 100 / 100 / 0 | 
| block8 | 20 | 10 | 10/17 | 2 | 10/9 1 0% 0% | 10 / 0 / 0 | 10/10 2 1% 0% | 7 / 0 / 0 | 10/10 2 1% 0% | 13 / 2 / 0 | 
| block8 | 100 | 10 | 10/17 | 68 | 10/9 1 0% 0% | 4 / 0 / 0 | 78/132 68 26% 0% | 100 / 27 / 0 | 10/17 256 100% 0% | 100 / 100 / 0 | 
| list4 | 20 | 10 | 4/4 | 3 | 5/4 1 50% 0% | 47 / 25 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 5/6 4 200% 0% | 100 / 100 / 0 | 
| list4 | 100 | 10 | 4/4 | 3 | 4/4 1 50% 0% | 61 / 35 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 
| list8 | 20 | 10 | 4/4 | 3 | 5/4 1 50% 0% | 54 / 30 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 5/6 3 150% 0% | 100 / 100 / 0 | 
| list8 | 100 | 10 | 4/4 | 3 | 4/4 1 50% 0% | 57 / 36 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 4/5 3 150% 0% | 100 / 100 / 0 | 
| loop4 | 20 | 10 | 7/17 | 5 | 4/3 1 3% 0% | 47 / 10 / 61 | 7/10 5 16% 0% | 65 / 46 / 27 | 6/10 6 19% 0% | 64 / 51 / 24 | 
| loop4 | 100 | 10 | 7/17 | 24 | 3/2 1 3% 0% | 44 / 24 / 69 | 14/33 24 73% 0% | 100 / 86 / 4 | 7/17 31 100% 0% | 100 / 100 / 0 | 
| loop8 | 20 | 10 | 11/33 | 6 | 3/2 1 0% 0% | 36 / 18 / 74 | 6/10 6 1% 0% | 63 / 45 / 37 | 6/10 7 1% 0% | 60 / 45 / 41 | 
| loop8 | 100 | 10 | 11/33 | 17 | 3/2 1 0% 0% | 38 / 25 / 73 | 16/28 17 3% 0% | 86 / 62 / 32 | 8/20 45 8% 0% | 84 / 72 / 20 | 
| shift2 | 20 | 10 | 6/7 | 4 | 5/4 1 25% 0% | 53 / 28 / 0 | 7/8 4 100% 0% | 100 / 100 / 0 | 6/7 4 100% 0% | 100 / 100 / 0 | 
| shift2 | 100 | 10 | 6/7 | 4 | 5/4 1 25% 0% | 47 / 27 / 0 | 7/9 4 100% 0% | 100 / 100 / 0 | 6/7 4 100% 0% | 100 / 100 / 0 | 
| shift4 | 20 | 10 | 10/13 | 10 | 8/6 1 6% 0% | 23 / 7 / 0 | 19/26 10 66% 0% | 100 / 64 / 0 | 10/13 16 100% 0% | 100 / 100 / 0 | 
| shift4 | 100 | 10 | 10/13 | 16 | 7/6 1 6% 0% | 26 / 9 / 0 | 16/23 16 100% 0% | 100 / 100 / 0 | 10/13 16 100% 0% | 100 / 100 / 0 | 
| shift6 | 20 | 10 | 14/19 | 10 | 10/10 1 2% 0% | 14 / 2 / 0 | 34/42 10 16% 0% | 100 / 16 / 0 | 14/19 64 100% 0% | 100 / 100 / 0 | 
| shift6 | 100 | 10 | 14/19 | 47 | 10/10 1 2% 0% | 13 / 0 / 0 | 52/79 47 73% 0% | 100 / 73 / 0 | 14/19 64 100% 0% | 100 / 100 / 0 | 
| shift8 | 20 | 10 | 18/25 | 3 | 14/12 1 0% 0% | 7 / 0 / 0 | 25/26 3 1% 0% | 32 / 0 / 0 | 16/20 24 9% 0% | 26 / 10 / 0 | 
| shift8 | 100 | 10 | 18/25 | 66 | 12/11 1 0% 0% | 6 / 0 / 0 | 120/172 66 26% 0% | 100 / 29 / 0 | 18/25 256 100% 0% | 100 / 100 / 0 | 

#### Shrinking from `t0`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 2 | 0/0/0/0 | 53 / 23 / 0 |
| block2 | 20 | lenient | 0/10 | 23 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 4 | 0/0/0/0 | 46 / 24 / 0 |
| block2 | 20 | learn | 0/10 | 234 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 2 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | strict | 0/10 | 11 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 3 | 0/0/0/0 | 52 / 26 / 0 |
| block2 | 100 | lenient | 0/10 | 16 | 1 | 4/3 | 4/5 | 1 | 25% | 0% | 4/3 | 0 | 3 | 0/0/0/0 | 48 / 25 / 0 |
| block2 | 100 | learn | 0/10 | 262 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 2 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 0/10 | 22 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 3 | 0/0/0/0 | 26 / 6 / 0 |
| block4 | 20 | lenient | 0/10 | 28 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 4 | 0/0/0/0 | 25 / 5 / 0 |
| block4 | 20 | learn | 0/10 | 426 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 4 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | strict | 0/10 | 20 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 3 | 0/0/0/0 | 24 / 5 / 0 |
| block4 | 100 | lenient | 0/10 | 26 | 1 | 6/5 | 6/9 | 1 | 6% | 0% | 6/5 | 0 | 4 | 0/0/0/0 | 24 / 4 / 0 |
| block4 | 100 | learn | 0/10 | 1120 | 3 | 6/9 | 6/9 | 16 | 100% | 0% | 8/14 | 6 | 21 | 2/0/2/16 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 8 / 0 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 5 | 0/0/0/0 | 7 / 0 / 0 |
| block8 | 20 | learn | 0/10 | 10200 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 32/62 | 42 | 154 | 20/0/6/76 | 100 / 100 / 0 |
| block8 | 100 | strict | 0/10 | 34 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 7 / 0 / 0 |
| block8 | 100 | lenient | 0/10 | 35 | 1 | 10/9 | 10/17 | 1 | 0% | 0% | 10/9 | 0 | 6 | 0/0/0/0 | 4 / 0 / 0 |
| block8 | 100 | learn | 0/10 | 8040 | 6 | 10/17 | 10/17 | 256 | 100% | 0% | 29/55 | 36 | 126 | 14/0/5/54 | 100 / 100 / 0 |
| list4 | 20 | strict | 0/10 | 16 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 53 / 26 / 0 |
| list4 | 20 | lenient | 0/10 | 24 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 6 | 0/0/0/0 | 50 / 21 / 0 |
| list4 | 20 | learn | 0/10 | 232 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 2 | 8 | 0/0/0/4 | 100 / 100 / 0 |
| list4 | 100 | strict | 0/10 | 16 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 3 | 0/0/0/0 | 59 / 37 / 0 |
| list4 | 100 | lenient | 1/10 | 22 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 5 | 0/0/0/0 | 64 / 43 / 0 |
| list4 | 100 | learn | 0/10 | 182 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 2 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list8 | 20 | strict | 0/10 | 14 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 47 / 25 / 0 |
| list8 | 20 | lenient | 0/10 | 16 | 1 | 5/4 | 4/4 | 1 | 50% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 48 / 27 / 0 |
| list8 | 20 | learn | 0/10 | 236 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 2 | 8 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | strict | 0/10 | 15 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 3 | 0/0/0/0 | 61 / 41 / 0 |
| list8 | 100 | lenient | 0/10 | 18 | 1 | 4/4 | 4/4 | 1 | 50% | 0% | 4/4 | 0 | 4 | 0/0/0/0 | 65 / 35 / 0 |
| list8 | 100 | learn | 0/10 | 201 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 2 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 8 | 1 | 4/3 | 7/17 | 1 | 3% | 0% | 4/3 | 0 | 6 | 0/0/0/0 | 45 / 6 / 54 |
| loop4 | 20 | lenient | 0/10 | 10 | 1 | 4/3 | 7/17 | 1 | 3% | 0% | 4/3 | 0 | 8 | 0/0/0/0 | 49 / 9 / 58 |
| loop4 | 20 | learn | 0/10 | 13144 | 18 | 7/17 | 7/17 | 31 | 100% | 0% | 10/26 | 52 | 1006 | 38/0/2/26 | 100 / 100 / 0 |
| loop4 | 100 | strict | 0/10 | 5 | 1 | 3/2 | 7/17 | 1 | 3% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 40 / 16 / 69 |
| loop4 | 100 | lenient | 0/10 | 8 | 1 | 3/2 | 7/17 | 1 | 3% | 0% | 3/2 | 0 | 8 | 0/0/0/0 | 40 / 20 / 70 |
| loop4 | 100 | learn | 0/10 | 13034 | 15 | 7/17 | 7/17 | 31 | 100% | 0% | 10/29 | 57 | 822 | 39/0/2/24 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 44 / 25 / 63 |
| loop8 | 20 | lenient | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 6 | 0/0/0/0 | 42 / 16 / 73 |
| loop8 | 20 | learn | 0/10 | 44952 | 20 | 11/34 | 11/33 | 512 | 100% | 0% | 23/78 | 256 | 2214 | 174/4/4/82 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 5 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 4 | 0/0/0/0 | 44 / 23 / 75 |
| loop8 | 100 | lenient | 0/10 | 6 | 1 | 3/2 | 11/33 | 1 | 0% | 0% | 3/2 | 0 | 5 | 0/0/0/0 | 39 / 21 / 74 |
| loop8 | 100 | learn | 0/10 | 47750 | 20 | 11/34 | 11/33 | 515 | 100% | 0% | 26/93 | 270 | 2226 | 190/4/5/82 | 100 / 100 / 0 |
| shift2 | 20 | strict | 0/10 | 18 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 54 / 24 / 0 |
| shift2 | 20 | lenient | 0/10 | 22 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 46 / 21 / 0 |
| shift2 | 20 | learn | 0/10 | 265 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 2 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift2 | 100 | strict | 0/10 | 16 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 3 | 0/0/0/0 | 54 / 25 / 0 |
| shift2 | 100 | lenient | 0/10 | 20 | 1 | 5/4 | 6/7 | 1 | 25% | 0% | 5/4 | 0 | 4 | 0/0/0/0 | 51 / 27 / 0 |
| shift2 | 100 | learn | 0/10 | 288 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 2 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 0/10 | 26 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 4 | 0/0/0/0 | 24 / 5 / 0 |
| shift4 | 20 | lenient | 0/10 | 28 | 1 | 8/6 | 10/13 | 1 | 6% | 0% | 8/6 | 0 | 4 | 0/0/0/0 | 21 / 4 / 0 |
| shift4 | 20 | learn | 0/10 | 1000 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 12/18 | 6 | 44 | 2/0/2/16 | 100 / 100 / 0 |
| shift4 | 100 | strict | 0/10 | 20 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 4 | 0/0/0/0 | 26 / 7 / 0 |
| shift4 | 100 | lenient | 0/10 | 26 | 1 | 7/6 | 10/13 | 1 | 6% | 0% | 7/6 | 0 | 4 | 0/0/0/0 | 23 / 6 / 0 |
| shift4 | 100 | learn | 0/10 | 1192 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 13/18 | 7 | 46 | 2/0/2/18 | 100 / 100 / 0 |
| shift6 | 20 | strict | 0/10 | 37 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 0 / 0 |
| shift6 | 20 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 0 / 0 |
| shift6 | 20 | learn | 0/10 | 2970 | 4 | 14/19 | 14/19 | 64 | 100% | 0% | 22/33 | 14 | 114 | 6/0/5/26 | 100 / 100 / 0 |
| shift6 | 100 | strict | 0/10 | 36 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 14 / 2 / 0 |
| shift6 | 100 | lenient | 0/10 | 40 | 1 | 10/10 | 14/19 | 1 | 2% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 16 / 3 / 0 |
| shift6 | 100 | learn | 0/10 | 2284 | 4 | 14/19 | 14/19 | 64 | 100% | 0% | 20/30 | 12 | 92 | 4/0/4/24 | 100 / 100 / 0 |
| shift8 | 20 | strict | 0/10 | 48 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 8 | 0/0/0/0 | 9 / 0 / 0 |
| shift8 | 20 | lenient | 0/10 | 52 | 1 | 14/12 | 18/25 | 1 | 0% | 0% | 14/12 | 0 | 8 | 0/0/0/0 | 8 / 0 / 0 |
| shift8 | 20 | learn | 0/10 | 9068 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 53/82 | 42 | 233 | 22/0/10/68 | 100 / 100 / 0 |
| shift8 | 100 | strict | 0/10 | 36 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 6 | 0/0/0/0 | 6 / 0 / 0 |
| shift8 | 100 | lenient | 0/10 | 37 | 1 | 12/11 | 18/25 | 1 | 0% | 0% | 12/11 | 0 | 6 | 0/0/0/0 | 6 / 0 / 0 |
| shift8 | 100 | learn | 0/10 | 8517 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 52/80 | 38 | 220 | 20/0/9/57 | 100 / 100 / 0 |

#### Shrinking from `exact`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 8/10 | 212 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 8 | 0/0/1/4 | 100 / 100 / 0 |
| block2 | 20 | lenient | 10/10 | 246 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/6 | 0 | 13 | 2/0/1/4 | 100 / 100 / 0 |
| block2 | 20 | learn | 8/10 | 254 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/4 | 100 / 100 / 0 |
| block2 | 100 | strict | 10/10 | 208 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/5 | 100 / 100 / 0 |
| block2 | 100 | lenient | 10/10 | 254 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 13 | 2/0/1/5 | 100 / 100 / 0 |
| block2 | 100 | learn | 10/10 | 249 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 5/7 | 0 | 8 | 0/0/1/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 0/10 | 222 | 1 | 10/13 | 6/9 | 8 | 53% | 0% | 12/17 | 0 | 12 | 0/0/0/0 | 100 / 56 / 0 |
| block4 | 20 | lenient | 8/10 | 748 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/17 | 0 | 48 | 4/0/3/10 | 100 / 100 / 0 |
| block4 | 20 | learn | 0/10 | 2013 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 12/21 | 4 | 37 | 4/0/2/20 | 100 / 100 / 0 |
| block4 | 100 | strict | 9/10 | 1446 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 32 | 2/0/2/20 | 100 / 100 / 0 |
| block4 | 100 | lenient | 10/10 | 636 | 3 | 6/9 | 6/9 | 16 | 100% | 0% | 11/18 | 0 | 30 | 4/0/2/10 | 100 / 100 / 0 |
| block4 | 100 | learn | 10/10 | 1649 | 4 | 6/9 | 6/9 | 16 | 100% | 0% | 11/19 | 0 | 34 | 3/0/2/19 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 11 / 0 / 0 |
| block8 | 20 | lenient | 0/10 | 55 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 7 | 0/0/0/0 | 13 / 0 / 0 |
| block8 | 20 | learn | 0/10 | 7316 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 28/53 | 35 | 129 | 18/0/6/59 | 100 / 100 / 0 |
| block8 | 100 | strict | 0/10 | 1854 | 1 | 78/132 | 10/17 | 68 | 26% | 0% | 78/132 | 0 | 18 | 0/0/0/0 | 100 / 19 / 0 |
| block8 | 100 | lenient | 10/10 | 2928 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 78/132 | 0 | 280 | 10/0/7/24 | 100 / 100 / 0 |
| block8 | 100 | learn | 0/10 | 19562 | 8 | 10/17 | 10/17 | 256 | 100% | 0% | 78/152 | 62 | 218 | 52/0/6/118 | 100 / 100 / 0 |
| list4 | 20 | strict | 7/10 | 147 | 2 | 5/6 | 4/4 | 4 | 175% | 0% | 5/6 | 0 | 8 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 20 | lenient | 9/10 | 374 | 3 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 17 | 1/0/0/2 | 100 / 100 / 0 |
| list4 | 20 | learn | 7/10 | 200 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 8 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | strict | 10/10 | 148 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | lenient | 10/10 | 302 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 14 | 0/0/0/2 | 100 / 100 / 0 |
| list4 | 100 | learn | 10/10 | 188 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list8 | 20 | strict | 8/10 | 156 | 2 | 5/6 | 4/4 | 3 | 150% | 0% | 5/6 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | lenient | 9/10 | 325 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 16 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 20 | learn | 8/10 | 247 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 8 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | strict | 10/10 | 167 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | lenient | 10/10 | 281 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 16 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 100 | learn | 10/10 | 209 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 74 | 1 | 7/10 | 7/17 | 5 | 16% | 0% | 7/10 | 0 | 28 | 0/0/0/0 | 63 / 50 / 25 |
| loop4 | 20 | lenient | 1/10 | 112 | 1 | 7/10 | 7/17 | 5 | 16% | 0% | 7/10 | 0 | 46 | 0/0/0/0 | 67 / 45 / 26 |
| loop4 | 20 | learn | 0/10 | 11038 | 14 | 7/17 | 7/17 | 31 | 100% | 0% | 9/23 | 49 | 808 | 36/0/1/24 | 100 / 100 / 0 |
| loop4 | 100 | strict | 0/10 | 3789 | 5 | 10/22 | 7/17 | 26 | 76% | 0% | 14/33 | 0 | 498 | 4/0/2/4 | 100 / 81 / 14 |
| loop4 | 100 | lenient | 10/10 | 991 | 3 | 7/9 | 7/17 | 4 | 15% | 0% | 14/33 | 0 | 709 | 8/0/1/8 | 88 / 15 / 79 |
| loop4 | 100 | learn | 0/10 | 9322 | 11 | 7/17 | 7/17 | 31 | 100% | 0% | 14/39 | 36 | 680 | 34/1/3/18 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 100 | 1 | 6/10 | 11/33 | 6 | 1% | 0% | 6/10 | 0 | 43 | 0/0/0/0 | 59 / 43 / 36 |
| loop8 | 20 | lenient | 0/10 | 126 | 1 | 6/10 | 11/33 | 6 | 1% | 0% | 6/10 | 0 | 54 | 0/0/0/0 | 66 / 42 / 42 |
| loop8 | 20 | learn | 0/10 | 47274 | 20 | 11/34 | 11/33 | 511 | 100% | 0% | 22/78 | 285 | 2287 | 186/5/4/74 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 476 | 1 | 16/28 | 11/33 | 17 | 3% | 0% | 16/28 | 0 | 156 | 0/0/0/0 | 84 / 63 / 30 |
| loop8 | 100 | lenient | 1/10 | 2332 | 4 | 12/20 | 11/33 | 12 | 2% | 0% | 16/28 | 0 | 1028 | 2/0/1/4 | 82 / 47 / 46 |
| loop8 | 100 | learn | 0/10 | 44692 | 20 | 11/33 | 11/33 | 511 | 100% | 0% | 29/96 | 262 | 2280 | 182/5/5/85 | 100 / 100 / 0 |
| shift2 | 20 | strict | 9/10 | 227 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/8 | 0 | 15 | 0/0/1/5 | 100 / 100 / 0 |
| shift2 | 20 | lenient | 10/10 | 354 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/8 | 0 | 57 | 2/0/1/5 | 100 / 100 / 0 |
| shift2 | 20 | learn | 9/10 | 275 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/5 | 100 / 100 / 0 |
| shift2 | 100 | strict | 10/10 | 240 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/6 | 100 / 100 / 0 |
| shift2 | 100 | lenient | 10/10 | 339 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 62 | 2/0/1/6 | 100 / 100 / 0 |
| shift2 | 100 | learn | 10/10 | 280 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 7/9 | 0 | 16 | 0/0/1/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 0/10 | 361 | 1 | 14/18 | 10/13 | 10 | 66% | 0% | 19/26 | 0 | 16 | 0/0/0/0 | 100 / 64 / 0 |
| shift4 | 20 | lenient | 8/10 | 1064 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 19/26 | 0 | 192 | 4/0/3/12 | 100 / 100 / 0 |
| shift4 | 20 | learn | 0/10 | 1833 | 3 | 10/13 | 10/13 | 16 | 100% | 0% | 19/28 | 4 | 56 | 6/0/4/19 | 100 / 100 / 0 |
| shift4 | 100 | strict | 9/10 | 1634 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 16/23 | 0 | 56 | 4/0/3/20 | 100 / 100 / 0 |
| shift4 | 100 | lenient | 10/10 | 1038 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 16/23 | 0 | 199 | 6/0/2/12 | 100 / 100 / 0 |
| shift4 | 100 | learn | 9/10 | 1806 | 4 | 10/13 | 10/13 | 16 | 100% | 0% | 16/23 | 0 | 66 | 4/0/3/20 | 100 / 100 / 0 |
| shift6 | 20 | strict | 0/10 | 266 | 1 | 34/42 | 14/19 | 10 | 16% | 0% | 34/42 | 0 | 14 | 0/0/0/0 | 100 / 11 / 0 |
| shift6 | 20 | lenient | 6/10 | 1753 | 4 | 14/19 | 14/19 | 64 | 100% | 0% | 34/42 | 0 | 352 | 4/0/5/12 | 100 / 100 / 0 |
| shift6 | 20 | learn | 0/10 | 5286 | 5 | 14/19 | 14/19 | 64 | 100% | 0% | 35/54 | 16 | 138 | 14/0/7/36 | 100 / 100 / 0 |
| shift6 | 100 | strict | 0/10 | 3413 | 2 | 51/74 | 14/19 | 50 | 73% | 0% | 52/79 | 0 | 61 | 0/0/0/0 | 100 / 67 / 0 |
| shift6 | 100 | lenient | 10/10 | 2092 | 6 | 14/19 | 14/19 | 64 | 100% | 0% | 52/79 | 0 | 446 | 9/0/5/16 | 100 / 100 / 0 |
| shift6 | 100 | learn | 0/10 | 8218 | 5 | 14/19 | 14/19 | 64 | 100% | 0% | 52/82 | 16 | 170 | 24/0/9/55 | 100 / 100 / 0 |
| shift8 | 20 | strict | 0/10 | 128 | 1 | 25/26 | 18/25 | 3 | 1% | 0% | 25/26 | 0 | 12 | 0/0/0/0 | 36 / 0 / 0 |
| shift8 | 20 | lenient | 2/10 | 178 | 1 | 22/24 | 18/25 | 3 | 1% | 0% | 25/26 | 0 | 15 | 0/0/0/0 | 23 / 2 / 0 |
| shift8 | 20 | learn | 0/10 | 9176 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 54/86 | 40 | 244 | 24/0/9/68 | 100 / 100 / 0 |
| shift8 | 100 | strict | 0/10 | 2145 | 1 | 120/172 | 18/25 | 66 | 26% | 0% | 120/172 | 0 | 22 | 0/0/0/0 | 100 / 24 / 0 |
| shift8 | 100 | lenient | 10/10 | 3968 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 120/172 | 0 | 784 | 11/0/8/26 | 100 / 100 / 0 |
| shift8 | 100 | learn | 0/10 | 16240 | 6 | 18/25 | 18/25 | 256 | 100% | 0% | 120/193 | 51 | 264 | 48/0/12/80 | 100 / 100 / 0 |

#### Shrinking from `ident`, K = 20 (medians over trials; cold = % of 50 cold replays that failed / that failed without leaving the graph / that were misjoined)

| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block2 | 20 | strict | 10/10 | 188 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 20 | lenient | 10/10 | 210 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 10 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 20 | learn | 10/10 | 232 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/4 | 100 / 100 / 0 |
| block2 | 100 | strict | 10/10 | 194 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | lenient | 10/10 | 218 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 12 | 0/0/0/5 | 100 / 100 / 0 |
| block2 | 100 | learn | 10/10 | 234 | 2 | 4/5 | 4/5 | 4 | 100% | 0% | 4/5 | 0 | 6 | 0/0/0/5 | 100 / 100 / 0 |
| block4 | 20 | strict | 8/10 | 356 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/9 | 100 / 100 / 0 |
| block4 | 20 | lenient | 8/10 | 398 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 18 | 0/0/0/9 | 100 / 100 / 0 |
| block4 | 20 | learn | 8/10 | 404 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | strict | 10/10 | 371 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | lenient | 10/10 | 409 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 17 | 0/0/0/10 | 100 / 100 / 0 |
| block4 | 100 | learn | 10/10 | 412 | 2 | 6/9 | 6/9 | 16 | 100% | 0% | 6/9 | 0 | 10 | 0/0/0/10 | 100 / 100 / 0 |
| block8 | 20 | strict | 0/10 | 54 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 6 | 0/0/0/0 | 8 / 0 / 0 |
| block8 | 20 | lenient | 0/10 | 56 | 1 | 10/10 | 10/17 | 2 | 1% | 0% | 10/10 | 0 | 8 | 0/0/0/0 | 8 / 1 / 0 |
| block8 | 20 | learn | 0/10 | 7024 | 7 | 10/17 | 10/17 | 256 | 100% | 0% | 26/48 | 35 | 125 | 16/0/5/52 | 100 / 100 / 0 |
| block8 | 100 | strict | 10/10 | 742 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 18 | 0/0/0/21 | 100 / 100 / 0 |
| block8 | 100 | lenient | 10/10 | 797 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 30 | 0/0/0/21 | 100 / 100 / 0 |
| block8 | 100 | learn | 10/10 | 794 | 2 | 10/17 | 10/17 | 256 | 100% | 0% | 10/17 | 0 | 18 | 0/0/0/21 | 100 / 100 / 0 |
| list4 | 20 | strict | 9/10 | 138 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 20 | lenient | 9/10 | 345 | 3 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 14 | 1/0/0/2 | 100 / 100 / 0 |
| list4 | 20 | learn | 9/10 | 194 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 8 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | strict | 10/10 | 138 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list4 | 100 | lenient | 10/10 | 293 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 14 | 0/0/0/2 | 100 / 100 / 0 |
| list4 | 100 | learn | 10/10 | 188 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/3 | 100 / 100 / 0 |
| list8 | 20 | strict | 9/10 | 156 | 2 | 5/6 | 4/4 | 3 | 150% | 0% | 5/6 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 20 | lenient | 9/10 | 322 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 5/6 | 0 | 16 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 20 | learn | 9/10 | 234 | 2 | 5/6 | 4/4 | 4 | 200% | 0% | 5/6 | 0 | 8 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | strict | 10/10 | 165 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| list8 | 100 | lenient | 10/10 | 288 | 2 | 4/4 | 4/4 | 2 | 100% | 0% | 4/5 | 0 | 13 | 0/0/0/2 | 100 / 100 / 0 |
| list8 | 100 | learn | 10/10 | 201 | 2 | 4/5 | 4/4 | 3 | 150% | 0% | 4/5 | 0 | 7 | 0/0/0/4 | 100 / 100 / 0 |
| loop4 | 20 | strict | 0/10 | 90 | 1 | 6/10 | 7/17 | 6 | 19% | 0% | 6/10 | 0 | 32 | 0/0/0/0 | 63 / 53 / 20 |
| loop4 | 20 | lenient | 1/10 | 114 | 1 | 6/9 | 7/17 | 5 | 16% | 0% | 6/10 | 0 | 50 | 0/0/0/0 | 67 / 50 / 23 |
| loop4 | 20 | learn | 0/10 | 11941 | 15 | 7/17 | 7/17 | 31 | 100% | 0% | 8/22 | 39 | 846 | 31/0/1/24 | 100 / 100 / 0 |
| loop4 | 100 | strict | 10/10 | 1163 | 4 | 7/16 | 7/17 | 26 | 85% | 0% | 7/17 | 0 | 254 | 2/0/0/3 | 100 / 86 / 12 |
| loop4 | 100 | lenient | 10/10 | 1046 | 3 | 7/10 | 7/17 | 6 | 19% | 0% | 7/17 | 0 | 691 | 8/0/0/9 | 92 / 18 / 77 |
| loop4 | 100 | learn | 10/10 | 9312 | 13 | 7/17 | 7/17 | 31 | 100% | 0% | 7/20 | 28 | 765 | 26/0/0/20 | 100 / 100 / 0 |
| loop8 | 20 | strict | 0/10 | 89 | 1 | 6/10 | 11/33 | 7 | 1% | 0% | 6/10 | 0 | 38 | 0/0/0/0 | 69 / 51 / 28 |
| loop8 | 20 | lenient | 0/10 | 114 | 1 | 6/10 | 11/33 | 7 | 1% | 0% | 6/10 | 0 | 51 | 0/0/0/0 | 61 / 45 / 35 |
| loop8 | 20 | learn | 0/10 | 47744 | 20 | 11/34 | 11/33 | 512 | 100% | 0% | 22/76 | 248 | 2208 | 190/3/4/86 | 100 / 100 / 0 |
| loop8 | 100 | strict | 0/10 | 326 | 1 | 8/20 | 11/33 | 45 | 8% | 0% | 8/20 | 0 | 96 | 0/0/0/0 | 86 / 76 / 18 |
| loop8 | 100 | lenient | 1/10 | 1918 | 4 | 8/16 | 11/33 | 20 | 4% | 0% | 8/20 | 0 | 899 | 2/0/0/4 | 83 / 50 / 46 |
| loop8 | 100 | learn | 0/10 | 41529 | 20 | 11/34 | 11/33 | 519 | 100% | 0% | 16/50 | 210 | 2202 | 165/4/3/86 | 100 / 100 / 0 |
| shift2 | 20 | strict | 10/10 | 225 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/5 | 100 / 100 / 0 |
| shift2 | 20 | lenient | 10/10 | 325 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 58 | 0/0/0/5 | 100 / 100 / 0 |
| shift2 | 20 | learn | 10/10 | 258 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/5 | 100 / 100 / 0 |
| shift2 | 100 | strict | 10/10 | 223 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift2 | 100 | lenient | 10/10 | 314 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 60 | 0/0/0/6 | 100 / 100 / 0 |
| shift2 | 100 | learn | 10/10 | 264 | 2 | 6/7 | 6/7 | 4 | 100% | 0% | 6/7 | 0 | 14 | 0/0/0/6 | 100 / 100 / 0 |
| shift4 | 20 | strict | 8/10 | 415 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 20 | lenient | 8/10 | 598 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 116 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 20 | learn | 8/10 | 454 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 100 | strict | 10/10 | 420 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 100 | lenient | 10/10 | 610 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 112 | 0/0/0/11 | 100 / 100 / 0 |
| shift4 | 100 | learn | 10/10 | 467 | 2 | 10/13 | 10/13 | 16 | 100% | 0% | 10/13 | 0 | 26 | 0/0/0/11 | 100 / 100 / 0 |
| shift6 | 20 | strict | 6/10 | 527 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/11 | 100 / 100 / 0 |
| shift6 | 20 | lenient | 6/10 | 818 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 154 | 0/0/0/11 | 100 / 100 / 0 |
| shift6 | 20 | learn | 6/10 | 674 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift6 | 100 | strict | 10/10 | 612 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift6 | 100 | lenient | 10/10 | 870 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 162 | 0/0/0/16 | 100 / 100 / 0 |
| shift6 | 100 | learn | 10/10 | 650 | 2 | 14/19 | 14/19 | 64 | 100% | 0% | 14/19 | 0 | 38 | 0/0/0/16 | 100 / 100 / 0 |
| shift8 | 20 | strict | 2/10 | 94 | 1 | 16/20 | 18/25 | 24 | 9% | 0% | 16/20 | 0 | 15 | 0/0/0/0 | 32 / 12 / 0 |
| shift8 | 20 | lenient | 2/10 | 119 | 1 | 16/20 | 18/25 | 24 | 9% | 0% | 16/20 | 0 | 19 | 0/0/0/0 | 34 / 10 / 0 |
| shift8 | 20 | learn | 2/10 | 2356 | 4 | 18/25 | 18/25 | 256 | 100% | 0% | 26/38 | 10 | 110 | 4/0/3/34 | 100 / 100 / 0 |
| shift8 | 100 | strict | 10/10 | 807 | 2 | 18/25 | 18/25 | 256 | 100% | 0% | 18/25 | 0 | 50 | 0/0/0/21 | 100 / 100 / 0 |
| shift8 | 100 | lenient | 10/10 | 1192 | 2 | 18/25 | 18/25 | 256 | 100% | 0% | 18/25 | 0 | 222 | 0/0/0/21 | 100 / 100 / 0 |
| shift8 | 100 | learn | 10/10 | 843 | 2 | 18/25 | 18/25 | 256 | 100% | 0% | 18/25 | 0 | 50 | 0/0/0/21 | 100 / 100 / 0 |

## Findings

1. **State identity from the span structure removes 018's blocker.** With states
   identified by (label, ordinal) frames instead of depth, `shift` behaves exactly like
   `block`: ideal graphs, every shape, no wrong path, 100% clean reproduction, from a
   single discovery run at k = 8 in ~9k executions. The identity needs nothing from the
   test but the spans real generators already open; the engine computes the frames from
   its span stack, the hook passes them, and the production draw path is untouched.

2. **The confirmation batch should be an identity merge.** The structural automaton of
   the failing runs (`ident`) is the ideal graph as soon as the runs cover the shapes
   (66 runs for 256 shapes at k = 8), and it is never wrong where identity is exact. 017's
   `compat` recombined by guessing futures; identity recombines by knowing positions. What
   is left for the shrinker from that start is values — 800 executions.

3. **Identity makes structural wrongness visible, but only for claims a replay can
   reach.** A misjoin — the identity the test reports is none of the targets the served
   edge offers — is a divergence, and it exposes `lenient`'s graphs (71–100% of cold
   replays misjoined) and rejects wrong contractions and merges by the hundreds. But a
   tie alternative the next draw never picks is a claim no replay can falsify: campaign
   1's `P5 → P7` contraction was served on every replay and settled on by none, and was
   accepted. The settlement rule is 018's evidence stance applied to edits: an edit
   counts only when a replay settled on it. The representation should carry the same
   distinction — an edge that has been settled on is evidence, one that has not is a
   hypothesis — and report only the former. Wrong *values* on a right structure (`sum`)
   remain invisible to replay, as in 018.

4. **Ties are necessary and they work.** A test whose structure after a draw is decided by
   a hidden coin (`loop`) cannot be a DFA over values; letting an (address, value) edge
   lead to several nodes, settled by the next draw's identity, represents it exactly:
   loop4's 31 shapes in 7 nodes / 17 edges, 100% clean, from one run. The pool
   representation has this for free (each timeline is a whole run) and the graph needs it
   as a feature. Merging by identity never closed a cycle in 440 trials (asserted).

5. **Values that determine structure do not shrink under judged single edits.** Shrinking
   `list`'s n from 3 to 1 changes what follows: the candidate's replays end after one
   piece where the graph still claims a second, so the candidate is never clean; under
   `learn` the realized run is foreign to the incumbent (the incumbent serves n = 3), so
   it is not grafted; the edit is rejected and n stays. Campaign 1 got there by accident —
   grafting the foreign run added an n = 1 branch that first-fit never took until the
   delete pass removed the n = 3 edge — through the churn that also ran trials to the
   cap. Hypothesis's shrinker has a compound pass for exactly this (shrink an integer and
   delete the block that follows); the graph needs its analogue: a value edit whose
   replays end early or misjoin should have its *candidate* re-derived from those replays
   — the graph the edited walk actually realizes, dead alternatives pruned — before being
   judged and compared. This is also where structural identity is genuinely insufficient:
   `P1` in an n = 2 run and in an n = 3 run are the same identity with different futures.
   Ties absorb the difference (the walk never misjoins on `list`), at the price of
   over-claiming (n = 3 with two pieces) until deletions prune it; nothing wrong survived
   to the end in any trial.

6. **Judging by K samples cannot certify coverage of many shapes.** loop8 has 511 shapes;
   20 replays see a rare tie alternative (seven continues, then the int arm: ~7%) about
   three times in four, so its deletion is accepted a quarter of the time it is tried,
   the warm-up grafts it back when a divergent replay happens to fail, and the shrink
   runs to the pass cap (all 60 loop8 `learn` cells, 42–48k executions) leaving un-shrunk
   values on the rare alternatives. Correctness does not suffer (0% wrong, 100% clean),
   minimality and cost do. The fix is not a larger K alone: deletion of an edge that has
   been settled on (evidence) should need more than 20 silent replays — the evidence a
   graph carries is exactly what the shrinker should weigh before deleting.

7. **Cost** is 018's: ~9k executions for k = 8 from one run (`block` and `shift` alike
   now), ~800 from the identity-merged batch, ~200 for `list`, 10–15k for loop4.

**What this says for the design.** (a) The engine's graph should carry node identity
from span frames — label and same-label ordinal per open span, the state being the prefix
of the next address through its first new frame — with `END` as one node; the frames are
a few integers per open span, computed from the span stack the engine already keeps.
(b) Edges are (address, value) with a set of targets; the walk settles a tie by the
identity the next draw reports, treats a misjoin as a divergence, and rejoins by identity.
(c) The confirmation phase becomes an identity merge of the failing runs into the graph;
the shrinker's warm-up is the same operation. (d) Every judged edit must be exercised by
settlement, and the stored graph should distinguish settled from unsettled material.
(e) Two shrinker problems are now specified rather than solved: structure-determining
values need a re-derive-the-candidate compound move (finding 5), and deletion needs to
weigh evidence rather than K silent replays (finding 6). (f) The 018 stall from a single
run persists at 1 in 60 cells at k = 8; a warm-up that keeps going while the graph is
still one run would close it.

**Unmeasured.** Tests that open no spans of their own (identity then degrades to
same-kind ordinals at the root, which is depth per kind — better than depth, still
aliasable); cloned streams (the hook passes `stream`, the harness ignores it); real
generators' span structure (hegel's collections draw their continue flags inside engine
spans, which gives per-element frames without a user span); value coupling (`sum`, as
018); the cost of `open_span_frames` at engine scale (linear in spans per draw as written;
a per-parent counter would make it constant).
