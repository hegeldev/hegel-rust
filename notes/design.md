# Handling nondeterministic tests

As-built description of the engine's nondeterministic-test handling, written after the
implementation landed (the pre-implementation design this evolved from is in the git history
at a1d1b6d2). `decisions.md` is the decision log; `research/` holds the code maps and
adversarial review the original design was grounded in; `experiments/` (write-ups) and
`../experiments/` (frozen harnesses) carry the measurements cited below.

## Problem

Hegel inherits Hypothesis's central invariant: no nondeterminism outside the system's control.
Concurrent stateful testing intrinsically breaks it, and the pre-branch handling was wholesale
surrender — a sticky flag disabled the data tree, novel-prefix generation, span mutation,
targeting, shrinking, persistence, and reproduce blobs, then reported at most one failure per
run from a capture-at-discovery stash. Every other nondeterminism source (cloned streams,
external randomness, timing) simply aborted the run as `Flaky`/`NonDeterministic` with no
failure report.

## Goals (met)

- Handle test cases that fail at least 10% of the times they are run; the replay budgets
  and confidence arithmetic derive from that target.
- Detect nondeterminism by observation only; nothing declares it up front (decision 70
  removed the concurrent-machine declaration).
- Restore shrinking, multi-failure reporting, database persistence, and reproduce blobs for
  nondeterministic tests.
- Bound how much failure probability shrinking can trade away, and raise it when cheap. The
  guard is statistical, not a strict never-lower; experiment 008's measured envelope: on a
  rising landscape the final failure probability holds a median 0.82 (p10 0.58) against a
  0.10 floor, and target-regime (p = 0.1) bugs survive shrinking at 100% against 67%
  shipped.
- Keep a caveat for the environment-modification hypothesis, with wording that admits it is
  indistinguishable from a very rare failure.

Non-goals: Antithesis (deterministic environment, separate path); controlling or replaying
thread schedules — we sample schedules, never replay one.

## Conceptual model

Two axes needing different machinery:

- **Generation nondeterminism**: the same replayed prefix produces a different draw structure —
  a representation problem.
- **Outcome nondeterminism**: the same realized choice sequence produces a different verdict —
  a statistics problem.

Failure probability is first-class: every decision that once assumed "interesting is a pure
function of the choice sequence" — shrinker acceptance, database reuse, the final replay, the
flakiness errors — is a decision about an estimated probability with explicit budgets.

Vocabulary: a **timeline** is the realized choice sequence of one execution; the **incumbent**
is the failing timeline held as an origin's best example; the **pool** is a bounded per-origin
set of other failing timelines; the **evidence ledger** is per-candidate `(fails, runs)`
counts, never persisted; the **anchor** is a Wilson lower confidence bound on the
incumbent's failure rate; the **gauntlet** is the evidence bar a shrink candidate must clear.

## Architecture

Engine-side everything lives under `hegel-c/src/native/`: the statistics in `nd/mod.rs`
(pure arithmetic, no engine state), the per-origin failing test case in `counterexample.rs`
(`Counterexample`: incumbent, pool, standing, evidence, history, budgets — decision 73), the
run orchestration in `test_runner.rs`, persistence formats in `blob.rs`.

### The counterexample (`counterexample.rs`)

The engine's representation of a failing test case is one value per origin,
`Counterexample`, held in `Engine.origins: Counterexamples` (a `BTreeMap` by origin). It
owns everything the run knows about that failure:

- the **incumbent**, `Option<Vec<ChoiceNode>>` — the best failing execution, with its
  constraints because the shrinker works on nodes; `None` after a bar rejection evicted it
  (the record survives so a re-sighting resumes against the same evidence and budgets, and
  the caveat-only report can quote them). An origin is *live* while it holds one; "the run
  has a failure" is `Counterexamples::any_live`.
- the **pool**, `Vec<Vec<ChoiceValue>>` — the timelines captured at confirmation or trust,
  the confirm-time incumbent first, truncated to `POOL_CAP`. Kept as captured: after
  shrinking moves the incumbent, the confirm-time example stays pooled as a replay
  fallback. `timelines()` composes the current incumbent ahead of it (`pooled_timelines`:
  deduplicated, `POOL_CAP` total, incumbent included).
- the **standing** (`Unconfirmed | Trusted | Confirmed { anchor, witness }`) and the
  replay evidence behind it (fails/replays, with report-time counts kept apart) — the old
  `OriginLifecycle` state machine, now per record.
- the pre-flip **history** for the backtrack, dropped by `confirm`.
- the per-run **budgets**: bar attempts, backtrack attempts, `gauntlet_spend`, and the
  first-check flag.

The stored form is `NdReproState` (`blob.rs`), produced by `Counterexample::repro_state`;
nothing else builds it. Admission rules live on the type: `adopt` founds or
shortlex-displaces (called only pre-flip or into a vacant origin — decision 20), `replace`
installs a validated result (shrink, backtrack restore), `reject` evicts unconfirmed
origins only, `confirm`/`trust` are the pool's only writers. Under deterministic handling
every failure is a `Counterexample` too, standing `Unconfirmed` with an empty pool — the
standing only matters once the run flips.

### Mode lifecycle and strictness

`Engine.nd_active` is the sticky run-level flag; `nd_flip()` sets it. Concurrency alone is
not a flip source: a properly serialized concurrent machine can fail deterministically, so
creating a concurrent machine declares nothing and `error` strictness has no concurrency
exception — every detection below aborts under `error` (decision 70). Flip sources:

- **detected**, within-run evidence only: an execution-cache verdict mismatch — the same
  realized values concluding with a different status or origin (`record_run`'s mismatch
  signal, in `test_function_tagged`) — or a replay whose outcome flips (the
  first-interesting check, the pre-shrink verify, and the final-replay status checks flip
  the run rather than aborting under deterministic handling). Every generation-discovered
  origin gets the first-interesting check before anything consumes it: `FIRST_CHECK_REPLAYS`
  = 4 exact replays of its incumbent sighting at sweep time (in-batch displacement may
  already have replaced the discovery), stopping at the first miss, whose evidence
  seeds the origin's discovery bar (decision 64). Under `error` strictness the kind ledger
  also aborts on within-run generation kind drift, and a check miss aborts — structural
  divergence with a position-naming diagnostic, an aligned outcome change as flaky. A
  stored DB entry that stops reproducing is staleness, never evidence (decision 9).
- **stored**: decoding a version-2 database entry (`run()`'s reuse loop) or ND reproduce blob
  (`reproduce_blob`) — state only a nondeterministic run writes — flips the run before any
  replay of it.

`nondeterminism_strictness = quiet | warn | error` (`hegel_settings_set_nondeterminism_strictness`),
default quiet: quiet flips silently, warn prints one notice, error reproduces the old
Flaky/NonDeterministic aborts verbatim for suites using determinism as a lint (decisions 1, 30).

### Statistics (`nd/mod.rs`)

- `Evidence`: plain `(fails, runs)` counts with Wilson bounds. Every replay is one
  Bernoulli trial of the test case under the standing replay procedure, whatever timeline
  it realized (decision 71): the statistics are about the test case, which can realize
  many timelines, not about tracking one realized timeline, so a structurally diverged
  miss counts in full — replaying the stored state and not seeing the failure is exactly
  what non-reproduction means. This is the setting the deriving experiments modelled
  (005A's DP is pure Bernoulli; 008's headline envelope is its w = 1.0 column), and it
  retires the verbatim watermark (decisions 22/45/57, superseded), whose weighting patched
  the per-timeline estimand rather than fixing it.
- Discovery bar (decision 23, experiment 005A): gate 10 misses, reject on zero
  failures; otherwise extend to 40 replays, accepting early on the 4th failure
  (`GATE_RUNS`, `CONFIRM_CAP`, `CONFIRM_MIN_FAILS`). Bar batches spend a per-origin
  per-run budget of `BAR_ATTEMPTS_PER_RUN = 5` across the sweep, shrink admission, and
  the pooled review (decision 72, experiment 014): at the cap the origin is rejected
  with evidence (0, 0) instead of batched, so a re-sighted fluke cannot recycle the
  bar's false-accept rate without bound.
- Gauntlet (experiments 001/003, recalibrated by 008/decision 54): accept on at least
  `GAUNTLET_MIN_FAILS = 4` failures with ledger LCB clearing `max(gamma * anchor, 0.05)`,
  where gamma is 0.8 below `RETENTION_HIGH_WATER = 0.8` and 1.0 at or above it (decision
  55); reject when the UCB proves the threshold unreachable or at 30 runs; short
  of the failure minimum the verdict is Continue, never Reject (`GAUNTLET_GAMMA`,
  `GAUNTLET_FLOOR`, `GAUNTLET_CAP`; the floor is derived: the min-fails acceptance
  boundary at the cap). Proposals spend a per-origin per-run alpha budget
  (`GauntletSpend`, `GAUNTLET_ALPHA_BUDGET = 0.02`, decision 72): each is charged its
  exact unconditional false-accept mass against a q = 0.02 fluke (an exact DP,
  `gauntlet_alpha`), and when a new candidate's charge is unaffordable the failure
  minimum escalates up to `GAUNTLET_MIN_FAILS_CEILING = 8`, pinned per candidate at
  first charge.
- Anchor seeding (decision 54): every anchor-seeding batch reaches `ANCHOR_SEED_RUNS = 20`
  runs — the discovery bar's batch extends past its accept, and a gauntlet
  accept's ledger is topped up — so anchors estimate the reproduction rate rather than the
  stopping rule.
- First-interesting check: `FIRST_CHECK_REPLAYS = 4` exact replays per discovered origin,
  stop on first miss (decision 64 — reproduction odds and the seeded bar make more
  redundant). Backtrack: the scan is capped at `BACKTRACK_SCAN_REPLAYS` = `CONFIRM_CAP` =
  40 replays and `BACKTRACK_BAR_ATTEMPTS = 3` discovery-bar batches, a budget held per
  origin per run across backtracks (decision 66, amended by 72), separate from
  `BAR_ATTEMPTS_PER_RUN` because history skews toward the real bug's pre-flip sightings.
- Replay budgets from the p >= 0.1 target: `replay_budget(rate, tolerance)` with 5% miss
  tolerance gives ~29 replays, early exit on failure, so live bugs cost ~1/p
  (`TARGET_FAILURE_RATE`, `reuse_replay_budget`).
- Continuation budget for replaying a stored timeline: the timeline plus `max(4, len/8)`
  fresh draws (experiment 004).

### Origin lifecycle (`Counterexample`'s standing, `counterexample.rs`)

Per-origin state machine on the counterexample's standing: `Unconfirmed -> Confirmed` (discovery bar),
`Unconfirmed -> Trusted` (database reproduction, decisions 24/47 — the prior run persisted
only validated origins, so trusted origins are exempt from the bar's verdict), `Trusted ->
Confirmed` (promotion by a failing shrink-time evidence batch, decision 47), monotone anchor
raises on validated accepts (decision 19). Confirmation gates origin *admission*:
`nd_discovery_sweep` runs the bar over every unconfirmed interesting origin after each
generation step, dropping origins that fail it (decision 24); raw interesting runs never
displace an occupied origin (decision 20). A trusted origin reaching the shrink loop runs
one evidence batch (`nd_evidence_batch`, the bar arithmetic as stopping rule only): any
failure promotes it with the batch's LCB as anchor and its pool merged fresh-first with the
stored one (decision 48); zero failures fold into the trusted counts and skip shrinking,
still reported and persisted. The promoting batch extends to `ANCHOR_SEED_RUNS` on accept
like every anchor-seeding batch (decision 54). `Trusted` carries the reproducing batch's
evidence, seeded by `trust()` on the database, blob, and deterministic replay paths. Confirmed and trusted
origins carry a pool (10 stored timelines total, incumbent first: `pooled_timelines` builds
every one, and `confirm`/`trust` truncate incoming pools) harvested from
capture-at-confirmation (decision 10). A first-check miss deposits its evidence in a
per-origin seed slot (`seed_evidence`/`take_seed`), consumed by the origin's next evidence
batch so the bar starts partially filled (decision 64). The counterexample also words its
failure's **caveat** from the run's own evidence — confirmed, trusted, dry-at-report-time
variants of both, or unconfirmed — quoting report-time replay counts apart from the
confirmation or reuse counts.

While the run is deterministic, `record_run` also appends every interesting execution to the
counterexample's **history** (`counterexample::History`: raw sightings and accepts,
deduplicated by serialized nodes, unbounded, dropped on confirmation or run end). A never-confirmed origin
that misses its shrink verify or final replay **backtracks** over that history to the
reproduction boundary — geometric probes over the accept segment plus every raw sighting,
binary refinement, then the full discovery bar on the candidate (up to
`BACKTRACK_BAR_ATTEMPTS` batches, the scan capped at `BACKTRACK_SCAN_REPLAYS` replays); a
cleared bar confirms the origin with the batch's witness and anchor, pools the scan's
other reproducing entries, and force-persists the restored incumbent past the Persister's
monotone `needs_save` (decision 66). Measurement runs never displace, persist, or enter
history, except the reuse phase's `nd_reproduce` replays, whose displacement is what
validates a v2 entry under `error` strictness (decision 65).

### Representation and persistence

`NdReproState` (blob.rs): stored timelines incumbent-first, an entropy seed, and the
continuation-budget extension. Serialized as the version-2 database entry and behind blob
prefixes 2/3 (self-identifying, decision 8; old readers reject unknown prefixes loudly). Only
origins past confirmation persist — confirmed or trusted; the end-of-run filter is the
lifecycle's `needs_confirmation`. Confirmation and gauntlet accepts also save the validated
incumbent mid-run via the `Persister`, each save landing before the bytes it supersedes are
deleted, so the primary key always carries the most recent validated example — Ctrl-C keeps
it (decision 44). A superseded same-run save is deleted, never demoted.
End-of-run reconciliation demotes only the run-start primary entry, leaving one secondary
deposit per origin per run. No rates or counters are ever persisted (decision 8): every run
stands alone. Hygiene is two strikes across two runs: primary miss demotes to the secondary
corpus, secondary miss deletes (decision 11), with `SECONDARY_CORPUS_CAP` (50 per key)
evicting the shortlex-largest at reconciliation as a resource bound outside the two-strike
scheme (decision 44). The pre-shrink secondary drain is v1-only and runs only under
deterministic handling, breaking on a mid-drain flip. A v2 entry is never drained: under decisions 20/24 a pre-shrink
reproduction can change no outcome, so its hygiene lives in the reuse phase's budgeted
strikes (decision 40).

Clone streams serialize values-only (tag 5); realized kinds are dropped. Verbatim replay is
unaffected — the only consumer of realized info is `resolve_choice`'s is-simplest pun, which
fires solely on constraint drift (decision 32, measured in 007).

### Replay-until-failure (`nd_reproduce`)

One primitive serves database reuse, the final replay, and blob replay (decision 25);
confirmation runs its own bar-driven batch (`nd_evidence_batch`). Every measurement replay
runs the whole counterexample as one test case (decision 74, `core/replay.rs`,
`NativeTestCase::for_counterexample`): the *live set* is the timelines that agree with every
value drawn so far in every stream; each draw is served from the first live timeline whose
stored value fits the request, and the live timelines whose value there differs drop out;
following a stored branch is not a divergence. A *divergence* is the moment no live timeline
fits — recorded once, as the stream and position, on `RunResult.divergence` — after which
the pruned timelines continue positionally where they fit (most recently pruned first) and
the run draws randomly where none does, under the continuation budget of the longest
timeline. Replay order in `nd_reproduce`: the whole set up to `reuse_replay_budget()` times,
then positional splices of random timeline pairs (10, decision 52), then fresh generations
where the caller allows them. Splices cut whole timelines at top-level positions, so a
clone stream — one `ChoiceValue::Clone` element — crosses over intact. Punning (a misfit
served as the draw's simplest or unit value, the sequence staying the positional prefix)
survives only as `Rescue::Pun`, the mode of shrink proposals and probes (`for_choices`,
`for_probe`), whose misfits are the shrink's own edits and never count as divergences.
Executions run through `measure()`, which detects nondeterminism and admits origins like
any run but moves none of the runner's quantitative state (below).

### Shrinking

Charge accepts, not rejects (decision 7): a candidate whose first run passes is rejected with
0/1 in the ledger; a candidate whose first run fails pays the gauntlet before displacing the
incumbent. Rejected candidates retry via pass repetition with evidence accumulating across
retries. Stopping is confirmed-dry (decision 18): after a dry sweep, one confirmation sweep
drives every proposal's cumulative evidence to a bound decision. The anchor is monotone, and
post-accept re-measurement of the standing incumbent never feeds it (decision 19); it
estimates the incumbent's reproduction rate under the engine's own pinned-replay procedure,
raised only at validated events, so candidate and incumbent sit on one estimand
(decision 46). An accept requires
`GAUNTLET_MIN_FAILS` failures, and the accepted ledger is topped up to `ANCHOR_SEED_RUNS`
before its bound can move the anchor (decision 54); at anchors of `RETENTION_HIGH_WATER`
and above the gauntlet runs at gamma 1.0, refusing to trade a zero-miss incumbent's
reliability down (decision 55). Every proposal on an unbound ledger is charged against
the origin's alpha budget before it runs (decision 72): the budget lives on the engine
(`Engine.gauntlet_spend`) so a re-shrink's rebuilt probe keeps spending from it, each
ledger's failure minimum pins at its first charge, and bound verdicts latch — a latched
accept keeps re-proposals of a conclusively accepted timeline acceptable (the nested
clone shrink's final-splice guard), a latched reject spends no further replays or
budget. There is no checkpoint/rollback (decision 17). All
acceptance paths gate on the same validated-accept event, which is a
gauntlet accept *and* the shrinker's adoption (`candidate_adopted`): an accepted candidate
the shrinker discards — a punned realization, a sort-key-larger mutation probe — raises no
anchor and persists nothing (decision 36). A nondeterministic flip during the shrink verify
backtracks over the origin's history when it has one (decision 66) and otherwise routes the
origin through the discovery bar; a flip during the shrink probes requeues one gauntleted
re-pass from the verified pre-shrink incumbent, discarding untrusted single-run progress
(decision 38).

Boost (`nd_boost`, gate G2/decisions 28 and 56): when a confirmed incumbent's anchor sits
below the reliability floor (`BOOST_RELIABILITY_FLOOR`, 0.30 in 20-run-batch LCB units),
successive halving over the incumbent, its pool, and prefix-mutant fills (up to
`BOOST_POOL` candidates), scored by raw in-race failure rate, the winner re-measured on a
`BOOST_HOLDOUT` (= `ANCHOR_SEED_RUNS`) holdout before seeding the anchor. Above the floor
it never runs; each race logs one Debug line at entry; there is no public setting.

### Shrinking the counterexample as a set (decision 75)

The gauntlet's reruns replay the candidate *set* — the candidate's realized values in
front of the origin's pool — and only reruns that stayed live on the candidate are its
evidence; a rerun that left it (a bounce: the test took another stored branch, or
diverged) is no evidence either way and is charged against a per-candidate bounce budget
derived from the incumbent's own bounce rate (`bounce_budget`; zero when the incumbent
never bounced). Past the budget the candidate is abandoned without a verdict and pass
repetition may retry it. After the per-timeline shrink, `nd_multiverse_shrink` shrinks
the set under `set_order` (fewer timelines, then `timeline_order` lexicographically):
delete a component, swap adjacent components toward sorted order (the order is state —
it decides which timeline serves at a disagreement — and sorted is the fixpoint), and
replace a component with a positional splice of another's prefix onto its tail when the
splice is smaller. Each candidate set faces `nd_evaluate_set` — the gauntlet driven to
a bound with every replay as evidence, since the set is the estimand — and a set whose
first timeline changed is installed only from a failing run that stayed live on it. The
bounds are the shrink deadline and `MULTIVERSE_ROUNDS`. Not built: a shared-prefix edit
across components (capture at confirmation plus the delete pass covers most of it), and
nodes for pool members (a promoted component's nodes come from its witness run).

### Targeting under ND handling

Targeting runs under ND handling as a measured race (`optimise_targets_nd`,
decision 68, experiment 013) — boost's design applied to user scores, replacing decision
39's full disablement. Under a nondeterministic score every rule the deterministic climber
trusts breaks the same way the shrink loop's did: the recorded per-label maximum is the max
of noisy draws (013: 1.6-1.7 sd above truth on normal noise, +36 raw with heavy tails), a
strict-improvement accept ratchets on flukes and then freezes (013: frozen in 92-100% of
trials with gradient remaining, final position 18 of 100), and `FindInteger`'s
bisection invariant corrupts on one noisy verdict. So under `nd_active` the recorded best
is demoted to seed material (observations are recorded again — the record itself is
harmless once nothing treats it as an estimate), and each label holds an `NdTarget`: a
reference timeline plus a monotone reference score estimated only from fresh unselected
batches (the median of `TARGET_ND_HOLDOUT` replays; a batch observing no score marks the
label dead). Per firing of the target phase, up to `TARGET_ND_RACES` races run: a pool of
`TARGET_ND_POOL` perturbations of the reference (single-node steps by power-of-two deltas,
plus boost's prefix-cut mutants — the only lever on structure the stepper cannot reach,
such as clone streams, and the recorded best when its raw score still exceeds the
reference), successive-halved on mean observed score, the winner adopted only when a fresh
`TARGET_ND_HOLDOUT` holdout clears the sign test (`target_adopt`: Wilson LCB of
strictly-beats-the-reference above 0.5, ties and unobserved runs counting against). On
adoption the reference is re-estimated on another fresh batch and only ever raised.
Firing requires the interesting map empty, every replay yields to a discovery, and race
replays are `measure()` executions: counted by the statistics line, excluded from
generation accounting, never recorded as observations.

### The execution cache and kind ledger

The data tree is gone (seam plan phase 15, closing decisions 6 and 29; experiment 010
measured what its four roles bought). Its replacements, both in `exec_cache.rs`:

- **The execution cache** keys every executed conclusion on its serialized realized values
  (`serialize_choices` semantics: floats by bit pattern, clones by child values; overruns
  concluded nothing and enter nothing). The digest tier holds a 128-bit fingerprint per
  conclusion: a generation-window repeat advances the consecutive-duplicate counter
  (`DUPLICATE_STOP` of them ends generation while no case is valid — the exhausted-space
  FilterTooMuch trigger; valid spaces are budget-bounded, since duplicate streaks are
  routine mid-size-space behavior), and a repeat concluding with a different status or
  origin is the verdict-flip nondeterminism evidence the tree could never see. The full
  tier keeps complete serving entries (status, origin, nodes, spans) outside the
  generation window, byte-bounded with oldest-first eviction, and
  `cached_test_function` serves exact repeats from it — 010's 85% shrink-serve win.
  Capabilities the tree had beyond exact repeats — trailing-unread proposals, predicted
  overruns, pun prediction, novel-prefix generation, proven exhaustion — are gone by
  measurement (serves ≈ exact repeats; recording alone cost 40-80% wall overhead).
- **The kind ledger** is `error` strictness's generation-nondeterminism detector: a map
  from rolling value-prefix hash to the choice kind (constraints included) drawn at the
  next position, compared within-run across executions, producing the tree's diagnostic
  verbatim. It is never fed between runs — a stored entry that stops reproducing is
  staleness (decision 9) — and quiet/warn don't maintain it: their detection is the
  verdict channel plus the replay checks.

Under ND handling both are disabled (gate G3/decision 29): cache recording and serving,
the duplicate stop, and the ledger are all off once `nd_active` is set — the flip flushes
the cache, and `cached_test_function` executes every replay, since serving the first
recorded verdict is exactly the bias the multi-run machinery exists to avoid. The flip
also stops any in-flight deterministic climb; targeting continues in its measured mode
(decision 68, above).

### Reporting

The engine owns the final replay (`final_replay`): every failure it is about to report
re-executes first — deterministic runs once (a miss flips the run to ND handling, or aborts
under `error`), ND runs through the replay primitive plus up to 4 fresh generations
(`FINAL_REPLAY_FRESH`, chosen not derived, decision 53). A deterministic miss on a
never-confirmed origin with history backtracks, and a restored incumbent re-shrinks under
the gauntlet on the shrink deadline's remaining budget before its pooled replay; origins
exactly replayed before a later origin's flip re-enter the queue for the pooled review
(decision 66). Executions whose failures can
become the report — confirmation batches, database-reuse replays, the final replay,
first-check replays, blob
replays, and generation cases once ND handling is active (decision 49) — are **stamped**
(`hegel_test_case_should_capture`, renamed from
`hegel_test_case_is_nondeterministic` because the stamp means capture, decision 50), telling
the client to capture output, diagnostic, and backtrace; shrink, gauntlet, and boost probes
stay cheap and unstamped. Stamping generation cases means an unconfirmed one-shot failure
still reports its discovering case's draws and diagnostic; decision 49's documented gaps
(gauntlet-discovered origins and the flip case itself) stay bare. Under `show_statistics`
the statistics block adds one line with the measurement replay count and its failures —
first-check replays included via the check-window flag — the only sub-Debug surface
revealing the flip and its cost (decision 51, amended by 64).

ND failures report as plain `FAILED` (gate G1/decision 27; `FAILED_NONDETERMINISTIC` is
retired) with a per-failure caveat accessor (`hegel_failure_caveat`) quoting the run's own
replay evidence, and a v2 reproduce blob when confirmed or trusted. Failures are assembled
from confirmed and trusted origins only: `build_report` partitions on the same
`needs_confirmation` predicate as the persistence filter, before the sort and the
single-failure truncation. An origin unconfirmed at report time — a bar reject, or one
first observed by a report-time measurement run — still fails the run (decision 3), reported
caveat-only with no blob, and only when nothing confirmed or trusted (decision 24). The
final replay evicts an origin its bar rejects; origins admitted during the final replay are
never barred and recycle via rediscovery next run (decision 35). A pending origin the
pooled review reproduces is not confirmed by that failure alone: the reproducing run faces
a standard evidence batch on the origin's remaining bar attempts, bounded by the shrink
deadline, and a rejected or unaffordable batch falls through to backtrack-then-evict
(decision 72 — the any-failure rule confirmed a q = 0.02 fluke about half the time). The frontend
(`src/run_lifecycle.rs::drive`) captures each interesting case's buffered output per origin
as the run pumps — replacement is rank-gated (diagnostic, then draw lines, then bare), newest
at the best rank, the panic payload travelling with its capture (decision 37) — then prints
each reported failure as one block (best capture, diagnostic, caveat, reproducer line) and
re-raises the failing test's own panic (or, for several distinct failures, a panic carrying
the count). A dry final replay prints the freshest stamped
failing execution, usually confirmation-time pre-shrink values, while the blob carries the
shrunk incumbent. Stamping gauntlet accepts would break decision 10's cost profile.

### Reproduce blobs

`hegel_run_start_blob` replays a blob as a run: a deterministic blob replays its choices once;
an ND blob runs the replay primitive over its stored pool with no fresh tier (decision 33).
`Hegel::reproduce_failure` drives it through the same frontend loop. `hegel_test_case_from_blob`
remains for embedders as a documented single attempt.

### Concurrency unification (experiment 007)

Concurrent-machine runs flow through the pipeline above like any other run: creation always
succeeds, the flip happens when the run's nondeterminism is observed (typically the first
replay miss or verdict flip — a machine whose failure reproduces exactly stays deterministic,
decision 70), and
concurrent failures are confirmed, shrunk, persisted, and blob-reproducible. The prior
regime's case stamping, sacrificed first case, shrink/persistence/span-mutation gates, and
blobless static-caveat reporting are gone. Measured at ceiling: 20/20 discovery and DB reuse,
60/60 blob replays on a genuinely racy machine. Off-ceiling (009a, re-verified on the
composed engine by 009b): database reuse >= 99% and blob replay >= 95.5% on the racy
machine across p = 0.1-0.9, at or above the 98% design point for p <= 0.3. The residual
misses lived in episodes that never flipped into ND handling — the G20 seam — and the
first-interesting check closed them: experiment 012 (same bodies, post-seam engine)
measured 0/200 never-flip episodes in every cell (was 23/200 at p = 0.9), blob
reproduction 200/200 on both bodies at p = 0.9 (was 180 and 191), and reuse at 100%.

### Accounting

`measure()` executions — confirmation batches, gauntlet runs, boost measurements,
replay-until-failure, and the final replay's deterministic re-execution — are excluded from
`valid_test_cases`, the invalid budget, health-check
counters, event statistics, targeting records, and the bug-window markers, which all describe
generation. Without the split every quantitative runner behavior silently changes meaning.

### ABI summary

Added: `hegel_settings_set_nondeterminism_strictness`, `hegel_failure_caveat`,
`hegel_run_start_blob`; blob prefixes 2/3. Changed: run status 3 retired;
`hegel_test_case_is_nondeterministic` renamed to `hegel_test_case_should_capture` with no
shim (decision 50), now covering every execution a failure report can be built from;
concurrent machine creation no longer rejects, and no longer declares the run
nondeterministic (decision 70). hegel-c's changelog carries the break; the
root crate's changelog covers only the user-facing behavior.

## Closed decisions

| Question | Outcome |
| --- | --- |
| Per-position divergence anchors; anchoring inside clone streams | Closed, none anywhere (decisions 14/31): fall-off positions are unpredictable (004), and positional splicing of stored timelines rescues the pool's residue (006B, 007 at ceiling; 009a off-ceiling — reuse/blob >= 98% at p <= 0.3 and the escalation signal did not fire, decisions 57/58; re-verified on the composed engine by 009b) |
| Merged trie encoding | Rejected (decision 5, hardened by 004: prefix sharing anticorrelates with pool need) |
| Checkpoint/rollback in the shrink loop | Dropped (decision 17) |
| `replay_aligned` under ND | Holds only when the stored incumbent realizes identically (outcome-ND bodies), skipping shrink; structurally-ND bodies misalign and re-shrink every run (005B measured the price) |
| FAILED vs FAILED_NONDETERMINISTIC | FAILED + caveat accessor (decision 27) |
| Boost default | Reliability-floor heuristic, no setting (decision 28) |
| Clone-kind serialization fidelity | Values-only kept (decision 32) |
| Multiple-testing correction | Sequential per-origin budgets, not Benjamini-Hochberg (decision 72): verdicts act immediately and irreversibly, so there is no p-value batch to rank |
| Replay semantics over the pool | First-fit per timeline retired (decision 74): the whole pool replays as one test case under the live set, deciding between stored branches only when the test's own choices reveal one; splices are the rescue tier. Reopens the replay conclusions of decisions 22/25/31; the pool as storage stands |

## Known risks (accepted)

- **Invisible divergence**: kind-compatible structural divergence can evade detection in
  principle; whole-timeline machinery is the backstop.
- **Origin instability**: a cross-thread panic re-raised on the test thread via
  `resume_unwind` (a ferried payload) collapses to `Panic at <unknown>`; a plain
  `join().unwrap()` instead pins the origin to the join site and loses the message; a
  never-joined thread's panic produces no failure at all. The fix is deferred to structured
  concurrency support.
- **Shrink wall clock**: multi-run accounting makes `MAX_SHRINKING_SECONDS` the binding
  constraint for slow concurrent bodies; a budget setting is possible later.
- **Caveat fatigue**: hence evidence-weighted wording and unconfirmed-only-when-nothing-
  confirmed reporting.
- **Anti-conservative statistics**: per-run peeking, stop-on-fail, and asymmetric miss
  weighting all bias the Wilson intervals toward acceptance relative to nominal coverage.
  The exact-DP operating points are the specification and z is a tuning constant.
  Experiment 008 measures the realized error per test; composition across repeated tests
  is bounded by decision 72's per-origin budgets.
- **Multiplicity control's power price** (decision 72, experiment 014): within-run
  recycling is capped, so a sub-target bug (p ~ 0.05) confirms in 42% of runs instead of
  near-certainly given a long one, leaning on cross-run recycling; a mixed bug-plus-fluke
  origin pays most (bug confirm 0.95/0.72/0.45 at fluke share 0/0.5/0.75); and
  floor-threshold shrinks that exhaust the alpha budget escalate the failure minimum,
  which also makes decision 18's "accepted nothing" stopping certificate easier to
  obtain — stopping earlier and missing recoverable reductions. All conservative under
  decision 2: refused candidates and confirms keep incumbents, never lose failures.
- **Shrink opacity below `Debug`**: a stalled shrink and a finished one print identically
  except at `Debug` verbosity.
- **Quiet-flip invisibility**: under quiet strictness nothing below `Debug` reveals that a
  run flipped into ND handling or what the measurement runs cost; G17's statistics line
  (phase 10) reveals both, but only under `show_statistics`.
- **The deterministic-to-ND seam**: a run that flips only on late detection has already
  spent its deterministic window — pre-flip `update_interesting` displacement walks the
  incumbent down the landscape before decision 20's guard engages. The seam plan (phases
  14–16: the first-interesting check, origin history, backtracking, the seeded bar)
  closed most of it: in 011's comparison the target-regime caveat-only rate fell 49% → 0,
  fluke caveats 15% → 0, and the gradient cell's final-p median rose 0.34 → 0.74 against
  the 0.82 envelope. Two residuals stay accepted: post-flip displacement freeze can
  report a confirmed flaky example where free displacement would have found a smaller or
  deterministic one the run never held (011's D2: 30/100), and a high-p origin can pass
  an honest check and never flip (priced by 012). A rerun of a run that persisted ND
  state enters ND from the stored flip and skips the seam; a caveat-only run persists
  nothing and re-races it.
- **Bindings**: the ABI break needs a coordinated rollout; hegel-c's RELEASE.md calls it out.
