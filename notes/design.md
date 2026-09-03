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

- Handle test cases that fail at least 10% of the times they are run; all budgets and
  confidence arithmetic derive from that target.
- Detect nondeterminism as well as accepting declarations (concurrent machines).
- Restore shrinking, multi-failure reporting, database persistence, and reproduce blobs for
  nondeterministic tests.
- Bound how much failure probability shrinking can trade away, and raise it when cheap. The
  guard is statistical, not a strict never-lower; experiment 008 measures the envelope.
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
set of other failing timelines; the **evidence ledger** is per-candidate `(fails, weighted
runs)` counts, never persisted; the **anchor** is a Wilson lower confidence bound on the
incumbent's failure rate; the **gauntlet** is the evidence bar a shrink candidate must clear.

## Architecture

Engine-side everything lives under `hegel-c/src/native/`: the statistics and lifecycle in
`nd/` (`mod.rs`, `lifecycle.rs`), the run orchestration in `test_runner.rs`, persistence
formats in `blob.rs`.

### Mode lifecycle and strictness

`Engine.nd_active` is the sticky run-level flag; `nd_flip()` sets it. Flip sources
(`test_function_tagged`):

- **declared**: the first executed case that creates a state machine with
  `max_concurrency > 1` (`FamilyCore::concurrent_machine`). Declared concurrency enters ND
  handling even under `error` strictness — the user asked for threads.
- **detected**, within-run evidence only (`record_run`'s mismatch signal): a choice-tree
  divergence on identical replayed choices, or a replay whose outcome flips (the final
  replay's miss under deterministic handling flips the run rather than aborting). A stored DB
  entry that stops reproducing is staleness, never evidence (decision 9).
- **stored**: decoding a version-2 database entry or ND reproduce blob — state only a
  nondeterministic run writes — flips the run before any replay of it.

`nondeterminism_strictness = quiet | warn | error` (`hegel_settings_set_nondeterminism_strictness`),
default quiet: quiet flips silently, warn prints one notice, error reproduces the old
Flaky/NonDeterministic aborts verbatim for suites using determinism as a lint (decisions 1, 30).

### Statistics (`nd/mod.rs`)

- `Evidence`: divergence-weighted Wilson bounds. A replay that diverged from its stored
  timeline before completing weighs its non-failure by the **verbatim watermark** — the
  fraction tracked before first divergence (decision 22) — so diverged misses don't count full
  weight toward demotion or confirmation misses.
- Discovery bar (decision 23, experiment 005A): gate 10 replays, reject on zero failures;
  otherwise extend to 40, accepting early on the 4th failure (`GATE_RUNS`, `CONFIRM_CAP`,
  `CONFIRM_MIN_FAILS`).
- Gauntlet (experiments 001/003): accept when the ledger LCB clears
  `max(0.8 * anchor, 0.05)`, reject when the UCB proves it never will, cap 30 physical runs
  (`GAUNTLET_GAMMA`, `GAUNTLET_FLOOR`, `GAUNTLET_CAP`).
- Replay budgets from the p >= 0.1 target: `replay_budget(rate, tolerance)` with 5% miss
  tolerance gives ~29 replays, early exit on failure, so live bugs cost ~1/p
  (`TARGET_FAILURE_RATE`, `reuse_replay_budget`).
- Continuation budget past a stored timeline: `len + max(4, len/8)` fresh draws
  (experiment 004).

### Origin lifecycle (`nd/lifecycle.rs`)

Per-origin state machine: `Unconfirmed -> Confirmed` (discovery bar),
`Unconfirmed -> Trusted` (database reproduction, decision 24 — the prior run only persisted
confirmed origins, so the bar is not re-run), monotone anchor raises on validated accepts
(decision 19). Confirmation gates origin *admission*: `nd_discovery_sweep` runs the bar over
every unconfirmed interesting origin after each generation step, dropping origins that fail it
(decision 24); raw interesting runs never displace an occupied origin (decision 20). Confirmed
origins carry a pool (10 stored timelines total, incumbent first: `pooled_timelines` builds
every one, and the lifecycle's writers truncate incoming pools) harvested from
capture-at-confirmation (decision 10). The
lifecycle also words each failure's **caveat** from the run's own evidence — confirmed,
trusted, confirmed-but-dry-at-report-time, or unconfirmed.

### Representation and persistence

`NdReproState` (blob.rs): stored timelines incumbent-first, an entropy seed, and the
continuation-budget extension. Serialized as the version-2 database entry and behind blob
prefixes 2/3 (self-identifying, decision 8; old readers reject unknown prefixes loudly). Only
confirmed origins persist, via the `Persister`, which buffers during shrinking and commits
validated incumbents — Ctrl-C keeps the last validated example. No rates or counters are ever
persisted (decision 8): every run stands alone. Hygiene is two strikes across two runs:
primary miss demotes to the secondary corpus, secondary miss deletes (decision 11). The
pre-shrink secondary drain is v1-only and runs only under deterministic handling, breaking on
a mid-drain flip. A v2 entry is never drained: under decisions 20/24 a pre-shrink
reproduction can change no outcome, so its hygiene lives in the reuse phase's budgeted
strikes (decision 40).

Clone streams serialize values-only (tag 5); realized kinds are dropped. Verbatim replay is
unaffected — the only consumer of realized info is `resolve_choice`'s is-simplest pun, which
fires solely on constraint drift (decision 32, measured in 007).

### Replay-until-failure (`nd_reproduce`)

One primitive serves confirmation, database reuse, the final replay, and blob replay
(decision 25): each stored timeline first-fit under a weighted per-timeline budget, then
positional splices of random timeline pairs (6), then fresh generations where the caller
allows them. Splices cut whole timelines at top-level positions, so a clone stream — one
`ChoiceValue::Clone` element — crosses over intact. Executions run through `measure()`, which
detects nondeterminism and admits origins like any run but moves none of the runner's
quantitative state (below).

### Shrinking

Charge accepts, not rejects (decision 7): a candidate whose first run passes is rejected with
0/1 in the ledger; a candidate whose first run fails pays the gauntlet before displacing the
incumbent. Rejected candidates retry via pass repetition with evidence accumulating across
retries. Stopping is confirmed-dry (decision 18): after a dry sweep, one confirmation sweep
drives every proposal's cumulative evidence to a bound decision. The anchor is monotone and
never fed by replay-sourced evidence (decision 19). There is no checkpoint/rollback
(decision 17). All acceptance paths gate on the same validated-accept event, which is a
gauntlet accept *and* the shrinker's adoption (`candidate_adopted`): an accepted candidate
the shrinker discards — a punned realization, a sort-key-larger mutation probe — raises no
anchor and persists nothing (decision 36). A nondeterministic flip during the shrink verify
or the shrink probes routes the origin through the discovery bar and one gauntleted re-pass
from the verified pre-shrink incumbent, discarding untrusted single-run progress
(decision 38).

Boost (`nd_boost`, gate G2/decision 28): when a confirmed incumbent's anchor sits below the
reliability floor (`BOOST_RELIABILITY_FLOOR`), successive halving over the incumbent, its
pool, and prefix-mutant fills (up to `BOOST_POOL` candidates), scored by raw in-race failure
rate, the winner re-measured on a `BOOST_HOLDOUT` holdout before seeding the anchor. Above
the floor it never runs; there is no public setting.

### Data tree under ND handling

Disabled (gate G3/decision 29): recording, tree-served replays, novel-prefix generation, and
targeting (the optimiser, its observation recording, and any in-flight climb) are all off
once `nd_active` is set — `cached_test_function` executes every replay,
since serving the first recorded verdict is exactly the bias the multi-run machinery exists to
avoid. Kind-set tolerance is the noted follow-up if generation cost ever shows up; experiment
007 measured none on the target workloads.

### Reporting

The engine owns the final replay (`final_replay`): every failure it is about to report
re-executes first — deterministic runs once (a miss flips the run to ND handling, or aborts
under `error`), ND runs through the replay primitive plus up to 4 fresh generations. Replays
of already-discovered failures — confirmation batches, database-reuse replays, the final
replay, blob replays — are **stamped** (`hegel_test_case_is_nondeterministic`), telling the
client to capture output, diagnostic, and backtrace; ordinary exploration and shrink probes
stay cheap and unstamped.

ND failures report as plain `FAILED` (gate G1/decision 27; `FAILED_NONDETERMINISTIC` is
retired) with a per-failure caveat accessor (`hegel_failure_caveat`) quoting the run's own
replay evidence, and a v2 reproduce blob when confirmed. Failures are assembled from
confirmed and trusted origins only: `build_report` partitions on the same
`needs_confirmation` predicate as the persistence filter, before the sort and the
single-failure truncation. An origin unconfirmed at report time — a bar reject, or one
first observed by a report-time measurement run — still fails the run (decision 3), reported
caveat-only with no blob, and only when nothing confirmed (decision 24). The final replay
evicts an origin its bar rejects; origins admitted during the final replay are never barred
and recycle via rediscovery next run (decision 35). The frontend
(`src/run_lifecycle.rs::drive`) captures each interesting case's buffered output per origin
as the run pumps — replacement is rank-gated (diagnostic, then draw lines, then bare), newest
at the best rank, the panic payload travelling with its capture (decision 37) — then prints
each reported failure as one block (best capture, diagnostic, caveat, reproducer line) and
re-raises the failing test's own panic. A dry final replay prints the freshest stamped
failing execution, usually confirmation-time pre-shrink values, while the blob carries the
shrunk incumbent. Stamping gauntlet accepts would break decision 10's cost profile.

### Reproduce blobs

`hegel_run_start_blob` replays a blob as a run: a deterministic blob replays its choices once;
an ND blob runs the replay primitive over its stored pool with no fresh tier (decision 33).
`Hegel::reproduce_failure` drives it through the same frontend loop. `hegel_test_case_from_blob`
remains for embedders as a documented single attempt.

### Concurrency unification (experiment 007)

Concurrent-machine runs flow through the pipeline above like any other ND run: creation always
succeeds, the flip happens at the first executed case that declares concurrency, and
concurrent failures are confirmed, shrunk, persisted, and blob-reproducible. The prior
regime's case stamping, sacrificed first case, shrink/persistence/span-mutation gates, and
blobless static-caveat reporting are gone. Measured at ceiling: 20/20 discovery and DB reuse,
60/60 blob replays on a genuinely racy machine.

### Accounting

`measure()` executions — confirmation batches, gauntlet runs, boost measurements,
replay-until-failure — are excluded from `valid_test_cases`, the invalid budget, health-check
counters, event statistics, targeting records, and the bug-window markers, which all describe
generation. Without the split every quantitative runner behavior silently changes meaning.

### ABI summary

Added: `hegel_settings_set_nondeterminism_strictness`, `hegel_failure_caveat`,
`hegel_run_start_blob`; blob prefixes 2/3. Changed: run status 3 retired; the stamp contract
(`hegel_test_case_is_nondeterministic`) now covers every replay of a discovered failure;
concurrent machine creation no longer rejects. Both crates' changelogs carry the break.

## Closed decisions

| Question | Outcome |
| --- | --- |
| Per-position divergence anchors; anchoring inside clone streams | Closed, none anywhere (decisions 14/31): fall-off positions are unpredictable (004), and positional splicing of stored timelines rescues the pool's residue (006B, 007 at ceiling) |
| Merged trie encoding | Rejected (decision 5, hardened by 004: prefix sharing anticorrelates with pool need) |
| Checkpoint/rollback in the shrink loop | Dropped (decision 17) |
| `replay_aligned` under ND | Essentially never holds; re-shrinking accepted (005B measured the price) |
| FAILED vs FAILED_NONDETERMINISTIC | FAILED + caveat accessor (decision 27) |
| Boost default | Reliability-floor heuristic, no setting (decision 28) |
| Clone-kind serialization fidelity | Values-only kept (decision 32) |

## Known risks (accepted)

- **Invisible divergence**: kind-compatible structural divergence can evade detection in
  principle; whole-timeline machinery is the backstop.
- **Origin instability**: panics from unjoined threads collapse to `Panic at <unknown>`;
  the fix is deferred to structured concurrency support.
- **Shrink wall clock**: multi-run accounting makes `MAX_SHRINKING_SECONDS` the binding
  constraint for slow concurrent bodies; a budget setting is possible later.
- **Caveat fatigue**: hence evidence-weighted wording and unconfirmed-only-when-nothing-
  confirmed reporting.
- **Anti-conservative statistics**: per-run peeking, stop-on-fail, and asymmetric miss
  weighting all bias the Wilson intervals toward acceptance relative to nominal coverage.
  The exact-DP operating points are the specification and z is a tuning constant.
  Experiment 008 measures the realized error.
- **Shrink opacity below `Debug`**: a stalled shrink and a finished one print identically
  except at `Debug` verbosity.
- **Quiet-flip invisibility**: under quiet strictness nothing below `Debug` reveals that a
  run flipped into ND handling or what the measurement runs cost; G17's statistics line
  (phase 10) closes this.
- **Bindings**: the ABI break needs a coordinated rollout; both RELEASE.md files call it out.
