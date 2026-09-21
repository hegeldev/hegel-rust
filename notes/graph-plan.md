# Plan: the counterexample as a graph, in the engine

Started 2026-09-17 (takeover turn 16; David: "build out an implementation that's good
enough to productionise"). Basis: experiments 017–019 (`notes/experiments/017-graph`,
`018-graph-shrink`, `019-graph-identity`), whose design points (a)–(f) this implements.
Replaces the pool of timelines (decisions 74–77) as the nondeterministic representation.
Built the same day; decision 78 records the result. This file is the plan with its
outcome marked; `notes/design.md` describes what stands.

## Representation (`native/graph.rs`) — built

- A draw's **address** is the spans open at it, outermost first, each as
  `(label, ordinal)` — the ordinal counting earlier same-label siblings under the same
  parent. Computed after a run from `RunResult.spans` (`draw_addresses`), and at draw time
  by `NativeTestCase::open_span_frames` for the walk; the two agree by construction.
- A **state** is where a run is between two draws, identified by the prefix of the next
  draw's address through its first frame not open at the previous draw. `Start` (before
  the first draw) and `End` (after the last) are distinguished identities.
- **Nodes are identities**: one node per identity, so inserting a run is the identity merge
  of experiment 019 (`ident`), and there is no merge move. Edges are `(address, value) →
  target`; several edges with the same address and value and different targets are a
  **tie**, settled by the identity the next draw reports (structure a hidden coin decides
  after a draw). The graph is not assumed acyclic (two runs may order sibling spans
  differently).
- Nothing is stored about the graph's paths; the reported example is a realized failing
  run (the incumbent), never a path read off the graph.
- Found in the build: `walk_verdict` must rule a run **foreign** (the walk serves another
  value at some state) before it rules an unknown later state a **gap** — a divergent
  replay's random run was otherwise grafted as a gap, and a value the walk never serves
  entered the incumbent. A node left without edges by a deletion is where a run ends: the
  edges into it lead to `End` (`delete_edge`), or the deletion of a terminal edge leaves a
  graph no run ends on.
- **Limitation**: two arms whose draws share an address (same open spans, same kinds, no
  span of their own) are one state; the graph serves one value there and the other arm is
  foreign. Arms need distinct spans, which generators supply and a bare `if` in a test body
  does not. Recorded in `design.md`'s risks.

## Replay (`core/replay.rs`, `Replay::graph`) — built

The walk holds the set of pending nodes (a tie's targets). At a draw it computes the
reported identity; no pending node of that identity is a **misjoin** (a divergence); the
draw is served first-fit from that node's edges at this address (no fitting edge: a
misfit, a divergence); on either the walk is rescued by the first node of the reported
identity with a fitting edge, and draws randomly when none. It records the edges it
**settled** on (the next identity picked the target, or the run ended on `End`), whether
it ended on `End`, and the divergence. Clone streams: the parent's clone edges at the
clone's address are the tie; the child replays their clone records as a live set
(decision 74's semantics inside the clone), and a child divergence is the walk's. The
live set over whole timelines (`for_counterexample`) survives only as a test seam.

## Lifecycle — built

- **Confirmation** (the discovery bar's batch): replays of the graph, every failing run
  grafted unless foreign — the batch is the identity merge. The bar's arithmetic is
  unchanged. A backtrack's candidate graph is the restored entry's run with the other
  reproducing history entries grafted in.
- **Reproduction** (`nd_reproduce`: reuse, blob, final replay): graph replays up to the
  budget, then the fresh tier where allowed. The splice tier goes: the rescue by identity
  is the splice.
- **Shrinking** (`native/graph_shrink.rs`, `EngineGraphProbe`): greedy passes over the
  graph — delete an edge, delete a span of the witness, shrink a value — each candidate
  judged by replays through the gauntlet (`nd::gauntlet`, clean failure = failed, no
  divergence, ended on `End`; a miss rejects at once in the fast sweep), a value edit
  accepted only when a judging replay settled on the edited edge (exercise by
  settlement). A failing divergent replay of a rejected candidate is grafted into the
  incumbent unless foreign. A value edit is compound: the edited graph is warmed up (its
  failing divergent replays grafted into it) and the edited edge's unsettled tie
  alternatives pruned before it is judged — finding 5 of 019, and how a
  structure-determining value shrinks. Added in the build, the **span pass**: the witness
  without one span's draws, its later same-label siblings renumbered as the engine would
  number them, proposed as the graph of that run alone; when the test does not follow it,
  retried with the nearest earlier integer draws lowered by one, up to three — a list's
  count shrinks with its elements. The contract move is dropped: ordinals are positional,
  so removing a piece changes every later identity; the span pass does that work.
  Accepts raise the anchor (capped at `nd::anchor_ceiling()`), install graph, witness and
  longest run on the counterexample and persist.
- **Persistence**: `NdReproState` version 3 carries the graph; version 2 (timelines) is
  no longer read (unreleased format). Blob prefixes 2/3 unchanged in meaning.
- **Boost** races the incumbent's values and prefix mutants as before; a winner is
  installed with the stored graph grafted with its run (its own graph when foreign).
  **ND targeting** never used the pool and is unchanged.

## Not in this cut (recorded, to do)

- Deletion weighing settlement evidence rather than K silent replays (019 finding 6).
- Value coupling across draws (`sum`): the representation over-claims; nothing checks it.
- The warm-up that keeps going while the graph is one run (019 (f)).
- Shrinking a clone's records (they are deleted whole or kept).
- An engine-side cost measurement of the graph shrink against the pool's (016's bodies);
  the experiments measured the harness only.

Outcome (2026-09-21, experiment 020, decision 79): the cost measurement was made through
the pipeline and found the graph port had dropped decision 54's anchor top-up and the bar
was replaying the raw run; both fixed, and the warm-up (019 (f)) is now the confirmation
batch's behaviour. Still to do from this list: deletion weighing settlement evidence
(loop8 runs to the deadline one or two edges short), value coupling, clone records.
