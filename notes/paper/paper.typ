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
      [Section 5]
    }
    were run for this paper. Not yet reviewed for submission.
  ]
]

#heading(numbering: none, outlined: false, level: 1)[Abstract]

Property-based testing engines in the Hypothesis tradition treat a test as a
deterministic function of a recorded sequence of random choices. Everything
downstream of generation leans on that assumption: shrinking replays edited
sequences and trusts each verdict, the failure database and reproduction
artefacts store one sequence, and a replay that disagrees with the recorded
outcome aborts the run as flaky. Real tests violate the assumption routinely
— concurrency, hidden state, timing — and engines respond by giving up: they
abort without a counterexample, or disable shrinking, persistence, and
reproduction wholesale.

We present the design and implementation of nondeterministic-test handling in
Hegel, a Hypothesis-descended engine, built on one premise: every test
execution is a Bernoulli trial, and failure probability is a first-class
quantity. A failure is believed only after a sequential confirmation rule
derived under an asymmetric loss. Shrinking accepts a candidate only on
statistical evidence that it reproduces nearly as reliably as the example it
would replace, so reduction cannot trade the bug away. Per-failure budgets
bound what unbounded within-run repetition can buy. A failing test case is
represented by a small collection of recorded executions, replayed until the
failure recurs. Runs assume determinism until an observation contradicts it,
then recover: deterministic failing tests pay four extra executions, and
passing suites pay nothing.

On failure landscapes where a determinism-enforcing baseline reports a
counterexample in 0–4% of runs, the engine reports a confirmed, shrunk,
reproducible counterexample in 98–100% while retaining the underlying bug in
98–100% of reports. A noise-only test yields a false "confirmed" report in
5% of runs, matching the derived bound. Stored failures at or above the
design's target failure rate reproduce across runs and processes at 96–100%.

= Introduction

Property-based testing (PBT) generates inputs, checks properties over them,
and, on failure, _shrinks_: it searches for a smaller input that still fails,
because the difference between a 200-element counterexample and a 2-element
one is the difference between a bug report a developer ignores and one they
fix @quickcheck @hughes-experiences. The value of a PBT run is concentrated
in its final report: a minimal example, plus enough to reproduce it.

Engines descended from Hypothesis @hypothesis get both from one mechanism:
they record every random choice a test makes, treat the test as a pure
function from the recorded choices to a verdict, and re-run it by replaying
edited recordings @reducer. Shrinking, deduplication, a failure database,
and one-line reproduction tokens are all corollaries of replay
(@sec:representation gives the background). The architecture has displaced
combinator-level shrinking in most modern implementations precisely because
replay composes: anything that can be recorded can be re-run, minimised,
cached, and shared.

All of it rests on an invariant the engine cannot enforce: the test must be
a _deterministic_ function of its recorded choices. Concurrency breaks the
invariant intrinsically — the thread schedule is not recorded — and so do
global state, time, iteration order of hashed collections, and the network.
At industrial scale, test flakiness is the dominant obstacle to acting on
test signal at all @flaky-empirical @harman-ohearn @deflaker @idflakies.

What do engines do when the invariant fails? Hypothesis detects the
disagreement and raises a `Flaky` error: the run aborts, and the user gets
neither a minimal example nor a reproduction token, for exactly the bugs —
races — where those artefacts matter most. QuickCheck-family tools
@quickcheck @proptest do not detect it: shrinking trusts each single
verdict, so on a probabilistic failure the search random-walks wherever
noise leads it (we measure this in @sec:gauntlet: it reports an _empty_,
effectively passing input as the "minimal counterexample" in every
noise-floor trial). Hegel, the system this paper modifies — a Rust engine
implementing the Hypothesis architecture behind a C ABI, with per-language
frontends — used to respond to declared concurrency by disabling shrinking,
persistence, and reproduction outright, and to every other detected
nondeterminism by aborting. We call this posture, common to all of the
above, _surrender_.

This paper replaces surrender with statistics. The premise is that a
counterexample to a nondeterministic test is not a value but a distribution:
each execution of a test case $x$ is a Bernoulli trial with some unknown
failure probability $p(x)$, and every question the engine used to answer by
lookup — is this failure real? is this smaller input still failing? does
the stored example still work? — becomes a question about an estimated
probability, answered with explicit error budgets. Two separable problems
fall out, and they get separate machinery. _Outcome nondeterminism_ — the
same input, a different verdict — is a statistics problem, and
@sec:shrinking presents the statistical core: an algorithm for finding,
confirming, and shrinking probabilistic failures, with determinism assumed
nowhere. _Generation nondeterminism_ — the same input prefix, a different
shape of execution — is a representation problem, and @sec:representation
replaces "the" choice sequence of a test case with a small collection of
recorded executions. Between them, @sec:determinism restores determinism to
its proper place: not an axiom but an optimisation, assumed until an
observation contradicts it and recovered from when one does.

*Contributions.*
(1) An algorithm for shrinking under nondeterminism with an explicit
statistical retention guarantee — reduction does not lower the reported
example's failure probability — built from four composable rules
(confirmation, a monotone anchor, a sequential acceptance test, and a
certificate-carrying stopping rule), with every constant derived by exact
dynamic programming, simulation, or measurement rather than chosen
(@sec:shrinking, appendices).
(2) A deterministic-by-default architecture around it: detection of
nondeterminism by observation alone, and recovery mechanisms for the seam
where a run discovers, mid-flight, that its earlier single-run decisions
were untrustworthy (@sec:determinism).
(3) A test-case representation for nondeterministic tests — a collection of
timelines — that keeps the serialisability the choice-sequence architecture
is built on (@sec:representation).
(4) An implementation in a production engine, invisible behind its C ABI,
and an evaluation on synthetic failure landscapes and racy test bodies
(@sec:eval), plus an account of what failed on the way (@sec:discussion).

= Shrinking under nondeterminism <sec:shrinking>

This section presents the statistical core of the engine as a
self-contained algorithm. Nondeterminism is assumed from the start;
@sec:determinism later adds the deterministic fast path around it.

== The setting <sec:setting>

Abstract the engine to three primitives. A _generator_ produces random test
cases: $"generate"() -> x$. A _reducer_ proposes, from a test case $x$,
candidates $y$ that are smaller in a fixed total order (in Hegel, shortlex
over the underlying representation): the transformation passes of a
conventional shrinker, unchanged. And _replay_ executes the test once
against a stored test case: $"replay"(x) -> "pass" | "fail"$. What a test
case concretely is and how replay works are the subject of
@sec:representation; nothing in this section depends on it.

The model is that each execution of $x$ is an independent Bernoulli trial:
it fails with unknown probability $p(x)$. A test can fail in more than one
way; failures are identified by their panic or assertion site, everything
below operates on one failure at a time, and "fails" always means "fails
_with the same failure_". Replays are never answered from a cache: under
nondeterminism a cached verdict is exactly the single-run trust this
machinery exists to remove, so every judgement below is made on fresh
executions.

The engine's job is to end the run holding some failing test case $x^*$,
as small as possible, together with honest evidence about it. Two
commitments shape everything:

- *Scope.* The machinery targets tests that fail at least 10% of the time
  they run ($p >= 0.1$). Rarer failures are still reported honestly, but
  are not promised confirmation or reproduction, and every replay budget
  below is derived from this target rate.
- *The retention contract.* Shrinking must not (statistically) lower the
  reported example's failure probability, and should raise it when cheap.
  A small example that no longer fails for the real reason is worse than a
  large one that does.

_Evidence_ is always a plain pair (fails, runs) of replay counts, and
confidence bounds on it are Wilson score bounds @wilson at $z = 1.96$: an
interval for a Bernoulli rate that stays sensible at the very small counts
these rules run at. We write $"LCB"(f\/n)$ and $"UCB"(f\/n)$ for the lower
and upper endpoints given $f$ failures in $n$ runs; the handful of values
the design leans on are $"LCB"(1\/1) = 0.21$, $"LCB"(4\/30) = 0.053$,
$"LCB"(20\/20) = 0.84$, and $"LCB"(30\/30) = 0.89$ (@app:bar has the
formula).

The run loop is now simple to state (@fig:algo): generate until an
execution fails; treat that as a _sighting_ and try to _confirm_ it
(@sec:bar); shrink a confirmed example under the _gauntlet_ (@sec:gauntlet)
until stopping carries a certificate (@sec:stopping); replay each held
example until it fails once more, and report it with the run's own replay
counts attached. @fig:report shows the resulting user experience. The rest
of this section explains each rule: the problem it solves, the rule itself,
why it has the shape it has, and what it costs.

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
        "run():
  while the case budget remains:
    x <- generate()
    if replay(x) failed:                                # a sighting
      if confirm(x) = accept: shrink(x)
  replay each held example until it fails once more; report it with its evidence

confirm(x):                                             # the confirmation bar (2.2)
  E <- (fails: 0, runs: 0)
  loop:
    E <- E + replay(x)
    if E.fails = 4:            replay until E.runs >= 20; anchor <- LCB(E); return accept
    if E = (0 fails, 10 runs)  or  E.fails + (40 - E.runs) < 4:             return reject

shrink(x):                                              # ledgers L[.] persist for the whole shrink
  repeat: sweep(fast)          until a sweep adopts nothing
  sweep(full)                                           # the confirmation sweep (2.5)
  if it adopted anything: resume the fast sweeps, else return x
                                                        # certificate: every candidate holds a bound reject

sweep(mode):
  for each candidate y proposed from x, y smaller than x:     # the reducer's passes, unchanged
    if judge(y, mode) = accept:
      replay y until L[y].runs >= 20                    # de-bias before touching the anchor (2.3)
      x <- y;  anchor <- max(anchor, LCB(L[y]))

judge(y, mode):                                         # the gauntlet (2.4)
  if L[y] holds a latched verdict: return it
  L[y] <- L[y] + replay(y)
  if mode = fast and that replay passed: return reject  # unlatched; total cost one run
  T <- max(gamma * anchor, 0.05)                        # gamma = 0.8, or 1.0 once anchor >= 0.8
  loop:                                                 # sequential test; verdict latches
    if L[y].fails >= 4 and LCB(L[y]) >= T:  return accept      # the 4 can be escalated (2.6)
    if UCB(L[y]) < T  or  L[y].runs >= 30:  return reject
    L[y] <- L[y] + replay(y)",
      )
    ],
  ),
  caption: [
    The run and shrink loops, with determinism assumed nowhere. replay(·)
    executes the test once against a stored test case — one Bernoulli
    trial — and "failed" always means "failed with the same failure".
    LCB/UCB are Wilson bounds on a ledger's (fails, runs).
  ],
) <fig:algo>

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
        "#[hegel::test]\nfn totals_agree(tc: &TestCase) {\n    let batch = tc.draw(gs::vecs(gs::integers::<u32>().max_value(1000)).max_size(20));\n    let expected: u32 = batch.iter().sum();\n    assert_eq!(expected, racy_sum(&batch), \"totals diverged\"); // drops a large job ~40% of the time\n}",
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
    shrunk under the retention contract, the caveat quotes this run's own
    replay counts, and the token replays a stored collection of executions
    until the failure recurs (@sec:representation), so it works despite the
    race.
  ],
) <fig:report>

== Believing a failure: the confirmation bar <sec:bar>

*The problem: a sighting is selection, not evidence.* The generation loop
executes thousands of test cases once each, and notices the ones that fail.
An input with a tiny failure probability gets thousands of one-shot chances
to fire, so conditioning on "it failed once" tells you very little about
$p$. This is not a corner case: on a landscape with a real bug at
$p = 0.9$ over a background of spurious failures at $p = 0.02$ (every
execution of _any_ input has a 2% chance of failing for an unrelated
reason — an infrastructure hiccup, a timeout), the first failing execution
a run sees is a background fluke roughly twice as often as it is the bug.
An engine that believes first sightings starts a third of its shrinks from
noise, and everything downstream — the shrink, the report, the stored
example — inherits the mistake.

*The rule.* A sighting earns belief by a sequential replay batch, the
_confirmation bar_: replay the sighting and

- *reject* if the first 10 replays all pass;
- otherwise continue to at most 40 replays, *accepting* on the 4th failure;
- *reject* as soon as 4 failures become unreachable.

Only a confirmed example is shrunk, persisted, or presented as a
counterexample; an unconfirmed sighting is still reported (the test did
fail), but as a caveated failure quoting its own counts — "failed 0 of 3
replays after the observed failure" — and only when the run found nothing
better.

*Why this shape.* The rule was chosen by exact dynamic programming over
(runs, fails) states, evaluating candidate rules — flat $k$-of-$N$ rules,
Wald's sequential ratio test @wald, Wilson-bound accept/reject pairs,
two-stage gates — against an asymmetric loss (@app:bar). The asymmetry is
the important design fact. A false _accept_ is sticky: the fluke becomes
the example the whole rest of the run invests in — the shrinker refines
it, the database stores it, the report presents it. A false _reject_ is
cheap: generation keeps running, a real bug gets sighted again, and a
fresh batch runs — so per-batch power compounds across sightings, and the
design can buy very low false-accept rates with power it recovers through
recycling. The chosen rule accepts a $p = 0.02$ fluke 0.6% of the time
per batch, and a $p = 0.1$ target-rate bug 45% of the time per batch —
which compounds past 95% within five sightings. The 10-replay opening
gate is where the cheapness lives: a fluke passes 10 straight replays 82%
of the time, so most flukes are dismissed for 10 replays, and a rejected
fluke costs about 15 replays on average, while a $p = 0.9$ bug confirms
in about 4.4. A sequential ratio test tuned for one-shot power instead
spends 52 replays per fluke buying power that recycling provides for
free.

*What it costs.* Per-discovery power at the design floor is only 45%, so
confirmation leans on the run being long enough to re-sight real bugs —
a dependency @sec:determinism has to repair when detection comes late, and
one of the priced costs in @sec:budgets.

== The anchor: what the current example is worth <sec:anchor>

*The problem: enforcing the retention contract needs a reference value,
and every obvious estimate of it is contaminated.* To refuse candidates
less reliable than the current example, the engine needs an estimate of
the current example's failure probability. But the evidence it naturally
has is biased by how it was collected. The confirming batch stopped _on_
the accepting failure, and evidence truncated by its own stopping rule
overestimates: a batch that accepts on four straight failures would
estimate $"LCB"(4\/4) = 0.51$ whatever the true rate. And re-measuring
the _incumbent_ — the example the search currently holds — during the
search, then letting the estimate drift down on
unlucky batches, makes the acceptance threshold incoherent — candidates
get judged against whatever the incumbent's estimate last wandered to.

*The rule.* The _anchor_ is a lower confidence bound on the current
example's failure probability, and it obeys two disciplines. First,
_de-biased seeding_: evidence may seed or raise the anchor only after its
batch is extended to 20 replays, past the stopping decision, so the bound
estimates the rate rather than the stopping rule. Second, _monotonicity_:
the anchor rises only at validated events — a bar accept, or the adoption
of a shrink candidate (whose ledger is first topped up to the same 20
runs) — and never falls; routine re-measurement of the standing incumbent
never feeds it.

*Why this shape.* Monotonicity is what makes estimation error safe: the
anchor only ever _prices candidates_, so an anchor that reads high refuses
too many candidates, which costs minimality — never failure probability.
Errors land on the conservative side of the contract by construction.
The alternatives were measured and rejected in simulation: letting the
anchor decay toward fresh measurements makes stopping incoherent (51–75%
of reachable reductions missed), and checkpoint/rollback schemes either
poison stable searches or never fire (@app:gauntlet). The extension
constant 20 is itself derived from reachability: a shrink candidate capped
at 30 replays can demonstrate at most $"LCB"(30\/30) = 0.89$, so anchors
seeded from 20-run batches ($"LCB"(20\/20) = 0.84$) are the strongest a
candidate can still match — seeding from full 40-run batches
($"LCB"(40\/40) = 0.91$) provably stalls shrinking against a
deterministic-looking incumbent.

The contract's other arm — _raise_ reliability when cheap — gets one
mechanism worth a sentence: when a confirmed example's anchor is below
0.30, the engine races it against its stored collection of timelines
(@sec:representation) and mutants of itself, and adopts a steadier starting example only when a
fresh 20-replay holdout beats the anchor, because in-race winners are
selection-biased upward and the race itself is never trusted.

== Judging a candidate: the gauntlet <sec:gauntlet>

*The problem: shrinking is a ratchet, and noise turns it.* Shrink
acceptance is irreversible: each accepted candidate becomes the new
incumbent, and the order guarantees the example only gets smaller. Under
determinism that is the whole point; under noise it is a mechanism for
walking off the bug one lucky replay at a time. At a 2% background
failure rate, a bug-free candidate's chance of failing its one audition
is 0.02 — and a shrink proposes thousands of candidates. In simulation,
single-run acceptance loses a $p = 0.9$ bug in 34% of noise-floor trials.
The seemingly safer rule "accept if it fails at least once in 10 replays"
loses it in _all_ of them — any-failure-within-$N$ amplifies noise
acceptance — and ends at an empty input failing at the 2% floor,
presented as the minimal counterexample. In the real engine, single-run
acceptance kept the bug in 3 of 100 noise-floor runs.

*The rule.* Candidate judgement is the `judge` procedure of @fig:algo.
Each candidate $y$ has a _ledger_ — cumulative evidence $(f, n)$ from
every replay of $y$ this shrink, kept for the whole search. A proposal
whose fresh replay passes is rejected immediately at the cost of that one
run (during fast sweeps). A proposal whose replay fails must clear the
_gauntlet_: replay until the sequential test latches a verdict —
*accept* when the ledger holds at least 4 failures _and_ its Wilson lower
bound clears the threshold $T = max(gamma dot "anchor", 0.05)$, with
$gamma = 0.8$; *reject* when the upper bound proves $T$ unreachable, or
at 30 runs. An accepted-and-adopted candidate becomes the incumbent and
may raise the anchor per @sec:anchor.

*Why this shape*, constant by constant:

- _Charge accepts, not rejects._ Most candidates a reducer proposes do
  not reproduce the failure; driving each to a statistically bound
  verdict would multiply shrink cost by roughly the 30-run cap. Rejecting
  on one passing run keeps the cost profile of deterministic shrinking —
  and it is safe because the two error directions are priced differently:
  a false reject costs only minimality, and the reducer's passes retry
  rejected transformations anyway, with the persistent ledger
  accumulating evidence across retries rather than starting over. A false
  accept is the irreversible direction, so acceptance is where the
  statistics are spent.
- _The minimum of 4 failures._ A fresh ledger's single failure already
  has $"LCB"(1\/1) = 0.21$, so any threshold below 0.21 would accept on
  the recruiting run alone. Anchors seeded from target-regime bugs sit
  well below that (median seed 0.066 at $p = 0.1$), so without a failure
  minimum the gauntlet silently degenerates into exactly the single-run
  trust it exists to prevent — across the entire target regime. The
  shipped system had precisely this calibration failure, losing 33% of
  target-regime bugs while every unit test passed; @sec:discussion
  returns to the lesson. With the minimum, measured on the same
  landscapes: bugs kept rise 51%/73%/97%/100% at minima 1/2/3/4, at
  3.0× the degenerate rule's cost — a cost that concentrates entirely
  in the target regime, since easier landscapes never hit the minimum
  (@app:gauntlet).
- _The floor 0.05._ Thresholds proportional to a low anchor can fall to
  meaninglessness. The floor is derived, not chosen: it is the largest
  value still below $"LCB"(4\/30) = 0.053$, the weakest evidence the
  4-failure minimum can ever accept at the run cap — so it refuses
  candidates that only fail at the noise floor while costing zero power
  against everything the minimum would accept.
- _$gamma = 0.8$, and 1.0 above an anchor of 0.8._ Requiring a candidate
  to statistically _prove_ $p(y) >= "anchor"$ exactly is unreachable at
  these sample sizes; $gamma = 0.8$ trades a bounded reliability loss per
  accepted step for reachability. The exception at high anchors is a
  detector, not a dial: an anchor of 0.8 is reachable only by zero-miss
  20-run evidence, i.e. by an incumbent indistinguishable from
  deterministic — and such an incumbent is never traded down
  ($gamma = 1$). This converts 33% displacement of deterministic
  incumbents to zero, at +26% cost on the landscape where it binds.

*What it costs.* Worst-case false accept per proposal is
$4.0 times 10^(-4)$ against a $p = 0.02$ fluke, and each accepted step
may genuinely trade up to 20% of reliability. What unbounded proposal
volume does to the per-proposal number is @sec:budgets's problem; what
the composed system actually retains is measured in @sec:eval (98–100% of
bugs kept).

== Stopping with a certificate <sec:stopping>

*The problem: under noisy rejects, a fixed point is not exhaustion.*
Deterministic shrinking stops when a full sweep of passes proposes nothing
that improves — a genuine fixed point. Under the fast-reject rule, every
sweep of a genuinely reducible example can end quietly because each good
candidate happened to pass its one audition (probability $1 - p$ per
candidate). Fixed "run $k$ extra dry sweeps" rules just resample the same
biased coin.

*The rule.* When a fast sweep goes dry, the engine runs one _confirmation
sweep_: every proposal is re-made, the fast reject is disabled, and every
candidate's ledger is driven to a latched, bound verdict. An accept
resumes fast sweeps; a confirmation sweep that accepts _nothing_ ends the
shrink. The stop then carries a certificate: every candidate the passes
can reach was rejected by a confidence bound, not by luck.

*Why, and the cost.* In simulation the rule halves the missed-reduction
rate of fixed dry-sweep counts at equal replay cost (18% → 10% on the
landscape where stopping is hardest), for about three sweeps' worth of
extra work at the end of each shrink (@app:gauntlet).

== Budgets: what unbounded repetition can buy <sec:budgets>

*The problem: each rule's error rate is per test, and a run repeats the
tests without bound.* The bar's 0.6% false-accept is per batch — but a
rejected fluke input is evicted, generation re-sights the same background
noise, and every re-sighting gets a fresh batch: recycled across a long
run, an uncapped $p = 0.02$ fluke confirms 21% of the time. The
gauntlet's $4 times 10^(-4)$ is per proposal — but one measured shrink
realised 42,000 distinct candidates, and at a thousand floor-threshold
proposals the uncorrected exposure already reaches 33%. Classical
multiple-testing corrections @benjamini-hochberg do not apply: there is no
family of p-values to rank at the end, because every verdict acts
immediately and irreversibly — anchors rise, incumbents change, examples
persist. Control must be _online_, with budgets fixed before the tests
run, in the spirit of alpha-investing @alpha-investing.

*The rules*, all per failure per run (derivations in @app:multiplicity):

+ *Confirmation batches are capped at 5.* At the cap a sighting is simply
  not believed this run. The cap pins the per-failure false-confirm
  probability at the 2.9% the bar's derivation assumed, while five
  batches keep $>= 95%$ power at the target rate. (Recovery batches at
  the seam, @sec:seam, hold a separate budget of 3; together the
  per-failure ceiling is 4.6%.)
+ *Gauntlet proposals spend an alpha budget of 0.02.* Before a candidate
  with an unlatched ledger runs, its sequential test is charged its
  _exact_ false-accept probability against a $p = 0.02$ fluke — computed
  by the same dynamic program that derived the rule, from the ledger's
  current state and threshold. By linearity of expectation, the sum of
  charges bounds the expected number of false accepts, however many
  candidates the body realises. Exact charging is what keeps ordinary
  shrinks free: a threshold an anchor of 0.9 sets is unreachable within
  the cap and charges exactly zero, mid-anchor thresholds charge about
  $10^(-6)$, and only floor-threshold proposals ($4 times 10^(-4)$ each)
  meaningfully spend. When the remaining budget cannot afford a fresh
  candidate, _new_ candidates require more failures — the minimum
  escalates from 4 toward 8, where a proposal charges under $10^(-7)$ —
  while a candidate's own stopping rule never changes once its test has
  begun.
+ *Report-time reproduction confirms nothing by itself.* The pre-report
  replay of a still-unconfirmed sighting makes around 40 attempts, and
  "confirm if any one fails" would admit a $p = 0.02$ fluke about half
  the time. A failure seen there is treated as one more sighting: it
  faces a standard bar batch on the remaining capped attempts (cutting
  that path's false confirms about 170×, to 0.3%), and reaches the user
  as a caveated report when the batch rejects.

*What they cost.* The budgets' power losses are deliberate and priced:
a below-target bug ($p = 0.05$) confirms in about 42% of runs rather than
almost surely given a long one, leaning on recycling across runs; and a
shrink that exhausts its alpha budget at the floor both accepts less and
stops earlier. Every such loss keeps an incumbent or refuses a claim — the
conservative side of the retention contract.

Finally, reporting closes the loop on honesty: every reported failure
carries a caveat quoting that run's own replay counts (@fig:report), a
confirmed failure that cannot be reproduced at report time switches its
wording rather than going unreported, and nothing estimated — no rates,
no counters — is ever persisted, so each run's claims stand on its own
replays.

= Starting from determinism <sec:determinism>

Almost all tests are deterministic, and for them the machinery of
@sec:shrinking is pure waste: a 20-replay confirmation of a failure that
reproduces every time buys nothing four replays wouldn't, and a gauntleted
shrink multiplies cost for no retention benefit. Determinism also buys
real optimisations — repeated executions served from a verdict cache (85%
of shrink replays, measured), single-replay shrink accepts, one exact
replay before reporting. So the engine assumes determinism, keeps the
assumption cheap to hold, and treats nondeterminism as something a run
_discovers_. Nothing is ever declared: creating a concurrent test changes
nothing until its behaviour is observed to vary.

== Detection by observation <sec:detect>

A run switches into nondeterministic handling — one sticky per-run flag,
never cleared within the run — when any of four observational channels
fires:

- *A verdict flip.* Every executed conclusion is fingerprinted by its
  realised test case. A repeat that concludes differently (the cache
  doubling as a detector) is direct evidence.
- *The first-sighting check.* Before a run acts on a newly discovered
  failure — before shrinking or persisting it, before even believing
  it — the sighting is replayed exactly, up to 4 times, stopping at the
  first miss. A miss flips the run, and the four observations are
  credited to the failure's first confirmation batch so they are not paid
  for twice (with one guard: a batch must contain its _own_ reproducing
  run to accept, so credited evidence alone can never confirm a failure
  the batch never saw fail).
- *Replay checks.* The verify-replay before shrinking and the final
  replay before reporting — the sites where a Hypothesis-style engine
  raises `Flaky` and aborts — flip the run instead.
- *Stored state.* A persisted example recorded under nondeterministic
  handling is marked as such, so a rerun of a known-flaky test flips
  before any replay and never re-pays detection.

The first-sighting check carries the load in practice (in the evaluation,
every flip on genuinely racy bodies landed there), and its arithmetic sets
the cost of the whole posture. A bug failing at rate $p$ whose replays
happen to survive structural divergence at rate $s$ escapes detection with
probability $(p s)^4$. A deterministic failing test pays exactly four
extra executions; a passing suite pays nothing at all. Those four replays
are spent _before_ deterministic trust can consume the discovery, which is
what makes the assumption safe to hold everywhere else.

The flip is silent by default (a `strictness` setting lets suites that use
determinism as a lint keep the old aborts). @tab:flip summarises what it
changes: everything in @sec:shrinking switches on, and every optimisation
that trusts a single execution switches off.

#figure(
  placement: top,
  table(
    columns: (auto, 1fr, 1.35fr),
    align: left,
    toprule,
    table.header([], [*deterministic run*], [*after the flip*]),
    midrule,
    [believe a failure],
    [first sighting is the example],
    [the confirmation bar (@sec:bar)],
    [shrink accept],
    [one failing replay, strictly smaller],
    [the gauntlet against the anchor (@sec:gauntlet)],
    [shrink stop],
    [passes reach a fixed point],
    [confirmation sweep with a certificate (@sec:stopping)],
    [repeated executions],
    [verdicts served from cache],
    [never served; every replay executes],
    [pre-report check],
    [one exact replay; a miss aborts],
    [replay-until-failure (@sec:representation)],
    [stored example],
    [one choice sequence],
    [a collection of timelines (@sec:representation)],
    [report],
    [values + reproduction token],
    [same, plus a caveat quoting this run's replay counts],
    botrule,
  ),
  caption: [What changes when a run discovers nondeterminism.],
) <tab:flip>

== Recovering at the seam <sec:seam>

*The problem: detection can come late, after trust has done damage.* The
channels above are sparse by design — that is what makes them cheap — so
a run whose nondeterminism is outcome-only (structure stable, verdicts
noisy) can spend much of its budget under deterministic trust before
anything fires. By then two irreversible things have happened. Single-run
shrink accepts have already ratcheted the incumbent down the very slope
@sec:gauntlet describes: measured on a landscape where failure probability
falls as inputs shrink, the incumbent's failure probability at flip time
was 0.26 at every percentile. And the generation budget the bar's
recycling assumption leans on is already spent. In the pre-fix
measurement, 49% of target-regime runs ended with _no_ reported
counterexample at all — the single largest loss mechanism we encountered,
bigger than any statistical miscalibration.

*The rules.* Recovery, not earlier detection, closes the seam:

- *History.* While the run is still deterministic, every failing
  execution of each failure is retained — the raw sightings and each
  accepted shrink step, deduplicated. History is dropped once the failure
  confirms; it exists to answer one question later.
- *Backtrack.* When a post-flip replay check misses on a never-confirmed
  failure, the engine scans that history for the newest entry that still
  reproduces: geometric probes back from the newest entry, then binary
  refinement between the newest reproducing and oldest non-reproducing
  probes, on a 40-replay budget. The scan is deliberately biased _old_,
  because the retention contract makes over-shooting safe: a too-old
  restore is merely bigger and re-shrinks under the gauntlet, while a
  too-new one is the noise-walked example the flip just discredited.
- *The bar, again.* The restored candidate earns nothing by being
  restored: it must clear the full confirmation bar, on the separate
  3-batch budget of @sec:budgets (history skews toward the real bug's
  pre-flip sightings, which is why the budget is separate; three batches
  compose to about 83% power at the target rate).

*What it bought.* After these landed, the no-counterexample rate on the
target-regime landscape fell from 49% to zero, and the median final
failure probability on the size-coupled landscape doubled (0.34 → 0.74). The
price is that the whole shrink now runs gauntleted once the flip happens
at discovery (1.54× executions on such bodies) — the flip moved from
"after the damage" to "before the commitment".

= Representing a nondeterministic test case <sec:representation>

The machinery so far treated "a stored test case" and "replay" as
primitives. This section supplies them.

== Choice sequences

A Hypothesis-style engine mediates every random decision a test makes.
When the body asks for an integer, a float, a boolean, a string, or a byte
buffer, the engine records the typed value (with the constraints it was
drawn under) into the current _choice sequence_; generators for compound
values are compositions of these primitive draws. The test is treated as a
function from the sequence to a verdict. Replay answers each draw from a
stored sequence, generating fresh values past its end; shrinking proposes
edited sequences (delete a span, zero a region, minimise a value) and
re-executes them @reducer. Because the sequence is a flat list of typed
values, it serialises trivially — and the failure database and the
one-line reproduction token are nothing but a stored sequence. That
serialisability is the property to preserve.

== Timelines

Under generation nondeterminism the architecture's central noun stops
referring: replaying the same prefix can produce a _different draw
structure_ — a retry loop that goes around once more, a worker thread
that interleaves differently and draws an extra value — so "the" choice
sequence of a test case is not well-defined. What is always well-defined
is the record of one particular run. Call the realised choice sequence of
one complete execution a _timeline_.

The representation of a nondeterministic failing test case is then a small
collection of timelines: the current (shrunk) best example first, plus up
to nine other timelines that were each observed to fail with this failure,
captured as by-products of confirmation and shrinking. The collection is
a sample from the failure's basin rather than a single point in it.

Replaying _one_ timeline is the `replay` primitive of @sec:shrinking:
answer draws from the stored timeline, tolerate structural divergence
rather than aborting on it, and allow $max(4, "len"\/8)$ fresh draws past
the stored end. Crucially, a replay that diverges and passes still counts
as one full trial in every ledger: the statistics of @sec:shrinking are
about _the test case_ — "replaying this stored state shows the failure" —
not about tracking a particular string of bytes, and failing to see the
failure is exactly what non-reproduction means. The continuation allowance
is measured, not chosen: with none, reproduction on structure-shifting
bodies runs at 53–78% (the failure mode is running out of recorded
choices, not wrong values); at $max(4, "len"\/8)$ it reaches 85–100%; more
adds nothing.

Replaying _the collection_ — used at report time, for database reuse, and
by the reproduction token — is _replay-until-failure_: try each stored
timeline in order under a budget of 29 replays split across the
collection, stopping at the first failure; if every whole timeline misses,
try 10 _splices_ — random cross-pairings of two stored timelines cut at a
random position. The budget is derived from the target rate:
$ceil(ln 0.05 \/ ln 0.9) = 29$ replays leave a $p = 0.1$ bug at most a 5%
escape chance, where a flat "replay 10 times" would miss it 35% of the
time — and since callers stop at the first failure, a live bug costs
about $1\/p$ replays and the full budget is paid only for stale entries.
Each piece of the design earns its place on adversarial
structure-shifting bodies: a single stored timeline replayed exactly once
— the old reproduction token — reproduced 13% of racy failures; a
collection of 10 lifts the worst body from 28% to its 72% plateau; and
splicing rescues 65–100% of the misses that remain when every whole
timeline fails to reproduce.

A collection of timelines is a list of choice sequences plus two replay
parameters, so it serialises exactly as easily as one sequence did.
Persistence therefore needs nothing new: the failure database and the
reproduction token simply store this object, and stored failures
reproduce across runs and in fresh processes at 96–100% (@sec:eval).

= Evaluation <sec:eval>

The statistical rules were _derived_ on exact dynamic programs and pure
simulation (appendices A–C), then validated in-engine. Here we evaluate
the composed system. All numbers were measured for this paper on the
final engine (commit `9f1eb7c6`; Apple M5 Pro; harness and seeds shipped
with the implementation), on two suites:

*Landscapes* (via the C ABI): bodies draw a list of up to 20 integer
"atoms" and fail with a probability determined by the drawn values —
L1 _rising_ ($p$ grows with input size from a 0.1 floor, ≥3 bug atoms
required: the shrink-quality stress, where minimising size fights
retention directly), L3 _constant_ ($p = 0.5$), L4 _noise-floor_ (bug at
0.9 over a 0.02 background), L4b _target-regime_ (bug at exactly the 0.1
design floor over the same background), D2 _deterministic-core_ (a
$p = 1$ core inside a $p = 0.7$ flaky region), N0 _noise-only_ (every
execution fails at 0.02; no bug), and D0, a fully deterministic control.
100 runs per cell, 500-case budget, two arms: the engine's default, and
`error` strictness — the determinism-as-invariant posture on identical
detection.

*Episodes* (via the Rust frontend): racy bodies with a hidden
schedule-noise generator that perturbs draw structure run to run — a
clone-stream body and a two-worker state-machine-shaped body — failing at
a schedule-independent rate $p$. Each episode is a discovery run (100
cases), then a database-reuse-only run, then a reproduction-token run in
a fresh engine. 200 episodes per cell, at
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
Under strict determinism (@tab:landscapes), every genuinely flaky
landscape yields a counterexample in 0–4% of runs (D2's 20% are runs that
reached the deterministic core before any replay check fired; L1's 4
survivors report examples degraded to $p = 0.26$ — the ratcheted-down
incumbents of @sec:gauntlet). The statistical engine reports a confirmed,
shrunk, token-carrying counterexample in 98–100% of runs on every
landscape with a real bug, and a caveated failure otherwise. It never
converts a failing run into a silent pass.

*RQ2 — soundness: what does noise buy?* On N0, where every failure is a
$p = 0.02$ fluke and there is nothing to find, 95/100 runs report a
caveated unconfirmed failure — the designed behaviour: the run did fail,
and the caveat says the failure did not reproduce — and 5/100 falsely
confirm, consistent with the derived per-failure ceiling of 4.6%
(@app:multiplicity; N0 re-sights one failure all run, the worst case).
Even a false confirm quotes its own replay counts ("failed 4 of 40
replays"), so the report is visibly weak.

*RQ3 — retention: does shrinking keep the bug?* Across all landscape
cells the reported example still contains the ground-truth bug in
98–100/100 runs — against 3/100 for single-run acceptance measured on
the same noise-floor bodies, and 34–100% loss in simulation
(@sec:gauntlet). On the rising landscape, where minimising size fights
retention directly, the median reported example fails at $p = 0.74$
against the 0.10 floor a probability-blind reducer converges to. D2 shows
the honest trade: the engine reports the deterministic core in most runs
(median final $p = 1.0$) but a confirmed $p = 0.7$ example in the rest,
where free displacement would sometimes have lucked into the core — the
price of refusing single-run trust.

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
    discovery run reported a failure. "DB reuse" and "token" count
    reproduction of confirmed failures in a following run and in a fresh
    process. Measurement replays are per episode; M is millions.
  ],
) <tab:episodes>

*RQ4 — reproduction.* On the racy episode suite (@tab:episodes) every
discovery run reported a failure, and every flip landed at the
first-sighting check. At $p = 0.1$, the design floor, 90–92% of episodes
confirm; at $p >= 0.3$, 98–100%. Confirmed failures reproduce from the
database in 96–100% of reuse runs and from the reproduction token, in a
fresh process, in 98–100%. The below-target $p = 0.05$ cell confirms 37%
of episodes — the budgets' price (@sec:budgets) — and reports caveated
unconfirmed failures otherwise; its confirmed failures still reproduce in
86–88%.

*RQ5 — what do deterministic tests pay?* The deterministic failing
control pays exactly 4 measurement replays per run (the first-sighting
check) and is otherwise byte-identical under both arms. The all-passing
control pays zero replays and never flips. The cost lands on the
genuinely nondeterministic bodies, and there it is heavy: the median
episode at $p = 0.05$ paid 109 measurement replays (unconfirmed failures
never shrink), while cells at $p >= 0.1$ paid 0.6–1.7 million, peaking at
the design floor (maximum 6.4 million). These bodies are the gauntlet's
worst case: every candidate fails at the same rate as the test itself, so
a floor-threshold ledger can neither accept quickly nor reject early
($"UCB"(0\/30) = 0.11$ never falls below the 0.05 floor) and the
confirmation sweep drives nearly every distinct candidate to the 30-run
cap. The replays are cheap on these bodies; a slow body would hit its
wall-clock deadline and stop early with a valid, larger example.

*Threats.* All bodies are synthetic, with i.i.d.-coin outcome noise and
seeded structural noise, where real races have correlated,
input-dependent failure probabilities — the Bernoulli model of
@sec:setting is the design assumption the suite shares. The landscapes
are, however, adversarial by construction (noise floors, deterministic
cores, size-coupled probability) in ways sampled real suites would not
be. The strict arm shares the final engine's detection, which is stronger
than the shipped baselines it stands in for (measured directly: the
unmodified engine aborted 29–69% of runs on these bodies, and single-run
shrinking kept 3/100 bugs — both worse than the strict arm shown).
Constants were derived and validated on the same landscape families; the
episode bodies and the N0/pass cells are held out from every derivation.
Single machine, fixed seeds, 100–200 trials per cell: proportions carry
$plus.minus$ 3–5% at 95% confidence.

= Discussion <sec:discussion>

Two lessons cost the most and seem most likely to transfer.

*Individually sound sequential rules composed back into the naive
policy.* The bar, the anchor, and the gauntlet were each derived in
isolation and each correct in isolation. Composed, the bar's honest
low-regime anchors set gauntlet thresholds below $"LCB"(1\/1) = 0.21$ —
so every candidate accepted on its recruiting failure, and the shipped
system silently reproduced single-run trust across the entire target
regime, losing 33% of target bugs in simulation while every unit test
passed (two of them pinning the degenerate behaviour as intended). The
fix (@sec:gauntlet) is three constants; the lesson is that the
composition, not the components, is the object of calibration — and that
only an end-to-end adversarial measurement sees it. The seam
(@sec:seam) is the same lesson at the architecture level: most measured
loss came not from any statistical rule but from the boundary where one
mode's decisions became another mode's inputs.

*Estimates contaminated by their own selection recur everywhere.* The
discovering run (selected for failing), a batch stopped by its own accept
rule, an in-race winner (selected for winning): each looked usable and
each biased a downstream decision until moved to a validated,
selection-free measurement. "A sighting is selection, not evidence"
ended up enforced at six separate sites in the engine.

*Limitations.* Shrink cost on bodies whose every candidate fails at the
test's own rate is measured in millions of replays per run, worst at the
design floor where no candidate can be cheaply rejected (@sec:eval); a
slow body hits the wall-clock deadline and stops early with a valid,
larger example, and any cheaper rule trades against the retention
contract — we leave the trade open. Below-target bugs ($p < 0.1$) get
honest caveated reports but confirm in well under half of runs (42%
derived, 37% measured) — the budgets' deliberate price. Failure identity
is the panic site, which cross-thread panic plumbing can blur. And
schedules are sampled, never controlled: the engine cannot promise to
re-trigger a race, only to keep honestly measuring whether it does.

= Related work <sec:related>

*PBT and shrinking.* QuickCheck @quickcheck established
generate-and-shrink. Hypothesis @hypothesis moved shrinking onto recorded
choice sequences @reducer, the architecture Hegel implements and most
modern engines (proptest @proptest and others) share. All inherit the
determinism invariant: Hypothesis documents it and aborts on violation;
QuickCheck-family reducers silently mis-shrink. Targeted PBT
@targeted-pbt guides generation by a score whose hill-climbing likewise
assumes deterministic scores; the engine replaces it under
nondeterministic handling with a holdout-gated race in the spirit of
@sec:anchor, not further discussed here. PULSE @pulse randomises Erlang
scheduling to make races findable by QuickCheck and reduces with a custom
shrinker under a user-supplied similarity relation — it controls the
scheduler; we assume no such control and handle the residual
nondeterminism statistically.

*Flaky tests.* Large-scale studies established flakiness as pervasive
@flaky-empirical @harman-ohearn; DeFlaker @deflaker and iDFlakies
@idflakies detect and classify flaky unit tests by re-running under
instrumentation. That line treats flakiness as a defect to be triaged; we
treat probabilistic failure as an operating condition for the test tool
itself, and use replays not to label the test but to confirm, minimise,
and reproduce its failures. The rerun-to-confirm intuition is folklore
(retry-on-red); the contribution here is deriving how many replays, in
what sequential rule, against what loss, and bounding the compounding.

*Reduction under unreliable oracles.* Delta debugging @ddmin and C-Reduce
@creduce reduce with re-executed interestingness tests, and practitioners
routinely bolt retry heuristics onto them for flaky oracles; Choi and
Zeller reduced failure-inducing _schedules_ given a deterministic
replayer @schedule-isolation. We are not aware of prior reducers with an
explicit statistical retention guarantee, a certificate-carrying stopping
rule, or alpha-spending across candidates.

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
in place of SPRT asymptotics @wald (the budgets are small and the DP is
cheap, so operating points are exact), and online alpha-spending in the
spirit of Foster and Stine @alpha-investing rather than batch FDR control
@benjamini-hochberg, because verdicts act immediately and irreversibly.

= Conclusion

The determinism invariant made replay-driven PBT possible. It also made
the tooling most brittle exactly where testing is hardest. Treating every
execution as a Bernoulli trial — and rebuilding confirmation, shrinking,
stopping, and reproduction as sequential decisions with derived budgets —
recovers the whole toolchain for nondeterministic tests: shrunk
counterexamples with a statistical retention guarantee, reports whose
every claim is backed by that run's own replays, and reproduction
artefacts that expect to need more than one try. The engineering is a few
thousand lines; the design content is the loss functions, the budgets,
and the places single-run trust hides. Heisenbugs @heisenbug have been
getting worse for forty years. Test tools no longer need to treat them as
someone else's problem.

#bibliography("refs.yml", style: "association-for-computing-machinery")

#counter(heading).update(0)
#set heading(numbering: "A.1", supplement: [Appendix])

= The confirmation bar <app:bar>

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

*Setting.* A failure has been sighted; replays of the sighting are
modelled i.i.d. Bernoulli($p$). Design points: the target rate
$p >= 0.1$; a background fluke rate $q = 0.02$ taken from the noise-floor
landscapes. The loss is asymmetric because a false accept is sticky (the
run commits to the fluke: shrinking, persistence, and the report all
build on it) while a false reject recycles through rediscovery, so
run-level power compounds across sightings while run-level false accepts
compound against the user.

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

*Witness and seeding.* An accepting batch must contain its own
reproducing run (evidence credited from the first-sighting check may
otherwise fill the quota) and extends to 20 replays before its LCB seeds
the anchor, removing stopping-rule bias: an un-extended 4-of-4 accept
would seed 0.51 regardless of $p$. Exact miscoverage of the seeded
anchor, $P("anchor" > p | "accept")$, is 9.4% at $p = 0.1$ against the
2.5% nominal, falling to 1.8% by $p = 0.3$, with the mean anchor at or
below truth everywhere (0.066 at $p = 0.1$) — optimistic-high anchors
only price candidates too high, the conservative direction, so the
residual selection bias is absorbed rather than corrected.

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
loss, matching the naive policy of @sec:gauntlet.

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

*Simulated envelope of the composed rule* (500 seeds/cell):
rising-landscape final failure probability median 0.82 (p10 0.58) against a
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
epochs (100k episodes) confirms an uncapped $q = 0.02$ fluke 4.6% of
the time by 40 epochs and 20.9% by 200; the 5-attempt cap pins both at
2.9%. The separate 3-attempt backtrack budget raises the per-failure
ceiling to $1 - (1 - 0.0059)^8 = 4.6%$. Priced costs: a $p = 0.05$ bug
falls from near-certain confirmation (given a long run) to 42% per run;
a failure whose sightings mix bug and fluke executions with fluke share
0/0.25/0.5/0.75 confirms its bug at 0.95/0.87/0.72/0.45.

*Gauntlet alpha-spending.* Each proposal on an unlatched ledger is
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
@alpha-investing (controls mFDR rather than a per-failure bound, and
needs its own calibration).

*The report-time review.* Confirming a pending failure on any single
reproduction among its pre-report replays is $Pr approx 0.49$ per
$q = 0.02$ fluke (0.59 with a stored collection); routing the failing run
into
a standard bar batch cuts it to 0.003 (about 170×) and costs
target-regime power 0.97 → 0.44 before the backtrack rescue. Failures
first _observed_ by report-time measurement runs are never barred at all
— confirming them could admit further failures without bound — and
recycle to the next run.

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
entries. The report-time review splits the same budget across the
collection ($ceil(29\/n)$ per timeline), giving a dry-review worst case
of 33 replays (lone timeline) to 44 (full collection, including splices
and 4 fresh generated cases, a tier only the report-time review has).

*Continuation budget.* A stored timeline replays with
$max(4, "len"\/8)$ fresh draws allowed past it. Measured on
structure-shifting bodies: extension 0 → 4 lifts single-timeline
reproduction from 78/53/61% to 100/85/98% on the three plateau bodies
(overruns at the end of the sequence, not value loss, are the failure
mode), and extension 64 adds nothing anywhere.

*Collection size and splices.* First-fit over stored timelines: cap 5
lifts the kind-flip body from 68% to 99% and the adversarial
heterogeneous body from 28% to 65%; cap 10 reaches the plateau (72% on
the adversarial body); cap 20 adds nothing. Prefix sharing
anticorrelates with the need for a collection (pair-LCP 0.32 on the
bodies that need one, 0.93–0.98 on those that do not), which is why a
merged prefix-tree encoding was rejected. Splicing — positional
crossover of random stored pairs, 10 attempts — rescues 84/100/65% of
whole-collection misses on the three bodies at 1.6–6.3 replays per
rescue, lifting the adversarial body to about 90%. Splices cut only at
top-level positions, so a worker thread's recorded sub-stream crosses
over intact.

*Detection and token arithmetic.* First-sighting check: escape
probability $(p s)^4$ for failure rate $p$ and per-replay structural
survival $s$; cost 4 replays per failing deterministic test, 0 per
passing run. Legacy (single-sequence) tokens replay up to 4 times under
the standard continuation budget — a single exact replay reproduced 13%
of racy failures; four budgeted attempts bound the joint
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
