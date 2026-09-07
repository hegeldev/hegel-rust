# The final replay

Every failure the run is about to report re-executes first, inside the
engine. `final_replay` in hegel-c/src/native/test_runner.rs runs after
the shrink phase over every origin in the interesting map: while the run is
deterministic each shrunk incumbent must survive one exact replay, and under
ND handling each origin faces a pooled review whose outcome decides
confirmation, eviction, or the caveat's wording. The replay is also the last
[detection](detection.md) channel (a miss here flips the run like any other
replay check) and the last point at which a shrunk-away failure can be
recovered from origin history.

Two parameters shape the pass. The deadline is the shrink phase's own
deadline, so any re-shrinking the final replay triggers spends what shrinking
left over. When the shrink phase did not run, a fresh window of
`MAX_SHRINKING_SECONDS` (300 s) opens instead. `reshrink` records whether
`Phase::Shrink` is enabled: a backtrack-restored incumbent re-shrinks only if
the user asked for shrinking at all.

Everything the final replay executes goes through `measure()`: it detects
nondeterminism and can fill vacant origins, but moves none of the counters
that describe generation.

## Queue discipline

`pending` holds the origins in sorted order, and `replayed` collects those
that already passed an exact deterministic replay. The loop pops the head of
`pending` and branches on `nd_handling()`.

The two lists exist because a flip invalidates the deterministic branch's
earlier verdicts. Whenever a deterministic replay does anything other than
reproduce cleanly, everything in `replayed` moves back into `pending`: those
origins' single replays predate what the run now knows, and each now faces
the pooled review instead.

## The deterministic branch

While `nd_handling()` is false, the incumbent replays once, exactly
(`for_choices`, no RNG, no continuation), stamped for capture. Under `error`
strictness a cache mismatch detected inside the replay propagates as the
usual abort instead of being converted into a flip.

A replay that reproduces at the same origin with the run still deterministic
pushes its origin onto `replayed` and the loop continues. For a
deterministic run the entire final replay is n executions, one per origin.

The cache-mismatch channel can also fire inside a *successful* replay. The
replay reproduced, but the run flipped while it ran: `replayed` re-enters the
queue and the current origin falls straight through to the pooled review,
because a single reproduction is no longer enough evidence.

A miss aborts with `RunError::Flaky` under `error` strictness, preserving the
pre-branch behaviour for suites that use determinism as a lint (decision 30). Otherwise the
run flips (`FlipSite::FinalReplay`) and, when the origin never confirmed and
has history, `backtrack` runs (below). On `Restored`, the restored nodes
become the incumbent, re-shrunk under the gauntlet on the remaining deadline
when `reshrink`, and control falls through to the pooled review. On
`Exhausted`, the origin is observed, rejected with the backtrack's
accumulated evidence, evicted from the interesting map if still unconfirmed,
and the loop continues without a pooled review.

The abort exists only on this branch. A run in ND handling under `error`
strictness (`nd_force`; nothing else reaches that state since decision 70)
takes the pooled review like any other.

## The pooled review

Past the deterministic branch the origin must still be in the interesting map
(a `hegel_internal_unwrap!` guards the invariant). Its timelines come from
`pooled_timelines(incumbent, pool)`: the incumbent first, then the lifecycle
pool deduplicated, capped at `nd::POOL_CAP` = 10 in total (decision 42). The
review is one stamped call:

```text
nd_reproduce(Some(origin), timelines,
             reuse_replay_budget().div_ceil(n),   // per-timeline replay budget
             REPRODUCE_SPLICES,                   // 10
             FINAL_REPLAY_FRESH)                  // 4
```

`nd_reproduce` (test_runner.rs) is the replay-until-failure primitive shared
with database reuse and blob replay (decision 25, see
[persistence](persistence.md)). Three tiers run in order, stopping at the
first run that concludes interesting at this origin, and evidence accumulates
across all of them:

1. **Per-timeline first-fit.** Each timeline replays up to the per-timeline
   budget. Each replay is `nd_replay_once`: `for_probe`
   with a spawned RNG and a continuation budget of `len + max(4, len/8)`,
   the stored timeline plus `max(4, len/8)` fresh draws past it
   (experiment 004), counted as one plain trial whatever it realizes
   (decision 71).
2. **Positional splices.** With at least two timelines,
   `REPRODUCE_SPLICES` = 10 crossovers: each picks a random ordered pair and
   a random cut at most the shorter length, then glues the left prefix to the
   right suffix.
   Experiment 006 measured splices rescuing 65–100% of full-pool misses.
3. **Fresh generations.** `FINAL_REPLAY_FRESH` = 4 new random cases, a
   chosen constant rather than a derived one (decision 53). Only their
   failures enter the evidence: a fresh generation is a rescue, not a trial
   of the stored state (decision 71). Only the
   final replay has this tier. Reuse and blob replay pass zero, because a
   fresh case could fail for an unrelated reason (decision 33). Here the
   origin is pinned, so a fresh failure counts only at the same origin.

### Budget arithmetic

`reuse_replay_budget()` is
`replay_budget(TARGET_FAILURE_RATE, REUSE_MISS_TOLERANCE)` =
`ceil(ln 0.05 / ln 0.9)` = 29 (hegel-c/src/native/nd/mod.rs): the smallest
count at which a bug failing at the target rate p = 0.1 escapes with
probability at most 5% (decisions 11, 16). The pooled review divides it
evenly across the pool, `ceil(29/n)` replays per timeline:

| Pool size n | Budget per timeline | Dry worst case (timelines + splices + fresh) |
|---|---|---|
| 1 | 29 | 29 + 0 + 4 = 33 |
| 2 | 15 | 30 + 10 + 4 = 44 |
| 10 | 3 | 30 + 10 + 4 = 44 |

Every tier exits on the first reproduction, so a live bug costs about 1/p
executions, and the worst case is paid only for a dry review.

### Outcomes

`batch` is the accumulated evidence's (fails, runs).

For a still-unconfirmed origin, a reproducing review run is a sighting, not a
confirmation (decision 72): the earlier any-failure rule confirmed a q = 0.02
fluke in 49% of lone-incumbent reviews (59% with a pool), because 33-44
replays find one failure about half the time. The reproducing run's realized
timeline faces a standard evidence batch instead, spending one of the
origin's remaining `BAR_ATTEMPTS_PER_RUN` bar attempts and bounded by the
final replay's deadline (an expired deadline rejects — a batch cut short
proves nothing; the batch is the one `nd_evidence_batch` caller passing a
deadline at all). An accept confirms: the anchor is the batch's Wilson lower
bound, there is no witness (the shrinker is done with this origin), the pool
keeps the map incumbent first with the batch's captures merged ahead of the
review timelines, history is dropped, and the review's own counts land in the
report counts. Experiment 014 prices the change: fluke confirms fall ~170x
(0.487 → 0.003) and target-regime power falls 0.97 → 0.44 before the
backtrack rescue — the failing execution still reaches the report as a
values-carrying unconfirmed caveat when the batch rejects.

A rejected batch, an out-of-attempts origin, and a dry review all take the
same fall-through: observe (defensively, since the origin may never have
reached the lifecycle), then backtrack when history is non-empty. On
`Restored`, the restored nodes become the incumbent, re-shrunk when
`reshrink`, and the origin is pushed back onto `pending`: the restored
incumbent goes through the pooled review again. On `Exhausted`, the
backtrack's (fails, runs) fold into the reject evidence alongside the
review's and the batch's. The rejection evicts
an unconfirmed origin from the interesting map, and it can then reach the
report only through the caveat-only fallback (decisions 24 and 35, with the
wording in [the lifecycle chapter](lifecycle.md)).

For a confirmed or trusted origin, `record_final_replay(batch)` folds the
evidence into the origin's report counts, kept apart from confirmation and
reuse counts. A dry review here never unreports the failure but switches the
caveat wording to "confirmed earlier this run … but not reproduced at report
time" (decision 3).

Origins that the review's own measurement runs discover fill vacant slots in
the interesting map but were never in `pending`: they are never barred
(confirming them could admit further origins without bound) and recycle
through rediscovery on the next run (decision 35).

## Origin history

`OriginHistory` (test_runner.rs) retains every pre-flip interesting execution
per origin: raw sightings and accepts alike, in execution order, deduplicated
by serialized nodes. The history is unbounded because gate G24 found that a
recency bound evicts exactly the entries an early slip-in needs. The `accept` flag marks entries
that became the incumbent when recorded, a founding sighting or a shortlex
displacement in `update_interesting`. Accepts strictly shrink, so the accept
entries form a shortlex-sorted segment. Recording happens in `record_run`'s
interesting arm, for pre-flip non-measurement runs plus reuse replays.
History is dropped when the origin confirms, which also keeps the accept
segment sorted: no post-restore accept is ever recorded.

## Backtrack

A never-confirmed origin with non-empty history that misses its pre-shrink
verify (see [shrinking](shrinking.md)) or its final replay backtracks
(`Engine::backtrack` in test_runner.rs, decisions 65/66, gate G25). The walk
hunts the reproduction boundary, the newest history entry that still
reproduces.

The scan probes with single continuation-tolerant `nd_replay_once` calls: the
accept segment at geometric offsets back from the newest (1, 2, 4, …), plus
the oldest accept, plus the newest accept itself when only one accept exists,
then every raw sighting once. The whole scan draws on a budget of
`BACKTRACK_SCAN_REPLAYS` = `nd::CONFIRM_CAP` = 40 replays. A raw-heavy
history spends the cap on raws, so there the cap acts as the budget rather
than headroom.
Binary refinement then searches the accept segment between the newest
reproducing probe and its nearest newer non-reproducing one.

The candidate is the refined accept, or failing that the shortlex-smallest
reproducing raw sighting. With no reproducing probe at all, a single second
pass walks the entries newest-first on the remaining budget, re-probing
earlier misses, stopping at the first failure. If nothing has reproduced after that, the walk returns
`Backtrack::Exhausted` with the accumulated (fails, runs).

The candidate faces the full discovery bar via `nd_evidence_batch`, spending
the origin's `BACKTRACK_BAR_ATTEMPTS` = 3 budget — held per origin per run
across backtracks, not per call (decision 72), and checked before the scan so
a spent budget costs no probes: a probed entry reaches
the bar at roughly its true reproduction rate, each attempt holds 45%
target-regime power, and three compose to ~83%. The budget is separate from
`BAR_ATTEMPTS_PER_RUN` because history skews toward the real bug's pre-flip
sightings. A reject marks the candidate
non-reproducing and resumes the loop, which picks an older candidate next.

A cleared bar confirms the origin and drops history: the anchor is the
batch's lower bound, the witness comes from the batch, and the pool is the
candidate plus the batch's captured timelines plus the scan's other
reproducing entries. The restored incumbent supersedes the barred shrunk save
through `Persister::supersede_nd`. The write is forced, since the restore is
shortlex-larger than what the monotone `needs_save` gate would accept, and
ordered save-then-delete for crash safety (decision 44, see
[persistence](persistence.md)).

Scan errors bias old, which decision 2 makes safe: a too-old restore
re-shrinks under the gauntlet, and a too-new one anchors low or gets
bar-rejected (decision 66). Backtrack is not the checkpoint/rollback the
shrink design rejected (decision 17): it is detection-triggered, fires only
on a flip, and restores nothing that has not cleared the same bar discovery
pays.

## Capture stamping

`capture_replays` is set around both the exact deterministic replay and the
pooled review, so every execution here is stamped: the client sees
`hegel_test_case_should_capture` and buffers the case's output, diagnostic,
and backtrace — the material the frontend builds the failure report from (see
[the ABI and frontend chapter](abi-frontend.md)). Backtrack's scan probes
stay unstamped measurement replays, while its bar batches stamp their own
replays like every evidence batch and restore the caller's flag on exit.
Shrink-gauntlet and boost probes are never stamped (decision 10's cost
profile). That is why a dry final replay leaves the freshest stamped failing
execution (often confirmation-time, pre-shrink values) as what the report
prints, while the blob carries the shrunk incumbent.
