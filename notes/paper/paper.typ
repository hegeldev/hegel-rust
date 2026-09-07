// ACM sigconf-like layout; ISSTA budget: ~10pp main text + references,
// appendices as supplementary material.

#let sans = ("Helvetica Neue", "Arial")
#set document(
  title: "Property-Based Testing Under Nondeterminism",
  author: "David R. MacIver",
)
#set page(
  paper: "us-letter",
  margin: (x: 0.75in, top: 0.9in, bottom: 1in),
  columns: 2,
  footer: context align(center, text(size: 8pt, counter(page).display())),
)
#set columns(gutter: 22pt)
#set text(font: "Libertinus Serif", size: 9pt)
#set par(justify: true, leading: 0.58em, spacing: 0.58em, first-line-indent: 1em)
#set heading(numbering: "1.1")
#show heading.where(level: 1): it => block(above: 1.1em, below: 0.6em, text(
  font: sans,
  size: 10.5pt,
  weight: "bold",
  it,
))
#show heading.where(level: 2): it => block(above: 1em, below: 0.5em, text(
  font: sans,
  size: 9.5pt,
  weight: "bold",
  it,
))
#show heading.where(level: 3): it => block(above: 0.9em, below: 0.45em, text(
  font: sans,
  size: 9pt,
  weight: "bold",
  style: "italic",
  it,
))
#show figure.caption: it => text(size: 8pt, it)
#show table: set text(size: 7.6pt)
#show raw.where(block: true): set text(size: 7.3pt)
#show raw.where(block: false): set text(size: 8pt)
#set table(stroke: none, inset: (x: 4pt, y: 2.6pt))
#show link: set text(fill: rgb("#00349c"))

#let toprule = table.hline(stroke: 0.7pt)
#let midrule = table.hline(stroke: 0.35pt)
#let botrule = table.hline(stroke: 0.7pt)

#place(top + center, scope: "parent", float: true, clearance: 1.6em)[
  #text(font: sans, size: 17pt, weight: "bold")[
    Property-Based Testing Under Nondeterminism
  ]
  #v(0.9em)
  #text(size: 11pt)[David R. MacIver]\
  #text(size: 9pt)[Antithesis]\
  #text(size: 8pt, font: "Andale Mono")[david.maciver\@antithesis.com]
  #v(0.5em)
  #text(size: 8pt, style: "italic")[
    Working draft, September 2026. Assembled by Claude from the project's
    design notes, decision log, and experiment reports, and checked against
    them. The measurements in
    #{
      show ref: it => it
      [Section 6]
    }
    were run for this paper. Not yet reviewed for submission.
  ]
]

#heading(numbering: none, outlined: false, level: 1)[Abstract]

Property-based testing engines in the Hypothesis tradition treat a test as a
deterministic function of a recorded sequence of nondeterministic choices.
Everything downstream of generation leans on that: shrinking replays modified
sequences and trusts each verdict, deduplication and caching key on the
sequence, the failure database and reproduction artefacts store one sequence,
and a replay that disagrees with the recorded outcome aborts the run as
flaky. Real tests violate the assumption routinely — concurrency, hidden
state, timing — and engines respond by giving up: they abort without a
counterexample, or disable shrinking, persistence, and reproduction
wholesale.

We present the design and implementation of nondeterministic-test handling in
Hegel, a Hypothesis-descended engine, built on one premise: every test
execution is a Bernoulli trial, and failure probability is a first-class
quantity. Failures are confirmed by a sequential replay rule derived by exact
dynamic programming under an asymmetric loss. Shrinking accepts a candidate
only on statistical evidence that it reproduces nearly as reliably as the
current example, so reduction cannot trade the bug away. Per-failure budgets
bound what unbounded within-run repetition can buy, in the manner of online
multiple-testing control. Reproduction replays a pool of stored executions
until the failure recurs, rather than expecting one replay to succeed.

On failure landscapes where a determinism-enforcing baseline reports a
counterexample in 0–4% of runs, the engine reports a confirmed, shrunk,
reproducible counterexample in 98–100% while retaining the underlying bug in
98–100% of reports. A noise-only test yields a false "confirmed" report in
5% of runs, matching the derived bound. Stored failures at or above the
design's target failure rate reproduce across runs and processes at
96–100%. Deterministic failing tests pay four extra
executions per failure, and passing suites pay nothing.

= Introduction

Property-based testing (PBT) generates inputs, checks properties over them,
and, on failure, _shrinks_: it searches for a smaller input that still fails,
because the difference between a 200-element counterexample and a 2-element
one is the difference between a bug report a developer ignores and one they
fix @quickcheck @hughes-experiences. The value of a PBT run is concentrated
in its final report: a minimal example, plus enough to reproduce it.

Engines descended from Hypothesis @hypothesis get both from one
representation. Every nondeterministic choice a test makes — every random
draw — is recorded as a _choice sequence_, and the test is treated as a pure
function from that sequence to a verdict. Generation samples sequences;
shrinking edits the recorded sequence and replays it, navigating input space
without understanding it @reducer; a failure database stores the best failing
sequence for the next run; a reproduction token embeds it in the failure
message. The architecture has displaced combinator-level shrinking in most
modern implementations precisely because replay composes: anything that can
be recorded can be re-run, minimised, cached, and shared.

All of it rests on an invariant the engine cannot enforce: _no
nondeterminism outside the engine's control_. The test body must be a
deterministic function of its draws. Concurrency breaks the invariant
intrinsically — the thread schedule is not in the choice sequence — and so
do global state, time, iteration order of hashed collections, and the
network. At industrial scale, test flakiness is the dominant obstacle to
acting on test signal at all @flaky-empirical
@harman-ohearn @deflaker @idflakies.

What do engines do when the invariant fails? Hypothesis detects the
disagreement and raises a `Flaky` error: the run aborts, and the user gets
neither a minimal example nor a reproduction token, for exactly the bugs —
races — where those artefacts matter most. QuickCheck-family tools
@quickcheck @proptest do not detect it: shrinking trusts each single
verdict, so on a probabilistic failure the search random-walks wherever
noise leads it (we measure this below: it reports an _empty_, effectively
passing input as the "minimal counterexample" in every noise-floor trial).
Hegel, the system this paper modifies — a Rust engine implementing the
Hypothesis architecture behind a C ABI, with per-language frontends — used
to respond to declared concurrency by disabling shrinking, targeting,
persistence, and reproduction outright and reporting at most one unshrunk
failure per run, and to every other detected nondeterminism by aborting.
We refer to this posture, common to all of the above, as _surrender_.

This paper replaces surrender with statistics. The design premise is that a
counterexample to a nondeterministic test is not a value but a distribution:
each execution is a Bernoulli trial with some failure probability $p$, and
everything the engine used to decide by lookup — is this failure real? is
this smaller input still failing? does the stored example still work? — it
must now decide by estimation, with explicit error budgets. Concretely:

- A run is deterministic until proven otherwise. Detection is by
  observation only (a replay or a repeated execution whose verdict
  disagrees), costs a failing deterministic test four extra executions,
  costs a passing suite nothing, and never requires the user to declare
  anything.
- A failure sighting is _selection, not evidence_: it was noticed because
  it failed, and at realistic noise floors the first sighting is more often
  a background fluke than the bug. Failures therefore earn reporting,
  shrinking, and persistence by passing a sequential _confirmation bar_
  whose accept/reject rule we derive by exact dynamic programming under an
  asymmetric loss (a false accept is sticky; a false reject is recycled by
  continued generation).
- Shrinking must not trade failure probability for size. A shrink candidate
  displaces the current example only after clearing a sequential test — the
  _gauntlet_ — against a monotone lower-confidence-bound estimate of the
  incumbent example's reproduction rate, and the search stops only when a
  final sweep has driven every candidate to a statistically bound verdict.
- One run performs thousands of these sequential tests, and their per-test
  error rates compound. Per-failure budgets — an attempt cap on
  confirmation and an exact alpha-spending scheme on shrink acceptances,
  in the spirit of online false-discovery control @alpha-investing — bound
  the compounding, at a priced cost in power.
- Reproduction stores _representations, never estimates_: a bounded pool of
  complete failing executions, replayed first-fit with a tolerance for
  structural divergence, then recombined pairwise when whole executions
  miss. Replay counts are derived from the design's target failure rate
  rather than chosen.

The result, measured in @sec:eval, is an engine where a racy test gets what
a deterministic test always had: a shrunk counterexample, an honest report
(every claim in it is backed by that run's own replay counts), a database
entry, and a reproduction token that works — at a cost that lands almost
entirely on tests that are actually nondeterministic.

*Contributions.*
(1) An analysis of what the determinism invariant carries in
choice-sequence PBT engines, and measurements of how single-run trust fails
without it (@sec:problem).
(2) A design that makes failure probability first-class across detection,
confirmation, shrinking, multiplicity control, reporting, and reproduction
(@sec:design, @sec:stats), with every constant derived — by exact dynamic
programming, simulation, or measurement — rather than chosen (appendices).
(3) An implementation in a production engine, replacing wholesale surrender
for concurrent and flaky tests. The machinery is language-agnostic and sits
behind the engine's C ABI, invisible to test authors.
(4) An evaluation on synthetic failure landscapes and racy test bodies
(@sec:eval), plus an account of what failed on the way (@sec:discussion) —
including a calibration lesson we believe generalises: sequential rules
that are individually sound can jointly reproduce the naive behaviour they
were built to prevent.

= Background: choice-sequence engines <sec:background>

A Hypothesis-style engine mediates every random decision a test makes. When
the body asks for an integer, a float, a boolean, a string, or a byte
buffer, the engine records the typed value (with the constraints it was
drawn under) into the current _choice sequence_. Generators for compound
values are user-level compositions of these primitive draws. The engine
additionally records _spans_ — labelled brackets grouping related draws —
which give structure-aware mutation and shrinking passes something to grab.

_Shrinking_ is search over sequences @reducer. A candidate is proposed by
editing the recorded failing sequence (deleting chunks, zeroing spans,
minimising values), the body is re-executed against it (draws are answered
from the candidate, with fresh generation past its end), and the candidate
is accepted if the run still fails _with the same failure_ and its sequence
is smaller in a shortlex order. Failure identity — which we call the
_origin_ — is the panic or assertion site. All reporting, storage, and
budgets in this paper are per-origin. Repeated executions are deduplicated
by caching verdicts keyed on realised values.

Persistence reuses the same representation. A _failure database_ maps each
test to its best known failing sequences. The next run replays them before
generating, so regressions are caught immediately and a bug, once found,
stays found. A _reproduction token_ (a compact serialisation of the failing
sequence, printed in the failure report) lets a developer replay one
failure in a debugger, on another machine, with no database. Before a
failure is reported, the engine replays it one final time to re-derive the
values it prints — and it is here, and at the analogous check before
shrinking, that a Hypothesis-style engine notices flakiness: a replay that
does not fail as recorded aborts the run.

Hegel implements this architecture as a native Rust engine exposed as a
C library. Language frontends (the Rust frontend is used in this paper)
drive it through typed draw calls and report each verdict back. It supports
stateful and concurrent testing: a concurrent test runs $n$ worker threads,
each drawing from its own recorded sub-stream (so replay of the _values_ is
schedule-independent), but the schedule itself is sampled by the OS and
never replayed. Nothing in this paper controls or replays schedules — the
engine treats a racy test as a Bernoulli trial, in deliberate contrast to
systematic concurrency testing @chess @pct and deterministic-replay systems
@rr @foundationdb @antithesis (see @sec:related).

= What single-run trust breaks <sec:problem>

Nondeterminism enters on two axes, and they need different machinery.
_Generation nondeterminism_: the same replayed prefix produces a different
draw structure (a retry loop draws once more, a worker interleaves
differently), so "the" choice sequence of a test case is not well-defined —
a representation problem. _Outcome nondeterminism_: the same realised
sequence produces a different verdict — a statistics problem. A racy test
usually has both.

Under outcome nondeterminism, every subsystem that trusts one execution
fails in a characteristic way. Each failure mode below was measured, in the
simulation and in-engine experiments described in @sec:eval and @app:setup.

*Discovery is selection-biased.* A long generation phase gives
low-probability flukes many chances to fire once. On a landscape with a
real bug at $p = 0.9$ over a background of spurious failures at
$p = 0.02$ (each execution of any input has a 2% chance of failing for an
unrelated reason — an infrastructure hiccup, a timeout), the first
interesting execution is a fluke roughly twice as often as it is the bug.
An engine that believes first sightings starts a third of its shrinks from
noise.

*Shrinking is a ratchet, and noise turns it.* Shrink acceptance is
irreversible: each accepted candidate becomes the new incumbent, and the
sort order guarantees the sequence only gets smaller. One lucky failing
run of a bug-free candidate — probability $0.02$ per proposal at the noise
floor, thousands of proposals per shrink — walks the incumbent off the bug
and down to noise, and there is no way back. In simulation, single-run
acceptance loses a $p = 0.9$ bug in 34% of noise-floor trials. The
seemingly safer rule "accept if it fails within 10 replays" loses it in
_all_ of them, reporting an empty input failing at $p = 0.02$ as the
minimal counterexample, because any-failure-within-$N$ amplifies noise
acceptance. In the real engine, single-run acceptance kept the bug in
3 of 100 noise-floor runs.

*Replay-once logic erases and cannot reproduce.* A stored $p = 0.1$
failure passes a single replay with probability 0.9 — so database reuse
that deletes on one miss erases a good corpus almost immediately, and a
reproduction token that replays once fails its user 90% of the time. On
genuinely racy bodies, a token that replays the recorded sequence exactly
once, with no tolerance for structural divergence, reproduced in 13% of
attempts.

*Flakiness checks convert all of the above into aborts.* The verify-replay
and final-replay disagreements that a deterministic engine treats as
corruption are, under nondeterminism, just sampling. Aborting on them
denies the user a report precisely when a failure was found. In our
baseline measurements the unmodified engine aborted 29–69% of runs on
outcome-nondeterministic bodies.

Two constraints shape everything that follows. First, a scope: the
machinery targets tests that fail at least 10% of the time they run
($p >= 0.1$). Rarer failures are still reported honestly but are not
promised confirmation or reproduction, and every replay budget in the
system is derived from this target. Second, a contract for shrinking: _the
reported example's failure probability must not be (statistically)
lowered by reduction, and should be raised when cheap_ — a small example
that no longer fails for the real reason is worse than a large one that
does.

= Architecture <sec:design>

#figure(
  placement: top,
  scope: "parent",
  block(
    width: 100%,
    fill: luma(248),
    inset: 7pt,
    radius: 2pt,
    align(left)[
      #set par(leading: 0.44em, first-line-indent: 0em)
      #raw(
        lang: "rust",
        "#[hegel::test]\nfn totals_agree(tc: &TestCase) {\n    let batch = tc.draw(gs::vecs(gs::integers::<u32>().max_value(1000)).max_size(20));\n    let expected: u32 = batch.iter().sum();\n    assert_eq!(expected, racy_sum(&batch), \"totals diverged\"); // racy_sum drops a large job ~40% of the time\n}",
      )
      #v(3pt)
      #line(length: 100%, stroke: 0.3pt + luma(180))
      #v(3pt)
      #raw(
        "let draw_1 = vec![96, 104];\nthread 'main' (1) panicked at src/main.rs:30:13:\nassertion `left == right` failed: totals diverged\n  left: 200\n right: 96\nnote: nondeterministic failure, confirmed: failed 18 of 20 replays at confirmation and 1 of 4 at report time\n\nTo reproduce this failure, add the attribute below #[hegel::test]:\n    #[hegel::reproduce_failure(\"A3icTYrBCYAwFEOTohe7gBcHcAJPenQOL4JbOIBDOkeh8JtCP/PwyC...\")]",
      )
    ],
  ),
  caption: [
    A flaky test and its verbatim report (token truncated). The example is
    shrunk under the retention guarantee, the caveat quotes this run's own
    replay counts, and the token replays a pool of stored executions until
    the failure recurs, so it works despite the race.
  ],
) <fig:report>

A run starts deterministic and stays so until evidence arrives. The switch
into _nondeterministic (ND) handling_ is one sticky per-run flag, set by
the detection channels of @sec:detect and never cleared within a run. A
`nondeterminism_strictness` setting governs presentation only: `quiet`
(default) switches silently, `warn` prints one notice, and `error`
preserves the abort behaviour for suites that use determinism as a lint.
Detection is never declared: creating a concurrent test changes nothing
until its behaviour is actually observed to vary.

Five terms recur. A _timeline_ is the realised choice sequence of one
execution. An _origin_ is failure identity (the panic site). The
_incumbent_ is the failing timeline currently held as an origin's best
example, and its _pool_ is a bounded set (10, incumbent included) of other
failing timelines captured for that origin. The _anchor_ is a monotone
lower confidence bound on the incumbent's reproduction rate under the
engine's own replay procedure. _Evidence_ is always a plain pair
(fails, runs) of replay counts. Each replay of a stored timeline is one
Bernoulli trial of "this test case reproduces this origin," whatever
structure the replay realises — a structurally diverged miss counts in
full, because failing to see the failure is exactly what non-reproduction
means. Confidence bounds on evidence are Wilson score bounds @wilson at
$z = 1.96$ (@app:bar).

Per origin, a lifecycle state machine gates every consumer:

- *Unconfirmed*: observed interesting, not yet past the confirmation bar.
  Reported only as a caveated failure ("unconfirmed: failed 0 of 3 replays
  after the observed failure"), never shrunk, never persisted, and only
  when no confirmed failure exists (one confirmed report plus fluke
  chatter is caveat fatigue).
- *Confirmed*: past the bar (@sec:bar), carrying an anchor, a witness run
  for the shrinker to start from, and a pool. Shrunk under the gauntlet
  (@sec:gauntlet), persisted, reported with a reproduction token.
- *Trusted*: reproduced from a stored database entry. The prior run's bar
  is not re-run — re-barring real $p approx 0.1$ bugs would drop 55% of
  them — but the origin is still measured, and any shrink-time failure
  promotes it to Confirmed.

@tab:flip summarises what the flip changes. Under ND handling the verdict
cache never serves (a cached verdict is exactly the bias the multi-run
machinery exists to avoid), a raw interesting run may fill a vacant origin
but never displaces an occupied one, and every irreversible action —
displacement, anchor movement, persistence — happens only at _validated
events_: a bar accept, or a gauntlet accept that the shrinker actually
adopts. Failures report as plain failures with a per-failure caveat
quoting the run's own measurements (@fig:report). No rates or counters are
ever persisted, so every run's claims stand on its own replays.

#figure(
  placement: top,
  table(
    columns: (auto, 1fr, 1.35fr),
    align: left,
    toprule,
    table.header([], [*deterministic run*], [*under ND handling*]),
    midrule,
    [believe a failure],
    [first sighting is the example],
    [sighting fills a vacant origin only; the discovery bar decides
      admission (@sec:bar)],
    [shrink accept],
    [one failing replay, strictly smaller],
    [gauntlet: Wilson LCB of candidate evidence clears the anchor-priced
      threshold, min. 4 failures (@sec:gauntlet)],
    [shrink stop],
    [passes reach a fixed point],
    [confirmed-dry: a final sweep drives every candidate to a bound
      verdict (@sec:gauntlet)],
    [dedup / caching],
    [verdicts served from cache],
    [never served; every replay executes],
    [pre-report check],
    [one exact replay; miss = flake],
    [replay-until-failure over the pool, splices, fresh cases
      (@sec:repro)],
    [persistence],
    [best failing sequence],
    [pool of timelines + replay parameters, at validated events only
      (@sec:repro)],
    [report],
    [values + reproduction token],
    [same, plus a caveat quoting this run's replay counts],
    botrule,
  ),
  caption: [What changes when a run flips into ND handling.],
) <tab:flip>

= The statistical machinery <sec:stats>

== Detection <sec:detect>

Four observational channels set the flag. All are within-run, because
between-run divergence from a stored entry overwhelmingly means the code
changed, not that it is flaky.

*Verdict-flip cache.* Every executed conclusion is fingerprinted by its
realised values. A repeat that concludes with a different status or origin
is direct evidence — the channel that catches outcome nondeterminism
during generation.

*The first-interesting check.* Before anything consumes a newly discovered
origin — before shrinking, persistence, or even belief — its sighting is
replayed exactly, up to four times, stopping at the first miss. A bug
failing at rate $p$ whose replays survive structural-divergence at rate
$s$ escapes detection with probability $(p s)^4$. A deterministic failure
pays exactly four extra executions, and a run that finds no failure pays
nothing. This check exists because the sighting is selection: spending
four replays _before_ the run commits to deterministic handling is what
makes the commitment safe. A miss flips the run, and the four
observations are credited to the origin's confirmation bar so they are
not paid for twice.

*Replay checks.* The pre-shrink verify and the pre-report final replay —
the sites where Hypothesis-style engines abort — flip the run instead
(under `error` strictness they still abort).

*Stored state.* A persisted nondeterministic entry or reproduction token
is self-identifying (@sec:repro). Decoding one flips the run before any
replay, so a rerun of a known-flaky test never re-pays detection.

In measurement (@sec:eval), every flip on genuinely racy bodies landed at
the first-interesting check: four exact replays are ample when structure
varies. The channels matter jointly, though — on outcome-only
nondeterminism the check can pass and the cache or replay channels fire
later, and the run must recover (@sec:seam).

== Confirmation: the discovery bar <sec:bar>

Once a run is in ND handling, an interesting execution is a sample, not a
fact. The _discovery bar_ decides origin admission: replay the sighting's
timeline repeatedly and

- *reject* if the first 10 replays all pass;
- otherwise continue to at most 40, *accepting* on the 4th failure;
- *reject* when 4 failures become unreachable.

The rule was chosen by exact dynamic programming over (runs, fails)
states, evaluating candidate rules — flat $k$-of-$N$, Wald's SPRT @wald,
a Wilson accept/reject pair, two-stage gates — against an asymmetric loss
(@app:bar). The asymmetry is what the design exploits: a
false _accept_ is sticky (the fluke occupies the origin, seeds the
shrinker's anchor with garbage, and the no-displacement rule then protects
it for the rest of the run), while a false _reject_ recycles (generation
keeps running, the same bug is re-sighted, and a fresh bar batch runs).
Per-discovery power is therefore cheap and false accepts are expensive:
the chosen rule accepts a $p = 0.02$ fluke 0.6% of the time per batch and
a $p = 0.1$ target bug 45% of the time — which compounds past 95% within
five sightings, while a rejected fluke costs about 15 replays. An SPRT tuned
for one-shot power spends 52 replays per fluke to buy power that
recycling provides for free.

Two rules keep the bar honest. An accepting batch must contain its own
reproducing run (a batch pre-credited with first-check evidence cannot
confirm an origin it never saw fail), and that run becomes the _witness_
the shrinker starts from. And an accepting batch is _extended_ to 20
replays before its Wilson lower bound may seed the anchor: stopping at
the accept itself would make the anchor an estimate of the stopping rule
rather than the rate (four straight failures would seed $0.51$ whatever
the truth). Twenty is the largest extension whose all-fail bound
($"LCB"(20\/20) = 0.839$) a shrink candidate can still match within the
gauntlet's replay cap. Extending to 40 provably stalls shrinking
(@app:gauntlet).

== Shrinking: the gauntlet <sec:gauntlet>

The shrinker's search machinery — passes, scheduling, the shortlex order —
is untouched. What changes is the probe that judges a candidate, governed
by the retention contract: _reduction must not lower the reported
example's failure probability_.

*Charge accepts, not rejects.* A candidate whose first run passes is
rejected at the cost of that one run. This keeps the search's cost profile
close to deterministic shrinking — most candidates fail to reproduce and
are discarded cheaply — and is safe because a false reject only costs
minimality: the pass machinery retries rejected transformations, and each
candidate's evidence accumulates in a per-candidate _ledger_ (keyed by
realised values, so a proposal that punned into another realization merges
with it) rather than resetting per attempt.

*A candidate that fails must clear the gauntlet.* Its ledger is replayed
until a sequential verdict: *accept* when it holds at least 4 failures
_and_ its Wilson lower bound clears
$max(gamma dot "anchor", 0.05)$; *reject* when its upper bound proves
the threshold unreachable, or at 30 runs. The floor 0.05 is derived, not
chosen: it sits just below $"LCB"(4\/30) = 0.0531$, the minimum-failures
acceptance boundary at the cap, so it costs no power while refusing
noise-floor candidates. $gamma = 0.8$ trades a bounded reliability loss
per accepted step for reachability. At anchors $>= 0.8$ — reachable only
by zero-miss evidence — $gamma$ becomes 1.0, so an incumbent
indistinguishable from deterministic is never traded down. The minimum of
4 failures exists because a fresh ledger's single failure has
$"LCB"(1\/1) = 0.2065$: without the minimum, every threshold below that
accepts on the recruiting run, and the whole target regime silently
degenerates to the single-run trust of @sec:problem. The shipped system
had exactly this calibration failure, caught by simulation
(@sec:discussion).

*The anchor is monotone.* It rises only at validated events — a bar
accept, or the first adoption of a gauntleted candidate (whose ledger is
first topped up to 20 runs, the same de-biasing as the bar's extension) —
and never falls, and re-measurement of the standing incumbent never feeds
it. Anchor decay and checkpoint/rollback schemes were both evaluated in
simulation and rejected: decay makes stopping incoherent (51–75% missed
reductions), rollback-on-uncertainty poisons stable landscapes, and
rollback-on-proof never fires. Monotonicity is also what makes estimate
errors safe: an anchor that reads high prices candidates too high, which
costs minimality, never failure probability.

*Stopping carries a certificate.* Under fast sweeps a candidate can be
rejected by one unlucky replay, so a fixed point is not evidence of
exhaustion. When a sweep goes dry, one _confirmation sweep_ re-proposes
everything and drives every ledger to a bound verdict. Only a confirmation
sweep that accepts nothing ends the shrink. In simulation this halves the
missed-reduction rate of fixed dry-sweep counts at equal cost.

*Boost.* Before shrinking a confirmed origin whose anchor is below 0.30
($= "LCB"(10\/20)$, the estimator's image of "true rate below 0.5"), the
engine races the incumbent, its pool, and prefix-cut mutants by
successive halving, adopting a steadier starting timeline only if a fresh
20-replay holdout beats the anchor — the "raise it when cheap" arm of the
retention contract, holdout-gated because in-race winners are selection-
biased upward. The same holdout-gated race design replaces hill-climbing
for targeted PBT @targeted-pbt under ND handling, where the recorded
maximum of a noisy score is inflated by about 1.7 standard deviations and a
strict-improvement climber freezes against its own luck in 92–100% of
trials.

== Multiplicity control <sec:multiplicity>

Each rule above has a calibrated per-test error, but a run repeats the
tests without bound: an evicted fluke origin is re-sighted and gets a
fresh bar batch (a $q = 0.02$ fluke recycled across a long run confirms
21% of the time); one shrink was measured realising 42,000 distinct
candidate timelines (at $4 times 10^(-4)$ false-accept each, unbounded
composition reaches 33% per thousand floor-threshold proposals); and the
pre-report replay of a still-unconfirmed origin, if allowed to confirm on
any single failure among its roughly 40 replays, would admit a fluke half the
time. Classical corrections @benjamini-hochberg do not apply: there is no
p-value family to rank, because verdicts act immediately and irreversibly
(anchors rise, entries persist, shrinks start). Control must be _online_
@alpha-investing: budgets fixed before the tests run.

Three budgets, all per-origin per-run (derivations and tables in
@app:multiplicity):

+ *Bar attempts are capped at 5* (the sweep, shrink admission, and the
  pre-report review share them; the seam recovery of @sec:seam holds a
  separate 3). At the cap a sighting is evicted without a batch. The cap
  pins the per-origin false-confirm at the 2.9% the bar's derivation
  assumed (4.6% with the separate backtrack budget) and keeps $>= 95%$
  power at the target rate.
+ *Gauntlet proposals spend an alpha budget of 0.02.* Every proposal on an
  unbound ledger is charged, before it runs, its _exact_ false-accept mass
  against a $q = 0.02$ fluke — a dynamic program over its ledger state and
  current threshold — so by linearity the expected false accepts per
  origin stay under budget however many candidates the body realises.
  Exact charging keeps measured regimes free (an unreachable high-anchor
  threshold charges zero; mid-anchor thresholds charge $approx 10^(-6)$ and
  afford thousands of proposals); when the remainder cannot afford a new
  ledger, the failure minimum escalates $4 -> 8$, for new ledgers only —
  a test's stopping rule never changes once it has begun.
+ *The pre-report review confirms through a standard bar batch*, spending
  one capped attempt, rather than on any single failure. This cuts the
  fluke-confirm rate of that path by about 170× (0.49 → 0.003), at a real
  power cost (0.97 → 0.44 at the target rate before the seam recovery of
  @sec:seam) — the failing execution still reaches the user as a
  values-carrying caveated report when the batch rejects.

The budgets' power costs are deliberate and priced: a below-target bug
($p = 0.05$) now confirms in about 42% of runs rather than almost surely given
a long one, leaning on cross-run recycling; an origin whose sightings mix
a real bug with fluke timelines pays most (bug-confirm 0.95/0.72/0.45 at
fluke share 0/0.5/0.75); and a shrink that exhausts its alpha budget at
the floor threshold both accepts less and stops earlier. Every cost lands
on the conservative side of the retention contract: a refused candidate
or confirmation keeps the incumbent and loses no failure.

== Recovering from a late flip <sec:seam>

The single largest loss mechanism we encountered was not any statistical
rule but the _seam_ between the modes. Detection is sparse, so a run can
spend its whole generation budget under deterministic trust before
flipping: by then, single-run shrink accepts have already walked the
incumbent down the landscape (on a rising landscape the incumbent's
failure probability at flip time was 0.26 at every percentile), and a
freshly flipped run has no generation budget left for the recycling the
bar's 45% power assumed. In the pre-fix measurement, 49% of target-regime
runs failed with no reported counterexample at all. Three mechanisms close
the seam, all of them making
pre-flip actions recoverable rather than detecting earlier:

- the first-interesting check (@sec:detect), which spends its four replays
  _before_ deterministic trust can consume a discovery;
- a per-origin _history_ of every pre-flip interesting execution (raw
  sightings and shrink accepts, deduplicated, dropped on confirmation);
- _backtracking_: a never-confirmed origin that misses its verify or
  final replay scans its history for the newest entry that still
  reproduces — geometric probes then binary refinement, 40 replays total,
  biased old under uncertainty because the retention contract makes an
  over-old restore safe (it re-shrinks under the gauntlet) — and the
  settled candidate must clear the full discovery bar on its own separate
  3-attempt budget. History skews towards the real bug's pre-flip
  sightings, which is why this budget is not shared with the sweep's.

After these landed, the no-counterexample rate on the target-regime
landscape fell from 49% to zero and the rising landscape's median final
failure probability doubled (0.34 → 0.74) — at the cost that the whole
shrink now runs gauntleted (1.54× executions), since the flip happens at
discovery rather than after the damage.

== Reproduction and persistence <sec:repro>

Nothing estimated is ever stored — no rates, no counters, no flakiness
flags. What persists is the _representation_: the incumbent plus its pool
of up to 10 failing timelines, an entropy seed, and a continuation budget,
serialised as a self-identifying versioned entry used for both the failure
database and the reproduction token (an old reader rejects it loudly
rather than misreading it). Every run's decisions then stand on its own
replays, and CI configurations with no database lose reuse and nothing
else.

Reproduction is _replay-until-failure_. Stored timelines replay first-fit,
each under a continuation budget of its length plus $max(4, "len"\/8)$
fresh draws (replay tolerates structural divergence rather than aborting
on it, and a diverged replay is still a trial). If the whole pool misses,
10 _splices_ — random cross-pairings of stored timelines, cut at a random
position — recombine them. Whole-timeline pools plateau at about 72%
reproduction on adversarial structure-shifting bodies, and splicing
rescues 65–100% of the residual misses. At the pre-report replay only, a few
fresh generated cases run last. The per-timeline replay budget is derived
from the target: $ceil(ln 0.05 \/ ln 0.9) = 29$ replays leave a
$p = 0.1$ bug a $<= 5%$ escape chance, where a flat "replay 10 times"
would miss it 35% of the time. Callers stop at the first failure, so a
live bug costs $approx 1\/p$ replays and the full budget is paid only for stale
entries. Database hygiene follows the same logic across runs: a stored
entry that misses its full budget is demoted to a secondary corpus
(strike one) and deleted only after a second budgeted miss.

The report itself is caveated from the run's own counts (@fig:report):
"confirmed: failed 18 of 20 replays at confirmation and 1 of 4 at report
time", "reproduced from stored timelines: …", or, for a failure that
never cleared the bar, "unconfirmed: failed 0 of 3 replays after the
observed failure — a rare failure, or the environment changed between
executions". A confirmed failure that cannot be reproduced at report time
switches its caveat wording rather than going unreported: the run measured
it, and the wording honestly admits that a very rare failure and an
environment change are indistinguishable.

= Evaluation <sec:eval>

The statistical rules were _derived_ on exact dynamic programs and
pure simulation (appendices A–C), then validated in-engine. Here we
evaluate the composed system. All numbers in this section were measured
for this paper on the final engine (commit `9f1eb7c6`; Apple M5 Pro;
harness and seeds shipped with the implementation), on two suites:

*Landscapes* (via the C ABI): bodies draw a list of up to 20 integer
"atoms" and fail with a probability determined by the drawn values —
L1 _rising_ ($p$ grows with input size from a 0.1 floor, ≥3 bug atoms
required, the shrink-quality stress), L3 _constant_ ($p = 0.5$),
L4 _noise-floor_ (bug at 0.9 over 0.02 background), L4b _target-regime_
(bug at exactly the 0.1 design floor over 0.02 background), D2
_deterministic-core_ (a $p = 1$ core inside a $p = 0.7$ flaky region),
N0 _noise-only_ (every execution fails at 0.02; no bug), and D0, a fully
deterministic control. 100 runs per cell, 500-case budget, two arms:
the engine's default, and `error` strictness — the
determinism-as-invariant posture of @sec:problem on identical detection.

*Episodes* (via the Rust frontend): racy bodies with a hidden
schedule-noise generator that perturbs draw structure run to run — a
clone-stream body and a two-worker state-machine-shaped body — failing at
a schedule-independent rate $p$. Each episode is a discovery run (100
cases), then a database-reuse-only run, then a reproduction-token run in
a fresh engine. 200 episodes per cell. Cells at
$p in {0.05, 0.1, 0.3, 0.9}$ for the clone body and
$p in {0.1, 0.3, 0.9}$ for the machine body, plus a deterministic failing
control and an all-passing control. Body definitions in @app:setup.

#figure(
  placement: top,
  scope: "parent",
  table(
    columns: (auto, auto, auto, auto, auto, auto, auto, auto),
    align: (left, center, center, center, center, center, center, center),
    toprule,
    table.header(
      [landscape],
      [*strict:* counterexample],
      [degraded],
      [*default:* confirmed + shrunk],
      [caveat-only],
      [bug kept],
      [final $p$ (p10/p50/p90)],
      [median execs],
    ),
    midrule,
    [L1 rising], [4/100], [4/4], [100/100], [0], [99/100],
    [0.26 / 0.74 / 0.82], [17,510],
    [L3 constant 0.5], [0/100], [—], [100/100], [0], [98/100],
    [0.50 / 0.50 / 0.50], [2,511],
    [L4 noise-floor 0.9], [0/100], [—], [100/100], [0], [100/100],
    [0.90 / 0.90 / 0.90], [1,753],
    [L4b target 0.1], [0/100], [—], [98/100], [2], [98/98],
    [0.10 / 0.10 / 0.10], [12,641],
    [D2 det-core], [20/100], [0/20], [100/100], [0], [99/100],
    [0.70 / 1.00 / 1.00], [1,712],
    [N0 noise-only], [0/100], [—], [5/100 (false)], [95], [n/a],
    [0.02 / 0.02 / 0.02], [590],
    [D0 deterministic], [100/100], [0/100], [100/100], [0], [100/100],
    [1.00 / 1.00 / 1.00], [708],
    botrule,
  ),
  caption: [
    Landscape suite, 100 runs per cell. _strict_ aborts on any detected
    nondeterminism (the surrender posture); _default_ is the statistical
    engine. "Degraded" counts strict-mode counterexamples that report a
    lower-probability example than the default arm's median. "Bug kept"
    checks the reported example against the landscape's ground truth. D0
    is byte-identical across arms.
  ],
) <tab:landscapes>

*RQ1 — usefulness: does a flaky failure produce an actionable report?*
@tab:landscapes: under strict determinism, every genuinely flaky landscape
yields a counterexample in 0–4% of runs (D2's 20% are runs that reached
the deterministic core before any replay check fired; L1's 4 survivors
report examples degraded to $p = 0.26$, the noise-walked incumbents of
@sec:problem). The statistical engine reports a confirmed, shrunk,
token-carrying counterexample in 98–100% of runs on every landscape with
a real bug, and a caveated failure otherwise. It never converts a failing
run into a silent pass.

*RQ2 — soundness: what does noise buy?* On N0, where every failure is a
$p = 0.02$ fluke and there is nothing to find, 95/100 runs report a
caveated unconfirmed failure — the designed behaviour: the run did fail,
and the caveat says the failure did not reproduce — and 5/100 falsely
confirm, consistent with the derived per-origin ceiling of 4.6%
(@app:multiplicity; N0 re-sights one origin all run, the worst case).
Even a false confirm quotes its own replay counts ("failed 4 of 40
replays"), so the report is visibly weak. The same arithmetic bounds
per-origin false confirmation on every landscape.

*RQ3 — retention: does shrinking keep the bug?* Across all landscape
cells the reported example still contains the ground-truth bug in
98–100/100 runs (@tab:landscapes) — against 3/100 for single-run
acceptance measured on the same noise-floor bodies, and 34–100% loss in
simulation (@sec:problem). On the rising landscape, where minimising size
fights retention directly, the median reported example fails at
$p = 0.74$ against the 0.10 floor a probability-blind reducer converges
to. D2 shows the honest trade: the engine reports the deterministic core
in most runs (median final $p = 1.0$) but a confirmed $p = 0.7$ example
in the rest, where free displacement would sometimes have lucked into
the core — the price of refusing single-run trust.

#figure(
  placement: top,
  table(
    columns: (auto, auto, auto, auto, auto, auto, auto),
    align: (left, ..(center,) * 6),
    inset: (x: 3pt, y: 2.6pt),
    toprule,
    table.header(
      [body],
      [$p$],
      [conf.],
      [caveat],
      [DB reuse],
      [token],
      [replays p50 (max)],
    ),
    midrule,
    [clone], [0.05], [74/200], [126], [64/74], [65/74], [109 (2.0M)],
    [clone], [0.1], [183/200], [17], [176/183], [179/183],
    [1.7M (6.4M)],
    [clone], [0.3], [197/200], [3], [197/197], [197/197],
    [0.77M (2.9M)],
    [clone], [0.9], [200/200], [0], [200/200], [200/200],
    [0.60M (1.5M)],
    [machine], [0.1], [179/200], [21], [175/179], [178/179],
    [1.7M (4.7M)],
    [machine], [0.3], [199/200], [1], [199/199], [199/199],
    [1.1M (3.0M)],
    [machine], [0.9], [200/200], [0], [200/200], [200/200],
    [0.76M (2.5M)],
    [det], [1], [200/200], [0], [200/200], [200/200], [4 (4)],
    [pass], [0], [—], [—], [—], [—], [0 (0)],
    botrule,
  ),
  caption: [
    Episode suite, 200 episodes per cell. Every failing episode's
    discovery run reported a failure. "Database reuse" and "token replay"
    count reproduction of confirmed failures in a following run and in a
    fresh process. Measurement replays are per episode; M is millions.
  ],
) <tab:episodes>

*RQ4 — reproduction.* On the racy episode suite (@tab:episodes) every
discovery run reported a failure, every flip landed at the
first-interesting check, and every persisted nondeterministic entry used
the self-identifying versioned format. At $p = 0.1$, the design floor,
90–92% of episodes confirm; at $p >= 0.3$, 98–100%. Confirmed failures
reproduce from the database in 96–100% of reuse runs and from the
reproduction token, in a fresh process, in 98–100%. The below-target
$p = 0.05$ cell confirms 37% of episodes — the multiplicity budgets'
price (@sec:multiplicity) — and reports caveated unconfirmed failures
otherwise, and its confirmed failures still reproduce in 86–88%.

*RQ5 — what do deterministic tests pay?* The deterministic failing
control pays exactly 4 measurement replays per run (the first-interesting
check) and is otherwise byte-identical under both arms, storing and
replaying the legacy deterministic entry format. The all-passing
control pays zero replays and never flips. The cost lands on the
genuinely nondeterministic bodies, and there it is heavy: the median
episode at $p = 0.05$ paid 109 measurement replays (unconfirmed origins
never shrink), while cells at $p >= 0.1$ paid 0.6–1.7 million, peaking
at the design floor (maximum 6.4 million). These bodies are the
gauntlet's worst case: every candidate fails at the same rate, so a
floor-threshold ledger can neither accept quickly nor reject early
($"UCB"(0\/30) = 0.11$ never falls below the 0.05 floor) and the
confirmation sweep drives nearly every distinct candidate to the 30-run
cap. The replays are cheap on these bodies, but a slow body would hit
its wall-clock deadline and stop early with a valid, larger example
(@sec:discussion).

*Threats.* All bodies are synthetic, with i.i.d.-coin outcome noise and
seeded structural noise, where real races have correlated,
input-dependent failure probabilities. The landscapes are, however,
adversarial by construction (noise floors, deterministic cores,
size-coupled probability) in ways sampled real suites would not be. The strict arm
shares the final engine's detection, which is stronger than the shipped
baselines it stands in for (measured directly: the unmodified engine
aborted 29–69% of runs and single-run shrinking kept 3/100 bugs — both
worse than the strict arm shown). Constants were derived and validated on
the same landscape families. The episode bodies and the N0/pass cells are
held out from every derivation. Single machine, fixed seeds, 100–200
trials per cell: proportions carry $plus.minus$ 3–5% at 95% confidence.

= Discussion: what failed on the way <sec:discussion>

Three lessons cost the most and seem most likely to transfer.

*Individually sound sequential rules composed back into the naive
policy.* The bar, the anchor, and the gauntlet were each derived in
isolation and each correct in isolation. Composed, the bar's honest
low-regime anchors ($p = 0.1$ seeds a median anchor of 0.066) set
gauntlet thresholds below $"LCB"(1\/1) = 0.2065$ — so every candidate
accepted on its recruiting failure, and the shipped system silently
reproduced single-run trust across the entire target regime, losing 33%
of target bugs in simulation while every unit test passed (two of them
pinning the degenerate behaviour as intended). The fix (a minimum failure
count, de-biased anchor seeding, a derived floor) is three constants;
the lesson is that the composition, not the components, is the object of
calibration — and that only an end-to-end adversarial measurement
(here, a simulation re-deriving the whole pipeline's operating points)
sees it.

*The seam dominates the statistics.* Most measured loss came not from any
statistical rule but from the boundary where deterministic trust hands
over to ND handling (@sec:seam): pre-flip single-run decisions acting
irreversibly on evidence the flip later invalidates. Mode boundaries in
adaptive systems deserve the same adversarial measurement as the modes.

*Estimates contaminated by their own selection process recur
everywhere.* The discovering run (selected for failing), the in-race
boost winner (selected for winning), the recorded maximum of a noisy
target score, an anchor seeded at the accept (selected by the stopping
rule): each looked usable and each biased a downstream decision until
moved to a validated, selection-free measurement. "A sighting is
selection, not evidence" ended up enforced at six separate sites.

*Limitations.* Shrink cost on bodies whose every candidate fails at the
test's own rate is measured in millions of replays per run, worst at the
design floor where no candidate can be cheaply rejected (@sec:eval), and
a slow body would hit the wall-clock deadline and stop early with a
valid, larger example. Any fix trades against the retention contract and
is left open. Below-target bugs ($p < 0.1$) get honest caveated reports
but confirm in well under half of runs (42% derived, 37% measured at
$p = 0.05$) — the multiplicity budgets' deliberate price. Failure identity is the panic site, which cross-thread
panic plumbing can collapse into one unknown origin. And schedules are
sampled, never controlled: the engine cannot promise to re-trigger a
race, only to keep honestly measuring whether it does.

= Related work <sec:related>

*PBT and shrinking.* QuickCheck @quickcheck established generate-and-
shrink. Hypothesis @hypothesis moved shrinking onto recorded choice
sequences @reducer, the architecture Hegel implements and most modern
engines (proptest @proptest and others) share. All inherit the
determinism invariant: Hypothesis documents it and aborts on violation;
QuickCheck-family reducers silently mis-shrink. Targeted PBT
@targeted-pbt guides generation by a score, and we show its hill-climbing
assumes deterministic scores and give a race-based replacement. PULSE
@pulse randomises Erlang scheduling to make races findable by QuickCheck
and reduces with a custom shrinker under a user-supplied similarity
relation. It controls the scheduler. We assume no such control and
handle the residual nondeterminism statistically.

*Flaky tests.* Large-scale studies established flakiness as pervasive
@flaky-empirical @harman-ohearn; DeFlaker @deflaker and iDFlakies
@idflakies detect and classify flaky unit tests by re-running under
instrumentation. That line treats flakiness as a defect to be triaged;
we treat probabilistic failure as an operating condition for the test
tool itself, and use replays not to label the test but to confirm,
minimise, and reproduce its failures. The rerun-to-confirm intuition is
folklore (retry-on-red). The contribution here is deriving how many
replays, in what sequential rule, against what loss, and bounding the
compounding.

*Reduction under unreliable oracles.* Delta debugging @ddmin and
C-Reduce @creduce reduce with re-executed interestingness tests, and
practitioners routinely bolt retry heuristics onto them for flaky
oracles; Choi and Zeller reduced failure-inducing _schedules_ given a
deterministic replayer @schedule-isolation. We are not aware of prior
reducers with an explicit statistical retention guarantee (the reported
example's failure probability is not lowered), a certificate-carrying
stopping rule, or alpha-spending across candidates.

*Making the world deterministic.* Systematic concurrency testing
@chess @pct, record-replay @rr, and deterministic-simulation platforms
@foundationdb @antithesis attack the same problem from the other side:
control or capture every nondeterministic input so single-run trust
becomes valid again. Where such control is available it is strictly
stronger; the machinery here targets the everywhere-else — ordinary test
processes with OS scheduling, real time, and real dependencies — and the
two compose (a deterministic environment simply never flips the engine).

*Statistics.* The sequential rules are bespoke compositions of classical
pieces: Wilson bounds @wilson as the interval, exact dynamic programming
in place of SPRT asymptotics @wald (our budgets are small and the DP is
cheap, so operating points are exact), and online alpha-spending in the
spirit of Foster and Stine @alpha-investing rather than batch FDR control
@benjamini-hochberg, because verdicts act immediately and irreversibly.

= Conclusion

The determinism invariant made replay-driven PBT possible. It also made
the tooling most brittle exactly where testing is hardest. Treating every
execution as a Bernoulli trial — and rebuilding confirmation, shrinking,
stopping, reproduction, and reporting as sequential decisions with
derived budgets — recovers the whole toolchain for nondeterministic
tests: shrunk counterexamples with a statistical retention guarantee,
reports whose every claim is backed by that run's own replays, and
reproduction artefacts that expect to need more than one try. The
engineering is a few thousand lines; the design content is the loss
functions, the budgets, and the places single-run trust hides. Heisenbugs
@heisenbug have been getting worse for forty years. Test tools no longer
need to treat them as someone else's problem.

#bibliography("refs.yml", style: "association-for-computing-machinery")

#counter(heading).update(0)
#set heading(numbering: "A.1", supplement: [Appendix])

= The discovery bar <app:bar>

*Evidence and intervals.* Evidence is a pair $(f, n)$: failures over
replays. Bounds are Wilson score bounds @wilson at $z = 1.96$ on
$hat(p) = f\/n$:

$
  "bound"(f, n) = (hat(p) + z^2 / (2n) plus.minus z sqrt((hat(p)(1 -
  hat(p)) + z^2 \/ (4n)) \/ n)) / (1 + z^2 \/ n),
$

clamped to $[0, 1]$; $"LCB"$ takes the minus branch. Reference values
used throughout: $"LCB"(1\/1) = 0.2065$, $"LCB"(4\/30) = 0.0531$,
$"LCB"(10\/20) = 0.299$, $"LCB"(20\/20) = 0.839$,
$"LCB"(30\/30) = 0.887$, $"LCB"(40\/40) = 0.912$.

*Setting.* An origin has been sighted; replays of its timeline are
modelled i.i.d. Bernoulli($p$). Design points: the target rate
$p >= 0.1$; a background fluke rate $q = 0.02$ taken from the noise-floor
landscapes. The loss is asymmetric because a false accept is sticky
(occupies the origin behind the no-displacement rule, mis-seeds the
anchor) while a false reject recycles through rediscovery, so run-level
power compounds across sightings while run-level false-accept compounds
against the user.

*Method.* Every candidate rule is evaluated by exact forward dynamic
programming over states $(n, f)$: propagate probability mass under
$(n, f) -> (n+1, f+1)$ w.p. $p$ else $(n+1, f)$, checking the
rule's verdict after every replay; accumulate accept mass and
$E["replays"]$. No simulation error. Selected rows (the full table
regenerates from the shipped harness in seconds):

#figure(
  table(
    columns: 9,
    align: (left, ..(center,) * 8),
    toprule,
    table.header(
      [rule],
      [$P("acc")$\ $p = .02$],
      [$.05$],
      [$.1$],
      [$.2$],
      [$E[n]$\ $p = .02$],
      [$.1$],
      [$.9$],
      [run-level\ false acc.],
    ),
    midrule,
    [flat 2-of-20], [.060], [.264], [.608], [.931], [18.9], [14.7],
    [2.2], [.266],
    [flat 4-of-30], [.003], [.061], [.353], [.877], [27.5], [26.4],
    [4.4], [.014],
    [gate 1/10, 3-of-30], [.016], [.143], [.476], [.871], [13.4],
    [17.3], [3.3], [.077],
    [*gate 1/10, 4-of-40*], [*.006*], [*.102*], [*.454*], [*.877*],
    [*15.2*], [*23.0*], [*4.4*], [*.029*],
    [SPRT (.01,.05) cap 80], [.022], [.359], [.899], [1.00], [51.9],
    [48.6], [3.3], [.104],
    [Wilson pair cap 40], [.261], [.651], [.931], [.999], [31.3],
    [13.9], [1.1], [.780],
    botrule,
  ),
  caption: [
    Exact operating characteristics of candidate confirmation rules.
    Run-level false accept is at $F = 5$ fluke exposures. The chosen rule
    is bold.
  ],
) <tab:bar-rules>

*The chosen rule* — reject on 0 failures in 10; else continue to 40,
accepting early on the 4th failure, rejecting when 4 becomes
unreachable — dominates: the cheap gate dismisses 82% of flukes for 10
replays each, and re-spending the savings on a 40-replay ceiling buys
more power at comparable false-accept than any flat rule. The Wilson
accept/reject pair is the wrong shape for confirmation (two early
failures clear a small floor long before the evidence separates 0.02
from 0.1); the SPRT @wald buys one-shot power at 52 replays per fluke,
which recycling makes a bad trade.

*Witness and seeding.* An accepting batch must contain a reproducing run
(evidence credited from the first-interesting check may otherwise fill
the quota) and extends to 20 replays before its LCB seeds the anchor,
removing stopping-rule bias: an un-extended 4-of-4 accept would seed
0.51 regardless of $p$. Exact miscoverage of the seeded anchor,
$P("anchor" > p | "accept")$, is 9.4% at $p = 0.1$ against the 2.5%
nominal, falling to 1.8% by $p = 0.3$, with the mean anchor at or below
truth everywhere (0.066 at $p = 0.1$) — optimistic-high anchors only
price candidates too high, the conservative direction, so the residual
selection bias is absorbed rather than corrected.

= Gauntlet calibration <app:gauntlet>

*Verdict rule.* For candidate evidence $e$ against anchor $a$, with
threshold $T = max(gamma(a) dot a, 0.05)$ and $gamma(a) = 0.8$ for
$a < 0.8$, else $1.0$: accept iff $e."fails" >= m$ and
$"LCB"(e) >= T$; reject iff $"UCB"(e) < T$ or $e."runs" >= 30$;
otherwise replay again. Base $m = 4$, escalated to at most 8 by the
alpha budget (@app:multiplicity). A fresh candidate is charged one run
for a passing first replay (fast reject); ledgers persist across pass
retries, and only re-proposals accumulate evidence.

*Degeneracy of the unguarded rule.* With no failure minimum, a single
failure bounds the rate above $"LCB"(1\/1) = 0.2065$, so any threshold
at or below 0.2065 accepts on the recruiting run. Bar-seeded anchors sit
inside that zone across the target regime (median seeds: 0.061 at
$p = 0.1$, 0.138 at 0.3, 0.250 at 0.5), so the shipped rule was
single-run acceptance in disguise; simulation measured 33% target-bug
loss, matching the naive policy of @sec:problem.

*The minimum-failures ladder*, measured on the target-regime landscape
(bug kept / cost versus the degenerate rule): $m = 1$: 51% / 1.00×;
$m = 2$: 73% / 1.37×; $m = 3$: 97% / 2.60×; $m = 4$: 100% / 3.01×.
$m = 4$ costs 1.00× on every landscape outside the target regime, so
its entire cost and value are concentrated exactly where it matters.
Worst-case false accept at the floor: $4.0 times 10^(-4)$ per proposal
against a $q = 0.02$ fluke.

*Derived floor.* $0.05$ is the largest floor below the $m = 4$
acceptance boundary at the run cap, $"LCB"(4\/30) = 0.0531$ — so it
costs zero power (at 0.08, power falls to 0.62 of ceiling).

*Anchor seeding at 20.* A candidate at the cap can reach at most
$"LCB"(30\/30) = 0.887$. Seeding anchors from 40-run batches
($"LCB"(40\/40) = 0.912 > 0.887$) makes deterministic incumbents
unmatchable and stalls those shrinks outright; 20-run seeding
($"LCB"(20\/20) = 0.839$) is the largest extension a candidate can
still match. The retention high-water 0.8 is reachable only by
zero-miss 20-run evidence ($"LCB"(19\/20) = 0.764$), making $gamma = 1$
a detector for effectively deterministic incumbents rather than a dial;
it converts 33% displacement of deterministic incumbents to zero at
+26% cost on the rising landscape.

*Simulated envelope of the composed rule* (500 seeds/cell): rising-
landscape final failure probability median 0.82 (p10 0.58) against a
0.10 floor; target-regime bugs kept 100% at 2,831 median executions;
100% kept on every landscape. The confirmed-dry stopping rule (one
confirmation sweep driving every ledger to a bound verdict) halves
missed reductions versus fixed dry-sweep counts (18% → 10% on the
constant-0.5 landscape) at the cost of roughly three dry sweeps;
checkpoint/rollback alternatives measured strictly worse
(rollback-on-uncertainty: 9–52% missed reductions at 2.3× cost;
rollback-on-proof: never fires) and anchor decay traded 1–2 length
units for 51–75% missed-reduction rates.

= Multiplicity control <app:multiplicity>

*Bar attempts.* Per-batch false accept $alpha = 0.0059$ composes over
attempts $F$ as $1 - (1 - alpha)^F$; power at $p = 0.1$ composes as
$1 - (1 - 0.454)^F$:

#figure(
  table(
    columns: 3,
    align: (center, center, center),
    toprule,
    table.header(
      [attempts $F$],
      [$P("false confirm")$, $q = .02$],
      [$P("confirm")$, $p = .1$],
    ),
    midrule,
    [1], [0.006], [0.454],
    [3], [0.018], [0.837],
    [5], [0.029], [0.951],
    [8], [0.046], [0.992],
    botrule,
  ),
  caption: [Exact composition of bar attempts.],
) <tab:attempts>

Uncapped recycling is unbounded in run length: Monte Carlo over sweep
epochs (100k episodes) confirms an uncapped $q = 0.02$ origin 4.6% of
the time by 40 epochs and 20.9% by 200; the 5-attempt cap pins both at
2.9%. The separate 3-attempt backtrack budget raises the per-origin
ceiling to $1 - (1 - 0.0059)^8 = 4.6%$. Priced costs: a $p = 0.05$ bug
falls from near-certain confirmation (given a long run) to 42% per run;
a mixed origin whose sightings land on a fluke timeline with share
0/0.25/0.5/0.75 confirms its bug at 0.95/0.87/0.72/0.45.

*Gauntlet alpha-spending.* Each proposal on an unbound ledger is
charged, before running, its exact false-accept mass against a
$q_0 = 0.02$ fluke: with $A(e, T, m)$ the DP accept-mass of the
evidence loop from ledger state $e$ at threshold $T$ and minimum $m$,
a fast-sweep proposal charges $q_0 dot A(e + (1,1), T, m)$ (the recruit
must fail) and a confirmation-sweep proposal charges $A(e, T, m)$ (it
drives to a bound verdict regardless). Charging per proposal makes
$sum "charges" >= E["false accepts"]$ by linearity, whatever the
candidate count. Reference charges at $m = 4$: threshold 0.05 —
$4.0 times 10^(-4)$ fast, $2.9 times 10^(-3)$ confirm (the confirmation
sweep is the exposure the original per-shrink arithmetic understated,
by 7×); threshold 0.24 — $3.1 times 10^(-6)$ / $5.5 times 10^(-6)$;
threshold 0.90 — exactly 0 (unreachable within the cap, so the
high-anchor regime spends nothing). Without a budget, flat-$m$ exposure
$1 - (1 - alpha_4)^K$ reaches 0.33 (fast) / 0.94 (confirm) at
$K = 1000$; measured shrinks realise $K$ up to 42,000.

The budget $B = 0.02$ affords, at the floor threshold, about 25 fast (3
confirm-driven) proposals at $m = 4$ before escalating, about 98 (18) more
at $m = 5$, and so on; at $m = 8$ a proposal charges
$<= 10^(-7)$, so proposals never stall and the total spend is $B$ plus
a negligible tail. A ledger's $m$ pins at first charge (a stopping
rule never changes mid-test; re-proposals of a pinned ledger charge
past the budget with overdraft bounded by one charge). Escalation's
power cost concentrates at the floor: recruited-accept of a true
$p = 0.1$ candidate against its realistic 0.053 threshold falls
0.57 / 0.33 / 0.16 / 0.06 at $m$ = 4/5/6/7 — and a stricter minimum
also makes the confirmation sweep's "accepted nothing" certificate
easier to obtain, stopping earlier. Both costs are conservative under
the retention contract. Rejected alternatives: count-based escalation
schedules (leak unboundedly at the terminal stage and charge mid-anchor
proposals as if at the floor) and alpha-investing with payouts
@alpha-investing (controls mFDR rather than a per-origin bound, and
needs its own calibration).

*The pooled review.* Confirming a pending origin on any single failure
among its pre-report replays is $Pr approx 0.49$ per $q = 0.02$ fluke
(0.59 with a pool); routing the failing run into a standard bar batch
cuts it to 0.003 (about 170×) and costs target-regime power 0.97 → 0.44
before the backtrack rescue. Origins first _observed_ by report-time
measurement runs are never barred at all — confirming them could admit
further origins without bound — and recycle to the next run.

*Anchor coverage (the FCR view).* Intervals are constructed only for
selected (accepting) batches, so coverage is checked conditionally:
$P("anchor" > p | "accept")$ per source. Bar accepts: 54.9% / 9.4% /
1.8% / 0% at $p$ = 0.05/0.1/0.3/0.9 (the 0.05 row is selection
near the accept boundary; its mean anchor is still 0.057). Gauntlet
adoptions: 33.7% at $p = 0.1$, 7.0% at 0.3. Mean anchors sit at or
below truth everywhere, so miscoverage is upper-tail spread around an
unbiased-to-low mean; since a high anchor only refuses candidates, no
haircut is applied and $z = 1.96$ stands as a tuning constant — the
exact DP rows, not nominal coverage, are the specification.

= Replay budgets and representation <app:repro>

*Replay budget.* The count that leaves a rate-$r$ bug an escape chance
$<= delta$ is $n(r, delta) = ceil(ln delta \/ ln(1 - r))$; at the
target $r = 0.1$, $delta = 0.05$: $n = 29$. A flat 10 misses a
$p = 0.1$ bug 35% of the time. Callers stop at the first failure, so
live bugs cost $approx 1\/p$ and the full budget is paid only for stale
entries. The pre-report review splits the same budget across the pool
($ceil(29\/n)$ per timeline), giving a dry-review worst case of 33
replays (lone incumbent) to 44 (full pool, including splices and 4
fresh cases).

*Continuation budget.* A stored timeline replays with
$max(4, "len"\/8)$ fresh draws allowed past it. Measured on
structure-shifting bodies: extension 0 → 4 lifts single-timeline
reproduction from 78/53/61% to 100/85/98% on the three plateau bodies
(overruns at the end of the sequence, not value loss, are the failure
mode), and extension 64 adds nothing anywhere.

*Pool and splices.* First-fit over stored timelines: cap 5 lifts the
kind-flip body from 68% to 99% and the adversarial heterogeneous body
from 28% to 65%; cap 10 reaches the plateau (72%); cap 20 adds
nothing. Prefix sharing anticorrelates with pool need (pair-LCP 0.32
on the bodies that need pooling, 0.93–0.98 on those that do not), which
is why a merged prefix-tree encoding was rejected. Splicing —
positional crossover of random stored pairs, 10 attempts — rescues
84/100/65% of whole-pool misses on the three bodies at 1.6–6.3 replays
per rescue, lifting the adversarial body to about 90%; the same construction
doubles as the mutation generator for boost. Splices cut only at
top-level positions, so a worker's recorded sub-stream crosses over
intact.

*Detection and token arithmetic.* First-interesting check: escape
probability $(p s)^4$ for failure rate $p$ and per-replay seam survival
$s$; cost 4 replays per failing origin, 0 per passing run. Legacy
(deterministic-format) tokens replay up to 4 times under the standard
continuation budget — a single exact replay reproduced 13% of
never-flipped racy failures; four budgeted attempts bound the joint
escape-then-miss below $1.2 times 10^(-3)$.

= Evaluation setup <app:setup>

*Landscape bodies* (C ABI): draw $n in [0, 20]$, then $n$ atoms in
$[0, 100]$; a _bug atom_ is $>= 10$; a _core atom_ $>= 95$. Failure
fires on an i.i.d. hidden coin at $p("atoms")$: L1 — if $>= 3$ bug
atoms, $p = min(0.1 + 0.08("len" - 1), 0.95)$, else 0; L3 — 0.5 if any
bug atom; L4 — 0.9 if any bug atom else 0.02; L4b — 0.1 / 0.02; D2 —
1.0 with a core atom, 0.7 with $>= 3$ bug atoms, else 0; D0 — 1.0 with
a core atom else 0; N0 — 0.02 always. 100 seeds per cell fixed in code,
500-case budget, database off. "Bug kept" = the reported example
satisfies the landscape's bug predicate; "degraded" = the strict arm's
example has lower ground-truth $p$ than the default arm's cell median.

*Episode bodies* (Rust frontend): _clone_ — one recorded worker stream,
8 rounds of a work draw with a schedule-noise retry draw (disjoint
value range) injected at 0.15/round by a hidden seeded generator;
_machine_ — two worker streams shaped like a concurrent state-machine
run, 4 steps of (op, value) draws with an extra draw at 0.2/step;
failure fires at rate $p$ independent of drawn values, so ground-truth
reproduction probability is exactly $p$. _det-control_ — fixed shape,
always fails; _pass_ — schedule noise, never fails. Episode =
discovery run (100 cases, database + token) → reuse-only run →
token-replay run in a fresh engine, 200 episodes/cell, seeds fixed in
code. Never-flip, flip sites, and measurement-replay counts come from a
test-only engine dump.

All harnesses, the exact DP and simulation code
(`experiments/`), and the raw outputs ship with the implementation;
every table regenerates with one command per harness.
