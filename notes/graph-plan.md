# Plan: the counterexample as a graph, in the engine

Started 2026-09-17 (takeover turn 16; David: "build out an implementation that's good
enough to productionise"). Basis: experiments 017–019 (`notes/experiments/017-graph`,
`018-graph-shrink`, `019-graph-identity`), whose design points (a)–(f) this implements.
Replaces the pool of timelines (decisions 74–77) as the nondeterministic representation.

## Representation (`native/graph.rs`)

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

## Replay (`core/replay.rs`, `Replay::graph`)

The walk holds the set of pending nodes (a tie's targets). At a draw it computes the
reported identity; no pending node of that identity is a **misjoin** (a divergence); the
draw is served first-fit from that node's edges at this address (no fitting edge: a
misfit, a divergence); on either the walk is rescued by the first node of the reported
identity with a fitting edge, and draws randomly when none. It records the edges it
**settled** on (the next identity picked the target, or the run ended on `End`), whether
it ended on `End`, and the divergence. Clone streams: the parent's clone edges at the
clone's address are the tie; the child replays their clone records as a live set
(decision 74's semantics inside the clone), and a child divergence is the walk's.

## Lifecycle

- **Confirmation** (the discovery bar's batch): replays of the graph, every failing run
  inserted — the batch is the identity merge. The bar's arithmetic is unchanged.
- **Reproduction** (`nd_reproduce`: reuse, blob, final replay): graph replays up to the
  budget, then the fresh tier where allowed. The splice tier goes: the rescue by identity
  is the splice.
- **Shrinking** (`native/graph_shrink.rs`): greedy passes over the graph — delete an
  edge, shrink a value — each candidate judged by replays through the gauntlet
  (`nd::gauntlet`, clean failure = failed, no divergence, ended on `End`; a miss rejects
  at once in the fast sweep), accepted only when a judging replay settled on the edited
  edge (exercise by settlement). A failing divergent replay of a rejected candidate is
  grafted into the incumbent unless foreign (the incumbent's walk would have served a
  different value). A value edit is compound: the edited graph is warmed up (its failing
  divergent replays grafted into it) and the edited edge's unsettled tie alternatives
  pruned before it is judged — finding 5 of 019, and how a structure-determining value
  shrinks. The contract move is dropped: ordinals are positional, so removing a piece
  changes every later identity; deletion after a value edit does its work.
- **Persistence**: `NdReproState` version 3 carries the graph; version 2 (timelines) is
  no longer read (unreleased format). Blob prefixes 2/3 unchanged in meaning.

## Not in this cut (recorded, to do)

- Deletion weighing settlement evidence rather than K silent replays (019 finding 6).
- Value coupling across draws (`sum`): the representation over-claims; nothing checks it.
- The warm-up that keeps going while the graph is one run (019 (f)).
- Boost and ND targeting under the graph (they used pool timelines as seeds).
