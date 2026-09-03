# Decision-log evaluation

Phase 8's closing audit: every entry in `decisions.md`, with where it lives in the
implementation or why it needed no code. Written 2026-09-03 against the phase-7 tree.
File paths are engine-side (`hegel-c/src/native/`) unless noted.

| # | Decision | Status |
| --- | --- | --- |
| 1 | Strictness defaults to quiet | Implemented: `settings.rs` (`NondeterminismStrictness`, default `Quiet`), `hegel_settings_set_nondeterminism_strictness`, honored in `test_runner.rs::nd_flip`/`test_function_tagged`. Pinned by `tests/test_flaky_replay.rs` (both strictness behaviors) |
| 2 | Shrinking must not lower failure probability | Implemented as the gauntlet: `nd/mod.rs::gauntlet` (4-failure minimum, LCB must clear `gamma * anchor` with floor 0.05, anchors seeded from 20-run batches — decisions 54/55 after experiment 008 showed the shipped rule degenerated to single-run accepts below anchor 0.258). γ = 0.8 is the accepted tolerance from the old deferred-decision table, rising to 1.0 for zero-miss incumbents at the 0.8 high water — not strict never-lower, which paralyzes shrinking at small samples. Boost raises p when the anchor is unreliable (decisions 28/56) |
| 3 | Unreproduced failures still fail the run | Implemented: `nd/lifecycle.rs::caveat` unconfirmed wordings; the frontend re-raises the captured payload (`src/run_lifecycle.rs::drive_run`). Pinned by the quiet-strictness vanishing-failure test and the unconfirmed concurrent tests |
| 4 | Per-origin identity | Unchanged: the interesting map and the frontend capture map key on the origin string |
| 5 | Timeline pool, not a merged trie | Implemented: `blob.rs::NdReproState.timelines` (incumbent first), pool cap 10 (`nd/mod.rs::POOL_CAP`), harvested at confirmation |
| 6 | Data tree never serves under ND | Implemented: `nd_active` gates `cached_test_function` serving, recording, novel prefixes, targeting. Restoring the tree was not attempted (gate G3, decision 29) |
| 7 | Charge accepts, not rejects | Implemented: single-run rejects with ledger retention, gauntleted accepts, pass repetition with cumulative evidence (`test_runner.rs` shrink probe + `shrinker/`) |
| 8 | Persist representation, never estimates | Implemented: v2 entries/blobs are `NdReproState` only — no rates, no counters (`blob.rs`) |
| 9 | Detection uses within-run evidence only | Implemented: a stored entry that misses is demoted/deleted (`test_runner.rs` reuse loop), never a flip source; flips come from `record_run` mismatches and declared concurrency |
| 10 | Capture at confirmation | Implemented as the stamp contract: `capture_replays` around confirmation batches, DB-reuse replays, the final replay, and blob replays; `hegel_test_case_is_nondeterministic` tells the client to capture. The sacrificed first case and `NondetStash` are gone |
| 11 | DB hygiene: demote then delete | Implemented: reuse loop `move_value` to secondary on primary miss, delete on secondary miss; budgets from `nd/mod.rs::reuse_replay_budget` |
| 12 | Workload priorities | Served: experiment 007 measured both workloads at ceiling; concurrent stateful is fully unified (phase 7) |
| 13 | Scope: ignore parallel-tests branch, Antithesis | Respected; the Antithesis integration path is untouched |
| 14 | Per-position anchoring deferred, decided by data | Closed by decision 31: whole-timeline + splices reproduce at ceiling; nothing built. Re-confirmed off-ceiling by 009a: reuse/blob >= 98% at p <= 0.3 and the escalation signal did not fire (decisions 57/58) |
| 15 | Branch process | Superseded by decision 26 (branch to production grade); this audit is part of that |
| 16 | p >= 0.1 target drives budgets | Implemented: `nd/mod.rs::TARGET_FAILURE_RATE`/`replay_budget`; the discovery bar and reuse budgets derive from it |
| 17 | No checkpoint/rollback in the shrink loop | Implemented by absence; capture-at-confirmation handles the pinning hazard at source |
| 18 | Confirmed-dry stopping | Implemented: the shrink probe's sweep modes (`SweepMode`, `set_sweep_mode`) run one confirmation sweep after a dry sweep |
| 19 | Monotone anchor; replay evidence never raises it | Implemented: `nd/lifecycle.rs` anchor raises only on gauntlet first-accepts and boost holdouts; `measure()` provenance keeps replay evidence out |
| 20 | Raw interesting never displaces an occupied origin | Implemented: `update_interesting` fills vacant origins only; displacement goes through validated accepts |
| 21 | Discovery confirmation is a prerequisite | Implemented: `nd_discovery_sweep` after each generation step; unconfirmed origins are dropped and generation keeps hunting |
| 22 | Pool cap 5-10, first-fit, small continuation budget, divergence weights evidence | Implemented: `POOL_CAP = 10`, `continuation_budget(len) = len + max(4, len/8)`, `verbatim_weight` on misses |
| 23 | Discovery bar: 10 gate / 40 cap / accept on 4th failure | Implemented: `GATE_RUNS`, `CONFIRM_CAP`, `CONFIRM_MIN_FAILS` in `nd/mod.rs`, exercised by `nd_confirm` |
| 24 | Confirmation gates admission; DB reproduction is trusted | Implemented: `needs_confirmation` sweep + `OriginLifecycle::trust`. Reworded by decision 47: trusted origins are exempt from the bar's *verdict*, not spared replay — the shrink-time evidence batch is real and intended, and a failing one promotes |
| 25 | Replay order: first-fit, splices, fresh; boost off outside rescue | Implemented: `nd_reproduce` (splices = 10 — decision 52 corrected a transcription of 006B's cap — fresh only where the caller allows), `nd_boost` behind the 0.5 reliability floor |
| 26 | This branch goes to production grade | This phase; the full gate run and this audit are its exit |
| 27 | G1: FAILED + caveat accessor, status 3 retired | Implemented: `hegel_failure_caveat`, status 3 deleted from `hegel-c/src/lib.rs`; changelogs call out the break. Binding survey done, recorded in production-plan.md phase 6: ts/ocaml never adopted status 3, go's handling is dead but harmless, cpp vendors the header. The enum rustdoc reserves value 3 against reuse |
| 28 | G2: boost as reliability-floor heuristic | Implemented: `BOOST_RELIABILITY_FLOOR`, holdout-gated, no public setting. Recalibrated by decision 56: 0.30 in 20-run-batch LCB units (the literal 0.5 over-triggers against honest anchors), holdout raised to `ANCHOR_SEED_RUNS` |
| 29 | G3: data tree disabled under ND | Implemented (see 6); kind-set tolerance not needed on measured workloads |
| 30 | G4: strictness surface | Implemented (see 1); `error` reproduces the old abort diagnostics verbatim (`flaky_diagnostic`, pinned by tests) |
| 31 | Decision 14 closed | Recorded with 007 data; no anchoring code exists. 009a's off-ceiling rates keep it closed (decision 58) |
| 32 | Clone serialization values-only | Implemented by keeping tag 5 as-is; measured in 007 |
| 33 | ND blobs replay until failure | Implemented: `reproduce_blob` / `hegel_run_start_blob` / frontend `drive_blob_replay` |

Residual items, deliberately not done on this branch:

- The binding survey for the retired status 3 (decision 27) is done — recorded in
  production-plan.md phase 6; what remains for the release that ships the header is
  hegel-cpp's compile-time migration on its next header sync.
- Kind-set tolerance for the data tree (decision 29's follow-up) waits for a workload where
  no-tree generation cost shows up; 007 found none.
- `replay_aligned` under ND stays as accepted re-shrinking (005B priced it); revisit if slow
  real bodies bite.
