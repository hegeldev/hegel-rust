# Shrinking under nondeterminism

The shrinker's search machinery (passes, scheduling, sort keys) is unchanged from
the deterministic engine. What nondeterminism changes is the probe: under ND handling
(the sticky `nd_active` flag, see [detection](detection.md)) a single run is no longer
a verdict, so the engine's `EngineShrinkProbe` replaces one-run judgements with a
statistical accept rule, the gauntlet, priced against a monotone estimate of the
incumbent's reproduction rate, the anchor. Origin admission is
[the lifecycle's](lifecycle.md) subject, and the report-time backtrack is
[the final replay's](final-replay.md).

## The retention rule

Decision 2 is the branch's contract for shrinking: shrinking must not lower the
reported example's failure probability, and should raise it when possible. If
the search reaches a deterministically-failing region, it stays there. The guarantee
is statistical rather than absolute. Three mechanisms compose:

- **Candidate pricing.** A candidate displaces the incumbent only when its cumulative
  evidence's Wilson lower bound clears `max(gamma * anchor, GAUNTLET_FLOOR)` — it must
  demonstrate reproduction at no less than 0.8 times the incumbent's validated bound,
  and at the full bound once the anchor sits at or above the retention high-water.
- **The monotone anchor.** The anchor estimates the incumbent's reproduction rate
  under the engine's own pinned-replay procedure, rises only at validated events (a
  bar accept, an adopted gauntlet first-accept, a boost holdout pass), and never
  falls. Post-accept re-measurement of the standing incumbent never feeds it
  (decisions 19, 46). Anchor decay was tried and rejected: it made stopping
  incoherent, missing 51–75% of reachable reductions (experiment 001, decision 19).
- **Minimum failures.** No candidate is accepted on fewer than four observed
  failures, whatever its interval says (decision 54).

Experiment 008 measured what this buys: on the rising landscape the final failure
probability has median 0.82 (p10 0.58) against a 0.10 starting floor, and
target-regime (p = 0.1) bugs survive shrinking 100% of the time, against 67% for the
pre-recalibration code that accepted on the recruiting run.

## The machinery underneath

`Shrinker` (`hegel-c/src/native/shrinker/mod.rs`) holds the shrink target
(`current_nodes` plus spans) and drives everything through a boxed `ShrinkProbe`.
Improvement always means a strictly smaller shortlex sort key (`sort_key`,
`hegel-c/src/native/core/choices.rs`). A `ShrinkRun` is either `Full`, replaying a
whole candidate with punning, or `Probe`, replaying a prefix and then drawing the
continuation live from a spawned engine RNG. There is no per-probe seed, so a repeated
probe is a fresh sample.

`consider` returns true without executing on an equal sort key, false without
executing on a larger one, and refuses candidates that change a forced node's value.
Otherwise it executes and adopts the run's *actual* nodes (early exit and punning can
differ from the proposal) iff the run is interesting and strictly smaller. `probe`
applies the same accept rule to random-continuation runs, and `replace` is `consider`
after per-index substitution. All adoption funnels through `accept_improvement`, which
is where the probe's `candidate_adopted` hook fires (see the gauntlet below).

The pass roster in `shrink_inner` runs span-structural passes first
(`remove_discarded`, `try_trivial_spans`, `pass_to_descendant`, `reorder_spans`), then
node programs and deletion, then per-kind value minimisation, ending with
`shrink_clone_streams` and `mutate_and_shrink`, the only pass marked stochastic.
Deterministic passes fixate after one non-improving step, and stochastic passes get
`STOCHASTIC_MAX_FAILURES` = 6 consecutive retries (an inherited budget rather than a
derived one). Between iterations passes re-sort by reorder key: deleted nodes first, then
shape changes, then useless. Before the fixate loop, `initial_coarse_reduction`
(`shrinker/coarse.rs`) runs once from `shrink_origin`: it re-randomises small integer
nodes (`value <= 10`, `min_value == 0`) that look like `one_of` branch selectors,
probing whether zeroing changes downstream shape and, when it does, trying each lower
branch value with up to three random continuations.

`mutate_and_shrink` (`shrinker/mutation.rs`) skips targets over `MAX_MUTATE_NODES` =
32 nodes and offsets each node's index by ±1..=5. A mutation whose observing replay
realises a branch switch (same length, different kind past the mutated position, per
`replay_observing_divergence`) gets `DIVERGENT_RANDOM_ATTEMPTS` = 32 random
continuations plus two-position variants at `RANDOM_ATTEMPTS` = 3 each.

## Budgets: logical calls, physical deadline

Decision 7 splits the budget in two. `calls` counts one per `consider`/`probe`
invocation (a logical candidate) and bounds the search: `max_improvements` defaults
to `MAX_SHRINKS` = 500, and a stall guard silently drops candidates once
`calls - calls_at_last_shrink` reaches `max_stall` (also 500, grown on each accept and
inside pass loops). A gauntleted candidate may physically rerun many times to bound
its failure rate, and none of that counts against the logical budget. Physical cost is
bounded solely by the wall-clock deadline, set by the runner to
`MAX_SHRINKING_SECONDS` = 300 seconds (`hegel-c/src/native/core/mod.rs`).
`run_test_fn` is the single execution choke point: past the deadline it returns
`ShrinkHalt::Stop`, which unwinds every pass, latches `timed_out` for the slow-shrink
warning, and ends the shrink with the best example so far.

## Entering a shrink

The shrink phase loops over pending origins in sorted order, calling `shrink_origin`
(`hegel-c/src/native/test_runner.rs`) per origin. Its job here is to produce a
starting run and an anchor. The admission mechanics belong to
[the lifecycle](lifecycle.md):

1. **Deterministic verify.** While not under ND handling, the engine does one exact
   replay of the incumbent. Reproducing at the same origin makes that run the shrink start with
   anchor 0.0 and no gauntlet. A miss aborts as `Flaky` under `error` strictness.
   Otherwise it flips the run and falls into the ND arms, since a flipped verify is
   never taken as a deterministic verify (decision 38).
2. **Stashed witness.** A confirmed origin yields its confirmation batch's witness
   run and anchor once, so no new replays are spent.
3. **Trusted origin.** An evidence batch runs with the discovery bar as its stopping
   rule only, and any failure promotes the origin to confirmed with the batch's lower
   bound as anchor (decision 47). A zero-fail batch skips shrinking, though the
   origin is still reported.
4. **Unconfirmed.** An origin with history backtracks first, and one without faces
   the full discovery bar. Rejects evict per the lifecycle's rules.

If the run flips mid-shrink (the shrink started non-gauntleted and `nd_handling()`
is now true), the origin's pre-shrink nodes are restored and it is not marked shrunk:
the outer loop re-enters `shrink_origin` for it under ND handling, where it faces
backtrack or the bar and then a gauntleted shrink (decision 38, amended by decision 66
when history exists). This terminates because `nd_active` never clears within a run.
Untrusted single-run shrink progress is deliberately discarded, which is the
conservative direction under decision 2.

The shrink probe's own executions are measurement runs: under ND handling an
interesting result from one may fill a vacant origin but never displaces, persists, or
writes history (decision 65).

## The gauntlet

When `shrink_origin` runs under ND handling it constructs `EngineShrinkProbe` with
`gauntlet: true`. The probe carries the target origin, the anchor, a sweep mode, and
two pieces of cross-candidate state:

- `ledger`: per-candidate gauntlet state (`CandidateLedger`), keyed by the serialized
  realized choices, not the proposal. A candidate that punned into another realisation
  merges evidence with it, because the realized timeline is what the evidence is
  about. Each entry carries cumulative `Evidence`, the failure minimum pinned by its
  first charge, and its latched bound verdict. The ledger persists for the whole
  shrink of one origin, so pass repetitions add power to retried rejects rather than
  starting over (decision 7's reject-retry requirement).
- `raised`: realized timelines whose first adoption already raised the anchor.
  Later accepts of the same timeline must not keep raising it, or the incumbent
  prices fresh candidates out (decisions 19, 46).

Per candidate, `EngineShrinkProbe::run` executes the proposal once through
`cached_test_function` and records the match into the ledger. Under
`nd_active` the execution cache never serves, so every replay runs the body
(experiment 002, see [detection](detection.md)). Before recording, an unbound
ledger's proposal is charged against the origin's alpha budget (next section); a
bound ledger's verdict is final — a latched reject returns false at the cost of the
proposal run alone, and a latched accept returns true with a fresh `PendingAccept`,
which is what protects the timelines a nested clone shrink's final splice
re-proposes. In Fast sweep an unbound miss rejects immediately: rejects are charged
one run (decision 7). Otherwise the probe loops on
`nd::gauntlet(evidence, anchor, min_fails)` at the ledger's pinned minimum. On
Continue it reruns the realized timeline via `nd_replay_once`, a
continuation-tolerant measurement replay counted as one plain trial of the candidate
test case whatever it realizes (decision 71, part of
[the lifecycle's](lifecycle.md) Evidence machinery), and records the result. On
Reject it latches false and returns false. On Accept it keeps replaying until the
ledger holds `ANCHOR_SEED_RUNS` = 20 runs, so the bound that may move the anchor is
not biased by the stopping rule (decision 54), then latches true, stashes a
`PendingAccept`, and returns true.

`nd::gauntlet` (`hegel-c/src/native/nd/mod.rs`) does the arithmetic: the threshold is
`max(gamma * anchor, GAUNTLET_FLOOR)`, with gamma = `GAUNTLET_GAMMA` below the
retention high-water and 1.0 at or above it. Accept requires `GAUNTLET_MIN_FAILS`
failures *and* a Wilson lower bound at or over the threshold. Short of four failures
the verdict is only ever Continue. Reject fires when the upper bound proves the
threshold unreachable or at `GAUNTLET_CAP` runs.

| Constant | Value | Provenance |
| --- | --- | --- |
| `GAUNTLET_GAMMA` | 0.8 | experiment 001's gamma sweep (the size-vs-reliability dial); ledger shape from decision 7 |
| `GAUNTLET_CAP` | 30 | experiment 001's P3 policy cap, carried into the engine by experiment 003 |
| `GAUNTLET_MIN_FAILS` | 4 | decision 54 (experiment 008, finding S1) |
| `GAUNTLET_FLOOR` | 0.05 | decision 54, derived from the min-fails boundary at the cap: 0.05 < LCB(4/30) = 0.0531, so the floor costs no power |
| `ANCHOR_SEED_RUNS` | 20 | decision 54: the largest seed whose all-fail LCB (0.839) a candidate can still match within `GAUNTLET_CAP` (40-run seeding stalls shrinking) |
| `RETENTION_HIGH_WATER` | 0.8 | decision 55 (experiment 008, finding S6) |
| z (Wilson) | 1.96 | retained by decision 54 (the exact-DP operating points are the specification, z is a tuning constant) |

The min-fails rule exists because a fresh ledger's single failure has Wilson lower
bound 0.2065, so without it every threshold below that accepts on the recruiting run.
Experiment 008 measured 33% target-regime bug loss from that degeneration.
The recalibrated worst-case false accept is 4.0e-4 per proposal against a p = 0.02
fluke (target 1e-3), pinned by the test `gauntlet_matches_the_008_operating_points`.

## The alpha budget

Per-proposal numbers compose without bound: a body can realize thousands of distinct
candidate timelines in one shrink (experiment 012 saw 42k), and at 4.0e-4 each the
uncharged exposure reaches 33% by a thousand floor-threshold proposals — more where
the confirmation sweep dominates, since a drive-to-bound of a bugless candidate at
the floor carries 2.9e-3, seven times the Fast number the shipped arithmetic
composed. Decision 72 bounds this with per-origin spending: `Engine.gauntlet_spend`
holds one `nd::GauntletSpend` per origin per run (on the engine, not the probe,
because the probe is rebuilt per `shrink_origin` call and flip requeues or backtrack
restores would silently reset a probe-local bound to per-pass).

Every proposal on an unbound ledger is charged, before its outcome is recorded, its
exact unconditional false-accept mass against a q = 0.02 fluke (`gauntlet_alpha`, an
exact DP over the (runs, fails) mass): in Fast mode q times the drive-to-bound
probability from the ledger's state plus one hypothetical failure, in Confirm mode
the drive probability from its current state. Charging per proposal makes the sum
bound E[false accepts] by linearity, and exact charging keeps the measured regimes
cheap — an unreachable high-water threshold charges zero (the 012 lottery spends
nothing), mid anchors charge ~1e-6 and afford thousands of proposals, and at the
floor the `GAUNTLET_ALPHA_BUDGET = 0.02` affords ~50 Fast proposals before the
failure minimum escalates, 4 → `GAUNTLET_MIN_FAILS_CEILING = 8`, for new ledgers
only (a pinned minimum never changes: the stopping rule is fixed per test, and a
pinned re-proposal charges even past the budget, with per-ledger overdraft bounded
by one charge). At the ceiling a proposal's charge is at most ~1e-7, so the total
per-origin spend is the budget plus a negligible tail whatever the body realizes.

The escalation's power cost lands on floor-threshold shrinks that exhaust the
budget: recruited-accept at p = 0.1 against its realistic 0.053 threshold falls
0.57/0.33/0.16 at minima 4/5/6 (experiment 014), and a stricter minimum also makes
the confirmation sweep's "accepted nothing" certificate easier to obtain, stopping
earlier and missing recoverable reductions — decision 18's amended cost. Both are
conservative under decision 2: a refused candidate keeps the incumbent, costing
minimality, never failure probability.

A gauntlet accept is not adoption (decision 36): it only stashes a
`PendingAccept`. The state moves in `candidate_adopted`, called solely from
`Shrinker::accept_improvement`. An accept the shrinker discards (a punned
realisation, a mutation probe result with a larger sort key) moves nothing. On
adoption, the first accept per realized timeline raises the anchor to the accept's 20-run lower
bound if higher, both in the probe and in the lifecycle (`raise_anchor`, itself
monotone), and the new incumbent is persisted via `record_nd_incumbent` (see
[persistence](persistence.md) for the save-then-delete discipline). Every acceptance
path therefore gates on the same validated-accept event: a gauntlet accept plus
adoption.

The high-water arm has a priced residual: at gamma 1.0 on constant p = 0.9 bodies the
gauntlet can burn on the order of a million replays per episode, nearly every genuine
reduction rejecting at the run cap. Experiment 012 measured this, and decision 67
escalated it as a follow-up outside the seam accounting.

## Sweep modes and confirmed-dry stopping

Under Fast sweeps a stochastic probe may reject a candidate on one unlucky
non-reproducing run, so a fixed point reached that way is not a certificate. Decision
18's answer is `SweepMode { Fast, Confirm }`: in Confirm the probe skips the
single-run fast reject and drives every proposal's cumulative ledger evidence to a
bound verdict. `ShrinkProbe::set_sweep_mode` returns the previously active mode, or
`None` for probes whose fast judgements are already exact (non-gauntleted engine
probes and the blanket `FnMut` impl), in which case the scheduler skips confirmation
entirely.

`fixate_shrink_passes` (`shrinker/scheduling.rs`) runs Fast iterations to a fixed
point, then one Confirm iteration. An improvement there resumes the Fast fixpoint. A
Confirm iteration that accepts nothing ends the shrink, and that stop carries a
certificate: every reachable proposal was driven to a bound decision. The rule's
evidence is experiment 001's follow-up on the constant-p = 0.5 landscape, where fixed
dry-sweep counts miss reachable reductions 18–46% of the time and confirmed-dry 10%,
at the cost of three fixed dry sweeps. The stall guard applies only in Fast and only
after the first improvement. Confirm disables it, because the certificate holds only
if every candidate actually executes. The scheduler also resets the stall window at
each iteration start, so quiet calls burned by stochastic passes cannot leave the
guard latched and fake a fixed point. It restores the probe's previous mode when its
sweep ends, so a nested clone shrink's sweeps leave the enclosing shrink's mode
intact.

The boundary of this design is decision 17. Checkpoint/rollback inside the shrink
loop was rejected outright: rollback-on-uncertainty poisons stable landscapes
(missed reductions 9% to 52%), and rollback-on-proof never fires. Confirmed-dry is the
accepted alternative on the stopping side. The backtrack over origin history
([final replay](final-replay.md)) is detection-triggered and bar-gated rather than a
rollback.

## Boost

Boost runs inside `shrink_origin`, after admission and before the shrink, only when
the run is under ND handling and the anchor sits below `BOOST_RELIABILITY_FLOOR` =
0.30 — LCB(10/20), decision 28's "true rate below 0.5" translated to a 20-run batch
(decision 56: the literal 0.5 over-triggered on 59% of true-0.7 incumbents).

It is decision 2's "raise it when possible" arm: a successive-halving race over up to
`BOOST_POOL` = 16 candidates, built from the incumbent and its pool entries and
topped up with prefix-cut mutants (replay a random-length prefix of the incumbent
with a small random continuation, the same shape as the replay splices in
[persistence](persistence.md)). Rounds score raw failure rate with replays per round
doubling from 2, keeping the top half. The winner faces a `BOOST_HOLDOUT` = 20
holdout of `nd_replay_once` measurements (in-race rates are selection-biased upward)
and replaces the shrink start and anchor only when the holdout produced a failing
witness and its lower bound beats the anchor (`nd_boost`,
`hegel-c/src/native/test_runner.rs`).

Experiment 006A measured the effect: on a deterministic-core landscape boost turns 27/30 runs
landing deterministic finals into 30/30 at +14% cost. On the coreless rising
landscape it trades size for reliability (p 0.26 to 0.42, length 3 to 5, +46%), and
on constant noise the holdout gate correctly refuses. The monotone anchor alone finds
deterministic cores in 90% of runs, and boost is the guarantee on top. There is no
public setting, and entry logs one Debug line (decisions 28 and 56, gates G2/G7).

## Clone shrink

`shrink_clone_streams` (`shrinker/clones.rs`) runs a full nested `Shrinker` over the
stream inside each clone node of the current best sequence. The nested probe,
`NestedCloneProbe`, splices each `Full` candidate into the parent sequence at the
clone's position (`splice_child`) and embeds `Probe` prefixes as a `Clone` value in
the outer values, then reads the realized child stream back out of the parent run's
node at that index. It forwards both `set_sweep_mode` and `candidate_adopted` to the
outer probe: a wrapping probe that fails to forward them silently exempts the inner probe
from sweep modes and swallows its accepts. The nested shrink shares the outer
deadline. When it finishes, the final child is spliced into the parent and passed
through `consider`. That final splice re-proposes a timeline whose ledger already
holds a latched accept, which is why a bound verdict short-circuits ahead of the
gauntlet's fast reject.

## Targeting and span mutation

Targeting runs under ND handling as a measured race (`optimise_targets_nd`,
decision 68), boost's design applied to user scores. It replaced decision 39's full
disablement, which experiment 013 priced: the deterministic climber applied to a noisy
score keeps a single-run maximum that sits 1.7 standard deviations above truth on
normal noise, and the climb freezes against that inflated bar in 92-100% of trials
after roughly ten runs.

The race trusts no single run. Each label holds a reference timeline and a monotone
reference score, the median of a fresh `TARGET_ND_HOLDOUT` = 20 replay batch (a batch
that observes no score marks the label dead, and the recorded per-label maximum, being
a max of noisy draws, is only ever seed material). Per firing of the target phase, up
to `TARGET_ND_RACES` = 4 races run: a pool of `TARGET_ND_POOL` = 16 perturbations of
the reference (single-node steps by power-of-two deltas, prefix-cut mutants for the
structure the stepper cannot reach, and the recorded best while its raw score still
exceeds the reference) is successive-halved on mean observed score, and the winner is
adopted only when a fresh holdout clears the sign test `target_adopt`: the Wilson
lower bound of strictly-beats-the-reference above 0.5, which at 20 runs means 15
beats, with ties and unobserved runs counting against. Adoption re-estimates the
reference on yet another fresh batch and only ever raises it. Race replays are
measurement executions, counted by the statistics line and excluded from generation
accounting, and every replay yields to a discovery, since a found failure hands the
run's replay budget to confirmation and shrinking. Experiment 013 measured the race
reaching the landscape maximum on every gradient it can move on for ~950 replays per
run, where the old climber froze at 18 of 100.

Span mutation stays on. `try_span_mutation` runs from the generation loop regardless
of `nd_active`, making up to `SPAN_MUTATION_ATTEMPTS` = 5 probes per eligible run
through `cached_test_function`. Under ND handling the cache never serves, so each
probe executes. It exploits same-label spans, which is one reason every
engine draw emits a kind-specific span. Its interesting finds land in the engine's
`interesting` map subject
to the same admission rules as any raw sighting: under ND they may fill a vacant
origin, never displace an occupied one, and face the discovery bar before anything
consumes them (decisions 20 and 24, detailed in [the lifecycle](lifecycle.md)).
