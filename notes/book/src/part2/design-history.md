# Sketch v0 and the design critiques

The design work in this chapter happened on 2026-09-02, before the branch carried a single commit. In
one day the project produced seven code maps of every determinism-dependent subsystem, a first
mechanism sketch (`notes/research/sketch-v0.md`), and four adversarial critiques of that sketch,
all compiled against commit a0185a65 — the branch base, equal to main at the time. The v0-to-v1
changes the critiques forced are recorded in `notes/design.md` and `notes/decisions.md`. The
dated sequence of everything that followed is in [the chronology](chronology.md).

## The pre-branch concurrent regime

The branch's prior art was concurrent stateful testing: PR #359 "cloned-test-cases" (merged
133a9fa4, 2026-07-03) and PR #360 "parallel-clones" (merged 33afef7b, 2026-07-06) landed the
clone/family stream machinery, and PR #378 "rdck/concurrent-stateful-testing" (merged 9729c1c2,
2026-08-20) built concurrent state machines on it. `map-concurrent-stateful.md` characterised its
handling of nondeterminism as wholesale surrender rather than accommodation (code cites in this
section are at a0185a65).

Requesting `max_concurrency > 1` set a sticky `Engine::nondeterministic` bool that disabled, at
individually enumerated sites in test_runner.rs: reuse replay, shrinking (generation stopped at
the first bug), novel-prefix generation, targeting, span mutation, the whole verify-plus-shrink
pass (and with it the Flaky check, so detection was disabled by the thing it would have
detected), end-of-run database reconciliation, per-failure blobs, data-tree recording and its
mismatch check, `test_is_trivial`, and incremental Persister saves. `Mode::SingleTestCase`
hardwired the flag false, so a failing single case reported plain FAILED.

Reporting depended on two workarounds. The run's first case creating a concurrent machine was
rejected as `EngineError::AssumeViolation` via `reject_concurrent_machine`
(hegel-c/src/native/data_source.rs), sacrificed purely so every later case could be stamped
nondeterministic before it started, because the frontend's capture decisions (emit, backtrace,
diagnostic) are all made before the body runs. The frontend read
`hegel_test_case_is_nondeterministic` at case start, switched to capture-at-discovery, and
stashed the last interesting case's lines, diagnostic, and panic payload in the single-slot
`NondetStash`. On `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` it printed the stash and re-raised
the stashed payload — no final replay, no blob, no database entry. Multiple distinct bugs
collapsed to one report, and the stash could lose to an engine-side family conclusion and be
discarded if the run then passed.

One reversal predates the branch: an explicit frontend `nondeterminism` setting was added and
removed within PR #378 itself (commit 5c7456b2 "Remove nondeterminism setting", 2026-08-14).
Declared-by-user lost to engine-derived before the branch existed, and the branch never reopened
the question — its strictness setting controls what a flip does, never whether one is recognised.

The map's RELEVANCE section fixed three constraints the whole design descended from.
Nondeterminism was declared, never detected, and the flag was run-granular and only ever went
false to true. Any incremental detection scheme faces the same too-late-to-capture problem the
sacrificed case existed to dodge. And the clone/family stream machinery (per-thread draws
recorded as an interleaving-free tree, value replay schedule-independent) was the strongest
deterministic foundation available; the timeline pool was built on it.

## The code maps

Seven maps were written, all at a0185a65, each ending in a RELEVANCE section that fed the sketch:

- **map-choices-replay.md** — replay is already tolerant: `resolve_choice` puns a misfitting
  value to `simplest()`/`unit()` rather than erroring, and determinism assumptions live almost
  entirely outside the draw path. Punning is a deliberate shrinking mechanism, the fact the
  representation critique later used to kill misfit-anchored widening.
- **map-concurrent-stateful.md** — the prior-art autopsy above.
- **map-database-blobs.md** — every persistence feature assumes a stored sequence
  deterministically reproduces: `Reuse` silently deletes an entry that replays non-interesting,
  so a p = 0.1 corpus is erased on the first lucky replay. The unknown-prefix-byte behaviour
  became the version gate for the v2 blob.
- **map-frontend-lifecycle.md** — all capture decisions precede the body, RunError variants
  flatten to one opaque string over the ABI, and Settings has zero flakiness or replay-count
  knobs. This map fed the lifecycle critique's case against "no stamping needed".
- **map-shrinker.md** — outcome-as-pure-function-of-sequence is load-bearing at four layers.
  Misalignment (the test drew differently than proposed) is well handled, but "the same sequence
  drew differently across runs" has no representation.
- **map-test-runner.md** — the three re-execution sites and their inconsistencies: the tree
  mismatch is checked at reuse, probe, generation, and verify but discarded for all shrink
  probes. The enumerated disable list above is from here, and the sketch's mode lifecycle is
  literally carving capabilities back out of that blanket disable.
- **map-docs-history.md** — the documented model is binary: deterministic with strong promises,
  or declared-ND with everything off, with choice-sequence shape and blobs as compatibility
  surfaces. This map is why the design treated reporting and documentation as a renegotiated
  contract rather than an internal change.

## Sketch v0

The sketch set constraints up front: handle tests failing at least 10% of the time, detected
rather than merely declared, under a new setting `nondeterminism_strictness = quiet | warn |
error` defaulting to quiet, with `error` preserving the existing aborts. Shrinking must not lower
failure probability — ideally boost it first and stay in deterministic regions when reached — and
cheap single-run predicates run repeatedly are preferred to N-running every candidate. Per-origin
identity stays, the data tree comes back for ND runs if feasible (low priority, disabling
acceptable), per-test ND status persists, and the workloads rank concurrent stateful first,
clone-based concurrency second.

The mechanism came in six numbered areas, which the critiques cite by number:

1. **Mode lifecycle**: the run flips deterministic to ND when declared or when any existing abort
   site fires under quiet/warn — including database-reuse divergence. ND status persists in a
   database sub-key, cleared when a full ND run observes zero divergence. It claimed that with
   unified reporting cases no longer need stamping before they start, so the sacrificed first
   case could go.
2. **Interestingness**: confirm a failure by replaying up to N = 10 times, stopping at the first
   failure. If nothing reproduces, still fail the run with a caveated report naming both
   hypotheses (rare failure versus environment modification).
3. **ND nodes**: when a recorded value or kind no longer fits a draw on replay, widen that
   position into a branch point holding multiple branches, each a head value plus a tree-shaped
   suffix. Replay picks the first branch whose head satisfies the constraint, else generates
   fresh under a capped continuation budget, and new timelines fold back in as branches. Storage
   was a new choice-sequence element, a new blob prefix byte, and an extended sort key.
4. **Data tree**: widen tree nodes on kind mismatch instead of erroring, make conclusions
   outcome distributions, treat ND nodes as never exhausted, restore novel-prefix generation.
5. **Shrinking**: single-run predicate per candidate inside a loop-until-K-dry fixpoint, with
   checkpoint validation at pass boundaries rolling back to the last checkpoint on collapse.
   Ratchet classes deterministic > high-p > low-p, with consecutive-failure sequential tests for
   cross-class accepts, a probability-boost phase before shrinking reusing the
   targeting/Optimiser machinery, and cross-timeline grafting of the failing branch's suffix
   onto passing-branch candidates.
6. **Reporting**: ND blobs in a new format. The final replay runs the blob up to N and reports
   the first failing execution fresh, else a caveated report from a per-origin discovery-time
   capture replacing the NondetStash. Database reuse replays each entry a small number of times
   before deleting.

## The four critiques

Each critique opens with a verdict and closes with improvements. `sketch-v0.md` line 27 is the
canonical list of what the review killed, mapped to what shipped in the table below.

### Lifecycle

The verdict named three load-bearing errors. N = 10 misses its own target: P(reproduce in 10 at
p = 0.1) = 0.65, so the caveated fallback fires roughly 35% of the time — the degraded path is
the common path, and 95% coverage at p = 0.1 needs about 29 replays. "No stamping needed" is
false: every capture decision is made before the body runs, so an unreproduced failure would
yield a values-less report. And database-reuse divergence is not ND evidence: it overwhelmingly
means a code change, and using it as a flip or persist site would mis-stamp deterministic tests
routinely. Beyond the verdict: ND persistence is inert in CI (`Database::Disabled` is the default
there), confirmation replays would corrupt budgets, health checks, and event statistics with no
replay bit in existence, and replays routed through `cached_test_function` would be served from
the tree cache, observing zero fresh randomness. Blob replay under the new format was
unspecified: the stale-blob panic would fire 90% of the time at p = 0.1.

Beyond the table's reversals, this critique yielded the measurement flag excluding replays from
every counter, budget, and statistic (the accounting split), the versioned v2 blob format, and
the capture stamp that later became `hegel_test_case_should_capture` (decision 50). The
flag-clearing question dissolved: with no persisted status (decision 8) there is nothing to
clear, and within a run `nd_active` never clears. Two findings were acknowledged and retained:
the quiet default suppresses the accidental-global-state lint (an owner constraint, decision 1;
`error` keeps today's aborts for exactly those users), and the caveated fallback report survived
as decision 3.

### Representation

The verdict: draw-time validation is far too weak to anchor branch points (booleans validate
unconditionally, integers only need containment), punning is a deliberate shrinking mechanism
that widening would break, and for the #1 workload the mechanism serves nothing —
concurrent-stateful divergence lives inside clone streams, where the codebase's own precedent
(`clone_subtree_disabled`) already concedes per-position prediction is hopeless, so ND nodes
either fire constantly on benign schedule noise or must be scoped out. The dominant divergence
class, different structure with the same kinds (a collection `reject()` firing in one timeline
only), is invisible to both widening triggers, so the ND node has nowhere to land. Branch
selection by "first branch whose head satisfies the constraint" is vacuous, while bit-exact kind
matching spuriously rejects. The merged multi-branch artifact has no stable trunk: every accepted
shrink invalidates all folded branches, and no comparison site ever sees an ND node. And
extend = 0 replay biases every ND replay toward overrun, undercounting failure probability.

This critique forced the biggest single design change of the review: the merged ND-node artifact
was abandoned for the timeline pool — a small set of complete realized failing sequences per
origin, each replayed with a continuation budget (decision 5, which records DRM conceding
tree-shaped suffixes as storage while keeping them as semantics). It also produced structural
divergence signals in place of misfit anchoring, the ancestor of the verbatim watermark
(decisions 22, 45), and cut the tree's ND role to detection plus novel prefixes, dropping
conclusions-as-distributions as consumer-less. Widening was scoped to verbatim-replay contexts
with punning kept for shrinker candidates, ND suffixes became values-only, and the persisted ND
bit moved to first-failure time, which became the v2-entry channel.

### Shrink loop

The verdict: area 5 is unimplementable without three prior decisions the sketch leaves open —
where branch structure lives relative to the flat `Vec<ChoiceNode>` everything operates on; what
replaces the tree cache, which makes every proposed re-sampling silently no-op and whose removal
inverts the cost model; and how sticky single-run accepts are stopped from burning the monotone
budgets (`MAX_SHRINKS = 500`, `max_stall`, the Persister ratchet) faster than rollback recovers.
`consider()` accepts on one interesting run with no un-accept, so expect tens of spurious accepts
per sweep at p = 0.1-class noise, each consuming irreversible budget, and one lucky failing run
in `BinSearchDown`'s CheckLo state teleports the incumbent to a near-zero-probability example in
one call. The cited rollback precedent, `last_checkpoint_nodes`, is only a diff baseline: nothing
in the shrinker restores state. Mid-shrink ND detection did not exist (the mismatch is discarded
for all shrink probes), and the Persister and `update_interesting` write spurious minima to the
primary database key before any validation.

Its yield beyond the gauntlet itself: rejected candidates must stay retryable with evidence
accumulating (DRM's addition to decision 7), persistence gated on validated acceptance,
loop-until-K-dry replaced by generalising the existing stochastic-pass retry machinery rather
than adding a new outer loop, and a prototype order putting cache semantics and a flat-timeline
shrink experiment before any ND-node work. The flat shrinker with a separate engine-side pool is
what shipped, and the shrinker never grew branch-aware state.

### Statistics

The verdict: the ratchet/boost/checkpoint architecture is directionally workable, but its
calibration contradicts the 10% target at three independent points, and the "deterministic =
never seen passing" class is assigned from stop-at-first-failure samples with near-zero power — a
p = 0.7 bug enters shrinking classified deterministic 70% of the time, and once misclassified
essentially all true reductions are rejected: shrink paralysis. Rollback against the last
checkpoint lets p decay geometrically (halvings compound across 150-plus checkpoints while every
individual checkpoint reads fine), and a tight threshold instead thrashes about eight spurious
rollbacks per shrink. Every p-statistic's population is undefined: per-origin versus per-test,
pooled candidate versus realized timeline. The Optimiser reuse is contradicted three ways by
targeting.rs, winner's curse under max-recording of noisy estimates freezes the climb and poisons
the ratchet class, and database reuse at small N deletes genuine p = 0.1 entries with roughly 73%
probability per run.

This critique supplied the shipped statistical architecture: the evidence ledger with Wilson
lower bounds, the monotone anchor, one predicate gating all three acceptance paths (later
decision 36's validated accept), demotion instead of hard deletion with replay budgets derived
as ceil(ln δ / ln(1 − p)) at the target rate (decision 11), and evidence keyed per
(origin, realized timeline). Its first improvement was methodological: build a pure-simulation
calibration harness before touching the engine. That sentence is the origin of the numbered
experiment series that runs through every later document (see [the experiments](experiments.md)).

## v0 against the as-built design

The eight reversals `sketch-v0.md` line 27 names as review-driven, plus the two largest later
abandonments:

| v0 proposed | Shipped | Forced by |
|---|---|---|
| Merged ND-node artifact: branch points with tree-shaped per-value suffixes in the choice sequence | Flat incumbent plus a bounded per-origin timeline pool, `POOL_CAP = 10` | Representation critique: no stable trunk under shrinking, no comparison site ever sees an ND node (decision 5) |
| Misfit-anchored widening at draw-time constraint violations | Structural divergence signals; the verbatim watermark weighting evidence | Representation critique: the dominant divergence class is kind-compatible and invisible to misfits (decisions 22, 45) |
| Database-reuse divergence as a flip site | Within-run evidence only; stored-entry staleness is never ND evidence | Lifecycle critique: reuse divergence is a code-change signal (decision 9) |
| Persisted ND status plus a zero-divergence clearing heuristic | Only the representation persists; the v2 entry is self-identifying | Lifecycle critique: inert in CI, and clearing flaps for passing-but-ND tests (decision 8) |
| "No stamping needed" once reporting is unified | Capture at confirmation: the engine stamps report-candidate executions | Lifecycle critique: capture decisions precede the body; an unreproduced failure would report no values (decision 10) |
| Ratchet classes deterministic > high-p > low-p from stop-at-first-failure samples | Wilson lower confidence bounds over an evidence ledger | Statistics critique: "never seen passing" has near-zero power (decision 7; arithmetic later fixed by decision 54) |
| Rollback to the last checkpoint on collapse | The monotone anchor; no checkpoint/rollback at all | Statistics critique: geometric p-decay under re-baselining; both rollback rules then measured and dropped in experiment 001's follow-up (decisions 17, 19) |
| Unguarded single-run accepts inside a loop-until-K-dry fixpoint | The gauntlet: single-run rejects, every accept pays a sequential test; confirmed-dry stopping | Shrink-loop critique: accepts are sticky and burn monotone budgets (decision 7); confirmed-dry measured later (decision 18) |
| Boost by Optimiser hill-climbing on failure rate | A holdout-gated successive-halving race over incumbent, pool, and mutant fills | Statistics critique: three verified contradictions with targeting.rs (decisions 25, 28, 56) |
| Restore the data tree for ND runs; conclusions as outcome distributions | Cut at v1 to detection plus novel prefixes; deleted outright in phase 15 for the execution cache | Representation critique (distributions have no consumer); experiment 010 and decision 60 — see [the seam plan](seam-plan.md) |

The last row is the sketch's "restore if feasible" ending in deletion: the critique shrank the
tree's ND role, experiment 010 measured the tree's recording, novel-prefix, and exhaustion roles
as buying nothing measurable on realistic workloads, and the seam work replaced it with a flat
cache that detects the verdict flips the tree never could.

## What became known risks

Each critique's header routes unresolved findings into the design's "Known risks" and "Deferred
decisions", and most of the as-built register traces straight back. Invisible divergence
(kind-compatible structural divergence evading detection, with whole-timeline machinery as the
backstop) is the representation critique's dominant-class finding, accepted rather than solved.
Origin instability, where cross-thread panics collapse to `Panic at <unknown>`, is from the
statistics critique's population-splitting finding, deferred to structured concurrency. Shrink
wall clock, where multi-run accounting makes `MAX_SHRINKING_SECONDS` the binding constraint on
slow concurrent bodies, is the shrink-loop critique's cost-model inversion. Caveat fatigue and
its evidence-weighted wording answer the statistics critique's finding of that name, and the
bindings-rollout risk is the lifecycle critique's coordinated-ABI-break finding. The rest
arrived later: the as-built review's omissions finding added anti-conservative statistics,
shrink opacity below `Debug`, and quiet-flip invisibility (the retained cost of decision 1) to
the register in phase 9, and the deterministic-to-ND seam entry was found by measurement in
phase 12 — see [the seam plan](seam-plan.md).

Two of the review's own products failed later: the gauntlet's original constants degenerated
back to single-run accepts in the exact target regime, and the quiet-flip lifecycle itself
became the dominant loss mechanism at the seam. Both are covered in
[the lessons chapter](lessons.md). The one composed safeguard from this review that survived
everything — the monotone anchor — is what kept the gauntlet's failure a recalibration rather
than a redesign.
