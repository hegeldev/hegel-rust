# Experiment sequence

| # | Experiment | Question | Status |
| --- | --- | --- | --- |
| 001 | Shrink-statistics simulation (`/experiments/shrink-sim`) | Calibrate gauntlet/ledger/stopping constants; drift, thrash, cost vs quality across accept policies; does checkpointing add anything; gamma tradeoff on deceptive landscapes; pass-repetition vs per-candidate-N cost | done — results and follow-ups in `001-shrink-sim/notes.md`; settled: confirmed-dry stopping, checkpointing dropped, anchor decay rejected |
| 002 | Cache seam + fixate cost | Cost of one fixate iteration with tree dedup off, on a real ~50-node target; where the resampling seam goes in `cached_test_function` | done — seam is `Engine::serve_replays` (prototyped); re-execution ~4us/replay engine overhead at 50 nodes; body cost dominates |
| 003 | Flat-timeline shrink in-engine | Do gauntlet + ledger tame drift on real synthetic flaky tests (controlled p via hidden per-execution counter/PRNG); final true p, size, executions | done — gauntlet holds never-lower-p in the real shrinker (L4: 99% bug kept vs naive 3%); two design-predicted leaks demonstrated and gated (raw `update_interesting` displacement, discovery confirmation); see `003-nd-shrink/notes.md` |
| 004 | Replay semantics | Pool fallback + continuation budgets in `resolve_choice`; structural divergence detector; instrument fall-off points and prefix sharing (feeds deferred anchor/trie decisions) | done — pool cap 5-10 first-fit, extend ~4 suffices, trie rejected harder (sharing anticorrelated with need), divergence = evidence weight not abort; see `004-replay-semantics/notes.md` |
| 005 | Lifecycle | Confirmation, capture-at-confirmation, persistence gating, unified reporting, blob v2, strictness setting | done — bar derived by exact DP (gate 1/10 then 4/40, decision 23); lifecycle validated end-to-end in-engine (139/139 cross-run reproduction, 0 false confirms on noise, caveated reporting live); confirmation must gate origin admission on every path (decision 24); blob v2 + strictness deferred as format/API work; see `005-lifecycle/notes.md` |
| 006 | Cross-timeline grafting + boost phase | Donor splicing at span granularity; successive-halving boost | done — positional splices rescue 65-100% of pool replay misses (per-position anchoring closed, decision 25); boost closes the deterministic-core tail and trades size for reliability elsewhere (ships behind policy); see `006-graft-boost/notes.md` |

Each experiment gets `notes/experiments/NNN-name/notes.md`: spec before, results and what we
learned after. Update `design.md` when a result changes the design; log reversals in
`decisions.md`.
