# Experiment sequence

| # | Experiment | Question | Status |
| --- | --- | --- | --- |
| 001 | Shrink-statistics simulation (`/experiments/shrink-sim`) | Calibrate gauntlet/ledger/stopping constants; drift, thrash, cost vs quality across accept policies; does checkpointing add anything; gamma tradeoff on deceptive landscapes; pass-repetition vs per-candidate-N cost | done — results and follow-ups in `001-shrink-sim/notes.md`; settled: confirmed-dry stopping, checkpointing dropped, anchor decay rejected |
| 002 | Cache seam + fixate cost | Cost of one fixate iteration with tree dedup off, on a real ~50-node target; where the resampling seam goes in `cached_test_function` | done — seam is `Engine::serve_replays` (prototyped); re-execution ~4us/replay engine overhead at 50 nodes; body cost dominates |
| 003 | Flat-timeline shrink in-engine | Do gauntlet + ledger tame drift on real synthetic flaky tests (controlled p via hidden per-execution counter/PRNG); final true p, size, executions | done — gauntlet holds never-lower-p in the real shrinker (L4: 99% bug kept vs naive 3%); two design-predicted leaks demonstrated and gated (raw `update_interesting` displacement, discovery confirmation); see `003-nd-shrink/notes.md` |
| 004 | Replay semantics | Pool fallback + continuation budgets in `resolve_choice`; structural divergence detector; instrument fall-off points and prefix sharing (feeds deferred anchor/trie decisions) | done — pool cap 5-10 first-fit, extend ~4 suffices, trie rejected harder (sharing anticorrelated with need), divergence = evidence weight not abort; see `004-replay-semantics/notes.md` |
| 005 | Lifecycle | Confirmation, capture-at-confirmation, persistence gating, unified reporting, blob v2, strictness setting | done — bar derived by exact DP (gate 1/10 then 4/40, decision 23); lifecycle validated end-to-end in-engine (139/139 cross-run reproduction, 0 false confirms on noise, caveated reporting live); confirmation must gate origin admission on every path (decision 24); blob v2 + strictness deferred as format/API work; see `005-lifecycle/notes.md` |
| 006 | Cross-timeline grafting + boost phase | Donor splicing at span granularity; successive-halving boost | done — positional splices rescue 65-100% of pool replay misses (per-position anchoring closed, decision 25); boost closes the deterministic-core tail and trades size for reliability elsewhere (ships behind policy); see `006-graft-boost/notes.md` |
| 007 | Clone streams and concurrency under ND handling | Do clone-bearing and concurrent-stateful bodies survive the full ND pipeline; clone serialization format | done — full pipeline works on clone-bearing bodies, both workloads at ceiling, clone serialization stays values-only (decision 32); drove the phase-7 unification; see `007-concurrent/notes.md` |
| 008 | Gauntlet calibration under the shipped rules (`/experiments/shrink-sim`) | Does the shipped parameterization degenerate to single-run accepts (critique S1-S6); which remediation constants restore the 003 numbers | done — min-fails 4, derived floor 0.05, 20-run anchor seeding, high-water 0.8, boost floor 0.30, frozen by 009a's weighting measurement (decision 57); see `008-gauntlet-calibration/notes.md` |
| 009a | Off-ceiling watermark measurement (`/experiments/watermark`) | What the shipped watermark records on genuinely racy bodies (G9); off-ceiling DB-reuse and blob rates (G10) | done — W50 0.28-0.44, no mass at zero (old weighting was the w = 0 column in practice), escalation signal not fired, reuse/blob >= 98% at p <= 0.3; G9/G10 closed no-change (decisions 57/58), 008 constants frozen; found and fixed the `bind_deletion` stale-index crash; see `009a-watermark/notes.md` |

Each experiment gets `notes/experiments/NNN-name/notes.md`: spec before, results and what we
learned after. Update `design.md` when a result changes the design; log reversals in
`decisions.md`.

## Outcome (2026-09-02)

All six experiments done. What the implementation inherits, beyond the per-experiment notes:

- **Statistics** (001, 005A): gauntlet accepts at LCB >= max(0.8 x anchor, 0.05), monotone
  anchor never fed by pinned-regime evidence, confirmed-dry stopping, no checkpointing, no
  decay; discovery bar = gate 1/10 then 4/40 (decision 23).
- **Mechanism** (002, 003, 005B): `serve_replays` is the whole resampling seam; confirmation
  gates origin admission on every path (decisions 20, 21, 24); the lifecycle runs end to end
  in-engine with 100% cross-run reproduction and correct caveated reporting.
- **Representation** (004, 006B): whole-timeline pool, cap 10, first-fit, small continuation
  budget; replay order pool -> splices -> fresh (decision 25); trie and per-position
  anchoring closed.
- **Boost** (006A): works, cheap, holdout-gated; ships behind reporting policy.
- **Deliberately left for implementation**: blob v2 format, strictness setting surface,
  demote-to-secondary, span-anchored split points, FAILED_NONDETERMINISTIC ABI semantics,
  generation strategy under ND (novel-prefix replacement; small budgets starve narrow
  structural bugs).

The engine scaffolding (`nd_experiment`, `nd_boost`, the `nd_*` fields in `test_runner.rs`)
is experiment-grade. Per decision 26 it is the seed of the real implementation: this branch
is brought to production quality in place (see `notes/production-plan.md`), and extraction/
pruning happens later, from the production-grade branch.
