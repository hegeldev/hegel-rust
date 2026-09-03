# Adversarial review of the as-built implementation

2026-09-03, branch head 9c800e8e (post phase 8). Method: nine subsystem reviewers over the
full branch diff plus four design-foundations auditors, every finding adversarially
verified by independent skeptics; six findings died in verification and are omitted; the
high-severity survivors were re-verified by hand against the code, including the Wilson
arithmetic. `just check` is green at this head. Findings carry stable IDs; the fix work is
planned in `../remediation-plan.md`, which cites them. Status: **confirmed** =
code-verified; **plausible** = skeptics split, verify before fixing. Line refs are as of
9c800e8e and will drift; anchor on the named functions.

Verdict: the architecture holds, but the statistics don't compose. The constants were each
derived in an isolated experiment and the composition — bar → anchor → gauntlet/boost,
watermark → evidence — was never measured end to end. It breaks exactly at the p ≥ 0.1
target (decision 16) and on the flagship concurrent workload (decision 12). Separately:
two persistence-destroying bugs, one reporting hole violating decision 24, and a
documentation layer that has drifted from the code it claims to describe as-built.

## S: statistical foundations

- **S1 (high, confirmed).** The gauntlet degenerates to naive single-run accepts across
  the design-target regime. Wilson LCB(1 fail/1 run) = 0.2065; accept threshold
  max(0.8·anchor, 0.05) with no minimum evidence; the recruiting failure counts as
  evidence; the verdict is checked before any rerun (`nd/mod.rs:121-131`,
  `test_runner.rs:2006-2026`). Any anchor ≤ 0.258 means every candidate is accepted on its
  single recruiting failure, and after the first such accept the anchor equilibrates at
  0.2065, so single-run accepts continue indefinitely. Bar-seeded anchors are ~0.04 at
  p = 0.1 and cap at
  LCB(4/4) = 0.51, so the whole target regime is degenerate. This is the P0 policy
  decision 7 rejected; experiment 001 measured P0 losing the bug in 34% of noise-floor
  trials. `mod_tests.rs:196-203` pins the degenerate case as intended.
- **S2 (high, confirmed).** The shipped anchor seeding was never simulated: 001/003 seeded
  anchors from 20-run confirmation batches (001 notes:46-47, 003 notes:12,20); shipped
  seeds from the bar's early-accept-at-4-fails batch (`test_runner.rs:701`, `:1644-1650`).
  The drift-protection numbers design.md cites under "Goals (met)" came from a materially
  stronger mechanism.
- **S3 (medium, confirmed).** Boost floor units error: `BOOST_RELIABILITY_FLOOR = 0.5`
  gates on the anchor, an LCB whose confirmation-time ceiling is 0.5101, so boost runs on
  every confirmation short of a perfect 4-for-4 — always-on in practice (~130-190
  `measure()` executions per origin), which decision 28 chose the floor to avoid
  (`test_runner.rs:715`, `nd/mod.rs:141`).
- **S4 (medium, confirmed).** `GAUNTLET_FLOOR = 0.05` has no recorded derivation from the
  target; its interaction with LCB(1/1) = 0.2065 is what makes S1 permanent.
- **S5 (medium, confirmed).** Weighted-trial Wilson plus optional stopping (stop on 4th
  fail; per-run peeking up to 30 looks) is systematically anti-conservative, asymmetric
  (failures always weight 1.0), and nowhere documented as carrying no coverage guarantee
  (`nd/mod.rs:16-72,92-103,121-131`).
- **S6 (medium, confirmed).** Deterministic-core retention (decision 2 "stay there") is
  unenforced: accepts ratchet the anchor only to small fixed points (0.2065 low, 0.51 via
  confirmation, 0.7224 via boost holdout), so a deterministic incumbent is displaceable at
  meaningful per-proposal rates; 006's ~0.78 retention anchor is reachable only via
  pre-shrink boost.
- **S7 (high, plausible).** Gauntlet accepts raise the anchor and persist via
  `record_nd_incumbent` inside the probe (`test_runner.rs:2015-2025`), before the
  shrinker's sort-key adoption decision (`shrinker/mod.rs:373,419`); mutation passes
  propose never-adoptable sort-key-larger candidates that could ratchet the anchor.
  Persistence is guarded by the Persister's sort-key check, so the anchor raise is the
  live hazard. Verify reachability, then move raise/persist to the adoption point.
- **S8 (medium, confirmed).** Decision 19's wording ("post-accept evidence never feeds the
  anchor", "evidence under timeline replay must not raise the anchor") is contradicted by
  both shipped anchor sources — gauntlet ledgers and boost holdouts are replay-sourced.
  The anchor's estimand (pinned-timeline reproduction rate vs fresh-region rate) is
  undefined; needs a clarifying decision rather than a doc fix.

## W: divergence weighting

- **W1 (high, confirmed).** `verbatim_weight` is element-granular over top-level
  `ChoiceValue`s and an entire clone stream is one element with whole-record equality
  (`nd/mod.rs:182-195`, `choices.rs:719,806-826`), so on concurrent machines any
  intra-stream divergence zeroes the miss weight. Evidence degenerates to a fail counter
  (a p ≈ 0.1 concurrent origin can confirm with anchor 0.51); the gauntlet UCB stays ~1 so
  reject-by-proof never fires and every reject burns the 30-run cap; the discovery gate
  never fires so a fluke costs 37 replays, not the documented ~15. Decision 22's weighting
  was derived on flat scalar bodies (004's explicit no-spans/no-clones caveat); 007 ran at
  ceiling, so misses never exercised it. Partially reopens the question decision 14/31
  closed.

## R: report-path admission and capture

- **R1 (high, confirmed).** Unconfirmed origins are reported with v2 reproduce blobs
  alongside confirmed failures: the report loop (`test_runner.rs:838-848`) blobs every
  origin in `interesting` with no `needs_confirmation` check (the persistence filter at
  `:782` does check). Reachable via origins first admitted by `measure()` runs during
  `final_replay` (snapshot at `:1423` vs admission at `:1817-1820`; caveat renders
  "failed 0 of 0 replays") and via the dry-flip branch ignoring `reject()`'s evict signal
  (`:1473-1476`). Violates decision 24 and design.md's "unconfirmed failure … and no
  blob".
- **R2 (high, confirmed).** The decision-3 report for a never-reproduced failure contains
  no counterexample: the discovering case is unstamped, so no draw lines or diagnostic
  exist (`run_lifecycle.rs:290-292,328-346`, `test_case.rs:427-431`); the fallback reports
  origin + caveat only. Pinned as intended by `test_concurrent_stateful.rs:729-734`;
  neither decisions.md nor design.md acknowledges the loss.
- **R3 (high, confirmed).** Unstamped shrink probes clobber the per-origin capture:
  `drive_run` inserts a `CapturedReport` for every interesting case
  (`run_lifecycle.rs:576-586`); gauntlet/boost probes are unstamped and empty, and
  shrinking runs between confirmation and final replay, so a confirmed-but-dry failure
  prints an empty block, against the doc's "newest earlier capture" promise.
- **R4 (medium, confirmed).** A mid-verify ND flip skips the discovery bar: when the
  shrink-loop verify replay flips `nd_active` but still fails at the origin,
  `deterministic_verify` is taken (`test_runner.rs:663-683`), so the origin reaches boost
  and the gauntlet with anchor 0.0, unbarred, and gauntlet accepts persist v2 entries for
  an unconfirmed origin.
- **R5 (medium, plausible).** Targeting observations are still recorded under `nd_active`
  (`record_run`, `test_runner.rs:1791-1794` has no guard) while the targeting phase is
  disabled — gate G3 half-on. Verify, then gate the recording or document it.

## L: trusted-origin lifecycle

- **L1 (medium, confirmed).** design.md:97 says the bar "is not re-run" for DB-trusted
  origins; the shrink loop runs the full bar on every one (`take_witness` is None for
  Trusted, so `test_runner.rs:686` falls to `nd_confirm` at `:690`), and promotes
  Trusted → Confirmed even on a non-accept, with the rejected batch's LCB as anchor
  (`:697-711`, `lifecycle.rs:210`). The re-run is pinned as intended
  (`test_runner_tests.rs:2091`); the docs and the promotion-on-non-accept semantics are
  not.
- **L2 (medium, plausible).** Shrink-time confirmation of a Trusted origin discards its
  stored v2 pool: `test_runner.rs:702-709` rebuilds the pool from this batch only;
  `confirm()` wholesale-replaces state (`lifecycle.rs:139-149`); nothing merges
  `nd_origins.pool(&origin)`, though `trust()` carefully preserves pools.
- **L3 (medium, confirmed).** `record_final_replay` is a no-op for Trusted origins, but
  Trusted origins reach the final replay (aligned reuse skips shrink at `:600`; a
  zero-fail shrink-time batch leaves Trusted via `:697-700`), so a trusted failure that
  goes dry at report time keeps its plain caveat — the promised wording switch never
  happens, and `lifecycle.rs:166-167`'s "only confirmed origins reach the final replay"
  is false.
- **L4 (low, confirmed).** `confirm()`'s Confirmed-prior arm (`lifecycle.rs:134-137`) is
  unreachable, and if reached would overwrite the monotone anchor downward.
- **L5 (low, confirmed).** `unconfirmed()`'s rejection-count payload
  (`lifecycle.rs:283-292`) has no production consumer.
- **L6 (medium, confirmed).** The dry-at-report caveat folds report-time replays into the
  counts it attributes to "confirmed earlier this run" (`lifecycle.rs:168-180,245-252`) —
  the string contradicts its own numbers.

## P: persistence hygiene

- **P1 (high, confirmed).** The pre-shrink secondary drain deletes v2 entries it never
  replayed: `test_runner.rs:626-640` replays only what `deserialize_choices` accepts (v1),
  then deletes unconditionally; a v2 entry demoted on a primary miss (strike one, `:396`)
  is destroyed with zero strikes when any shrink phase runs. Violates decision 11.
- **P2 (high, confirmed).** The secondary corpus grows without bound while an ND failure
  stays live: every gauntlet accept persists and demotes the previous entry
  (`test_runner.rs:2023-2024` → `:1162-1165`), end-of-run reconciliation demotes the
  superseded primary (`:802-805`), `replay_aligned` essentially never holds under ND so
  every run re-shrinks and deposits one entry per accepted step, nothing reachable cleans
  them, and every run fetches the whole corpus (`:305-308`, `:613-616`).
- **P3 (low, confirmed).** `decompress_to_vec_zlib` on untrusted blob bytes has no output
  limit (`blob.rs:197,202`); miniz_oxide has a `_with_limit` variant.

## D: documentation drift

- **D1 (confirmed).** design.md:97-98 "the bar is not re-run" — false (see L1).
- **D2 (confirmed).** design.md:140-142 boost sentence wrong twice: candidates are
  incumbent + pool + mutant fills, not "16 mutation-generated variants"
  (`test_runner.rs:1537-1554`); the halving race scores raw failure rate, not "ledger LCB"
  (`:1571-1575`).
- **D3 (confirmed).** design.md's flip-source list omits the third source: decoding a v2
  entry or ND blob flips the run.
- **D4 (confirmed).** design.md:26 and both RELEASE.md files claim shrinking never lowers
  reproduction probability; the machinery enforces an LCB-vs-0.8·anchor bound at best and
  (pre-S1-fix) nothing below anchor 0.26. evaluation.md:10 already concedes "not strict
  never-lower".
- **D5 (confirmed).** Decision 19's absolutist wording vs the shipped anchor sources (S8).
- **D6 (confirmed).** Stamp-contract docs stale: `hegel-c/src/lib.rs:1634` ("under
  nondeterministic handling" is false — the stamp covers deterministic final replays too;
  blob replays omitted) and `src/ffi.rs:422` (retired contract). The symbol
  `hegel_test_case_is_nondeterministic` now means "capture this case".
- **D7 (confirmed).** `src/runner.rs:126` and `src/stateful.rs:921-924` claim failures are
  confirmed before reporting — contradicts decision 3 and the branch's own tests.
- **D8 (confirmed).** `Hegel::reproduce_failure` rustdoc (`src/runner.rs:489`) still
  describes retired single-replay semantics.
- **D9 (confirmed).** The ND-blob stale message (`run_lifecycle.rs:721-724`) omits the
  rare-failure hypothesis on the one path designed to miss 5% of live p = 0.1 bugs.
- **D10 (confirmed).** lifecycle.rs doc falsehoods: `:6` "only the discovery bar moves an
  origin to Confirmed" (false at two call sites), `:47` Trusted-pool claims, `:166-167`
  (see L3).
- **D11 (confirmed).** Smaller rot: `slow_shrink_warning` rustdoc fused onto
  `nondeterminism_notice` (`test_runner.rs:987-992`); `blob.rs:72` references removed
  `decode_failure`; `backend.rs:316` describes the retired reporting model;
  `hegel-c/src/lib.rs:663` claims nondeterminism errors the run without the strictness
  qualifier; `:5291` blob accessor doc not updated for v2; `src/ffi.rs:971` Drop comment
  names the removed `from_blob` constructor; `nd/mod_tests.rs:1` header names the wrong
  source path.
- **D12 (confirmed).** Decision 27 closure is inconsistent: production-plan.md:310-315
  records the bindings survey done; evaluation.md:35,45 says it "remains to do"; the enum
  carries no "value 3 retired, never reuse" note, so reserved-vs-removed is undecided in
  the artifact that matters.
- **D13 (confirmed).** Known-risks omissions: the statistics' bias direction under
  weighting + stopping; stalled-vs-finished shrink indistinguishable below Debug; the
  quiet flip undiagnosable below Debug (no statistics or verbosity surface reveals the
  flip or the measurement-run cost).

## M: constants, conventions, test gaps

- **M1 (low, confirmed).** POOL_CAP off-by-one: `< POOL_CAP` at capture
  (`test_runner.rs:1640,704`) vs `<= POOL_CAP` at persistence/final replay (`:1674`), so
  persisted state carries 11 timelines against a documented cap of 10.
- **M2 (medium, confirmed).** `REPRODUCE_SPLICES = 6` ships 006's mean cost per miss as
  the attempt budget; the 65-100% rescue rates were measured at a 10-splice cap; a fitted
  geometric puts budget-6 rescue near 47% on B5-class bodies. The doc comment
  (`nd/mod.rs:197-199`) misattributes the measurement.
- **M3 (low, confirmed).** `FINAL_REPLAY_FRESH = 4` is the one budget constant with no
  derivation.
- **M4 (low, confirmed).** Convention violations in new tests: `.expect("static")`
  (`embed_tests.rs:499`, `test_runner_tests.rs:~2540`), `let _ =` on unused draws
  (`test_flaky_replay.rs:47`).
- **M5 (confirmed).** Test gaps: nothing pins that `Evidence::record` ignores `weight` on
  failures (production relies on it at `test_runner.rs:1399`); nothing pins decision 33's
  no-fresh-tier for blob replay; no test seeds a v2 entry into secondary before a
  shrinking run (would have caught P1); splice/pool-cap constants unpinned.

## What held up

The review verified these as sound: the v2 blob format (version byte with
v3 headroom, bounds-checked parsing, 64-timeline cap, linear allocations); decision 8
field-by-field, with the content-hash entropy required for dedup; decision 32's
"only consumer" claim exactly (`state.rs:2438-2442`); the bar's operating points
re-derived by exact DP in the test suite; `error` strictness byte-identical to the old
diagnostics; caveats quoting physical counts; unconfirmed-suppression implementing
decisions 3+24 jointly; evidence keyed on realized timelines surviving pass boundaries;
the accounting split everywhere probed except R5; and the monotone anchor
preventing multiplicative threshold decay — the reason S1/S2 are a recalibration, not a
redesign.
