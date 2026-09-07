# Detection

Every run starts deterministic. Detection is the machinery that notices when that assumption
fails and switches the run into ND handling. The switch is one sticky flag, `Engine.nd_active`
(`hegel-c/src/native/test_runner.rs`), set by `nd_flip` and never cleared within a run. This
chapter covers what triggers that flag and what it changes. What happens to an origin after
the flip belongs to [the lifecycle](lifecycle.md).

## Strictness

`NondeterminismStrictness` has three values, `Quiet`, `Warn`, and `Error`, and defaults to
`Quiet` (decisions 1, 30). The engine's copy lives in `hegel-c/src/settings.rs`. The Rust
frontend mirrors it in `src/runner.rs`, where the `Settings::nondeterminism_strictness`
builder method sets it, and the value reaches a test through `Hegel::settings` or the
`settings` parameter of `#[hegel::test]`. Other bindings set it over the ABI with
`hegel_settings_set_nondeterminism_strictness`, which takes a
`hegel_nondeterminism_strictness_t` (`hegel-c/src/lib.rs`). There is no CLI or environment
surface for it.

At a detection event the three values behave as follows.

**Quiet** flips without printing anything. The run switches into ND handling: failures are
confirmed by repeated replay before they are shrunk or persisted, every report carries a
caveat quoting the run's replay evidence, and an unconfirmed failure still fails the run
(decision 3). Nothing
below Debug verbosity reveals the flip or its measurement cost except the statistics line
described below. This is an accepted risk (G17).

**Warn** flips identically and prints `nondeterminism_notice()`
(`hegel-c/src/native/test_runner.rs`) once per run at the flip, suppressed under
`Verbosity::Quiet`: "Nondeterministic test behavior detected: failures are now confirmed by
repeated replay before being shrunk or persisted, and unconfirmed failures are reported with a
caveat. …".

**Error** aborts instead of flipping, for suites that use determinism as a lint: the detecting
site returns a `RunError` and the run ends with no failure report. Decision 30 splits the
error type by axis: `RunError::NonDeterministic` for generation drift (a kind-ledger
contradiction, or a first-check structural divergence) and `RunError::Flaky` for an outcome
change (a cache verdict mismatch, an aligned first-check miss, a shrink-verify miss, a
final-replay miss). The diagnostics reproduce the pre-branch aborts verbatim
(`flaky_diagnostic()` and the tree's kind-drift wording), except `first_check_diagnostic`,
which is richer: it names the divergence position and quotes the choice that changed
(decision 64).

One channel is an exception under `Error`. A stored v2 database entry or blob does not flip the run: it
replays with `nd_active` still false, and a database entry's reproductions displace and persist
through the `reuse_replays` exemption (decision 65). A stored entry that stops reproducing is staleness,
never nondeterminism evidence (decision 9).

There is one escape hatch: `Settings::nd_force`, a test-only field documented as "Not
reachable from any public API" (`hegel-c/src/settings.rs`). It starts the engine flipped at
construction, with no detection event and no seam site, so tests exercise the ND machinery
deterministically. A forced run skips the first-interesting check, because there is nothing
left for it to detect.

## The flip and nd_active

`nd_flip` (`hegel-c/src/native/test_runner.rs`) is idempotent. On the first call it sets
`nd_active`, clears the execution cache (post-flip, identical timelines need not conclude
identically, so nothing recorded pre-flip may be served or compared again), clears the kind
ledger, resets the duplicate counter, and prints the warn notice when strictness is `Warn`.
Downstream code reads the flag directly or through its alias `Engine::nd_handling()`.

While `nd_active` is set:

- The execution cache neither records nor serves. Every replay executes the body, since
  serving the first recorded verdict is exactly the bias the multi-run machinery exists to
  avoid (experiment 002).
- The duplicate stop is frozen at zero, and `record_execution` is skipped entirely, so the
  kind ledger goes unfed too.
- Targeting switches modes (decision 68, superseding decision 39's full disablement).
  `Optimiser::budget_exhausted` (`hegel-c/src/native/targeting.rs`) still treats
  `nd_active` as exhaustion, so a flip stops an in-flight deterministic climb, but the
  target phase keeps firing: under ND handling it runs `optimise_targets_nd`, a
  holdout-gated race that trusts no single run (see [shrinking](shrinking.md)). Span
  mutation stays on too.
- A raw interesting run fills only a vacant origin, and displacement of occupied origins is
  frozen (decision 20). Admission from there is [the lifecycle](lifecycle.md)'s business.
- Reports and persistence switch to v2 ND state with caveats (see
  [persistence](persistence.md) and [the ABI and frontend](abi-frontend.md)).

Concurrency is not a flip source (decision 70): a state machine with `max_concurrency > 1`
carries no declaration, because a properly serialized concurrent machine can fail
deterministically. Concurrent runs flip through the observational channels like any other
run, and once flipped flow through the same ND pipeline (experiment 007).

Under the `__bench` feature, `seam_flip` records each flip's detection site, call count, and
the interesting map at flip time for experiment 011's seam dump. The `FlipSite` enum
(`hegel-c/src/native/nd/mod.rs`) names the seven sites: `Concurrency`, `CacheMismatch`,
`FirstCheck`, `ShrinkVerify`, `FinalReplay`, `StoredV2Reuse`, `StoredV2Blob`. A companion
`flip_site_hint` lets the shrink verify and the deterministic final replay claim a cache
mismatch detected inside their own replay, rather than having it attributed to the generic
cache channel.

The flip's one surface below Debug verbosity is the statistics block: whenever any measurement
ran, it prints `* nondeterministic handling: measurement replays {N}, failing {M}`
(`RunStatistics::render_measurement`, `hegel-c/src/native/events.rs`). The counts accumulate
in `record_run` while `nd_active` or `check_window` is set, so first-check replays count
despite running pre-flip (decision 51, amended by 64). `show_statistics` is settable from the
frontend via the `HEGEL_STATISTICS` environment variable.

## Detection channels

### Execution-cache verdict mismatch

While the run is deterministic, `record_run` feeds every executed conclusion through
`record_execution` to `ExecCache::record` (`hegel-c/src/native/exec_cache.rs`), keyed on the
run's serialized realized values. A digest hit whose stored verdict differs in status or
origin is the flake the data tree could never see, returned as `Recorded::verdict_mismatch`
and surfaced as `RunError::Flaky(flaky_diagnostic())`. In `test_function_tagged` that error
aborts under `Error` strictness, and otherwise the run flips and the error is swallowed.
Overruns enter neither cache tier, because a proposal that ran out of data concluded nothing.

### Kind ledger

The `KindLedger` (`hegel-c/src/native/exec_cache.rs`) is the generation-drift detector,
maintained only under `Error` strictness (decision 62). It maps a 128-bit rolling FNV-1a hash
of each serialized value prefix to the `ChoiceKind` drawn at the next position, constraints
included, so a `min_value` shift is a kind change. A within-run contradiction returns the
tree's diagnostic verbatim ("the choice kind changed from X to Y") as `RunError::NonDeterministic`.
It is never fed between runs: the planned reuse-comparison channel was rejected as a
violation of decision 9, since every legitimate generator refactor would abort the next run
against its old entries. At `KIND_LEDGER_CAP` entries it stops learning but keeps checking,
which degrades detection without affecting correctness. Quiet and warn do not maintain it.
Their generation-level channel is the first-interesting check, which replays a recorded
sighting where the tree's version fired 0 times in 600 trials (experiment 011).

### First-interesting determinism check

`first_check_sweep` (`hegel-c/src/native/test_runner.rs`) is the universal per-origin
determinism check (decision 64, extending decision 21's principle to every run). The history
is in [the seam plan](../part2/seam-plan.md). It runs while the run is still deterministic, at
the same call sites as the discovery sweep and before it: after each generated case and once
after the generation loop. For each interesting origin not yet in `first_checked`, it replays
the incumbent's exact choices `FIRST_CHECK_REPLAYS = 4` times, stopping at the first miss. A
replay reproduces only if it concludes interesting at the same origin with the same realized
values. Detection probability is 1 − (p·s)⁴ for a bug failing at rate p with seam survival s.
A deterministic origin pays exactly +4 executions, and a run that finds no bug pays nothing.

On a miss under `Error`, a structural divergence aborts as `RunError::NonDeterministic` with
the diagnostic that names the position, and an aligned outcome-only change aborts as
`RunError::Flaky`. Under quiet and warn, the check seeds the origin's evidence
(`seed_evidence`) and flips: the observations pre-fill the origin's first discovery-bar batch
so they are not paid for twice, and seeding is not a rejection (see
[the lifecycle](lifecycle.md)). The replays run stamped for capture and inside the check window
(`Engine.check_window`), which counts them on the statistics line and attributes a cache
mismatch they trigger to the `FirstCheck` site. Three cases are exempt: database-reuse
reproductions are marked `first_checked` at the reuse site (the reuse phase already replayed
them), `nd_force` starts flipped and skips it, and origins first admitted at shrink verify or
final replay keep decision 35's path.

### Shrink verify and final replay

While the run is deterministic, each origin's shrink opens with one exact replay of its
incumbent (`shrink_origin`). A replay with the wrong status or origin aborts as `Flaky` under
`Error` and otherwise flips at the `ShrinkVerify` site. Likewise `final_replay` replays each
shrunk incumbent exactly once before reporting. A miss there aborts under `Error` and otherwise
flips at the `FinalReplay` site, after which origins already replayed re-enter the queue for
the pooled review (see [the final replay](final-replay.md)). A cache mismatch inside either
replay flips the run through the same channel.

### Concurrency (not a channel)

There is no declared-concurrency channel (decision 70; it existed until 2026-09-07).
Creating a state machine with any concurrency bound changes nothing about detection: a
machine whose failure reproduces exactly stays deterministic end to end (pinned by
`a_worker_panic_is_reported_with_its_real_origin_and_buffered_output`), and a racy machine
flips at whichever observational channel its behaviour first trips — usually the
first-interesting check or a cache verdict mismatch. The old declaration also carried the
one `Error`-strictness exception, so its removal makes `error` uniform: every detection
aborts, threads or not.

### Stored v2 state

ND-ness is carried by the representation (decision 8), so encountering it flips the run
before any replay. In the reuse phase, a database entry that decodes as ND state flips unless
strictness is `Error`, and `reproduce_blob` flips likewise on a `DecodedBlob::Nd`. Under
`Error` the entry still replays, with `nd_active` false throughout (see the strictness
section above and [persistence](persistence.md) for the v1/v2 formats).

## What replaced the data tree

The data tree is gone (decision 60, closing decisions 6 and 29). Experiment 010 priced its
roles: recording alone cost 40–80% wall overhead, and its serves were almost entirely exact
repeats. The removal's history lives in [the seam plan](../part2/seam-plan.md). What stands in
its place is the execution cache, the duplicate stop, and the kind ledger described above.

The `ExecCache` (`hegel-c/src/native/exec_cache.rs`) keys every executed conclusion on its
serialized realized values, with `serialize_choices` semantics (floats by bit pattern, clones
by child values). The digest tier holds a 128-bit FNV-1a fingerprint of that key for every
conclusion and is what duplicate detection and verdict-flip detection read. The full tier
holds complete serving entries (status, origin, nodes, spans), byte-bounded at
`FULL_TIER_MAX_BYTES` with oldest-first eviction. Full entries are kept only outside the
generation window (`keep_full = !collect_statistics`): generation keeps digests alone, because
its duplicates are the duplicate-stop signal and must execute.

`cached_test_function` is the single replay chokepoint for shrinking and span mutation. While
deterministic, an exact repeat of an executed conclusion is served from the full tier without
running the body (experiment 010's 85% shrink-serve win). Under ND handling nothing is served
and every replay executes.

The duplicate stop replaces the tree's exhaustion signal, rescoped by decision 61:
`DUPLICATE_STOP = 10` consecutive duplicate conclusions in the generation window end
generation only while no valid case exists, feeding the exhausted-space FilterTooMuch health
check. The planned unconditional stop ended a 32-way `one_of` before reaching every
alternative, because late in coupon collection a duplicate streak is routine. The counter
ignores measurement runs, resets on any novel conclusion, and is frozen under ND handling.
The tree's early exit on tiny all-passing spaces is given up as worthless.

## Health checks

No health check consults `nd_active`: the flip neither disables nor triggers any of them. All
are evaluated only while the interesting map is empty, and FilterTooMuch, TooSlow, and
TestCasesTooLarge additionally only while the run has fewer than `HEALTH_CHECK_MAX_VALID`
valid cases. Measurement runs cannot trip them, because `record_run`'s measurement guard moves
no counters. The separate invalid budget derives thresholds (458, 100) from
`INVALID_TARGET_RATE` and `INVALID_TARGET_CONFIDENCE`, so an always-reject test gives up after
459 cases.

The one ND-era rewiring is decision 61's: the exhausted-space FilterTooMuch variant reads the
duplicate stop instead of the tree's `is_exhausted`. The stop itself ends generation even when
FilterTooMuch is suppressed (only the error report goes away), pinned by
`duplicate_stop_stays_active_under_health_check_suppression` in
`hegel-c/tests/embedded/native/test_runner_tests.rs`.

## Detection-side constants

| Constant | Value | Where | Meaning |
|---|---|---|---|
| `FIRST_CHECK_REPLAYS` | 4 | `test_runner.rs` | Exact replays in the first-interesting check, stop on first miss; detection 1 − (p·s)⁴ |
| `DUPLICATE_STOP` | 10 (= `RANDOM_GENERATION_BATCH`) | `test_runner.rs` | Consecutive generation-window duplicates ending zero-valid generation |
| `FULL_TIER_MAX_BYTES` | 8 MiB | `exec_cache.rs` | Execution-cache full-tier byte budget, oldest-first eviction |
| `KIND_LEDGER_CAP` | 65536 | `exec_cache.rs` | Kind-ledger entry cap; past it, it stops learning but keeps checking |
| `FILTER_TOO_MUCH_THRESHOLD` | 50 | `test_runner.rs` | Invalid cases before FilterTooMuch, mirroring Hypothesis's `max_invalid_draws` |
| `INVALID_TARGET_RATE` / `INVALID_TARGET_CONFIDENCE` | 0.01 / 0.99 | `test_runner.rs` | Invalid-budget derivation, yielding thresholds (458, 100) |
| `TOO_SLOW_THRESHOLD` | 30 s | `test_runner.rs` | Cumulative generation wall clock before TooSlow; there is deliberately no per-case deadline setting |
| `HEALTH_CHECK_MAX_VALID` | 10 | `test_runner.rs` | Valid-case count past which FilterTooMuch/TooSlow/TestCasesTooLarge stop being evaluated |
| `MAX_OVERRUN_DRAWS` | 20 | `test_runner.rs` | Overruns tripping TestCasesTooLarge, mirroring Hypothesis's `max_overrun_draws` |
