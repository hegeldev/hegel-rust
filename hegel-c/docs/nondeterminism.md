# Handling nondeterministic tests

How the engine handles a test whose behaviour is not a function of the
generated data: the problem, the model, and each mechanism with the
reasoning behind it. Module docs in `hegel-c/src/native/` carry the
per-function detail; this document is the map. File names below are
relative to `hegel-c/src/native/`.

## The problem

Property-based testing rests on one invariant: given the same choice
sequence, the test does the same thing. Everything downstream assumes it —
the shrinker judges a candidate by one execution, the database replays a
stored example once and deletes it on a miss, the final replay reports
the example it just re-ran, and a stored reproduce blob is expected to
reproduce on the first attempt.

Tests break the invariant in two distinct ways:

- **Outcome nondeterminism**: the same realized choice sequence passes
  once and fails once (a race, a timing window, an outside service).
  This is a statistics problem — whether an example "fails" is a
  probability, and every yes/no decision above becomes a decision about
  an estimated probability.
- **Generation nondeterminism**: replaying the same prefix of choices, the
  test draws a different structure (a hidden coin decides which branch
  runs, and so which generators are called). This is a representation
  problem — one choice sequence no longer describes the failing example.

Before this work either kind aborted the run as flaky, and concurrent
state-machine tests were handled by a separate degraded regime with no
shrinking, persistence, or blobs. The engine now handles both kinds for
tests that fail at least around 10% of the time when replayed; that
target sizes every budget below.

## Detection and the run's mode

A run starts deterministic and may **flip** once into nondeterministic
handling (`Engine.nd_active`, never cleared within a run). Nothing
declares nondeterminism up front — a concurrent machine that fails
reproducibly is treated as a deterministic failure — the engine watches
for it:

- The **execution cache** (`exec_cache.rs`) keys every executed
  conclusion on its serialized realized values. The same values
  concluding with a different status or origin is the flip's main
  channel; it also lets exact repeats of a conclusion be served without
  re-running the body while the run is deterministic.
- Every generation-discovered failure gets a **first-interesting check**
  before anything consumes it: `FIRST_CHECK_REPLAYS` (4) exact replays,
  stopping at the first miss. A miss flips the run and the check's
  counts seed the origin's confirmation. This is why a race that used to
  surface at shrink time now surfaces at discovery.
- The pre-shrink verify and the final replay flip the run instead of
  aborting when a stored example stops reproducing within the run.
- A version-3 database entry or a nondeterministic reproduce blob —
  state only a nondeterministic run writes — flips the run before it is
  replayed.

A stored database entry that no longer matches the test is staleness,
never nondeterminism evidence: the engine only compares executions
within one run.

The `nondeterminism_strictness` setting decides the reaction: `quiet`
(default) flips silently, `warn` prints one notice, `error` aborts the
run with the old flaky-test diagnostics, for suites that use determinism
as a lint. Under `error` the engine also keeps a **kind ledger** (map
from value-prefix hash to the choice kind drawn next) that catches
structural drift between executions with a position-naming diagnostic.

Once flipped: the execution cache stops recording and serving (serving
the first recorded verdict is exactly the bias the multi-run machinery
exists to avoid), the consecutive-duplicate stop is off, and targeting
switches to a measured race (below). Every replay executes the body.

## The counterexample

One `Counterexample` (`counterexample.rs`) per failure origin (a panic
site) holds everything the run knows about that failure:

- The **incumbent**: the best failing execution, as nodes with the spans
  it was realized under. The reported example.
- The **graph** (`graph.rs`): the origin's failing executions merged into
  one structure and replayed as one test case (below). Present once the
  origin is confirmed or trusted; an unconfirmed origin replays as its
  incumbent's run alone.
- The **standing**: `Unconfirmed` → `Confirmed { anchor, witness }` by a
  passed discovery bar; `Unconfirmed` → `Trusted` by reproducing a
  database entry or blob (a previous run validated it); `Trusted` →
  `Confirmed` by a failing evidence batch. Rejection never demotes. The
  **anchor** is a Wilson lower confidence bound on the failure rate,
  monotone, raised only by validated accepts.
- The pre-flip **history** a late detection backtracks over.
- Per-run **budgets** for the statistical tests, so a fluke cannot retry
  them without bound.

Every failure under deterministic handling is a `Counterexample` too,
standing `Unconfirmed` with no graph; the standing only matters once the
run flips.

## Statistics (`nd/mod.rs`)

Pure arithmetic over `Evidence = (fails, runs)` with Wilson bounds. Every
replay is one Bernoulli trial of the *test case* under the standing
replay procedure, whatever structure it realized: a replay that
diverged from the stored example and passed is a miss in full, because
not seeing the failure is exactly what non-reproduction means. Each rule
is tested against the exact-DP operating points its constants were
chosen for.

- **Discovery bar** (`discovery_bar`): reject on zero failures in the
  first 10 replays, otherwise continue to 40, accepting early on the 4th
  failure. Operating points: 0.6% false accept per p = 0.02 fluke, 45%
  power per discovery at p = 0.1, ~15 replays per rejected fluke, ~4.4
  per p = 0.9 confirmation. An origin may spend `BAR_ATTEMPTS_PER_RUN`
  (5) batches per run; recycling a re-sighted fluke into fresh batches
  otherwise compounds the false-accept rate without bound.
- **Anchor seeding**: every batch that seeds an anchor runs to
  `ANCHOR_SEED_RUNS` (20) past its accept, because stopping at the
  accept biases the estimate toward the stopping rule (four straight
  fails seed 0.51 whatever the true rate). `anchor_ceiling()` =
  LCB(20/20) ≈ 0.84 caps every anchor at the resolution it was seeded at,
  so accepts cannot ratchet it beyond what any ledger can match.
- **Gauntlet** (`gauntlet`): a shrink candidate is accepted when its
  ledger holds at least `GAUNTLET_MIN_FAILS` (4) failures and its lower
  bound clears `max(gamma · anchor, 0.05)`, rejected when its upper bound
  proves that unreachable or at 30 runs, and otherwise re-run. Gamma is
  0.8 — a shrink step may trade some reliability away — except at
  anchors of 0.8 and above, where only zero-miss evidence reaches, and
  gamma is 1.0: an incumbent indistinguishable from deterministic is not
  traded down. The failure minimum exists because one failure on a fresh
  ledger bounds the rate above 0.2, so without it every low-anchor
  threshold would accept on the recruiting run.
- **Alpha budget** (`GauntletSpend`): each proposal is charged its exact
  false-accept probability against a p = 0.02 fluke before it runs,
  against a per-origin per-run budget of 0.02; when a proposal is
  unaffordable the failure minimum escalates (to at most 8), pinned per
  candidate at its first charge so no stopping rule changes mid-test.
- **Replay budgets**: `replay_budget(rate, tolerance)` replays before
  concluding a stored example no longer fails, so a bug failing at the
  target rate slips through with probability at most the tolerance (5%
  → 29 replays; callers stop at the first failure, so a live bug costs
  ~1/p). `continuation_budget(len) = len + max(4, len / 8)` fresh draws
  past a stored run, for replays whose structure runs longer.

## The graph

A nondeterministic failure is not one choice sequence: after a hidden
coin the test may draw different generators, so its failing executions
are many sequences with shared parts. The graph (`graph.rs`) stores them
together.

Each draw has an **address**: the spans open at it, outermost first,
each as `(label, ordinal)` — the ordinal counting earlier same-label
siblings under the same parent — ending in a frame counting the earlier
draws made directly in the innermost open span. That last frame is what
makes two draws never share an address: the engine wraps every
user-visible draw in a kind span of its own, but its bookkeeping draws
(a collection's continue/stop booleans, a state machine's round and
rule choices) are bare in the enclosing span, and without it a
collection's `more` draws all merged into one graph state.

The graph's **nodes are states** — where a run is between two draws —
and its **edges are draws**: address, value, and the next state. A
state's identity is the prefix of the next draw's address through its
first frame not open at the previous draw: the first span the run
enters after the last one it left. Inserting a run merges it by
identity, so two runs that reach the same state share everything after
it. Where the same address and value lead to different states — the
hidden coin's two arms — the node holds a **tie**, one edge per target.

**Replaying the graph** (`core/replay.rs`, `GraphWalk`): at each draw
the walk computes the identity the draw reports, arrives at that node
(settling the edge that led there, which is how ties are resolved: by
what the test does next), and serves the first edge at the draw's
address whose kind fits. No such node or edge is a **divergence** —
recorded once, as stream and position — after which the walk is rescued
by the next identity it recognizes and draws at random where it
recognizes none, under the continuation budget of the longest stored
run. Clone streams replay their stored records as a live set (the
records that agree with everything drawn so far).

## Admission and confirmation

Confirmation gates admission. After each generation step
(`nd_discovery_sweep`), every unconfirmed interesting origin faces the
discovery bar: a batch of replays of its graph (`nd_evidence_batch`),
each failing replay grafted into the graph unless the walk would have
served a different value somewhere (a **foreign** run, which no replay
of the graph produces). A rejected origin is **evicted** — the incumbent
dropped but the record kept, so a re-sighting resumes against the same
evidence and budgets — and a run with only evicted origins fails with a
caveat quoting the evidence. Under nondeterministic handling a raw
interesting execution never displaces an occupied origin; only
validated results move the incumbent.

A never-confirmed origin whose shrink verify or final replay misses
**backtracks** over its history — every pre-flip failing execution,
raw sightings and shrink accepts alike — for the reproduction boundary:
single probes at geometric offsets from the newest accept, the oldest
accept, and each raw sighting, then binary refinement, then the full
discovery bar on the candidate. This exists because a run that flips
late has already shrunk the incumbent under single-run judgments that
were never sound, walking it down to an example that may fail rarely or
never. History is bounded at `HISTORY_BYTES` (32 MiB) of nodes and spans
per origin: over the bound raw sightings go oldest first, then accepts
from the old end of the segment, keeping the oldest accept and the dense
run of newest ones the geometric probes land on.

## Shrinking

Shrinking under nondeterministic handling is a `GraphShrinker`
(`graph_shrink.rs`) over the origin's graph and its **witness**, the
smallest clean failing run replayed from the graph — the example the
failure is reported with. Its passes propose smaller graphs:

- **Delete**: the graph without each edge in turn (a node left without
  edges is where a run ends).
- **Span**: the witness without each of its spans, later same-label
  siblings renumbered, retried with the nearest earlier integer draws
  lowered by one so a list's count shrinks with its elements.
- **Value**: each edge's value replaced by a simpler one under the
  constraint of the draw that realized it, learned from every replay.

Every candidate is judged by replays through the gauntlet. A **clean**
replay fails with the origin, never diverged and ended on `End`. In the
fast sweep one unclean replay rejects; stopping is **confirmed-dry** —
after a sweep that accepts nothing, one confirmation sweep drives each
candidate's cumulative evidence to a bound decision, since on a p = 0.5
landscape fixed dry-sweep rules miss reachable reductions 18–46% of the
time and confirmed-dry 10%. A value edit must be **exercised**: accepted
only once a clean replay settled on the edited edge, because an edit no
failing run drew is an unfalsifiable claim; while its replays fail with
runs the candidate cannot produce they are grafted in, up to
`WARM_UP_GRAFTS` (8). An accept's ledger is topped up to
`ANCHOR_SEED_RUNS` before its bound moves the anchor, and the accept
installs graph, witness, anchor and longest run on the counterexample
and persists at once. Candidates are ordered by `GraphKey` — fewer
edges, fewer reachable nodes, then edge values in shrink order — and
must be strictly smaller.

A flip *during* a deterministic shrink stops that shrink at once (its
single-run judgments would be discarded anyway) and requeues the origin
from its verified pre-shrink nodes for one gauntleted pass, on the
remaining shrink deadline. A flip at the shrink verify routes the origin
through backtracking or the bar. `MAX_SHRINKING_SECONDS` is the binding
constraint for slow bodies, since every judgment is several executions.

**Boost** (`nd_boost`): when a confirmed incumbent's anchor is below
`BOOST_RELIABILITY_FLOOR` (0.30, the image of "true rate below 0.5" in
20-run LCB units), successive halving over the incumbent and
prefix-mutants of it finds a steadier timeline, re-measured on a fresh
20-run holdout before it seeds the anchor. Above the floor it never runs.

## Targeting

The deterministic climber trusts single-run scores; under a
nondeterministic score its recorded maximum is the max of noisy draws
and a strict-improvement accept ratchets on flukes and freezes. Under
`nd_active` targeting runs as a measured race (`optimise_targets_nd`):
per label a reference timeline with a reference score estimated only on
fresh unselected batches (median of `TARGET_ND_HOLDOUT` = 20 replays),
up to `TARGET_ND_RACES` (4) successive-halving races per firing over
perturbations of the reference, the winner adopted only when a fresh
holdout clears a sign test (Wilson LCB of strictly-beats above 0.5).

## Persistence and blobs

`NdReproState` (`blob.rs`) — the graph, an entropy seed, and the longest
run's flattened length — is both the version-3 database entry and the
payload behind blob prefixes 2/3. Whether an entry is nondeterministic is
carried by its representation: a v3 entry opens with a choice count no
choice sequence can have, so an older reader rejects it as corrupt
rather than misreading it. Only confirmed or trusted origins persist.
Confirmation and every gauntlet accept save the validated incumbent
mid-run, each save landing before the bytes it supersedes are deleted so
an interrupted run keeps its most recent validated example. No rates or
counters are persisted; every run measures afresh. Hygiene is two
strikes across two runs (primary miss demotes to the secondary corpus,
secondary miss deletes), with the secondary corpus capped at 50 per key.

Replaying stored state (`nd_reproduce`) is one primitive for database
reuse, blob replay, and the final replay: the graph up to
`reuse_replay_budget()` (29) times, stopping at the first failure, then
where the caller allows it a few fresh generations. A version-1
(deterministic) blob replays with the continuation budget and
`V1_BLOB_REPLAYS` (4) attempts, so a blob recorded before a test went
nondeterministic still reproduces. `hegel_run_start_blob` runs a blob as
a run; `hegel_test_case_from_blob` remains a documented single attempt.

## Reporting

The engine owns the **final replay**: every failure it is about to report
re-executes first — once under deterministic handling (a miss flips the
run, or aborts under `error`), through the replay primitive plus
`FINAL_REPLAY_FRESH` (4) fresh generations under nondeterministic
handling. Failures report as plain `FAILED`; each carries a **caveat**
(`hegel_failure_caveat`) quoting the run's own evidence — "confirmed:
failed 7 of 20 replays at confirmation and 1 of 2 at report time" — so
the wording is as strong as what was measured. An unconfirmed origin
still fails the run, caveat-only and without a blob, and only when
nothing confirmed or trusted was found, so a leaked fluke never displaces
a real failure. Under `show_statistics` one line reports the measurement
replays and how many failed — the only sub-Debug surface that reveals a
quiet flip and its cost.

Executions whose failure can become the report are **stamped**
(`hegel_test_case_should_capture`): confirmation batches, database-reuse
and blob replays, first-check replays, the final replay, and generation
cases once the run has flipped. The client captures a stamped case's
output, diagnostic, and backtrace per origin and prints each reported
failure from its freshest best capture — a dry final replay prints the
confirmation-time capture while the blob carries the shrunk incumbent.
Shrink, gauntlet, and boost probes stay unstamped and cheap.

## Accounting

Executions made to measure — confirmation batches, gauntlet runs, boost
and targeting races, replay-until-failure, the final replay — go through
`measure()` and are excluded from `valid_test_cases`, the invalid
budget, health checks, event statistics, targeting observations, and the
bug-window markers, all of which describe generation. Without the split
every quantitative runner behaviour silently changes meaning.

## Known limits

- **Same-address arms**: two arms of a hidden coin whose draws have the
  same address — the same spans open and the same kinds, from a bare
  `if` in the test body over the engine's own draws — are one state to
  the graph, which serves them one value; the other arm's runs are
  foreign and never stored. Generators' spans keep arms apart.
- **Rare structures at the bar**: the bar's 0-of-10 gate judges a
  single run before the batch has learned anything, so a failure with
  many structures (each rare on replay) is gated out about half the
  time; late first sightings leave no re-sighting to spend another
  attempt on.
- **Deletion evidence on large graphs**: 20 clean replays of a graph
  with hundreds of shapes never walk its rare edges, so a rarely walked
  edge can be deleted, grafted back by the replay that walks it, and
  deleted again.
- **Cost outside the deadline**: only the graph shrinker and the final
  review are bounded by the shrink deadline; confirmation batches,
  backtracks, boost, and reuse replays are bounded by their own budgets
  (a few hundred executions per origin in the worst case), which for a
  slow body is minutes.
- **Anti-conservative intervals**: per-run peeking, stop-on-fail, and
  the failure minimum bias the Wilson intervals toward acceptance
  relative to nominal coverage. The exact-DP operating points are the
  specification and z = 1.96 is a tuning constant; composition across
  repeated tests is bounded by the per-origin budgets.
- **The late-flip seam**: a run that flips only on late detection has
  already spent its deterministic window; backtracking recovers the
  reproduction boundary but a post-flip run can still report a confirmed
  flaky example where free displacement would have found a deterministic
  one it never held.
- **Origin instability across threads**: a panic ferried from a worker
  thread and re-raised on the test thread keeps its origin only through
  the frontend's panic-info capture; a never-joined thread's panic
  produces no failure at all.
