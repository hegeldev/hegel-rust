# Overview

## The problem

Hegel's engine is replay-driven. A test case is treated as a function from a
choice sequence to a verdict, and everything downstream of generation leans on
that: shrinking proposes smaller sequences and re-executes them, the failure
database stores the choices of the best failing example, a reproduce blob is a
serialized choice sequence, and the flakiness errors fire when a replay
disagrees with what was recorded. Hegel inherits Hypothesis's central
invariant — no nondeterminism outside the system's control.

Concurrent stateful testing breaks the invariant intrinsically: the thread
schedule is not in the choice sequence, so the same choices can pass or fail,
as they can in any body with hidden state, external randomness, or timing
dependence. Pre-branch, the handling was wholesale surrender: concurrency set a
sticky flag that disabled the data tree, novel-prefix generation, span
mutation, targeting, shrinking, persistence, and reproduce blobs, and reported
at most one failure per run from a capture-at-discovery stash. Every other
nondeterminism source aborted the run as `Flaky`/`NonDeterministic` with no
failure report ([design history](../part2/design-history.md)).

The branch replaces surrender with handling: a test whose behaviour is not a
function of its choices still gets found, confirmed, shrunk, persisted,
reported, and reproduced, with the uncertainty stated rather than hidden.

## Goals and non-goals

The stated goals, all met (notes/design.md):

- Handle tests that fail at least 10% of the times they are run. The replay
  budgets and confidence arithmetic derive from that target (decision 16,
  `TARGET_FAILURE_RATE = 0.1` in `hegel-c/src/native/nd/mod.rs`).
- Detect nondeterminism as well as accept declarations (concurrent machines).
- Restore shrinking, multi-failure reporting, database persistence, and
  reproduce blobs for nondeterministic tests.
- Bound how much failure probability shrinking can trade away, and raise it
  when possible (decision 2). The guard is statistical, not a strict never-lower:
  experiment 008's envelope holds a median final failure probability of 0.82
  (p10 0.58) on a rising landscape against a 0.10 floor, and target-regime
  (p = 0.1) bugs survive shrinking at 100% against 67% shipped.
- Keep a caveat for the environment-modification hypothesis, worded to admit
  it is indistinguishable from a very rare failure (decision 3).

Non-goals: Antithesis, a separate deterministic-environment path (decision
13), and thread-schedule control — schedules are sampled, never replayed.

## Two kinds of nondeterminism

Two axes need different machinery (notes/design.md):

**Generation nondeterminism**: the same replayed prefix produces a different
draw structure. This is a representation problem. The answer is to stop
pretending one choice sequence describes the test and store whole realized
**timelines** (the realized choice sequence of one execution) in a bounded
per-origin pool, replayed first-fit with a continuation budget for fresh draws
past the stored timeline, and spliced pairwise when whole-timeline replay misses.

**Outcome nondeterminism**: the same realized choice sequence produces a
different verdict. This is a statistics problem. The answer is to treat every
verdict as a sample: replays accumulate `Evidence` (failures, physical runs,
weighted misses), and decisions are made on Wilson confidence bounds over the
estimated failure probability, with explicit budgets derived from the p >= 0.1
target.

## Probabilistic bugs

Failure probability is first-class. Every decision that once assumed
"interesting is a pure function of the choice sequence" — shrinker acceptance,
database reuse, the final replay, the flakiness errors — becomes a decision
about an estimated probability with an explicit budget. A single failing run
is selection, not evidence: on a noisy test the first interesting sighting is
a background fluke more often than a real bug (decision 21), so nothing is
believed until it reproduces. Conversely a single passing replay proves
nothing against a p = 0.5 bug, so nothing is disbelieved on one miss either.

Five terms recur. An **origin** is failure identity: the panic site as a
`file:line:col` string (decision 4). The **incumbent** is the failing timeline
held as an origin's best example. The **pool** is a bounded per-origin set of
other failing timelines, incumbent first. The **anchor** is a Wilson lower
confidence bound on the incumbent's reproduction rate under the engine's own
pinned-replay procedure, monotone and raised only at validated events
(decisions 19, 46). The **gauntlet** is the evidence bar a shrink candidate
must clear before displacing the incumbent.

## The shape of the answer

A run is deterministic until proven otherwise. `Engine.nd_active` is a sticky
run-level flag, and the **flip** into ND handling comes from one of seven
sites: declared concurrency, an execution-cache verdict mismatch, a
first-interesting check miss, replay checks at the shrink verify and final
replay, and stored ND state from the database or a blob. The
`nondeterminism_strictness` setting (quiet, warn, error; default quiet)
governs what the flip says: quiet is silent, warn prints once, error keeps the
old aborts for suites using determinism as a lint (decisions 1, 30). The data
tree is gone, replaced by a flat execution cache that doubles as the
verdict-mismatch detector. Under ND handling caching, targeting, and the
duplicate stop are all off ([detection](detection.md)).

Confirmation gates origin admission on every path. An observed origin starts
**Unconfirmed** and must clear the **discovery bar** — a gate-then-extend
replay batch — before it is **Confirmed** and carries an anchor, a witness,
and a pool. An origin reproduced from the database is **Trusted** without
re-running the bar's verdict. Raw interesting runs never displace an occupied
origin. Pre-flip sightings are kept in a per-origin history so a late flip can
backtrack to the reproduction boundary. [The origin lifecycle](lifecycle.md)
covers admission, evidence, and caveat wording.

Shrinking charges accepts, not rejects (decision 7): a candidate whose first
run passes is cheaply rejected and retried later, while one that fails must
clear the gauntlet against the incumbent's anchor before displacing it.
Stopping is confirmed-dry — a final sweep drives every proposal's cumulative
evidence to a bound decision, so stopping carries a certificate. A low-anchor
incumbent can be boosted onto a steadier timeline before shrinking begins.
[Shrinking under nondeterminism](shrinking.md) covers the arithmetic.

The engine owns the final replay: every about-to-be-reported failure
re-executes first. Deterministic origins get one exact replay, and a miss
flips the run rather than silencing the report. ND origins get
**replay-until-failure** (`nd_reproduce`): stored timelines first-fit, then
splices, then a few fresh generations. A confirmed origin that stays dry at
report time switches its caveat wording instead of being unreported.
[The final replay](final-replay.md) covers the pooled review and backtrack.

Persistence stores the representation, never estimates (decision 8): a
version-2 database entry or blob is `NdReproState` (the pooled timelines plus
replay parameters), self-identifying, so the next run enters ND handling from
the stored state and every run stands alone. Database hygiene is two strikes:
a primary miss demotes, a secondary miss deletes.
[Persistence and reproduction](persistence.md) covers formats and reuse.

At the ABI, an ND failure reports as plain `FAILED` with a per-failure caveat
accessor. The retired `FAILED_NONDETERMINISTIC` status is reserved, never
reused (decisions 27, 43). The engine stamps the executions a report can be
built from via `hegel_test_case_should_capture`, and `hegel_run_start_blob`
replays a blob as a full run. [The C ABI and the frontend](abi-frontend.md)
covers the break, the capture contract, and how the frontend builds reports.

The code sits in six places: decision arithmetic in
`hegel-c/src/native/nd/mod.rs` (pure, no engine state), the origin state
machine in `nd/lifecycle.rs`, run orchestration with history and backtrack in
`test_runner.rs`, persistence formats in `blob.rs`, the tree replacements in
`exec_cache.rs`, and frontend reporting in `src/run_lifecycle.rs`.

## One failure, end to end

A test body races and fails perhaps a third of the time. Generation finds an
interesting case: a panic at one site, the origin, and the raw sighting fills
the vacant origin as its incumbent. The run is still deterministic, and this
one sighting is selection, not evidence.

Before anything consumes it, the first-interesting check replays the sighting
exactly, stopping at the first miss, and a replay passes. The run flips —
`nd_active` sets, the execution cache flushes, and under the default quiet
strictness nothing is printed — and the check's evidence seeds the origin's
discovery bar ([detection](detection.md)).

The discovery sweep now runs the bar over the unconfirmed origin: pinned
replays accumulate evidence until the accepting failure arrives within budget.
The origin confirms, taking the batch's first reproducing run as witness, an
anchor seeded from the extended batch, and a pool of failing timelines
harvested at confirmation ([the origin lifecycle](lifecycle.md)).

Shrinking starts from the witness. Candidates that fail once pay the gauntlet
before displacing the incumbent, the anchor rises only at adopted accepts, and
the run stops confirmed-dry. Along the way each validated accept persists the
new incumbent, save-then-delete, so an interrupt loses nothing
([shrinking](shrinking.md), [persistence](persistence.md)).

The final replay re-executes the failure through `nd_reproduce`: the shrunk
incumbent reproduces on the second pooled replay, and the report evidence is
recorded ([the final replay](final-replay.md)).

The run fails with one reported failure: the captured diagnostic, a `note:`
whose caveat quotes the run's own counts — failed so many of so many replays
this run — and a reproducer line carrying a version-2 blob. The next run
decodes the version-2 database entry under the primary key, flips before any
replay, replays the stored pool until a failure, and trusts the origin on
reproduction ([the ABI and frontend](abi-frontend.md),
[persistence](persistence.md)).
