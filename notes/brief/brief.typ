#set page(paper: "a4", margin: (x: 2.4cm, y: 2.6cm), numbering: "1 of 1")
#set text(size: 10.5pt)
#set par(justify: true, leading: 0.62em)
#set heading(numbering: "1.1")
#show heading: set block(above: 1.4em, below: 0.7em)
#show raw: set text(size: 9.5pt)
#set table(stroke: 0.4pt)
#show table: set text(size: 9pt)

#align(center)[
  #text(size: 16pt)[The nondeterminism branch: a review brief]

  #text(size: 9.5pt)[
    Compiled by Claude from the branch's book (`notes/book/`), notes, and code, 2026-09-07,
    covering the branch through decision 72. \
    Written for a reader who knows pre-branch Hegel and none of this work. Ordered for
    unpacking: the problems, then the design in brief, then the detailed considerations. \
    Primary sources remain authoritative: `notes/design.md`, `notes/decisions.md`,
    `notes/experiments/`.
  ]
]

= The problems <problems>

Hegel's engine treats a test as a function from a choice sequence to a verdict, and
everything downstream of generation leans on that identity. Shrinking proposes a smaller
sequence, re-executes it, and keeps it when the test still fails. The failure database
stores the best failing example's choices and replays them next run. A reproduce blob is a
serialized choice sequence. The flakiness errors fire when a replay disagrees with what was
recorded. Each of these is sound because replaying the same choices re-observes the same
fact.

Nondeterminism breaks the identity on two independent axes, and the split runs through the
whole design. *Generation nondeterminism*: the same replayed prefix produces a different
draw structure. Concurrent stateful testing is the canonical source, because the thread
schedule is not in the choice sequence, but hidden state and external randomness do it too.
The stored sequence no longer describes the test, so this is a representation problem.
*Outcome nondeterminism*: the same realized choice sequence produces a different verdict. A
run stops being a verdict and becomes a sample, so this is a statistics problem.

The pre-branch engine surrendered on both. Concurrency set a sticky flag that disabled the
data tree, novel-prefix generation, span mutation, targeting, shrinking, persistence, and
reproduce blobs, then reported at most one failure per run from a capture-at-discovery
stash. Every other nondeterminism source aborted the run as `Flaky` or `NonDeterministic`
with no failure report at all. Reports vanished exactly where they matter most: a racy bug
is the kind a user is least able to reproduce by staring at their own code.

Handling instead of surrender turns out to require solving five distinct problems.

*Selection masquerading as evidence.* The run that discovers a failure was noticed because
it failed, so treating it as an observation of the failure rate conditions on the outcome.
Over a long generation phase, low-probability flukes get many chances to fire once, and
first sightings over-represent exactly the failures least likely to reproduce. Experiment
003 measured it in the real shrinker: with a background fluke rate present, the first
interesting execution was a fluke twice as often as a real bug, and letting raw runs
displace incumbents let those flukes destroy a 20-of-20-confirmed discovery in about 80% of
trials. Nothing observed once can be believed.
Nothing that fails half the time can be disbelieved on one passing replay either.

*Shrinking is an optimiser pointed at noise.* The deterministic shrinker accepts any
candidate that fails once. Under outcome nondeterminism that rule trades the reported
example's failure probability away step by step — a p = 0.5 failure shrinks into a
p = 0.001 husk that happened to fail at the right moments — or a lucky fluke teleports the
incumbent off the bug entirely. The branch's contract (decision 2) is that shrinking must
not lower the reported example's failure probability, and should raise it when cheap.
Enforcing that needs a statistical accept rule priced against an estimate of the
incumbent's own rate, and the pricing must survive its degenerate corners: the first
shipped rule collapsed whenever the incumbent's estimated rate sat at or below 0.258,
because the acceptance threshold then fell below the Wilson lower bound of a single failing
run (0.2065), every candidate was accepted on the run that recruited it, and target-regime
bugs were lost a third of the time (experiment 008).

*Replay infrastructure rots.* Deterministic hygiene deletes a database entry after one
failed replay, which for a bug failing a third of the time is a two-to-one coin toss
against a live entry, and a fixed allowance has the same flaw: ten replays miss a p = 0.1
bug 35% of the time. Exact-choice blobs are worse, because a nondeterministic body only has
to elongate slightly for an exact replay to overrun. Exact blobs of never-flipped
concurrent failures reproduced 13% of the time while the same bytes replayed with alignment
tolerance reproduced 99% (experiment 009a): the fragility was alignment, not example
quality, so the fix has to live in the representation.

*Estimates contaminated by their own selection.* Every estimate the engine keeps is
produced by a process that selects on it, and the winner's curse appears wherever that is
ignored. A confirmation batch that stops on its accepting failure estimates the stopping
rule, not the rate: four straight failures seed 0.51 whatever the truth. A recorded
per-label targeting maximum is the max of noisy draws, about 1.7 standard deviations above
truth on normal noise, and a climber ratcheting against it freezes within about ten runs in
92--100% of trials (experiment 013). The winner of any race scored on its own in-race performance is
inflated the same way. The recurring fix is to move every accounting event onto a fresh, unselected
measurement — a validated event — and the branch applies it at five separate seams.

*Bounded tests, repeated without bound.* Once everything is a hypothesis test, one run
performs many of them, and per-test error rates compose. The admission test the design
arrives at (the discovery bar, @lifecycle) has an honest 0.6% false-accept rate per batch,
but re-running it each time generation re-finds a rejected fluke confirms a q = 0.02 fluke
21% of the time over a long run. The shrink-time accept rule (the gauntlet, @shrinking) is
honest at 4.0e-4 per proposal, but one shrink can realize tens of thousands of candidate
timelines (experiment 012 saw 42,000), and a thousand proposals at the rule's floor compose
to 33%. The final replay's original any-failure rule confirmed a lingering fluke about half
the time, because 30-odd replays find one failure in a q = 0.02 body about half the time.
None of this fits a batch correction in the Benjamini-Hochberg style, because every verdict
acts immediately and irreversibly — there is never a set of p-values to rank. The control
has to be sequential and online (experiment 014, decision 72).

= The design in brief <shape>

Ten commitments carry the design. Everything in @details unpacks one of them.

*A run is deterministic until observed otherwise.* `Engine.nd_active` is a sticky run-level
flag with six flip sources, all evidence and none declaration: a verdict mismatch in the
execution cache, a miss at any of the three replay checks (the first-interesting check,
the shrink verify, the final replay), and stored nondeterministic state in either form, a
database entry or a blob, both of which flip the run before any replay. Creating a
concurrent state machine declares nothing, because a properly serialized concurrent machine
can fail deterministically (decision 70). A `nondeterminism_strictness` setting says what a
flip does: `quiet` (the default) handles silently, `warn` prints one notice, and `error`
aborts with the pre-branch diagnostics for suites that use determinism as a lint — under
`error` every detection aborts, threads or not, and stored state (a prior run's fact, not
a detection) instead replays with handling off.

*Timelines answer the representation problem.* The engine stores whole realized *timelines*
(the realized choice sequence of one execution) in a bounded per-origin pool of 10,
incumbent first. Reproduction is replay-until-failure: each stored timeline first-fit with
a small budget of fresh continuation draws past its end, then positional splices of random
timeline pairs, then, at the final replay only, a few fresh generations. One primitive
serves database reuse, the final replay, and blob replay.

*Evidence answers the statistics problem.* Every measurement replay is one Bernoulli trial
of the test case, counted as plain (fails, runs) whatever timeline it realizes
(decision 71). Decisions are Wilson confidence bounds at z = 1.96, and every budget derives
from one target: handle bugs that fail at least 10% of the time they run. The generic
budget at that target with 5% miss tolerance is 29 replays, and callers stop at the first
failure, so a live bug costs about 1/p.

*Failures have identity and a lifecycle.* A failure is its origin, the panic site as a
`file:line:col` string. Once handling is on, a raw interesting run may fill a vacant
origin but never displaces an occupied one, and it contributes no evidence: a single
failing run is selection. An
origin starts Unconfirmed, is Confirmed by the discovery bar (a gate-then-extend replay
batch over its incumbent), or is Trusted when a stored database entry reproduces it.
Nothing consumes an origin — shrinking, persistence, blob emission — until it is confirmed
or trusted.

*Shrinking charges accepts, not rejects.* A candidate whose first run passes is rejected
for the price of that one replay, with the outcome remembered so retries accumulate
evidence. A candidate whose first run fails is the dangerous one: it faces the gauntlet, a
cumulative-evidence bar priced against the *anchor*, a monotone Wilson lower bound on the
incumbent's own reproduction rate. Stopping is confirmed-dry: after a sweep with no adopted
accept, a confirmation sweep drives every proposal's evidence to a bound verdict, so
stopping carries a certificate rather than a count.

*Estimates move only at validated events.* Anchors rise only at bar accepts, adopted
gauntlet accepts, and boost holdout passes, and confirmation batches extend past their
accepting failure to 20 runs so they estimate the rate rather than the stopping rule.
Racing mechanisms (boost, targeting) re-measure their winner on a fresh holdout before
anything moves. This is the same fix applied at every seam the winner's curse touches.

*Reliability is raised when it is cheap to.* A confirmed incumbent with a low anchor is
*boosted* before shrinking: a successive-halving race over the incumbent, its pool, and
prefix-cut mutants, holdout-gated. Targeting under ND handling is the same design applied
to user scores, replacing the deterministic hill climber that freezes on noise.

*Repetition spends bounded budgets.* Per origin per run: five discovery-bar batches shared
by the sweep, shrink admission, and the report-time review (plus three for the backtrack),
and a 0.02 false-accept budget the gauntlet spends by charging every proposal its exact
false-accept mass, escalating the required failure count from 4 towards 8 when a new
candidate is unaffordable. The report-time review's reproducing run faces a standard
evidence batch instead of confirming on any failure (decision 72).

*The engine owns the final replay, and reports carry their evidence.* Every failure about
to be reported re-executes first. A nondeterministic failure reports as plain `FAILED` with
a caveat quoting the run's own replay counts, and an unreproduced failure still fails the
run — the caveat changes wording instead of the report disappearing. Persistence stores the
representation and never estimates: a version-2 database entry or blob is the pooled
timelines plus replay parameters, self-identifying, so the next run flips before replaying
it and every run stands alone.

*Measurement is fenced off the run's own accounting.* Replays made to measure reproduction
run through `measure()`, which moves none of the counters that describe generation (valid
counts, health checks, event statistics, targeting observations). One statistics line
reports the measurement replay count and its failures, the only surface below Debug
verbosity that reveals a quiet flip's cost.

What all of it costs: 1.6--2.1x total measurement replays at p ≤ 0.3
against the pre-composition engine (4--6x at p = 0.9), four extra replays per discovered
origin on a fully deterministic run, and the multiplicity budgets' power prices below
target (@residuals).

= Detailed considerations <details>

== The flip and the run under handling <flip>

The first question the design answers is who decides a test is nondeterministic. Nobody
does: the engine watches. Declaration was tried — the old ABI carried an
`is_nondeterministic` stamp, later a concurrency declaration — and retired as a plaster
(decision 70): a concurrent machine whose failures reproduce exactly is deterministic in
every way the engine cares about, and a "deterministic" test with hidden state is not.

`nd_flip` is idempotent and sticky for the run. On the first call it clears the execution
cache and the kind ledger (nothing recorded pre-flip may be served or compared again),
resets the duplicate counter, and prints warn's one notice when that is the strictness.
`error` aborts at the detecting site instead of flipping, splitting by axis: generation
drift (a kind-ledger contradiction, a structural divergence at the first check) aborts as
`NonDeterministic`, and an outcome change (a cache verdict mismatch, an aligned replay
miss) aborts as `Flaky`. The pre-branch diagnostics are reproduced verbatim so
determinism-as-lint suites keep their output, except the first check's structural
diagnostic, which is new and richer: it names the divergence position and quotes the
choice that changed. The one strictness asymmetry is stored state: a v2 database entry under
`error` replays with `nd_active` still false, because a stored entry that stops reproducing
is staleness, never nondeterminism evidence — between-run divergence overwhelmingly means
the code changed.

While `nd_active` is set, the execution cache neither records nor serves, so every replay
executes the body — serving the first recorded verdict is exactly the bias the multi-run
machinery exists to remove. The duplicate stop and kind ledger are off, and a raw
interesting run fills only vacant origins. Targeting switches from the climber to the
measured race (@targeting), reporting and persistence switch to the v2 machinery, and span
mutation stays on unchanged.

Two accounting rules keep a flipped run's numbers meaningful. Executions made to measure
reproduction go through `measure()` and move none of the generation counters: valid-case
counts, the invalid budget, health checks, event statistics, recorded target observations,
bug-window markers. And when statistics are enabled, one line counts measurement replays
and their failures, which is the number to look at when a run feels expensive.

== Detection <detection>

With no declaration, detection carries the design, and the branch deleted its most
expensive detector. The data tree — 825 lines recording every execution — was priced by
experiment 010 on production main: recording alone cost 40--80% wall overhead on passing
bodies, novel-prefix generation and exhaustion bought nothing measurable outside tiny
spaces, and its serving win was almost entirely exact repeats. Two small structures replace
the live roles.

The *execution cache* keys every executed conclusion on its serialized realized values. A
digest tier (128-bit fingerprints) drives two signals: a repeat inside the generation
window advances the duplicate counter, ending generation after 10 consecutive duplicates
only while no valid case exists yet (the exhausted-space trigger, and nothing more — an
unconditional version ended a 32-way `one_of` early, because late in coupon collection a
duplicate streak is routine); and a repeat concluding with a different status or origin is
a verdict flip, the outcome-nondeterminism evidence the tree structurally could not see,
since it overwrote differently-concluding leaves. A full tier serves exact repeats outside
the generation window, byte-bounded, which recovers the tree's one measured win (85% of
shrink-time serves).

The *kind ledger* is `error` strictness's generation-drift detector: a map from rolling
value-prefix hash to the choice kind drawn next, compared within one run, reproducing the
tree's old diagnostic verbatim. Quiet and warn do not maintain it. Their generation-level
channel is the first-interesting check below, and the tree detector the ledger reproduces
fired zero times in 600 seam trials (experiment 011).

The *first-interesting check* exists because verdict-flip detection alone is sparse, and
the expensive failures were never detection failures but pre-flip actions taken irreversibly
on single observations. Before anything consumes a generation-discovered origin, its
incumbent sighting gets four exact replays, stopping at the first miss. A miss flips the
run, and the check's observations seed the origin's discovery bar so no replay is paid for
twice. Detection probability is 1 − (p·s)⁴ for a bug failing at rate p with structural
stability s. A deterministically failing origin pays exactly four extra replays, and a run
that finds no bug pays nothing, which is the whole measurement cost of a deterministic run.
Database-reuse reproductions are exempt (the reuse replay already checked them), as are
origins first admitted at the shrink verify or final replay.

The *per-origin history* covers the remaining window. While the run is still deterministic,
every interesting execution of an origin — raw sightings and shrink accepts alike — is
appended to an unbounded per-origin history, deduplicated, dropped on confirmation or run
end. Pre-flip displacement can walk an incumbent far down the landscape before any detector
fires, and the history keeps what displacement would otherwise destroy. Keep-everything is
deliberate: evicting old entries deletes the reproduction boundary exactly when shrinking
went nondeterministic early, and the memory a bound would defend against left with the
tree. The backtrack that consumes it is @finalreplay's subject.

== Timelines, replay, and evidence <evidence>

Most of the machinery is one of two tools applied at some seam.

The representation tool: an origin carries a bounded pool of failing timelines
(`POOL_CAP` = 10 in total, incumbent included; experiment 004 measured 5 as near-ceiling
and 10 as the plateau). A single replay of a stored timeline runs it with a continuation
budget of `len + max(4, len/8)` total draws, the excess drawn fresh past the stored end, so
a replay that runs slightly long draws fresh values instead of overrunning (a flat budget
of 4 absorbed all measured elongation). When the whole pool misses, positional splices of
random timeline pairs are the rescue tier: each picks an ordered pair and a random cut and
glues the left prefix to the right suffix. Splices rescue 65--100% of whole-pool misses
(experiment 006), and because they cut only at top-level positions, a clone stream — one
timeline element — always crosses over intact. A merged trie encoding was rejected early:
prefix sharing anticorrelates with pool need.

The statistics tool: every replay outcome lands in an `Evidence` ledger as one plain
Bernoulli trial — (fails, runs), a miss counting in full whatever the replay realized
(decision 71). The estimand is the test case, which can realize many timelines, not any one
realized timeline. Until 2026-09-07 misses were weighted by how far the replay tracked the
stored timeline before diverging, which patched a per-timeline estimand instead of fixing
it; plain counting is what the deriving experiments modelled all along (005A's DP is pure
Bernoulli, 008's headline envelope is its weight-1.0 column), so the shipped operating
points hold without recalibration. The honest cost is that divergence-heavy bodies now
measure at the rate a user replaying the stored state actually sees.

Decisions over evidence are Wilson bounds at z = 1.96. The intervals are not
textbook-honest — per-run peeking and stop-on-fail bias them towards acceptance — so the
exact-DP operating points are treated as the specification and z as a tuning constant, with
the realized error measured per test (experiment 008) and the composition across repeated
tests bounded by budgets (@multiplicity). All sizing flows from decision 16's target:
handle tests failing at least 10% of the time. `replay_budget(rate, tolerance)` gives the
replay count after which a bug failing at `rate` escapes with probability at most
`tolerance`; at the target and 5% that is 29, the database-reuse budget.

== The origin lifecycle <lifecycle>

A failure's identity is its origin, the panic site as a `file:line:col` string. Two panic
sites are two failures even when one timeline reaches both. Under ND handling an origin is
Unconfirmed, Confirmed, or Trusted, and confirmation gates admission on every path: nothing
consumes an origin until it is past the bar or trusted.

*The discovery bar* is the admission test, sized by an exact DP (experiment 005A): reject
on zero failures in the first 10 replays, otherwise extend to a cap of 40, accepting early
on the 4th failure and rejecting when the quota is unreachable. Operating points: 0.6%
false accepts per q = 0.02 fluke, 45% per-discovery power at the p = 0.1 target, roughly 15
replays per rejected fluke, about 4.4 per p = 0.9 confirmation. The asymmetry is
deliberate. A false accept is sticky — it occupies the origin behind the displacement
freeze for the rest of the run — while a false reject recycles through rediscovery while
generation is alive, so power is the cheap side. The rejected alternatives were a
Wilson-LCB-over-noise-floor rule (26% false accepts) and an SPRT (50+ replays buying power
rediscovery gives free). Bar batches spend a per-origin per-run budget (@multiplicity), and
two rules keep each batch honest: an accept must hold an in-batch reproducing witness (a
first-check seed can carry the whole failure quota, and a seeded quota with no reproduction
of its own rejects instead), and an accepting batch extends to 20 runs before its lower
bound may seed an anchor, because a batch stopped at its accepting failure estimates the
stopping rule (four straight failures would seed 0.51 whatever the truth).

*Confirmed* origins carry the batch's first reproducing run as witness (the shrinker's
starting point), an anchor, and a pool of failing timelines harvested from the batch's
captures. The anchor is a Wilson lower confidence bound on the incumbent's reproduction
rate under the engine's own replay procedure, the same one candidates are measured by, so
candidate and incumbent sit on one estimand. It is monotone and raised only at validated
events.

*Trusted* origins reproduced from stored state skip the bar's verdict, not the replays: the
prior run persisted only validated origins, and re-barring a real p ≈ 0.1 bug would drop it
about 55% of the time. At shrink entry a trusted origin runs an evidence batch with the bar
as stopping rule only. Any failure promotes it to Confirmed with the batch's lower bound as
anchor and the stored pool merged fresh-first; a zero-failure batch skips shrinking but the
origin is still reported and persisted.

A bar rejection evicts an unconfirmed origin from the interesting map on the spot —
generation keeps hunting, and a rediscovery faces the bar afresh — but never deletes the
lifecycle's counts, which feed the report. A never-confirmed origin still fails the run
(decision 3), reported caveat-only and only when nothing confirmed or trusted survived, to
avoid caveat fatigue. Caveats quote the run's own evidence and nothing else, since no rates
are ever persisted: a confirmed failure reads "nondeterministic failure, confirmed: failed
{k} of {n} replays this run", a confirmed-but-dry one "confirmed earlier this run … but not
reproduced at report time — a rare failure, or something in the environment changed after
discovery", and the unconfirmed wordings admit that a non-reproducing failure is
indistinguishable from a very rare one.

== Shrinking <shrinking>

The shrink passes, scheduling, and sort keys are untouched. What changes is the probe:
under ND handling a single run is not a verdict, so the accept rule is statistical, and the
contract it enforces is decision 2 — shrinking must not lower the reported example's
failure probability, and should raise it when cheap. If the search reaches a
deterministically-failing region, it stays there.

*Charging accepts.* A candidate whose first run passes is rejected at the cost of that
replay, the 0/1 outcome recorded in a ledger keyed by the candidate's realized timeline (a
proposal that punned into another realization merges evidence with it, because the realized
timeline is what would be adopted). Pass repetition retries rejected candidates with
evidence accumulating, so cheap rejects lose no reachable reductions. A candidate whose
first run fails is the dangerous case — one lucky failing run used to teleport the
incumbent — so it pays the gauntlet before displacing anything.

*The gauntlet.* Accept requires at least 4 failures and a Wilson lower bound clearing
`max(gamma × anchor, 0.05)`. Reject fires when the upper bound proves the threshold
unreachable, or at a physical cap of 30 runs. Short of 4 failures the verdict is only ever
continue. Each constant is doing one job. The failure minimum exists because a single
failure's lower bound is 0.2065, so without it every threshold below that accepts on the
recruiting run — the degeneration that lost a third of target-regime bugs. Gamma is 0.8,
rising to 1.0 once the anchor reaches the retention high-water of 0.8: under 20-run seeding
only zero-miss evidence gets that high, so the high-water marks incumbents
indistinguishable from deterministic and refuses to trade their reliability down at all. The floor is derived, not
chosen — 0.05 sits just under the acceptance boundary at the cap (LCB(4/30) = 0.0531) so it
costs no power. Experiment 008 measured the composition: neither min-fails nor 20-run
seeding works alone, and together they hold a median final failure probability of 0.82
against a 0.10 starting floor with 100% target-regime bug retention (67% shipped).

*Accepts move nothing by themselves.* A gauntlet accept only stashes a pending result.
State moves at the shrinker's adoption: an accepted candidate the shrinker then discards (a
punned realization, a mutation probe with a larger sort key) raises no anchor and persists
nothing. On adoption the anchor rises to the accept's topped-up 20-run lower bound (once
per realized timeline), and the new incumbent is persisted save-then-delete
(@persistence), so an interrupt at any moment loses nothing.

*The alpha budget.* Per-proposal false-accept rates compose without bound, so every
proposal on a ledger not yet at a bound verdict is charged, before its outcome is recorded,
the exact false-accept mass it adds against a q = 0.02 fluke (an exact DP over the ledger's
state). Charges are debited from a per-origin per-run budget of 0.02 held on the engine,
where re-shrink probe rebuilds keep spending from it. Exact charging keeps the measured
regimes free: an unreachable high-water threshold charges zero, mid anchors charge about
1e-6, and at the floor a fast-sweep proposal charges 4.0e-4, so the budget affords about 50
floor proposals before a *new* candidate's charge is unaffordable and the failure minimum
escalates, 4 towards a ceiling of 8, where a proposal costs at most about 1e-7. A ledger's
minimum pins at its first charge and its bound verdict latches, so a stopping rule never
changes mid-test. A pinned re-proposal charges even past the budget (overdraft is bounded
by one charge), a latched reject costs only its proposal run, and a latched accept keeps a
conclusively accepted timeline acceptable when it is re-proposed — which happens routinely,
because shrinking a clone stream runs a nested shrinker whose final result is spliced back
into the parent and proposed again. Charging per proposal makes the spent sum bound the
expected false accepts by linearity, and the sizing argument is @multiplicity's.

*Stopping.* Under fast sweeps a stochastic probe can reject a candidate on one unlucky
miss, so a fixed point reached that way certifies nothing. After a sweep with no adopted
accept, one confirmation sweep re-proposes everything with the fast reject disabled and
drives each cumulative ledger to a bound verdict; a confirmation sweep that accepts nothing
ends the shrink with a certificate that every reachable proposal was decided. Fixed
dry-sweep counts missed 18--46% of reachable reductions where confirmed-dry missed 10%
(experiment 001). The stall guard is off during confirmation sweeps — the certificate holds
only if every candidate executes. Checkpoint/rollback was rejected outright
(rollback-on-uncertainty poisons stable landscapes, rollback-on-proof never fires), and so
was anchor decay (it makes stopping incoherent).

*Boost* is the "raise it when cheap" arm. When a confirmed anchor sits below 0.30,
successive halving races the incumbent against its pool and prefix-cut mutants, and the
winner must beat the anchor on a fresh 20-run holdout before anything moves, because
in-race rates are selection-biased upward. The floor is the 20-run image of "true rate
below 0.5": the literal 0.5 over-triggered on 59% of true-0.7 incumbents, a lesson in how
changing an estimator quietly re-prices every threshold written against it. On a
deterministic-core landscape boost turned 27/30 runs landing deterministic finals into
30/30 at +14% cost (experiment 006A).

One priced pathology remains. Above the high-water at high p, gamma 1.0 means only another
all-fails 30-run batch can accept (probability about 4% at p = 0.9), so nearly every real
reduction rejects at the cap and re-proposes: experiment 012 measured 240k--3.8M
measurement replays per 100-case episode. Minima stay correct and slow bodies hit the
300-second shrink deadline instead. Every candidate fix trades against decision 2, so it is
escalated rather than fixed.

== Targeting <targeting>

Targeting was disabled under ND handling for most of the branch, and the honest reason was
that nothing had paid for restoring it. The deterministic climber applied to a noisy score
is unsound the same ways the old shrink loop was: it keeps a single-run maximum that sits
about 1.7 standard deviations above truth, ratchets against it, and freezes within about
ten runs in 92--100% of trials with gradient remaining (experiment 013).

The replacement is boost's design applied to user scores. Each label holds a reference
timeline and a monotone reference score, the median of a fresh 20-run batch — never the
recorded per-label maximum, which is demoted to seed material. Per firing of the target
phase, up to 4 races run: a pool of 16 perturbations of the reference (single-node
power-of-two steps, plus prefix-cut mutants for structure the stepper cannot reach, such as
clone streams) is successive-halved on mean observed score, and the winner adopts only when
a fresh 20-run holdout clears a sign test — the Wilson lower bound of
strictly-beats-the-reference above 0.5, meaning 15 of 20, with ties and unobserved runs
counting against. Adoption re-estimates the reference on yet another fresh batch and only
ever raises it.

Measured: the gate passes a true 75%-beat improvement 62% of the time per race at a 2.1%
false-adopt rate, and the race reaches the landscape maximum everywhere it can move for
about 950 replays per run. On a flat landscape the realized false-adopt rate is nearer 4%
per race, because the reference being beaten is itself an estimate. That is accepted
because a false adopt costs a lateral move and one holdout batch — no bug is lost and no
anchor moves. Race replays are measurement executions, and everything yields to a
discovery: once any origin exists, the run's replay budget belongs to confirmation and
shrinking.

== The final replay and backtracking <finalreplay>

Every failure the run is about to report re-executes first, inside the engine, on whatever
the shrink deadline left over. There are two regimes.

A deterministic origin gets one exact replay. A miss does not silence the report the way
the old flakiness abort did: it flips the run (or aborts under `error`), and origins whose
single replays predate the flip re-enter the queue, because the run now knows those
verdicts prove less than it thought.

An ND origin gets the pooled review: replay-until-failure over the pooled timelines with
the 29-replay reuse budget split evenly across the pool, then 10 splices, then 4 fresh
generations (a chosen constant, and the only caller with a fresh tier — a fresh case here
is pinned to the origin, while for reuse or blob replay a fresh failure could be unrelated).
The fresh tier records only failures, since a fresh generation is a rescue, not a trial of
the stored state. Every tier exits on the first reproduction, so a live bug costs about 1/p
executions and the worst case (33--50 replays, depending on how the budget rounds across
the pool) is paid only for a dry review.

The outcomes enforce the lifecycle at the seam. For a confirmed or trusted origin the
review's counts fold into the report evidence, and a dry review switches the caveat wording
rather than unreporting. For a still-unconfirmed origin, a reproducing review run is a
sighting, not a confirmation: it faces a standard evidence batch on the origin's remaining
bar attempts, bounded by the deadline (an expired deadline rejects — a batch cut short
proves nothing). An accept confirms with the batch's lower bound as anchor and no witness,
the shrinker being done. A rejected batch, an out-of-attempts origin, and a dry review all
fall through to the backtrack when history exists and otherwise to eviction. Origins first
*observed* by the review's own measurement runs are never barred — confirming them could
admit further origins without bound — and recycle via rediscovery next run.

The *backtrack* is where the per-origin history pays off. A never-confirmed origin that
misses its shrink verify or final replay scans its history for the reproduction boundary:
single probes at geometric offsets over the accept segment plus every raw sighting, then
binary refinement, on a budget of 40 replays. The scan biases old under uncertainty — a
newest-first walk would spend the budget re-ratifying the degraded tail — and decision 2
makes the old bias safe: a too-old restore re-shrinks under the gauntlet, a too-new one
anchors low or gets rejected. The settled candidate faces the full discovery bar on a
separate budget of three attempts (about 83% composed power in the target regime), held per
origin per run. A cleared bar confirms the origin, force-persists the restored incumbent
past the monotone save gate, and re-enters shrinking under the gauntlet. This is not the
checkpoint/rollback the shrink design rejected: it is detection-triggered, fires only on a
flip, and restores nothing that has not cleared the same bar discovery pays.

Capture stamping shapes what the user sees. Executions whose failures can become the report
are stamped for capture — final-replay and first-check replays even on deterministic runs,
confirmation batches, reuse and blob replays, and generation cases once ND handling is
active — while shrink-gauntlet and boost probes stay cheap and unstamped, because capturing
and symbolising output for every discarded probe is the dominant cost of failing-heavy
runs. A dry final replay therefore prints the freshest stamped failing execution, usually
confirmation-time pre-shrink values, while the blob carries the shrunk incumbent.

== Persistence and reproduction <persistence>

Persistence stores the representation of a failing example and nothing else: no rates, no
miss counters, no status flags. Every run recomputes its estimates and stands alone. A v1
entry is a plain serialized choice sequence. A v2 entry is `NdReproState` — the pooled
timelines incumbent-first, an entropy seed, and the continuation-budget extension — and the
same bytes serve as the database entry and the payload behind the blob's ND prefixes.
ND-ness is carried by the format itself: the encoding opens with four 0xFF bytes, an
impossible choice count, so a pre-v2 reader rejects it as corrupt instead of misreading it,
and a v2-aware run flips before any replay. Decoding is hardened with bounds deliberately
looser than the write side (64 timelines against the pool's 10, a 16 MiB decompression
bound), so raising write-side caps later never invalidates stored corpora and a hostile
blob cannot force an arbitrary allocation.

Only origins past confirmation persist, and mid-run saves land at validated events only
(confirmation and adopted gauntlet accepts). Every save is ordered save-then-delete: the
new incumbent's bytes are written before the bytes they supersede are removed, so the
primary key carries the most recent validated example at every instant and Ctrl-C
mid-shrink loses nothing. A superseded entry that started the run on the primary key is
demoted to the secondary corpus (it ended some run as a best example, so it gets a
staleness strike, not deletion), while a superseded same-run save is deleted outright. The
secondary corpus is capped at 50 per key as a resource bound.

Reuse replays each stored entry until failure under the 29-replay budget, first-fit then
splices. Hygiene is two strikes across two runs: a primary-key miss after the full budget
demotes, a secondary miss deletes, so one unlucky run cannot destroy a live entry. A
reproduction trusts the origin (@lifecycle) and skips the first-interesting check, and when
the stored incumbent reproduces node-for-node the shrink phase is skipped entirely. The
pre-shrink secondary drain — one exact replay then delete — survives only for v1 entries
under deterministic handling, because a single-replay delete under ND is a zero-strike
deletion.

Blobs replay as runs: `hegel_run_start_blob` drives a v1 blob through up to four
continuation-tolerant attempts (one bare exact replay reproduced 13% of never-flipped
concurrent blobs; four attempts bound the joint escape-then-miss at 1.2e-3) and a v2 blob
through the replay primitive with no fresh tier. A reproduction trusts the origin and
re-reports the failure with its caveat; no failure within budget reports the blob as stale,
naming both hypotheses (fixed, or nondeterministic and unlucky).
`hegel_test_case_from_blob` remains for embedders as a documented single attempt.

== The ABI and the frontend <abi>

The break is deliberate and small:

#table(
  columns: (auto, 1fr),
  align: (left, left),
  [*Retired*], [Run status 3 (`FAILED_NONDETERMINISTIC`), reserved forever: an ND failure
    is plain `FAILED` plus `hegel_failure_caveat`, one nullable string per failure. Caveat
    standing is per-origin information a run-level status cannot carry.],
  [*Renamed*], [`hegel_test_case_is_nondeterministic` → `hegel_test_case_should_capture`,
    no shim. The contract changed from "which runs are doomed" to "which executions should
    capture", and a silent semantic change under the old name would be worse than a
    compile error.],
  [*Changed*], [`hegel_new_state_machine` always succeeds and declares nothing
    (decision 70). The old first-case rejection and the sacrificed-case protocol are gone.],
  [*Added*], [`hegel_settings_set_nondeterminism_strictness`, `hegel_run_start_blob`
    (replays a blob as a full run), blob prefixes 2/3 (the v2 format), and the statistics
    line's measurement count.],
)

The capture contract replaces client-side blob replay: the engine stamps the executions a
report can be built from, the client buffers a stamped case's output and diagnostic keyed
by origin, and the report is built from the freshest capture at the best rank (diagnostic
beats draw lines beats a bare record, so an unstamped probe can never clobber a good
capture). The Rust frontend prints each failure as one block — captured lines, diagnostic,
`note: {caveat}`, reproducer line — and re-raises the failing test's own panic payload.

Clone streams cross the ABI values-only and reassemble into one tree-shaped timeline: a
clone records a single Clone node at the parent's current position, each stream replays its
own values, and the cross-thread interleaving of side effects is sampled fresh every
execution, never replayed. That asymmetry is why a shrunk racy failure reproduces only
sometimes on exact replay, and why concurrent reproduction is carried by the pool and
splice machinery — a splice cuts at top-level positions, so a clone stream crosses over
intact. Concurrent failures otherwise flow the whole pipeline like any ND failure:
confirmed, shrunk, persisted, blob-reproducible (measured at ceiling in experiment 007,
off-ceiling at ≥ 98% for p ≤ 0.3 in 009a/b, with the residual never-flip episodes closed by
the first-interesting check — 0/200 against 23/200 baseline in experiment 012).

== Multiplicity <multiplicity>

The per-test statistics above are honest, and @problems ends with why that is not enough:
one run repeats them without bound. Three sites composed unboundedly — bar recycling (21%
fluke confirm over a long run), gauntlet proposals (33% per thousand at the floor, and a
confirmation-sweep drive of a bugless candidate carries 2.9e-3, seven times the fast-sweep
number the original arithmetic composed), and the report-time any-failure rule (roughly
50% per lingering fluke).

Nothing here fits a batch correction. Benjamini-Hochberg ranks a set of p-values and
rejects a subset; the engine's verdicts act the moment they are computed and are
irreversible (an eviction, a displacement, a persisted incumbent), so there is never a
batch to rank. Alpha-investing was rejected because it bounds a global ratio (mFDR), not
the per-origin error a sticky false confirm actually costs, and needs its own payout
calibration. A count-based doubling schedule was rejected because its terminal stage still
leaks unboundedly and it overcharges the mid-anchor proposals that are nearly free under
exact charging. What fits is sequential, online, per-origin budgets:

- *Bar attempts.* Five bar batches per origin per run, shared by the discovery sweep,
  shrink admission, and the pooled review; at the cap the origin is rejected without a
  batch and evicted, with the full unconfirmed treatment. Five holds the composition at the
  2.9% the bar's own derivation assumed while keeping over 95% power at the target rate.
  The backtrack gets a separate budget of three because its candidates come from history,
  which skews toward the real bug's pre-flip sightings; the ceiling of eight batches
  composes to 4.6%.
- *Gauntlet alpha.* A spend of 0.02 per origin per run, charged per proposal with the exact
  DP (@shrinking), so the spent sum bounds the expected false accepts by linearity and
  regimes with nothing to fear pay nothing. Exhaustion escalates the failure minimum for
  new candidates rather than refusing them, keeping the tail below 1e-7 per proposal.
- *The review bar.* The pooled review's reproducing run faces a standard evidence batch on
  the remaining bar attempts instead of confirming outright, cutting fluke confirms about
  170x (0.487 → 0.003).

The budgets are priced, and the price lands below the target regime. A p = 0.05 bug
confirms in 42% of runs instead of near-certainly given a long one, leaning on cross-run
recycling, while p ≥ 0.1 loses at most 5 points. Mixed bug-plus-fluke origins pay most, the
bug's
confirm probability falling 0.95/0.72/0.45 as the fluke's share of sightings rises
0/0.5/0.75. The review bar drops report-time rescue power from 0.97 to 0.44 at the target
before the backtrack recovers some of it. Escalated failure minima cost floor-threshold
shrinks acceptance power (a p = 0.1 candidate's acceptance falls 0.57/0.33/0.16 at minima
4/5/6) and make the confirmed-dry certificate easier to obtain, stopping some shrinks
earlier. All of it is conservative in decision 2's direction: a refused candidate or
confirm keeps the incumbent and never loses a failure. Anchors keep z = 1.96 with no
haircut, because their selection miscoverage concentrates at the accept boundary while mean
anchors sit at or below truth — the conservative direction, absorbed by the gamma slack.

== What the experiments established <experiments>

The design above stands on measurements the reader has not seen. The harnesses are frozen
under `/experiments` and the write-ups under `notes/experiments/`. This is what each one
established.

#table(
  columns: (auto, 1fr),
  align: (left, left),
  [*001*], [Pure shrink simulation. Charging accepts (not rejects) is the only policy that
    survived the noise floor: the naive policy lost the bug in 34% of noise-floor trials
    and per-candidate fixed-N lost it in 100%. Confirmed-dry stopping replaced fixed dry
    sweeps at equal cost and half the missed reductions. Checkpoint rollback and anchor
    decay both died here.],
  [*003*], [The same rules in the real shrinker. Flukes displaced a 20/20-confirmed
    discovery in about 80% of noise-floor trials, hence never-displace-an-occupied-origin
    and the-discovery-is-selection.],
  [*004*], [Replay semantics. A pool of 5--10 first-fit timelines is the plateau, a flat
    continuation budget of 4 absorbs elongation, and the trie encoding was rejected
    because prefix sharing anticorrelates with pool need.],
  [*005*], [Lifecycle. 005A's exact DP derived the gate-then-extend bar and its operating
    points. 005B showed run-triggered confirmation letting span-mutation executions
    confirm flukes in 26 of 30 pure-noise runs, hence confirmation gates admission on
    every path.],
  [*006*], [Grafting and boost. Splices rescue 65--100% of whole-pool misses. Boost by
    Optimiser hill-climbing had already died in the statistics critique, and 006 validated
    its replacement, the holdout-gated halving race.],
  [*007*], [Concurrency. Whole-timeline pool replay reproduces concurrent stateful and
    clone-flaky failures at ceiling (20/20 discovery and reuse, 60/60 blob replays),
    closing per-position anchoring and unifying the concurrent regime into the ND path.],
  [*008*], [The recalibration. The shipped gauntlet had degenerated to single-run accepts
    across the whole target regime; each constant had been derived alone and the
    composition never measured. Fixed by min-fails 4 plus 20-run anchor seeding plus the
    derived floor plus the high-water gamma, whose envelope (median final failure
    probability 0.82, 100% bug retention) is what the design now quotes.],
  [*009a/b*], [Reproduction off the ceiling. Reuse and blob reproduction ≥ 98% at p ≤ 0.3
    on the composed engine, measurement cost 1.6--2.1x there and 4--6x at p = 0.9. 009a
    also found the never-flip corner: 23 of 200 clone episodes at p = 0.9 never flipped
    and emitted v1 blobs reproducing at 13%. (These on-engine numbers describe the
    since-retired weighted estimator; see @residuals.)],
  [*010*], [What the data tree buys, on production main. Recording cost 40--80% wall on
    passing bodies, serving was almost all exact repeats, novel prefix and exhaustion were
    inert outside tiny spaces. Unblocked the removal.],
  [*011*], [The seam, instrumented. 88% of the target-regime caveat-only rate was bar
    power meeting a spent budget, the gradient cell's loss was entirely pre-flip
    displacement, and the tree's own detection channel fired zero times in 600 trials.
    After the seam work: caveat-only 49% → 0 and 15% → 0, bug kept 100/100 everywhere but
    one cell (99/100).],
  [*012*], [Detection escape, re-run post-seam. Never-flip 0/200 in every ND cell against
    the 23/200 baseline, every flip at the first check, blob reproduction 200/200 at
    p = 0.9, the deterministic control paying exactly 4 measurement replays per episode.
    Also surfaced the gauntlet cost lottery (@residuals).],
  [*013*], [Targeting. The deterministic climber freezes on noisy scores in 92--100% of
    trials against a winner's-curse maximum. Derived the race and its constants.],
  [*014*], [Multiplicity. Priced the uncontrolled composition (21% fluke confirm from bar
    recycling, 33% gauntlet exposure per thousand floor proposals, roughly 50% from the
    review's any-failure rule) and derived the budgets: five bar attempts hold 2.9% at
    over 95% target power, exact per-proposal alpha charges with the 4 → 8 escalation, and
    the review bar's 170x fluke cut at 0.97 → 0.44 target power.],
)

The methodological lesson the series kept re-teaching: an estimate contaminated by the
selection process that produced it will bite, and the fix is always to move the accounting
to a validated event. The gauntlet accept, the anchor's estimand, the boost holdout, the
targeting reference, and the in-batch witness rule are the same fix at different seams.

== Costs and residuals <residuals>

*What handling costs.* 1.6--2.1x total measurement replays at p ≤ 0.3 against the
pre-composition engine, concentrated in fail-heavy evidence top-ups, and 4--6x at p = 0.9.
A deterministic run pays exactly four extra replays per discovered origin (the first check)
and nothing else.

*The known pathological regime* is the gauntlet cost lottery above the retention high-water
at high p (@shrinking): shrinking reaches correct minima but can burn millions of replays
or hit the deadline part-way. Every fix trades against decision 2, so it is escalated, not
fixed.

*Priced residuals, kept deliberately:*

- Pre-flip single-run trust inside a checked origin's shrink: an origin that passes an
  honest first check still shrinks on single-run trust until something flips the run.
  Price: final failure probability median 0.74 against the 0.82 simulated envelope,
  executions 1.54x against the 1.5x acceptance criterion (experiment 011).
- The never-flip share that passes an honest check: the escape rate (p·s)⁴ is small, not
  zero. What escapes carries a v1 blob whose four continuation replays keep it
  reproducible (012: 200/200 blob reproduction at p = 0.9, zero escapes observed in 200
  episodes per cell).
- Post-flip displacement freeze can report a confirmed flaky example where free
  displacement would have found a smaller or deterministic one the run never held (30/100
  in 011's worst cell).
- The multiplicity budgets' power prices (@multiplicity): the sub-target confirm rate, the
  mixed-origin penalty, the review's 0.97 → 0.44, and the escalated minima's earlier
  stopping.
- A concurrent one-shot failure on a never-flipped run reports caveat-only without the
  discovering case's draws, because pre-flip generation cases are not stamped for capture
  (decision 70). If values-less unconfirmed reports bite, the fix is stamping generation
  executions unconditionally, a capture-cost trade not taken.
- The on-engine measurements in 009a/b, 011, and 012 describe the retired weighted
  estimator (decision 71): divergence-heavy bodies now measure at the lower rate a user
  replaying stored state actually sees. Re-measurement is a candidate follow-up.

*Smaller open items:* the bytes-increment shrinker hole (both eras stall on about 1 in 20
random starts; waiting on a probe-based increment variant), the recursive depth-spread
regression from the tree removal (chain-only recursive generators lose novelty forcing),
`replay_aligned` under ND accepted as re-shrinking every run for structurally-ND bodies,
experiment 010's untested caveats (novel-prefix value on rare bugs), targeting's ~4%
per-race flat-landscape false-adopt rate (a lateral move, accepted), and hegel-cpp's
compile-time
migration for the retired status 3.

*Extraction* of the final implementation from this branch — the step deliberately left
outside all the plans — remains open.

#pagebreak()

= Appendix: the constants

#table(
  columns: (auto, auto, 1fr),
  align: (left, left, left),
  [*Constant*], [*Value*], [*Derivation*],
  [`TARGET_FAILURE_RATE`], [0.1], [Decision 16, the design target. Everything else is
    sized against it.],
  [`GATE_RUNS`], [10], [Discovery bar gate (005A exact DP).],
  [`CONFIRM_CAP`], [40], [Discovery bar physical cap (005A).],
  [`CONFIRM_MIN_FAILS`], [4], [Discovery bar accept (005A).],
  [`BAR_ATTEMPTS_PER_RUN`], [5], [Bar batches per origin per run, shared by sweep, shrink
    admission, and pooled review: composition 2.9% at > 95% target power (014,
    decision 72).],
  [`ANCHOR_SEED_RUNS`], [20], [Batches extend past their accept so anchors estimate the
    rate, not the stopping rule. The largest size whose all-fail LCB a candidate can match
    within the gauntlet cap; 40-run seeding stalls shrinking outright (008).],
  [`GAUNTLET_MIN_FAILS`], [4], [Without it, any threshold at or below LCB(1/1) = 0.2065
    accepts on the recruiting failure. Keeps 100% of target-regime bugs at 3x the shipped
    cost (008).],
  [`GAUNTLET_MIN_FAILS_CEILING`], [8], [Escalated minimum when a new candidate's alpha
    charge is unaffordable; at 8 a proposal charges ≤ ~1e-7 (014, decision 72).],
  [`GAUNTLET_CAP`], [30], [Physical cap per proposal (008 rows).],
  [`GAUNTLET_GAMMA`], [0.8], [Bounded-loss retention below the high-water (decision 2's
    budget).],
  [`RETENTION_HIGH_WATER`], [0.8], [Zero-miss detector: only LCB(20/20) = 0.839 reaches it
    under 20-run seeding. Converts a 33% displacement cell to zero for 1.26x replays
    (008).],
  [`GAUNTLET_FLOOR`], [0.05], [Derived: just under LCB(4/30) = 0.0531, so it costs no
    power at min-fails 4 (008).],
  [`GAUNTLET_ALPHA_BUDGET`], [0.02], [Per-origin per-run false-accept spend; ~50 fast
    floor proposals before escalation (014, decision 72).],
  [`CHARGE_FLUKE_RATE`], [0.02], [The fluke rate the alpha DP charges against, matching
    the bar's fluke sizing (014).],
  [`POOL_CAP`], [10], [Timelines per origin, incumbent included. 5 is near-ceiling, 10 is
    the plateau (004).],
  [`REPRODUCE_SPLICES`], [10], [65--100% rescue measured at a 10-splice cap (006). The
    shipped 6 was a transcription error (decision 52).],
  [`FINAL_REPLAY_FRESH`], [4], [Chosen, not derived (decision 53).],
  [`V1_BLOB_REPLAYS`], [4], [Bounds the joint escape-then-miss at 1.2e-3 (decision 59).],
  [`FIRST_CHECK_REPLAYS`], [4], [Detection 1 − (p·s)⁴, cost +4 on a deterministically
    failing origin (decision 64).],
  [`BACKTRACK_SCAN_REPLAYS`], [40], [Set equal to `CONFIRM_CAP` (decision 66).],
  [`BACKTRACK_BAR_ATTEMPTS`], [3], [Composes to about 83% target-regime power; held per
    origin per run, and with the bar budget a ceiling of eight batches at 4.6% (decisions
    66, 72).],
  [`BOOST_RELIABILITY_FLOOR`], [0.30], [LCB(10/20), the 20-run image of "true rate below
    0.5". Recall 0.991, precision 1.000 on the calibration population (008,
    decision 56).],
  [`BOOST_POOL`], [16], [Halving race width (006).],
  [`BOOST_HOLDOUT`], [20], [Set equal to `ANCHOR_SEED_RUNS`, so boosted anchors are
    estimated on seeded batch size (decision 56).],
  [`TARGET_ND_POOL`], [16], [Set equal to `BOOST_POOL` (decision 69).],
  [`TARGET_ND_HOLDOUT`], [20], [15/20 beats is the sign-test boundary. 62% power on a
    true 75%-beat improvement at 2.1% false adoption. 10 stalls on ties, 30 buys nothing
    (013).],
  [`TARGET_ND_RACES`], [4], [Full progress at about 950 replays per run. 8 doubles cost
    and flat-landscape false adoption for nothing (013).],
  [`DUPLICATE_STOP`], [10], [Exhausted-space trigger only while no valid case exists
    (decision 61).],
  [`SECONDARY_CORPUS_CAP`], [50], [Per key, shortlex-largest evicted at reconciliation
    (decision 44).],
  [`ND_STATE_MAX_TIMELINES`], [64], [Decode-side bound, deliberately looser than
    `POOL_CAP`.],
  [`MAX_DECOMPRESSED_LEN`], [16 MiB], [Decode hardening, sized with headroom over the
    largest state the decoder would accept.],
  [Reuse replay budget], [29], [`replay_budget(0.1, 0.05)`: a flat 10 misses a p = 0.1 bug
    35% of the time (decisions 11, 16).],
  [Continuation budget], [len + max(4, len/8)], [Flat 4 absorbs all measured elongation,
    the len/8 term scales long timelines (004).],
  [z], [1.96], [Deliberate retention: the exact-DP operating points are the
    specification, z is a tuning constant, and the composition across tests is bounded by
    decision 72's budgets (008, 014).],
)
