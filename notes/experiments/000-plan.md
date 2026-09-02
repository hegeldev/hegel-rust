# Experiment sequence

| # | Experiment | Question | Status |
| --- | --- | --- | --- |
| 001 | Shrink-statistics simulation (`/experiments/shrink-sim`) | Calibrate gauntlet/ledger/stopping constants; drift, thrash, cost vs quality across accept policies; does checkpointing add anything; gamma tradeoff on deceptive landscapes; pass-repetition vs per-candidate-N cost | first results in `001-shrink-sim/notes.md`; follow-ups pending |
| 002 | Cache seam + fixate cost | Cost of one fixate iteration with tree dedup off, on a real ~50-node target; where the resampling seam goes in `cached_test_function` | pending |
| 003 | Flat-timeline shrink in-engine | Do gauntlet + ledger tame drift on real synthetic flaky tests (controlled p via hidden per-execution counter/PRNG); final true p, size, executions | pending |
| 004 | Replay semantics | Pool fallback + continuation budgets in `resolve_choice`; structural divergence detector; instrument fall-off points and prefix sharing (feeds deferred anchor/trie decisions) | pending |
| 005 | Lifecycle | Confirmation, capture-at-confirmation, persistence gating, unified reporting, blob v2, strictness setting | pending |
| 006 | Cross-timeline grafting + boost phase | Donor splicing at span granularity; successive-halving boost | pending |

Each experiment gets `notes/experiments/NNN-name/notes.md`: spec before, results and what we
learned after. Update `design.md` when a result changes the design; log reversals in
`decisions.md`.
