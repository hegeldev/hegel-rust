# The production plan and its gates

With experiments 001–006 closed, the branch held a working scaffold and
twenty-five decisions. `notes/production-plan.md` (27efbc6b, 2026-09-02) mapped
those decisions and the experiment results onto eight phases with per-phase
tests and exit criteria, and was itself revised against a 21-finding adversarial
critique (three critics, each finding verified) before it was committed. Its
governing frame is decision 26, correcting a misreading of decision 15: the
branch itself goes to production grade in place, the `nd_*` scaffolding
refactored into the real implementation rather than discarded, with extraction
happening later from the production-grade branch. Phases 1–6 landed on
2026-09-02, phases 7–8 on 2026-09-03.

## The production bar

The plan defined production grade as five criteria:

- Nondeterministic tests handled by default per decisions 1–25: quiet
  detection-driven flip, confirmation-gated origins, gauntleted shrinking,
  pool/splice/fresh replay, caveated reporting, self-identifying persistence.
- `NdExperiment` gone as a mode entry, replaced by detection plus the public
  `nondeterminism_strictness` setting and a test-only force flag.
- Every CI gate green, including 100% line coverage on new code with no ratchet
  increase and no new `nocov`.
- The ABI and frontend exposing the new semantics, the header regenerated,
  RELEASE.md entries written.
- The bandaids deleted: the NondetStash, the sacrificed-first-case trick, the
  flaky abort on ND tests, and `reject_concurrent_machine` as the sole
  concurrency handling.

Three reds were known at plan time. Coverage was red because the unconditionally
compiled `nd_*` code was exercised only by `__bench` experiment binaries outside
the workspace, so phases 1–2 owned closing it, phase 2 carrying smoke tests so
the repo-wide coverage gate held at every phase boundary. A `variant Resample
never constructed` warning stood under default features. And the `experiments/`
crates, pinned to engine internals via `__bench`, were frozen against a1d1b6d2
rather than maintained.

## The four gates: decisions 27–30

Everything else in the plan had a settled answer or a stated recommendation.
Four questions changed user-visible surface and needed DRM before their phases.
All four were resolved the same day (06f0b81a) as decisions 27–30.

**G1, the ABI shape for ND failures** (blocked phase 6).
`FAILED_NONDETERMINISTIC = 3` meant the old concurrent-machine path: no blob, no
shrinking, NondetStash capture. The options were reusing value 3 with changed
semantics and no signal to existing consumers, appending a v2 status, or
collapsing to plain `FAILED` plus a per-failure caveat accessor. It closed as
the third (decision 27): since decision 8 makes ND-ness a property of the entry
rather than the run, the caveat is
per-origin information a run-level status cannot carry, and frontends that never
call the accessor keep working. A bindings survey was a precondition, recorded
with phase 6 below. Decision 43 later reserved value 3 against reuse.

**G2, the boost default** (blocked phase 5's policy). Decision 25 had foreclosed
always-on, leaving default-off behind a setting versus a reliability-floor
heuristic. It closed as the heuristic (decision 28): boost runs only when the
confirmed incumbent's LCB sits below a floor of 0.5 and accepts only
holdout-passing improvements, the rescue case where experiment 006 showed pure
win. Decision 56 later re-derived the floor as 0.30 in 20-run-batch LCB units
([remediation](remediation.md)).

**G3, the data tree under ND** (blocked phase 3). The options were staying
disabled under ND, tree surgery with per-(kind, value) child edges, or kind-set
tolerance. It closed as disabled (decision 29), with phase 7 assigned to measure
the cost on workload #1. Decision 60 later superseded this with the tree's
removal, though the invariant that ND handling never serves cached conclusions
survives verbatim on the execution cache ([the seam plan](seam-plan.md)).

**G4, the strictness surface** (blocked phase 3). It was confirmed as proposed
(decision 30): `nondeterminism_strictness = quiet | warn | error`, default
quiet, with `error` reproducing the old abort diagnostics verbatim for suites
using determinism as a lint and `warn` printing once per run rather than per
flip. Decisions 60 and 64 later amended `error` to abort earlier.

## Phase 1: the statistics module

Phase 1 pulled the decision arithmetic into a pure module (the Wilson bounds,
the discovery bar (decision 23), the gauntlet rule (decisions 7/17/19), boost
and continuation arithmetic, and the replay budgets of decisions 11 and 16) and
replaced the ledger's bare tuple with a named `Evidence` type carrying decision
22's weighted counting. At exit the module existed, `test_runner.rs` called it,
constants lived in one place, and fixtures from experiments 005A, 001, and 003
passed with no behaviour change. It landed as 00d7e39e, creating
`hegel-c/src/native/nd`, with one deliberate behaviour change: the flat
`ND_REUSE_TRIES = 10` placeholder became the derived 29-replay budget (decision
11).

## Phase 2: the origin lifecycle state machine

Phase 2 replaced the five parallel maps (`nd_confirmed`, `nd_pool`, `nd_anchor`,
`nd_witness`, `nd_unconfirmed`) with one `OriginLifecycle` mapping each origin
to an explicit `OriginState`, behind a single admission function enforcing
decisions 20 and 24: raw interesting fills vacant origins only, and the
discovery bar is the sole Unconfirmed-to-Confirmed edge. The Persister began
buffering under ND, committing only validated incumbents. At exit the five maps
were gone, admission was single-pathed (Persister included), regression tests
locked 003's noise-floor displacement leak and 005B's span-mutation slip, and
coverage was green repo-wide. It landed as 66539c50, with the enum already
carrying the Trusted state for database-reproduced origins.

## Phase 3: detection, mode entry, strictness

Phase 3 made the plan's largest semantic change: ND handling became
detection-driven instead of settings-gated. Mode entry needed no new detector,
since the choice-tree comparison in `record_run` already surfaced structural
divergence and outcome
flips surfaced as verify status/origin flakes, with within-run evidence only
(decision 9). The flip triggers unified onto the engine's one sticky flag (renamed
`nd_active`), set by detection and never cleared within a run, its roughly
twelve existing gate sites reviewed one by one. `nondeterminism_strictness` was
plumbed through the whole stack, from engine settings to `#[hegel::test]`.
`NdExperiment` and the `__bench` harness functions naming it were deleted, and a
`pub(crate)` test-only `nd_force` flag took their place. The scope was
deliberately interim: concurrent-machine flips set the same flag, but those
failures kept the old FAILED_NONDETERMINISTIC/NondetStash reporting until phase
6 and stayed unshrunk until phase 7.

It landed as beb9dbe5, with detection driving the flip for both structural and
outcome nondeterminism, the setting reachable from the macro, and the header
drift test green. Immediately after it, 84d37318 pinned the phase-7 early smoke
(ND handling surviving a clone-bearing body behind the force flag)
deliberately out of phase order, so a structural mistake in the representation
would surface before phases 4–6 hardened the wrong shape.

## Phase 4: the replay stack and persistence

Phase 4 built the one engine-owned replay-until-failure primitive of decision 25
(pool first-fit at cap 10, then positional splices of pool pairs, then fresh
generation, under the continuation budget), serving confirmation, shrink verify,
gauntlet reruns, and database reproduction. Each execution carries provenance:
replay evidence never raises the anchor (decision 19), and measurement runs are
excluded from `valid_test_cases`, the invalid budget, health-check counters,
event statistics, targeting records, and the bug-window markers. Without that
split, every quantitative runner behaviour changes meaning and nothing reports
it.

Persistence gained the version-2 format (decision 8): incumbent, timeline pool,
entropy seed, and continuation budget, doubling as blob (prefixes 2 and 3)
and database entry, so a v2 entry switches a new run into ND handling and
cross-run reproduction survives the quiet default. Confirmation replays run
capture-enabled and feed the pool (decision 10). Hygiene followed decision 11: a
primary miss demotes to the secondary corpus, a secondary miss deletes, and no
counters persist. At exit ND failures reproduced cross-run from both database
and blob, and 005B's 139/139 standard became an automated test. It landed as
b327fd1f.

## Phase 5: shrinker integration

Phase 5 wired confirmed-dry stopping into the shrinker (decision 18): after a
dry sweep, one confirmation sweep in which every proposal skips the single-run
fast reject and drives its cumulative ledger evidence to a bound verdict,
stopping only if it accepts nothing. The plumbing was a defaulted
`set_sweep_mode` method on `ShrinkProbe`, leaving the blanket `FnMut` impl and
the embedded test call sites untouched. The accounting followed decision 7:
`calls`, the stall guard, and `MAX_SHRINKS` charge one logical call per
candidate regardless of gauntlet physical depth, with the 300-second deadline as
the physical backstop. At exit the shrinker met decision 2 under test, stopping
was confirmed-dry, and the accounting was documented and tested.

The plan recorded one measured disposition inline: 68–72% of full-run shrink
proposals are repeats, and the cost was accepted rather than adding resumable
search state. Under ND a repeated proposal is how retried rejects accumulate
ledger evidence, so resuming the searches would trade that power for savings
only the ND fast path sees. It landed as dc6b50cb, with boost shipping as G2's
reliability-floor heuristic and the `nd_boost` setting deleted.

## Phase 6: reporting, the ABI break, and the frontend

Phase 6 (097275b3) made the new semantics visible. Per G1, failures carry
origin, shrunk nodes, a blob (v2 when ND), and an evidence-weighted caveat
quoting the run's own replay evidence: confirmed ("failed k of n replays this
run"), confirmed-but-dry at report time, unconfirmed-rare, or
unconfirmed-environmental (decision 3). Concurrent-machine runs get a static
caveat. Unconfirmed failures report with a clean origin and the
caveat field, and only when nothing confirmed (decision 24). Multi-failure ND
runs report each origin with its own caveat and blob.

The bindings survey G1 demanded landed in the plan before the break did:
hegel-typescript and hegel-ocaml never adopted status 3 (their enums stop at
`ERROR = 2`), hegel-go's own handling of 3 is dead but harmless once the engine
stops emitting it, and hegel-cpp vendors `hegel.h`, so dropping the value is a
deliberate compile-time migration signal on its next header sync. The ABI then
broke in two places, coordinated in the RELEASE.md files:
`HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` was retired, and
`hegel_failure_caveat` was new.

The final replay moved engine-side. The frontend had replayed each blob exactly
once after the engine future completed. Now, between shrink end and persistence,
the engine re-executes every reported origin. A deterministic run replays its
shrunk incumbent once (a mismatch aborts under `error` and flips otherwise), and
an ND run replays incumbent, pool, splices, and fresh generations up to the
reuse budget, feeding the lifecycle. Replay executions were stamped through the
existing capture channel, still named `hegel_test_case_is_nondeterministic` at
this point (decision 50 renamed it to `hegel_test_case_should_capture` in the
remediation era).

On the frontend, `drive()` began capturing every interesting case's report
material per origin and printing reported failures from the freshest capture.
The client-side blob replay, its FLAKY_DIAGNOSTIC panic, and the single-slot
NondetStash, all of which misfired on ND failures, were deleted. A vanishing
failure still fails the run under quiet strictness, re-raising the test's own
panic. `CompiledRegex` and `DomainSpec` also became `pub` (unnameable) to
satisfy the private-interfaces lint, a wart phase 8 reverted. The exit gate was
met: a flaky test failing at least 10% of the time fails a frontend run with a
shrunk, caveated, blob-reproducible failure, quietly, end to end.

## Phase 7: experiment 007 and the concurrency unification

Workload #1 (decision 12) had never run on any of the machinery built above: the
shrink phase was gated on the engine's sticky `nondeterministic` flag, so
clone-bearing tests bypassed everything. The pre-branch regime (the sacrificed
first case, per-case stamping, the blobless NondetStash report) is described
in [the design history](design-history.md). Phase 7 was part experiment, part
implementation. Its questions had stood since decision 14: whether
whole-timeline pool replay reproduces concurrent-stateful failures at useful
rates or splice shifts across clone-stream boundaries demand span-anchored
splicing, whether boost must descend into clone streams, and what tree-free
generation costs here.

What landed (7a4fd194) routes concurrent-machine runs through the same pipeline
as every other ND source. Machine creation always succeeds, and a declared
concurrency bound above 1 flips the run into ND handling at the first executed
case declaring it. Concurrent failures are confirmed, shrunk, persisted, and
reported with a v2 blob and lifecycle caveat like any other nondeterministic
failure. The unification deleted the per-case concurrent stamp, the sacrificed
first case (`reject_concurrent_machine`'s first-case-Invalid), and the
concurrent gates on shrinking, span mutation, persistence, and the final replay.

Experiment 007 ran two workloads twenty trials each (a genuinely racy
concurrent counter machine and a plain clone-based flaky body) through
discovery, database reuse on the same directory, and three blob replays per
trial. Both hit ceiling: 20/20 discovery, 20/20 reuse, 60/60 blob replays, every
failure carrying the confirmed caveat and a v2 blob. Blob replay needed
rerouting to get there: single-shot incumbent replay through
`hegel_test_case_from_blob` reproduced the racy machine's shrunk failure 4/30,
because a shrunk minimal schedule fires its race only ~13% of the time in a
fresh process. The fix was `hegel_run_start_blob`, replaying a blob as a run
through the replay primitive: a deterministic blob once, an ND blob via
`nd_reproduce` over its stored pool with splices but no fresh tier, since a
fresh case could fail for an unrelated reason. That took reproduction to 30/30
and completed decision 25's one-replay-primitive rule (decision 33).

Two more dispositions came with data. Clone serialization stayed values-only
(decision 32): the round-trip drops realized kinds, whose only consumer is
`resolve_choice`'s is-simplest check, firing solely on constraint drift, and
measured clone replays re-raised the stored shrunk value exactly. Decision 31
closed decision 14: whole-timeline pool replay plus positional splices
reproduced both workloads at ceiling, and a splice structurally cannot tear a
clone record because a whole clone stream is a single timeline element, so
per-position and per-stream anchoring stay unbuilt unless a real workload shows
the pool and splices missing. Boost never engaged, anchors sitting well above
the G2 floor, and tree-free generation showed no cost pathology. That met the
exit gate: confirmed, shrunk, reproducible, caveat-correct failures on both
workloads, rates recorded in the notes, and decision 14 dispositioned with data.

## Phase 8: the production bar sweep

Phase 8 (9c800e8e) deleted the residual experiment plumbing
(`fixate_cost_experiment`, `replay_once`, the widened `__bench` re-exports),
folded `serve_replays` into `nd_active`, and restored `pub(crate)` on
`CompiledRegex` and `DomainSpec`. It froze `experiments/` with a README
recording what still builds, excluded `notes/` and `experiments/` from the
published package, rewrote `notes/design.md` as-built, and added
`notes/evaluation.md` auditing decisions 1–33 against the implementation. The
exit gate was that the branch is the artefact decision 26 describes.

## Sequencing and risks

The plan ordered phases 1→2→3 strictly, 4 before 5, 6 after both, phase 7's
smoke after 3 and its full form after 6, and 8 last, with G1 resolved before
phase 6, G2 before 5, and G3/G4 before 3. It recorded four risks: clone streams
arriving late (the smoke bounded the exposure to phases 4–6 rework), the roughly
twelve gate sites each being a behaviour change when unified, error-path
coverage forcing testable seams rather than `nocov`, and the gauntlet's physical
depth against the 300-second shrink deadline.

## How the plan ended

Phase 8's exit held for less than a day. On 2026-09-03, the same day it landed,
an adversarial review of the as-built branch at 9c800e8e produced the 41-finding
register whose verdict, "the architecture holds, but the statistics don't
compose", opened [the remediation plan](remediation.md). Three of the four gate
outcomes were later revised: decision 56 re-derived G2's floor, the seam-plan
era removed G3's tree outright, and decisions 60 and 64 amended G4's `error`
semantics. The structural output survived: the statistics module, the origin
lifecycle, detection-driven entry, the replay primitive, the v2 format, and the
unified concurrency pipeline. Every later phase built on these rather than
replacing them.
