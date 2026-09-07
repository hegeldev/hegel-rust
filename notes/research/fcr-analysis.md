# Multiplicity control for the ND statistics (decision 72 working analysis)

Every accept gate on the branch is a sequential test whose *per-test* error is
calibrated by exact DP (bar 0.6% per fluke batch, gauntlet 4.0e-4 per
Fast-mode proposal, targeting 2.1% per sign test and ~4% composed per race).
No mechanism controls the *number* of tests, and one gate has no per-test
control at all. This analysis picks the family-level guarantees, the
mechanism per site, and what experiment 014 must derive. The notes' own
multiplicity accounting, all of it local to one family with an assumed
exposure count, is in 005 notes lines 22-60 (bar, F = 5), 008 notes lines
158-176 (gauntlet, measured K = 13 with K = 30 as a sensitivity column), 013
notes lines 95-98 (targeting), and the backtrack constant's doc
(test_runner.rs, ~83% composed power); "multiple testing", "FDR", and
"familywise" have zero hits anywhere in notes/. An adversarial review of this
document's first draft (four lenses, per-finding verification) drove the
mechanism revisions below; its confirmed findings are folded in where they
bit.

## What guarantee, over what family

Two object types need control:

- **Discoveries.** A confirmed origin changes user-visible state: confirmed
  wording, a persisted entry, a reproduce blob, a shrink. The guarantee is
  FDR-shaped, and the enforceable unit is **per origin per run**: a fluke
  origin's probability of ever confirming in one run is bounded once its
  evidence batches are counted — five bar attempts plus the backtrack's own
  three compose to `1-(1-0.006)^8 ≈ 4.7%` at q = 0.02. Run-level exposure
  composes over *distinct fluke origins*, a body property (an origin is an
  assertion site, bounded by the test, not by run length) — the same shape
  as every cost letter on the branch. Cross-run, a false confirm persists
  and can return as Trusted (decision 24, bar-exempt); that residual is the
  per-run bound times the entry's own reproduction chance at reuse, and it
  is priced, not eliminated. 014 measures the composed run-level FDR on
  representative origin mixes, including the within-origin fluke/bug
  timeline mixing that 008's L4b measured (displacement can leave a fluke
  standing as the incumbent at a real bug's origin).
- **Anchors.** Every confirm seeds an anchor from the accepting batch's
  Wilson LCB, and that interval is *used* as the gauntlet threshold. The
  guarantee is FCR-shaped: intervals constructed only for selected batches
  should cover at close to nominal rate. The existing defence is the
  `ANCHOR_SEED_RUNS` extension (decision 54), which removes stopping-rule
  bias only for batches that accept before 20 runs — an accept whose fourth
  failure lands on runs 21-40 seeds its stop-timed LCB with no extension at
  all, and at p = 0.1 that is the majority of accepts. 014 item 4 measures
  the realized miscoverage per source exactly, extension and no-extension
  paths included.

Nothing user-facing quotes an interval (caveats quote raw counts), so FCR
here is about the engine's internal thresholds, not reported statements.

## Why online control, not post-hoc BH

BH needs the whole p-value family at once and a decision that can wait.
Neither holds: confirms and adopts act immediately (anchors raise, shrinks
start, entries persist), and the family size is unknown until the run ends. A
report-time BH pass over the confirmed set could only demote origins whose
shrink and persistence already happened, and the per-origin evidence at that
point gives p-values far below any plausible BH threshold — it would
essentially never fire. Rejected: the correction has to be **online** — caps
and spending budgets fixed before the tests run, the Foster-Stine family
rather than BH proper. Stopping rules inside each test are already priced by
the exact DPs (the DP models the peeking and the early exits); the
corrections below compose those per-test numbers and inherit their honesty.
The care required is three rules:

1. every batch counts against its budget however it ends (accept, reject,
   witness timeout, deadline cut);
2. a test's stopping rule never changes mid-test: a ledger's failure minimum
   is pinned when it first reaches the evidence loop, and a bound verdict is
   final — escalation applies only to tests not yet started;
3. anchor seeding keeps the extension exactly as decision 54 built it, and
   the residual (conditioning on the accept, and the no-extension path for
   late accepts) is measured rather than assumed away.

## Site 1: discovery-bar attempts per origin (unbounded today)

**Exposure.** A bar reject evicts the origin from the interesting map but
leaves the lifecycle entry `Unconfirmed` with `needs_confirmation` true, so
any later sighting re-inserts it and the next sweep runs a fresh batch
(test_runner.rs `record_run` re-insertion arm). Nothing counts attempts. The
005A run-level figure (~3% false accept per run) assumed F = 5 fluke
exposures; a high-rate fluke in a long run takes arbitrarily many attempts
at 0.6% each — the Monte Carlo in 014 puts an uncapped q = 0.02 origin at
~21% false confirm over a 200-epoch run.

**Mechanism.** A per-origin, per-run attempt counter on the lifecycle entry,
consulted **only at the batch-spawn sites**: the discovery sweep, shrink
admission's bar arm, and the site-3 review batch share
`BAR_ATTEMPTS_PER_RUN` = 5; the backtrack keeps decision 66's own separate
3-attempt budget. At the cap a sighted origin is observed and evicted
without a batch — exactly the treatment of a just-rejected origin — so it
keeps the full Unconfirmed treatment everywhere else: caveat-only report
gated by decision 3, no persistence, no blob, and its history and sighting
counts still accumulate through re-insertion. It recycles into the next run
with a fresh counter.

What the cap must NOT be: a `needs_confirmation` flip. That predicate means
"past the bar, replay state usable" and gates persistence (test_runner.rs
persistence filter), the blob-carrying report partition (decision 35), the
trusted shrink-admission arm (which confirms on any single witness failure,
~18% per batch at q = 0.02 — thirty times weaker than the bar), and the
final replay's record-and-report routing. Flipping it at the cap would hand
a capped-out fluke exactly the confirmed-side treatment the cap exists to
deny. The first draft of this document specified that flip; the adversarial
review caught it.

**Why the backtrack budget stays separate.** The shared-counter variant
starves decision 66's rescue in its motivating regime: on noise-floor bodies
the sweep's attempts land on whatever timeline re-inserted first after the
last eviction, a mix of fluke and bug timelines at the same origin (005A:
unconfirmed flukes are retried by later flukes; 008's L4b: displacement
leaves a q = 0.02 fluke standing at the real bug's origin). Attempts spent
on fluke timelines burn the budget at 0.6% accept while testing nothing the
bug needs, so "five attempts compound to >95% power" holds only for pure
origins; the backtrack — which probes *history*, the pre-flip sightings that
are disproportionately the real bug — must not inherit a drained counter.
Cost of separation: the per-origin false-confirm bound is
`1-(1-0.006)^8 ≈ 4.7%`, not 3%. 014 item 2 models the mixed-timeline shape
explicitly (fluke share of attempts 0 to 0.75) with and without the cap.

**Accepted consequences.** (i) A bug at p just above the noise floor (say
0.05) loses real confirmation probability to the cap — ~41% capped vs ~88%
uncapped over a 40-epoch run — because recycling was precisely the mechanism
that eventually confirmed it; p ≥ 0.1 is the branch's stated target
(decision 16) and sub-target bugs still report caveat-only and recycle
across runs, but a run where another origin confirms silences them entirely
(decision 3's fatigue gate). Priced, not fixed. (ii) A capped origin evicted
per-sighting churns the interesting map (insert then evict, no replays);
harmless but visible in traces. (iii) An escalating-bar alternative
(attempts 6+ at a stricter bar rather than zero) was considered and
rejected: it changes the bar's DP identity mid-family for marginal power in
a regime the cross-run recycle already serves.

## Site 2: gauntlet proposals per shrink (measured 42k vs assumed 13-30)

**Exposure.** The 008 per-shrink composition `1-(1-4.0e-4)^K` used K = 13
(the L4b median count of distinct bugless candidates; 30 as sensitivity).
Two gaps. First, no cap enforces any K: experiment 012 measured 42,125
distinct candidate timelines in one episode (true p = 0.9 candidates, where
accepting is correct — but the count demonstrates the exposure is a body
property with no bound). Second, the composition priced every proposal at
Fast mode's 4.0e-4, but the confirmation sweep (decision 18) drives *every*
proposal to a bound verdict, and a bound-verdict test of a bugless candidate
at the floor threshold carries 2.9e-3 (exact DP) — seven times the number
the arithmetic used. The shipped design's own realized per-shrink rate is
understated by roughly that factor whenever the confirmation sweep dominates
the exposure.

**Mechanism: per-origin alpha spending with exact charges.** Each origin
gets a per-run gauntlet budget `GAUNTLET_ALPHA_BUDGET` (working point 0.02,
014's affordability tables size it), held in engine state keyed by origin so
it survives re-shrinks — the shrink probe is rebuilt per `shrink_origin`
call (flip requeues and backtrack restores re-enter it), so probe-local
state would silently reset the bound to per-pass. Charges:

- Every proposal on an unbound ledger is charged, before its outcome is
  recorded, its exact unconditional false-accept mass at the design fluke
  q0 = 0.02: in a Fast sweep, q0 times the DP drive-to-bound probability
  from the ledger's state plus one hypothetical failure (the recruit must
  fail for the drive to happen); in a confirmation sweep, the DP
  probability from the ledger's current state (the drive is
  unconditional). At the floor a fresh Fast proposal charges 4.0e-4 and a
  confirmation-sweep drive from empty charges 2.9e-3. Charging per
  proposal rather than once per ledger is what makes the sum an upper
  bound on E[false accepts] by linearity — a candidate re-proposed across
  passes accumulates recruit chances, and each chance pays its own way.
- A ledger's failure minimum is pinned by its first charge and never
  changes (the stopping rule never changes mid-test); re-proposals of a
  pinned ledger are charged at its own minimum even past the budget (the
  overdraft per ledger is bounded by one charge). Bound verdicts latch: an
  accepted ledger stays accepted (the fast-sweep guard that protects a
  nested clone shrink's final splice consults the latched verdict), a
  rejected one stays rejected, and a bound ledger is never charged again.
- When the remaining budget cannot afford a **new** ledger's charge at the
  current failure minimum, the minimum escalates (4 → 5 → … → 8); at 8 the
  per-proposal charge is at most ~1e-7 (Fast) / ~1e-6 (Confirm), so the
  total spend is bounded by the budget plus a negligible tail for any
  realized K. The engine spends greedily (no stage-splitting), so at the
  floor the budget affords ~50 Fast proposals at the base minimum before
  the first escalation.
- The charge is computed at the threshold in force at charge time; the
  anchor is monotone, so later thresholds only lower the true alpha below
  what was charged.

Exact charging is what keeps the two measured regimes cheap: at high-water
anchors the accept threshold is unreachable by a fluke within
`GAUNTLET_CAP`, the charge is zero, and the 012 lottery spends nothing and
loses nothing; at mid anchors (threshold 0.24) the charge is 3.1e-6 and a
budget of 0.02 affords thousands of base-minimum tests. The genuinely
exposed case — floor-threshold confirmation sweeps — escalates within a
handful of driven ledgers, which is the honest price of driving every
proposal to a bound verdict at the loosest threshold. The conservative
direction is safe under decision 2: a harder late-shrink accept keeps the
incumbent, costing minimality, never failure probability.

**Cost to decision 18.** Escalation inverts the confirmation sweep's
certificate economics: at a stricter minimum the sweep accepts less, so
"accepted nothing" — the stopping certificate — becomes *easier* to obtain,
and the stop can fire while recoverable reductions remain (the L3
18% → 10% missed-reduction benefit erodes back toward its pre-decision-18
level in floor-threshold shrinks that exhaust the budget). 014 item 3
prices the per-candidate power loss (at p = 0.1 against its realistic
0.053 threshold: recruited-accept 0.57 at m 4, 0.33 at 5, 0.16 at 6);
decision 72 must record this as decision 18's amended cost.

**Rejected alternatives.** A count-based doubling schedule (K* ledgers at
minimum 4, 2K* at 5, …): its terminal stage still leaks unboundedly, it
charges mid-anchor proposals as if they were floor-threshold ones, and a
012-style lottery would escalate the minimum for no false-accept reduction
(every lottery accept carries ≥ 25 failures regardless). Alpha-investing
with accept payouts: controls mFDR rather than a per-origin bound, strictly
more machinery, and its payout parameter would need its own calibration
experiment.

## Site 3: pooled-review confirmation (no bar at all today)

**Exposure.** At final replay, a still-unconfirmed origin confirms on *any
single failure* in the pooled review: against a q = 0.02 fluke that is ~49%
per origin over the 33-replay single-timeline review, and the anchor it
seeds is the LCB of stop-at-first-fail evidence — the exact bias
`ANCHOR_SEED_RUNS` exists to remove (lifecycle.rs: "the final replay's
pooled review, which confirms on any failure with no bar"). This path is
not rare: any mid-run flip puts the whole interesting map through the
pooled review, and unconfirmed origins park there routinely — measurement
replays re-insert evicted origins (decision 65's guard covers only the
pre-flip arm), and an origin bar-rejected at shrink admission re-enters the
map on any later sighting and waits, unreviewed, for the final replay.

**Mechanism.** The review's first failure is selection, not evidence — the
same rule the recruiting run already follows. Hand the failing realized run
to the standard `nd_evidence_batch` at the origin: the bar decides
confirmation, an accept extends toward `ANCHOR_SEED_RUNS` before seeding
the anchor, and the batch spends one attempt from site 1's counter (an
origin at the cap is evicted on the existing dry-review path instead). A
bar reject falls through to today's dry-review consequences — backtrack
when history exists (its own budget, so the rescue survives the spent
counter), then evict-if-unconfirmed, caveat-only report gated by
decision 3 — except the review's stamped failing execution still gives the
report its values. Cost: ≤ `CONFIRM_CAP` = 40
replays per unconfirmed origin (the seed extension lives inside the cap; it
does not add to it), at most once per origin per run. The batch runs under
the final replay's deadline: a deadline-cut batch counts as an attempt and
takes the reject path — insufficient evidence stays unconfirmed; today
`nd_evidence_batch` checks no deadline at all, so the implementation adds
one.

The book's argument for the old rule — "the review holds a stamped failing
execution, which is exactly the material a report needs"
(final-replay.md) — keeps its force for *reporting*: the values survive in
the caveat either way. Confirmation, persistence, and the anchor now pay
the same bar as every other admission. Decision 35's headline ("no
post-final-replay bar") is explicitly amended by decision 72 for origins
already pending at the review; its deliberate non-fix — origins first
*observed by* a report-time measurement run are never barred, because
confirming them could admit further origins without bound — stands
unchanged.

## Sites reviewed and left alone

- **Trust at reuse (decision 24).** One test per stored entry, entries are a
  prior run's confirmed output, and re-barring drops real p ~ 0.1 bugs ~55%
  of the time. The cross-run residual (a prior false confirm surviving via
  reuse) is now the enforced per-run bound times the entry's reproduction
  chance at reuse; trusted reports sit outside the per-run FDR family by
  construction, and 014's notes must say so rather than fold them in.
- **Trusted promotion (decision 47).** ≤ 1 per trusted origin per run, on an
  origin that cleared a prior run's bar; the promotion batch extends before
  seeding. No change.
- **Boost holdout (decisions 28/56).** ≤ 2 per origin per run, fresh-holdout
  by design (the winner's-curse fix), adoption stakes are an anchor raise.
  No change; 014 measures its adopt rate at flat truth alongside the rest.
- **ND targeting (decisions 68/69).** ≤ ~8 sign tests per run, false adopt
  is a lateral move losing no bug and moving no anchor, priced by 013 (2.1%
  per sign test, ~4% composed per race). No change.
- **First-check, v1-blob replay.** Fixed exposure (4 replays), priced
  (decisions 59/64/67). No change.

## Anchor coverage (the FCR side)

After sites 1-3, anchor-seeding batches keep decision 54's extension where
it applies, the review path stops seeding stop-at-first-fail LCBs, and the
remaining bias is conditioning on the accept verdict (plus the no-extension
path for accepts after run 20). 014 computes P(anchor > p | accept) exactly
per source: the bar's seeded anchor at p = 0.1 miscovers ~9% against the
2.5% nominal with mean anchor 0.066 (optimistic-low in the mean, fat in the
upper tail), the gauntlet's ~34% at its own accept boundary, and both fall
to near-nominal by p = 0.3. The operational reading: anchors sometimes sit
above the true rate, which prices candidates too high — the conservative
direction under decision 2, absorbed by the gamma = 0.8 slack. No haircut
unless 014's numbers say the p ≈ 0.1 regime materially degrades retention;
the z = 1.96 tuning-constant posture stands (decisions 53/54).

## Experiment 014

A frozen pure-simulation crate (`experiments/fcr-sim`, no engine
dependency), the 005A/008 method:

1. **DP rows.** Exact DP for the gauntlet loop at failure minima 4-8, per
   sweep mode (Fast conditions on a failing recruit; Confirm drives from
   empty), across thresholds and fluke rates — the spending charges — and
   the bar composed over 1..8 attempts. Pins the budget's affordability
   profile and the per-origin rate.
2. **Run-level composition.** Monte Carlo origin episodes: pure flukes and
   pure bugs across rates, plus the L4b-shaped mixed origin (bug and fluke
   timelines sharing one origin, attempts landing on a fluke share of 0 to
   0.75), with and without the cap. Outputs false-confirm, bug-confirm
   (retention), attempts, and replays per episode — power costs stated, not
   just error rates.
3. **Shrink-level.** Flat-minimum exposure at K ∈ {10², 10³, 10⁴} for
   contrast; the budget's affordability per (mode, threshold, budget); the
   escalation's power cost on true candidates at p ∈ {0.1, 0.3, 0.9} with
   both idealized and realistic thresholds — the decision-2 and decision-18
   regression check.
4. **Anchor miscoverage.** P(anchor > p | accept) exactly per source and p,
   extension and no-extension paths included; evaluates the haircut only if
   needed.

Constants out: `BAR_ATTEMPTS_PER_RUN` (5), the per-origin gauntlet budget,
and a yes/no on the anchor haircut per source.

## Interactions with standing constraints

- **Decision 2 (no probability loss).** All three mechanisms only make
  accepting *harder*; the incumbent always survives a refusal. The measured
  risk is minimality and power, which 014 prices.
- **Decision 18 (confirmed-dry stopping).** Amended: escalation makes the
  certificate easier to obtain, trading missed recoverable reductions for
  the spend bound in floor-threshold shrinks (see site 2).
- **Decision 23 (recycling as the power mechanism).** Preserved up to the
  cap the power arithmetic assumed, per run; below-target bugs now lean on
  cross-run recycling. What is removed is only the unbounded within-run
  tail the false-accept arithmetic never priced.
- **Decision 35 (report seam).** Amended at its headline: the pooled review
  now bars pending unconfirmed origins. Its deliberate non-fix for origins
  first observed by report-time measurement runs stands.
- **Decision 54 (z stays 1.96; per-shrink composition).** The per-test DP
  operating points stay the specification; decision 72 replaces the K-based
  composition claim with the enforced budget and corrects the
  confirmation-sweep alpha it understated.
- **Decision 66 (backtrack).** Its 3-attempt budget and ~83% composed power
  claim survive intact because the budget stays separate; the total
  per-origin exposure statement moves to 8 batches.
- **Decisions 24/47 (trust).** Unchanged; within-run confirmation is now
  strictly harder than cross-run trust, which is the design's existing
  asymmetry (trust rides a prior run's bar), not a new one.
