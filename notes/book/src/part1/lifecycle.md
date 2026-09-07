# The origin lifecycle

Once a run flips into ND handling (see [detection](detection.md)), an interesting execution stops being self-evident: any single failure may be a background fluke, and every claim about a failure must be bought with replays. The per-failure state that enforces this is `OriginLifecycle` (hegel-c/src/native/nd/lifecycle.rs), driven from `test_runner.rs`. The arithmetic it consults is pure and engine-state-free in hegel-c/src/native/nd/mod.rs.

## Origin identity

A failure's identity is its origin: the panic site rendered as a `file:line:col` string ("Panic at …", built by the frontend in src/run_lifecycle.rs and reported through `hegel_mark_complete`). Shrinking, persistence, lifecycle state and reporting are all per-origin (decision 4). Coalescing origins observed from one choice sequence was rejected: two panic sites stay two failures, even when one timeline reaches both.

## Sightings are selection, not evidence

The run that discovers an origin was noticed because it failed. Treating it as one Bernoulli observation of the failure rate conditions on the outcome: over a long generation phase, low-probability flukes get many chances to fire once, so first sightings over-represent exactly the failures least likely to reproduce. Experiment 003 measured this directly: at the noise floor a first-interesting execution was a fluke more often than a real bug (2:1), and letting raw runs displace incumbents allowed noise-floor flukes to displace a 20/20-confirmed discovery in about 80% of trials (decisions 20, 21).

So under ND handling a raw interesting run does exactly two things. It may fill a vacant slot in the interesting map (never displace an occupied one), and it calls `OriginLifecycle::observe`, which creates `Unconfirmed { fails: 0, replays: 0 }` on first sighting and never changes existing state: "a raw run is selection, not evidence." `observe` runs from `record_run` when a post-flip interesting conclusion fills a vacant origin, and again defensively at the final replay. Reproduction-rate claims come only from replays made for that purpose. As `nd_evidence_batch`'s doc puts it: "The triggering run is selection, not evidence — only these fresh replays count."

## Evidence and Wilson bounds

`nd::Evidence` is the counting type every replay-driven decision shares: `(fails, physical, weighted_misses)`. A failure counts in full. A miss counts its verbatim watermark (decisions 22, 45): the flat-length-weighted fraction of the stored timeline the replay tracked before first divergence. A verbatim miss weighs 1.0, an early-diverging replay weighs only its tracked prefix, and a diverged clone pair still earns credit for its tracked children, recursively. A diverged run said little about whether the stored timeline reproduces, so it rejects more slowly than a verbatim one. The physical count is kept separately: the statistics discount divergence but cost caps stay exact.

The watermark is what keeps divergence-heavy workloads measurable. Experiment 009a ran it on genuinely racy clone and state-machine bodies: median miss weights of 0.28 to 0.44 in every cell with no mass at zero, where the earlier scalar-prefix weighting had put 78-97% of misses at exactly zero and so fed the bar almost nothing but failures (decision 57).

`lower_bound` and `upper_bound` are Wilson score bounds at z = 1.96 over `fails / (fails + weighted_misses)`, clamped to [0, 1]. The constants below are built from these reference values: LCB(4/30) = 0.0531, the Wilson upper bound at 1/1 = 0.2065, LCB(20/20) = 0.839, LCB(10/20) ≈ 0.30.

## The discovery bar

`nd::discovery_bar` (decision 23) is the gate-then-extend accept/reject rule for admitting an unconfirmed origin:

- accept once `fails >= CONFIRM_MIN_FAILS`.
- reject on zero failures once weighted misses reach `GATE_RUNS`.
- reject when the quota is unreachable, that is when `fails + (CONFIRM_CAP − physical) < CONFIRM_MIN_FAILS`.
- otherwise continue.

| Constant | Value | Role |
|---|---|---|
| `GATE_RUNS` | 10 | zero-fail rejection threshold, in weighted misses |
| `CONFIRM_CAP` | 40 | physical replay cap per batch |
| `CONFIRM_MIN_FAILS` | 4 | failures an accept requires, taken early on the fourth |
| `ANCHOR_SEED_RUNS` | 20 | physical runs an accepting batch extends to before seeding an anchor |
| `POOL_CAP` | 10 | stored timelines per origin, incumbent included |

The operating points come from an exact-DP derivation (experiment 005A): 0.6% false accepts per p = 0.02 fluke, 45% per-discovery power at the p = 0.1 target, ~15 replays per rejected fluke, ~4.4 per p = 0.9 confirmation. The asymmetry is deliberate. A false accept is sticky, occupying the origin behind the displacement gate for the rest of the run, while a false reject recycles through rediscovery, so power is the cheap side of the trade. The rejected alternatives were a Wilson-LCB-over-noise-floor rule (26% false accepts) and an SPRT (50+ replays buying power that rediscovery gives free).

Real bodies move the cost letter but not the verdicts: experiment 009a measured fluke rejection at 26-27 physical replays against a target of 20 (a body property), with `CONFIRM_CAP` bounding the worst case at 37 (decision 57). The intervals themselves are not textbook-honest: per-run peeking, stop-on-fail, and the asymmetric miss weighting all bias them towards acceptance. The design treats the exact-DP operating points as the specification and z = 1.96 as a tuning constant, and experiment 008 measures the realized error.

## The evidence batch

`nd_evidence_batch` (hegel-c/src/native/test_runner.rs) is the bar's driver. It replays the origin's incumbent through `nd_replay_once`: one continuation-tolerant replay budgeted at `nd::continuation_budget(len) = len + max(4, len/8)`, the timeline plus `max(4, len/8)` fresh draws (experiment 004), where "failed" means concluded interesting at the target origin and a miss's weight is its watermark. The batch runs with `capture_replays` set so failing replays carry report material, records weighted evidence, collects failing realized timelines (deduplicated, up to `POOL_CAP`), and takes the first failing run as witness. Two rules keep its statistics honest.

**An accept needs an in-batch witness.** A batch starts from the origin's first-check seed when one exists, so its evidence can open with failures the batch itself never saw: "a first-check seed can carry the bar's whole failure quota, and a seeded quota with no in-batch reproduction rejects at `CONFIRM_CAP` physical runs instead of confirming an origin the batch never saw fail." On an accept verdict without a witness the loop keeps replaying, and if no replay in this batch fails by `CONFIRM_CAP` physical runs, the batch rejects. The witness is stored on `Confirmed` and taken once by `take_witness` as the shrinker's starting point, so a confirmation must rest on a reproduction the batch actually holds.

**An accept extends to `ANCHOR_SEED_RUNS` physical runs** before its LCB may seed an anchor (decision 54). Stopping at the accept itself biases the estimate towards the stopping rule: a four-straight-fail batch would seed 0.51 whatever the true rate. Twenty is the largest batch size whose all-fail LCB (0.839) a shrink candidate can still match within `GAUNTLET_CAP`, and seeding from 40-run batches stalls shrinking outright (LCB(40/40) = 0.912 exceeds the cap-reachable 0.887). A reject stops at the bar. What the anchor prices is the subject of [shrinking](shrinking.md).

For trusted origins the same batch runs with the bar arithmetic as its stopping rule only, since any failure is evidence enough (see below).

## First-check seeding

Before anything consumes a generation-discovered origin, its incumbent sighting is replayed `FIRST_CHECK_REPLAYS = 4` times exactly, and a miss flips the run. The check itself is [detection](detection.md)'s material. Its observations are not paid twice (decision 64): a miss deposits the check's evidence per origin via `seed_evidence`, and the origin's first evidence batch consumes it through `take_seed`, so "the discovery bar begins partially filled instead of from zero." Seeding is not a rejection: it records no bar verdict and changes no origin state, because the check is detection rather than judgement.

Not every origin passes through the check. Database-reuse reproductions are exempted at the reuse site, because their reproduction already replayed the entry. `nd_force` starts the run flipped and skips the check entirely. Origins first admitted at the shrink verify or the final replay keep decision 35's path.

## The three states

`OriginState` (hegel-c/src/native/nd/lifecycle.rs):

- `Unconfirmed { fails, replays }`: observed interesting but not past the bar. The counts accumulate the physical evidence behind rejected batches, for the caveated report.
- `Trusted { pool, fails, replays, report_fails, report_replays }`: reproduced from the database, so exempt from the bar's verdict and from eviction, carrying the stored v2 entry's timeline pool (empty for v1) but no anchor until promotion.
- `Confirmed { anchor, witness, pool, fails, replays, report_fails, report_replays }`: past the bar, or promoted from Trusted. The anchor is the monotone failure-rate estimate the gauntlet prices candidates against, the witness is the confirmation run the shrinker starts from, and the pool is the captured failing timelines, incumbent first.

Every stored pool obeys `POOL_CAP` = 10 timelines total, incumbent included: decision 22 measured K = 5 as near-ceiling and K = 10 as the plateau for reproduction, and the lifecycle's writers truncate incoming pools so the invariant holds at the single point of storage. The decode-side format bound is deliberately looser ([persistence](persistence.md)).

The transitions are exactly what the lifecycle's methods implement:

- Unconfirmed → Confirmed: a discovery-bar accept (the sweep or shrink admission), a backtrack's bar accept, or the final replay's pooled review, which confirms on any failure with no bar.
- Unconfirmed → Trusted: database reproduction.
- Trusted → Confirmed: promotion by a failing shrink-time evidence batch.
- Rejection never demotes and never removes state.

Confirming an already-confirmed origin is an internal error, and every `confirm` caller sits behind a `needs_confirmation` or `take_witness` check. `needs_confirmation` (true for absent or Unconfirmed origins) is the predicate the report partition and the end-of-run persistence filter share: an origin's replay state is usable only past it. Three smaller methods round out the surface. `raise_anchor` is monotone and a no-op unless Confirmed (decision 19). `record_final_replay` folds in the report-time replay counts apart from the confirmation or reuse counts, driving the caveat variants below. `pool` exposes the stored timelines for replay and persistence.

## Admission paths

**The discovery sweep.** `nd_discovery_sweep` runs under ND handling after each generation iteration and once after the loop. It loops because confirmation replays can themselves discover origins, and it runs per iteration rather than per generated case because span-mutation executions also fill vacant origins. Decision 24's motivating leak was exactly that: hooking confirmation on the generation run's own status let span mutation fill origins unconfirmed, producing false confirms in 26 of 30 pure-noise runs. Each origin still needing confirmation faces one evidence batch on its incumbent. An accept confirms with the batch LCB as anchor, its witness, and the captured timelines pooled behind the incumbent (`pooled_timelines`, the single build site for every pool, capped at `POOL_CAP` total). The origin's pre-flip history is dropped and the incumbent is persisted as a v2 entry ([persistence](persistence.md)). A reject evicts (next section).

**Shrink admission.** When shrinking reaches an origin under ND handling, `shrink_origin` climbs a ladder:

1. a stashed confirmation witness and anchor (`take_witness`, yielded once per confirmation).
2. for a trusted origin, an evidence batch (bar as stopping rule only).
3. for a never-confirmed origin with pre-flip history, a backtrack over that history, whose candidate faces the full bar up to `BACKTRACK_BAR_ATTEMPTS = 3` times. Each batch holds the bar's 45% target-regime power and three compose to ~83% ([the final replay](final-replay.md) covers the scan).
4. otherwise a fresh bar batch on the incumbent.

An accept here must hold a witness, and its absence at `confirm` time is an internal error.

**The pooled review.** At report time every reported origin replays until failure, and any failure confirms a still-unconfirmed origin with no bar: a fresh reproduction is a reportable failing execution in its own right, whatever the rate estimate. Origins first observed by report-time measurement runs are never barred (confirming them could admit further origins without bound), so they report caveat-only and recycle via rediscovery next run (decision 35). The details are in [the final replay](final-replay.md).

## Trust via database reproduction

A reproduced stored entry's origin is Trusted without facing the bar (decision 24): "the prior run persisted only confirmed origins, and subjecting real p ~ 0.1 bugs to the bar again would drop them ~55% of the time." `trust` is called from the reuse phase after a reproducing replay (which also marks the origin `first_checked`) and from `reproduce_blob`'s ND path. It carries the reproducing v2 entry's timeline pool, truncated to `POOL_CAP` (a decoded entry may carry up to the looser format bound), folds the reproducing batch's physical counts into the trusted evidence, never demotes a Confirmed origin, and never replaces an existing pool with an empty one.

Trust exempts the origin from the bar's verdict, not from measurement (decision 47, the honest rewording of "the bar is not re-run", which was never true). At shrink time a trusted origin runs an evidence batch with the bar as stopping rule only. Any failure promotes it to Confirmed: anchor from the batch LCB, pool merged fresh-first with the stored one (decision 48: the stored pool is the previous run's validated replay state, and the earlier promotion path forgot exactly what had just reproduced the failure). A zero-fail batch folds its counts through `record_trusted_batch`: the origin stays Trusted, skips shrinking, and is still reported and persisted.

## Rejection and eviction

`reject` folds the rejecting batch's physical counts into the origin's evidence and reports whether the caller must evict: true for Unconfirmed origins, false for Trusted and Confirmed ones, which are exempt. Eviction removes the origin from the interesting map: generation keeps hunting, and a rediscovery faces the bar afresh, which is why the bar can afford 45% per-discovery power. The lifecycle entry itself survives ("Rejection never demotes and never removes state"), so the accumulated counts still feed the caveat. A rejected origin reaches the report only through the caveat-only fallback, and only when nothing confirmed or trusted survived: unconfirmed reporting is gated to avoid caveat fatigue (decisions 3, 24).

## Caveat wording

`OriginLifecycle::caveat` renders each reported failure's note from the origin's state, quoting only in-run measurements. No rates are ever persisted, so every run's caveat stands on its own replays (decisions 3, 8). It returns `None` for an origin the lifecycle never saw: a deterministic failure carries no caveat. The environment-modification hypothesis appears only where non-reproduction is surprising given the evidence, and report-time counts are quoted apart from confirmation or reuse counts.

Confirmed:

- live: "nondeterministic failure, confirmed: failed {fails} of {replays} replays this run"
- with report-time replays: "nondeterministic failure, confirmed: failed {fails} of {replays} replays at confirmation and {report_fails} of {report_replays} at report time"
- dry at report time (`report_replays > 0`, `report_fails == 0`): "nondeterministic failure, confirmed earlier this run (failed {fails} of {replays} replays) but not reproduced at report time — a rare failure, or something in the environment changed after discovery"

Trusted:

- live: "nondeterministic failure, reproduced from stored timelines: failed {fails} of {replays} replays this run"
- with report-time replays: "nondeterministic failure, reproduced from stored timelines: failed {fails} of {replays} replays at reuse and {report_fails} of {report_replays} at report time"
- dry at report time: "nondeterministic failure, reproduced from stored timelines earlier this run (failed {fails} of {replays} replays) but not reproduced at report time — a rare failure, or something in the environment changed after discovery"

Unconfirmed:

- some failures below the bar: "unconfirmed failure: failed {fails} of {replays} replays this run, below the confirmation bar — likely rare"
- observed once, never replayed: "unconfirmed failure: observed once, never replayed — a rare failure, or the environment changed between executions"
- zero of N after the sighting: "unconfirmed failure: failed 0 of {replays} replays after the observed failure — a rare failure, or the environment changed between executions"

The dry variants switch wording rather than unreporting: a confirmed origin that comes up dry at report time is still a failure the run measured (decision 3). The frontend prints the caveat as `note: {caveat}` beneath each failure block ([the C ABI and the frontend](abi-frontend.md)).
