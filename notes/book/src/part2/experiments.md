# The experiments

The branch's constants and mechanisms trace to twelve numbered experiments.
`notes/experiments/000-plan.md` is the program's status table. Each experiment has a directory
`notes/experiments/NNN-name/notes.md` whose spec was written before the run and whose results and
lessons were appended after. A result that changed the design updated `design.md`, and reversals
were logged in `decisions.md`. The code harnesses live in `/experiments` at the repository root,
frozen after use as standalone crates outside the workspace, unmaintained and some no longer
building.

The program ran in three eras. Experiments 001–006 (all closed 2026-09-02) were a designed
sequence, and 007 closed it alongside the phase-7 concurrency unification. Experiments 008,
009a, and 009b served [the remediation plan](remediation.md), answering the as-built critique's
findings S1–S6 and W1. Experiments 010, 011, and 012 (all run 2026-09-04) served
[the seam plan](seam-plan.md), gate G20's resolution.

Two experiment numbers are reused. `remediation-plan.md` sketched a conditional "010 — splice
budget", resolved without running when decision 52 corrected `REPRODUCE_SPLICES` to 10 from
006B's data, and a conditional "011 — stamp overhead" that never ran. The shipped 010 and 011 are
different experiments, and 000-plan.md records both reuses. A third conditional, "012 — trusted
anchor", also never ran.

| # | name | harness | consequence |
| --- | --- | --- | --- |
| 001 | shrink-statistics simulation | `/experiments/shrink-sim` | gauntlet shape; confirmed-dry stopping; no checkpointing; no anchor decay (decisions 17–19) |
| 002 | cache seam + fixate cost | `/experiments/fixate-cost` | `serve_replays` is the whole resampling seam; body cost dominates |
| 003 | flat-timeline shrink in-engine | `/experiments/nd-shrink` | gauntlet holds in the real shrinker; displacement and confirmation gates (decisions 20/21) |
| 004 | replay semantics | `/experiments/replay-semantics` | pool cap 5–10 first-fit, extend 4, trie rejected (decision 22) |
| 005 | lifecycle | `/experiments/confirm-bar`, `/experiments/nd-lifecycle` | the discovery bar (decision 23); admission gating (decision 24) |
| 006 | grafting + boost | `/experiments/nd-boost`, splice tier in the 004 harness | splices close per-position anchoring (decision 25); boost policy |
| 007 | concurrency under ND handling | `/experiments/concurrent-replay` | decision 14 closed (31); values-only clones (32); `hegel_run_start_blob` (33) |
| 008 | gauntlet calibration | `/experiments/shrink-sim`, `/experiments/gauntlet-calibration` | min-fails 4, 20-run seeding, derived floor, high-water gamma (decisions 54–56) |
| 009a | off-ceiling watermark | `/experiments/watermark` | G9/G10 closed (decisions 57/58); found the never-flip corner |
| 009b | composed-rules re-verification | `/experiments/watermark` (`composed`) | validated decisions 54–58 as composed |
| 010 | data-tree value | `experiments/tree-value` on `claude/experiment-010-tree-value` | tree removed (decisions 60–63) |
| 011 | instrumented seam spot check | `/experiments/gauntlet-calibration` (`seam`) | G20's acceptance run; both letter misses escalated (decision 67) |
| 012 | detection-escape recheck | `/experiments/detection-escape` | never-flip corner closed; found the cost lottery (decision 67) |
| 013 | targeting under ND | `/experiments/target-sim` | post-plan: ND targeting race constants (decisions 68/69, see [shrinking](../part1/shrinking.md)) |

## 001: shrink-statistics simulation

**Question.** Calibrate the open constants of the shrink loop's statistics (gauntlet thresholds,
gamma, stopping rules, budgets), and settle whether checkpointing adds anything and how
pass-repetition compares to per-candidate-N cost.

**Method.** The harness is pure simulation, with no engine. A test case is a `Vec<u64>` of atoms
0..=100, an atom at or above 50 is a bug atom, and every "execution" is one Bernoulli draw from
the landscape's true failure probability. Starts are length 20 with at least three bug atoms,
the sort key is shortlex, and each (policy, landscape) cell ran 200 seeds. The landscapes are L1
rising-with-size, L2 deterministic-core, L3 constant 0.5, and L4 noise-floor (bug 0.9, bugless
0.02). The policies are P0 naive single-run accepts, P1 per-candidate-N (fails-within-10), P2 a
fixed gauntlet (five consecutive failures), P3 a ledger gauntlet (single-run rejects with
evidence retained, sequential Wilson accepts at LCB >= gamma x anchor, monotone anchor), and
P4 = P3 plus checkpoint/rollback.

**Results.** P0 loses the bug on L4 in 34% of trials. P1 loses it in 100%, reporting the empty
test case at p = 0.02: accepting on any failure within 10 gives a bugless candidate an 18%
acceptance chance per proposal, and each noise accept ratchets the sort key down irreversibly.
P3 keeps the bug in 100% of trials on every landscape at roughly 1.5–2x naive cost, except where
the never-lower-p constraint binds (L1: ~2.6k median executions, ~20x naive). P2's fixed
threshold misclassifies near the true rate, so the threshold must come from the incumbent. Gamma
is a size-for-reliability dial on L1 (0.5 lands p 0.34 at length 4, 0.8 lands 0.58 at 7, 1.0
lands 0.74 at 9) and barely matters on L4. Checkpointing tracked P3 exactly at 10–40% more cost.

**Follow-up run.** Confirmed-dry stopping (a dry sweep, then one confirmation sweep driving
every proposal's cumulative evidence to a bound decision) cost no more than three fixed dry
sweeps, halved L3's missed-reduction rate (18% to 10%), and terminates with a certificate
(decision 18). The L5 mixture landscape showed capture-at-confirmation kills the pinning hazard
at source, while rollback-on-uncertainty fires constantly on stable landscapes (L1: 2.3x cost,
missed reductions 9% to 52%) and rollback-on-proof never fires, so checkpointing was dropped
(decision 17). Anchor decay bought one to two L1 length units at +20–40% cost with 51–75% missed
rates and was rejected. The anchor stays monotone, and post-accept evidence never feeds it
(decision 19).

**Later revised.** Experiment 008 found the shipped parameterisation of this gauntlet
degenerated to single-run accepts (a fresh ledger's single failure has Wilson LCB 0.2065, above
every bar-seeded threshold in the low-to-mid regime) and recalibrated it (decisions 54–55).

## 002: cache seam and fixate cost

**Question.** Where does the resampling seam go in `cached_test_function`, and what does one
fixate iteration cost with tree dedup off on a ~50-node target?

**Method.** The experiment ran in-engine. The seam was located by reading `test_runner.rs`, and
the cost was measured by recording one interesting case whose body draws N booleans (N in
{10, 50, 200, 1000}) and
replaying it 10,000 times with tree-serving on versus off, engine overhead only.

**Results.** `Engine::cached_test_function` was the only place the choice tree served a recorded
conclusion instead of executing. So "the tree never serves conclusions under ND handling" is one
boolean, `Engine::serve_replays`, and flipping it converts every replay consumer to resample
semantics without touching recording. Cost was linear on both sides: ~25–100 ns/node served
versus ~100–300 ns/node executed (1,918 vs 5,899 ns/replay at 50 draws, and 24,624 vs
104,439 ns at 1000). At the 50-node target a full 30-run gauntlet adds ~120 us of engine time per
candidate, and any body worth ND treatment costs orders of magnitude more per run, so no
result-cache substitute is needed under ND handling and budgets follow body cost and statistics
rather than engine throughput.

**Later revised.** The data tree itself was removed outright (decision 60, via experiment 010),
so the seam finding outlived the structure it gated. 010 confirmed the bet from the other side:
tree-serving's one real win is recoverable with a flat cache.

## 003: flat-timeline shrink in-engine

**Question.** Do gauntlet plus ledger tame drift on real synthetic flaky tests, with the
statistics running in the actual shrinker rather than 001's simulation?

**Method.** The experiment ran in-engine, gated on a new `Settings::nd_experiment`, with three
modes: Baseline (the untouched engine), Resample (002's seam flipped, single-run accepts: 001's
P0 transplanted), and Gauntlet (Resample plus 001's P3 in `EngineShrinkProbe`, the ledger keyed
on serialized realized choices). Bodies draw n in 0..=20 then n atoms in 0..=100, failing via a
hidden per-trial PRNG at 001's L1/L3/L4 rates on real choice sequences. Each cell ran 100 seeds
at a 500-case budget.

**Two leaks.** Both were predicted by the design and demonstrated live. The first was raw
`update_interesting` displacement: with post-discovery generation running, a 20/20-confirmed
discovery was displaced by p = 0.02 noise flukes before shrinking started in ~80% of L4 trials,
and the gauntlet then anchored on the fluke and shrank garbage. Under ND handling a raw
interesting run may fill a vacant origin but never displace an occupied one (decision 20).
The second was that discovery-time confirmation is a prerequisite: first-interesting on L4 is
a noise fluke about 2:1 over genuine bugs, a population the old Flaky abort had been filtering
by accident. The scaffold's flat 2-in-20 confirmation batch was adopted as explicitly
provisional (decision 21).

**Results.** Baseline aborts 29–69% of runs as Flaky/NonDeterministic, while Gauntlet completes
100% everywhere. L4 bug kept: Baseline 69/71 of completed runs, Resample 3/100 (final p median
0.02, empty counterexamples), Gauntlet 99/100 (median 0.90). On L1 Resample teleports to p 0.26
while Gauntlet holds a 0.50 median at length 6, costing 13,556 median executions. On L3/L4 the
gauntlet pays only +18–43% over resampling. The lessons recorded were that naive resampling is
not a viable intermediate mode ("if ND mode ships anything, it ships the gauntlet") and that
discovery plus the post-discovery generation window are part of the statistical surface.

**Later revised.** The flat 2-in-20 bar was confirmed unsafe and replaced by 005A (decision 23).
The gauntlet parameterisation was re-derived by experiment 008 (decision 54), which measured the
fix at cost 1.00x on this experiment's landscapes.

## 004: replay semantics

**Question.** How should ND mode replay stored timelines (pool fallback, continuation budgets,
structural divergence) on bodies whose structure changes run to run? It is the complement of
003, with a deterministic verdict and flaky structure, and feeds the deferred
per-position-anchoring question (decision 14).

**Method.** The harness orchestrates an in-engine replay primitive. Bodies draw integer atoms
0..=100, and hidden per-execution coins change draw structure but never the verdict, which fails
deterministically iff at least three atoms are 90 or above. The bodies are late-coin (B1),
step-coins (B2), kind-flip (B3), stable-prefix (B4), het-shift (B5, heterogeneous draws plus
shifts, the adversarial case), and a deterministic control. Each trial (40 per body) discovers a
failing timeline T0, builds a pool per capture-at-confirmation, and then makes 50 cold attempts
per strategy: T0 replay at extend {0, 4, 16, 64}, pool first-fit at caps K in {1, 2, 5, 10, 20},
and a fresh-generation control.

**Results.** Extend 4 captures the entire single-timeline benefit (B1 78% at extend 0 to 100%,
B2 53 to 85, B4 61 to 98, and extend 64 adds nothing anywhere), so bare replay's losses come
from end-of-sequence overruns rather than value loss. B3 sticks at 66% (~0.85^3) regardless of
extend and B5 craters at 19–28%, so positional punning holds exactly when constraints are
homogeneous. The pool is the recovery mechanism: K=5 takes B3 from 68 to 99% and B5 from 28 to
65%, K=10 reaches the plateau (B5 72%), and K=20 adds nothing, at a worst-case cost of 3.2
replays per attempt. Prefix sharing
is anticorrelated with pool need (pair-LCP 0.32 on B3 and 0.48 on B5, against 0.93–0.98 on the
bodies K=1 already handles), so a merged trie would compress the timelines that don't need
pooling and fail on the ones that do. The verbatim watermark understates value survival (B3's
LCP falls to 0.40 while 66% of replays still fail), so first divergence must weight evidence
rather than abort the replay.

**Consequence.** Decision 22 set the pool at cap 5–10, first-fit, with a small continuation
budget, kept the trie rejected, and made divergence weight evidence rather than abort. Decision
14 stayed deferred with B5's ~27% residue as its measured target.

**Later revised.** 006B recovered most of the residue by splicing (decision 25), experiment 007
closed decision 14 outright (decision 31), and 009a re-validated the operating points off the
reproduction ceiling (decision 58).

## 005: lifecycle

**Question.** Exercise the failure lifecycle end to end: confirmation, capture-at-confirmation,
persistence gating, and unified reporting. Part A discharges decision 21's assignment: derive the
discovery-confirmation rule from the noise-floor caution.

### 005A: the confirmation bar

**Method.** The analysis is exact dynamic programming over (runs, fails) states with no
simulation noise, at noise p = 0.02, target p = 0.1, and Wilson z = 1.96. The rules compared
were flat k-of-B, Wald SPRT, a Wilson accept/reject pair, and two-stage gates. The loss function
is asymmetric by the recycling argument: a false accept is sticky (it occupies the origin,
anchors the gauntlet on garbage, and the displacement gate then protects it) while a false
reject recycles through rediscovery, so P(accept | noise) is minimised hard and per-discovery
power traded away.

**Results.** The scaffold's flat 2/20 is 6.0% false accept per fluke (26.6% run-level at five
exposures), confirmed unsafe. The Wilson pair is 26.1% (wrong shape), and SPRT buys power only
by spending 51.9 expected replays per p = 0.02 fluke, a bad trade given recycling. The winner
was gate 1/10 then 4/40: reject on zero failures in ten replays, otherwise continue to 40
total, accepting early on the fourth failure. Its operating point is 0.6% false accept per
fluke, 45% per-discovery power at p = 0.1 compounding past 95% by the fifth discovery, 15
replays per rejected fluke, and 4.4 for a p = 0.9 bug. The 1/10 gate alone dismisses 82% of
p = 0.02 flukes. This became decision 23, shipped as `GATE_RUNS = 10`, `CONFIRM_MIN_FAILS = 4`,
and `CONFIRM_CAP = 40` (hegel-c/src/native/nd/mod.rs).

### 005B: the lifecycle prototype

**Method.** The scaffold gained the 005A bar, capture-at-confirmation feeding a per-origin pool
(cap 10 per decision 22), persistence of the shrunk incumbent plus pool entries as additional
primary database entries, budgeted reuse, and caveated reporting per decision 3. The
harness is two runs: run 1 discovers, confirms, shrinks, and persists into a fresh in-memory
database copied into run 2, which measures cross-run reproduction. The bodies are 003's L1/L3/L4
plus step-coins (S2), het-shift (S5), and a pure-noise body N0 with no real bug, at 30 seeds per
body and budgets of 300 and 1000 cases.

**Results.** L1/L3/L4 and S2 at 300 cases confirmed 30/30 and reproduced 30/30 in run 2 (2–4
executions for outcome-ND bodies, with S2 re-shrinking at 1,950 median). S5 at 300 confirmed
20/30 (discovery starvation rather than confirmation failure), all 20 reproduced at 8,285 median
executions because structural misalignment forces a re-shrink, and 29/30 at 1000. N0 confirmed
0/30 at both budgets, every run failed caveated, nothing was persisted, and there were zero
false confirms. There were zero run errors and 139/139 cross-run reproductions across both
budgets.

**Consequence.** Confirmation is a property of origin admission, not one execution path: the
first cut hooked it on the generation run's own status, and on pure noise 26/30 runs "confirmed"
a fluke through a witness-only fallback. The fix sweeps every unconfirmed interesting origin
after each generation iteration (decision 24). Reused entries are trusted on reproduction, since
re-running the bar would drop real p ~ 0.1 bugs ~55% of the time. Cross-run cost splits exactly
on structure, pricing the deferred `replay_aligned` question. Small budgets starve narrow
structural bugs of discoveries rather than confirmations.

**Later revised.** Decision 47 replaced blanket reuse-trust with an evidence batch at shrink
time. Decision 64 added the first-interesting check on every generation-discovered origin, and
experiment 012 closed the residual never-flip escape.

## 006: cross-timeline grafting and boost

**Question.** Does a pre-shrink boost phase (decision 2's "raise p when possible") deliver
steadier incumbents at acceptable cost, and does donor splicing recover the ~27% replay residue
whole-timeline pools plateau under?

**Method.** 006A ran in-engine behind `Settings::nd_boost`: after confirmation and before
shrinking, successive halving over 16 candidates (incumbent, pool entries, probe mutants) for
128 replays total, plus a 10-run holdout on the winner because in-race rates are
selection-biased upward. The bodies were D1 deterministic-core, L1 rising, and L3 constant
control, at 30 seeds per cell. 006B ran in the 004 harness: whenever all K=10 pool entries
missed a cold attempt, it tried up to ten positional splices of random pool pairs, deliberately
a cheap lower bound on span-anchored grafting.

**Results, boost.** On D1 the gauntlet alone lands deterministic finals in 27/30 runs, and boost
makes it 30/30 at +14% cost (1,792 to 2,044 median executions). On L1 boost moves the p median
from 0.26 to 0.42 at length 3 to 5, at +46% cost, a size-for-reliability trade with no core to
find. L3 is unchanged at +12% cost, where the holdout gate correctly refuses to raise the anchor
on noise. The monotone anchor does most of boost's job where a deterministic core exists, so
boost is a guarantee rather than a discovery mechanism, and the coreless trade is reporting
policy.

**Results, grafting.** Splicing rescued 26/31 B2 misses (84%, 4.8 splice replays per rescue),
15/15 on B3 (100%, 1.6), and 350/541 on B5 (65%, 6.3), lifting B5's overall reproduction from
72% to ~90%. The residue needs recombination rather than per-position anchoring, a trie, or live
re-execution anchoring: replay-until-failure becomes pool first-fit, then splices, then fresh
generation (decision 25).
The boost mutant generator and the splice construction turned out to be the same shape.

**Later revised.** Decision 52 corrected `REPRODUCE_SPLICES` to 10, after decision 25's "~6
replays/miss" cost figure had been mistranscribed as the splice cap. Decision 28's boost floor
of 0.5 was re-derived by experiment 008 as `BOOST_RELIABILITY_FLOOR = 0.30` in 20-run-batch LCB
units (decision 56).

## 007: clone streams and concurrency under ND handling

**Question.** Do clone-bearing and concurrent-stateful bodies survive the full ND pipeline
(discovery, confirmation, gauntleted shrink, persistence, reproduction), and what clone
serialisation format is needed?

**Method.** A standalone frontend binary (`experiments/concurrent-replay/`, driven by its
`drive.py`) runs one process per run, with a fresh temp database per trial and 20 trials per
workload. The two workloads are *racy*, a `#[hegel::concurrent_state_machine]` counter with a
load/yield/store increment losing updates under `run_concurrent(m, tc, 2, 4)`, and *clone*, a
plain body drawing an integer through `tc.clone()` and failing only every third call, whose
deterministic choices and flaky outcome isolate round-trip fidelity from scheduling noise. Each
trial is a discovery run, a database-reuse run, and three blob replays via
`Hegel::reproduce_failure`.

**Results.** Both workloads ran at ceiling: 20/20 discovery (medians 0.61 s racy, 0.02 s clone),
20/20 database reuse, and 60/60 blob replays, with every failure reporting the confirmed caveat
and a v2 blob. Getting there required routing blob replay through the replay primitive: before
the fix,
`reproduce_failure` on the racy blob reproduced 4/30, because the frontend used
`hegel_test_case_from_blob` (incumbent-only, single attempt) and the shrunk minimal schedule
fires its race only ~13% of the time in a fresh process. `hegel_run_start_blob` replays the blob
as a run (ND blobs through `nd_reproduce` with no fresh tier, since a fresh case could fail for
an unrelated reason) and reproduced 30/30 (decision 33). Clone serialisation stays values-only:
the only consumer of realized prefix nodes is `resolve_choice`'s is-simplest check, which
verbatim replay never consults (decision 32). Whole-timeline pool replay at ceiling with splices
as rescue closed decision 14 (no per-position or per-stream anchoring), and splices structurally
cannot tear a clone record, pinned by
`a_positional_splice_carries_whole_clone_records_across_intact` (decision 31). Boost never
engaged, and disabling the data tree under ND handling showed no generation-cost pathology.

**Later revised.** Nothing in the notes. 007's ceiling rates became the baseline that 009a
measured off and that decision 58 references.

## 008: gauntlet calibration under the shipped rules

**Question.** (H1) Does the shipped parameterisation (bar-seeded anchors, recruit-counted
evidence, no minimum evidence, floor 0.05, flat gamma 0.8) degenerate to single-run accepts
across the target regime, losing 001/003's drift protection? (H2) Do the remediation rules
restore the 003 numbers, at what constants and cost?

**Method.** `/experiments/shrink-sim` extended with an exact model of the shipped rules from
`hegel-c/src/native/nd/mod.rs` at 9c800e8e, a factorial over anchor seeding x accept rule x
floor x gamma x miss weight {1.0, 0.2, 0}, and an exact-DP module using the 005A method.
The landscapes were 001's L1–L5 plus L4b noise-floor-lo (bug p = 0.1 over 0.02 background, the
decision-16 target regime), D1 (006's deterministic core), and D2 (the core with the flaky
region at 0.7, finding S6's displacement case). N was 500 for headline cells, 200 for the
factorial, and 10,000 for seeding, with byte-identical outputs and ~40 s of CPU.

**H1 result.** The shipped policy is degenerate at or below anchor 0.258: a fresh ledger's single
failure has Wilson LCB(1/1) = 0.2065, so any threshold at or below it accepts every candidate on
its recruiting failure regardless of true rate. Bar-batch median anchors sit inside that zone
throughout the low-to-mid regime (p = 0.1 seeds 0.061, 0.3 seeds 0.138, 0.5 seeds 0.250). In
simulation L4b loses the bug in 33% of trials to p = 0.02 noise accepts, matching 001's P0,
which lost 34% on L4. Findings S1/S2 confirmed.

**Constants decided** (decisions 54–56, all in `hegel-c/src/native/nd/mod.rs`):

- `GAUNTLET_MIN_FAILS = 4`. The L4b accept-rule ladder (bug kept / cost vs shipped): shipped
  51%/1.00x, m2 73%/1.37x, m3 97%/2.60x, m4 100%/3.01x, m3-recruit-excluded 98%/3.39x. m = 4
  costs 1.00x on L1/L3/L4/D1, so its entire 3x cost and its entire value live in the target
  regime.
- `GAUNTLET_FLOOR = 0.05`, now derived: 0.05 < LCB(4/30) = 0.0531, the min-fails acceptance
  boundary at the 30-run cap, so the floor costs zero power at m = 4 (at 0.08, power falls to
  0.618 of the ceiling). False accept at the floor is 4.0e-4 unconditional per proposal and 0.5%
  per shrink at the measured exposure of 13 candidates. Resolves finding S4.
- `ANCHOR_SEED_RUNS = 20` at both seeding sites. 40-run seeding is structurally excluded:
  LCB(40/40) = 0.912 exceeds the cap-reachable LCB(30/30) = 0.887, so under gamma = 1 no
  candidate can match a deterministic incumbent and those cells stall outright.
- `RETENTION_HIGH_WATER = 0.8`, gamma 1.0 at or above it: a zero-miss detector rather than a
  tuning dial, since with 20-run seeding only LCB(20/20) = 0.839 reaches 0.8. It converts D2's 33%
  displacement of deterministic incumbents to zero, at +26% L1 cost (missing the gate-G6 letter
  by six points, accepted as decision 2's intended behaviour).
- `BOOST_RELIABILITY_FLOOR = 0.30` in extended-20 LCB units, the boundary image
  LCB(10/20) ≈ 0.299 of "true rate below 0.5" through the honest estimator. Decision 28's
  literal 0.5 triggered on 59% of true-0.7 incumbents versus 5% at 0.30.
- The recruiting run stays counted (exclusion's 7x fresh-ledger DP advantage does not survive
  ledger retention across retries, and m4 dominates it outright) and z stays 1.96. The DP rows
  become the specification, pinned by `gauntlet_matches_the_008_operating_points`.

Composition mattered: extension without min-fails is worse than shipped (51% versus 67% L4b
retention), because honest 20-run ledgers stop the anchor ratcheting to 0.2065 flukes while the
accept rule stays degenerate, so neither piece works alone. The chosen rule's drift envelope has
L1 final-p median 0.82, L4b 100% kept at 2,831 median executions, and 100% bug kept on every
landscape. The weighting columns showed w = 0.2 preserves every headline at +60% L1 cost but
w = 0 breaks min-fails itself (L4b 89%, D2 39%), so the constants were marked PRELIMINARY until
009a measured the real weight distribution.

**Revision: the in-engine spot check** (appended for phase 12). A new frozen crate,
`/experiments/gauntlet-calibration`, drove the fixed engine (at c68a89eb) through the public C
ABI on 003's bodies, 100 seeds per cell, with no `nd_force`: runs start deterministic and flip
on production detection, itself part of what was measured. The recalibrated mechanics reproduced
their simulated behaviour wherever a confirmed origin entered shrinking, but three headline
verdicts all sat in the deterministic-to-ND seam: L1 final-p median 0.34 against the 0.82 envelope
(pre-flip `update_interesting` displacement walks the incumbent down before ND handling exists),
caveat-only rates of 15% (L4) and 49% (L4b) where the failure fails the run but no
counterexample is reported, and D2 passing trivially with 0/100 flips. This spot check raised
gate G20: the seam is outside 008's model, and fixing it was remediation work beyond these
constants.

## 009a: off-ceiling watermark measurement

**Question.** What the flat-length clone-descending verbatim watermark (decision 45) records on
genuinely racy bodies off 007's ceiling: (H1) whether miss weights recover from the W1 degeneracy
or stay near zero, (H2) the resulting bar and gauntlet operating points and which 008 weighting
column the distribution selects, and (H3) whether off-ceiling database reuse and blob replay hold
the decision 11/31 design points.

**Method.** The frozen `/experiments/watermark` crate drives the real engine through the
`hegeltest` frontend, with a `__bench` hook (`nd::watermark_dump`) recording (stored, realized,
weight, failed) at every measurement replay. The pre-decision-45 scalar-prefix weighting is
recomputed offline from the same pairs. Two bodies with hidden seeded schedules stand in for
thread interleaving, failing at rate p independent of drawn values, with structural divergence
injected at disjoint value ranges: *clone* (one clone stream, retry draw at 0.15/round) and
*machine* (two worker clone streams shaped like `run_concurrent`). The cells are
{clone, machine} x p in {0.1, 0.3, 0.9} at 200 episodes each, an episode being a discovery run
and, when confirmed, a reuse-only run plus one `reproduce_failure`. Bar and gauntlet numbers are
empirical replays of the `nd::discovery_bar` and `nd::gauntlet` arithmetic over resampled
weights.

**Results, H1.** The new watermark's W50 is 0.400/0.444/0.429 (clone at p = 0.1/0.3/0.9) and
0.278/0.294/0.333 (machine), with the share at weight zero exactly 0.000 everywhere. The old
weighting on the same pairs put W50 at 0.000 in every cell with 78–97% of misses at exactly
zero, which is 008's w = 0 breakage column in practice. The measured distribution sits strictly
between 008's w = 0.2 and w = 1.0 columns, both of which preserve every 008 headline, so the
phase-12 constants froze and the w = 0 escalation path was ruled out.

**Results, H2.** Fluke rejection cost falls from 37-always to a median of 20–21 (clone) and
26–27 (machine) physical replays. Machine misses the <= 20 letter because the cost is
10/mean-weight, a body property, accepted in decision 57. The escalation signal did not fire
(median confirmed anchor at true p = 0.1: 0.106 clone, 0.130 machine, against the <= 0.2 line),
so decision-14 machinery stayed closed. Where a threshold exists (p = 0.9 anchors) gauntlet
rejects go from 30-always-cap with no proofs to 96–100% proof-rejects at median 19–25. At lower
anchors the shipped m = 1 rule still accepts every fluke on its recruiting failure, owned by
phase 12.

**Results, H3.** Database reuse held 98.5–100% and blob replay 98–100% at p in {0.1, 0.3}, so no
flag fires and decision 31 stands. The one dip was clone blob replay at 90% at p = 0.9.
Instrumented, 23/200
episodes never flipped into ND handling (at p = 0.9 the verify replay almost always reproduces),
emitted v1 exact-choice blobs, and those reproduced at 3/23 (13%) while all 177 v2 blobs
reproduced. Database reuse held 99% on the same episodes because the reuse path allows
continuation.

**Consequence.** Decision 57 closed gate G9 (the shipped watermark stands, and no physical gate
is added, since it would guard a regime the measurement says is empty). Decision 58 closed G10,
recording the v1-blob fragility under gate G20's seam family, and the never-flip finding fed
decision 59 (`V1_BLOB_REPLAYS = 4`) and experiment 012's criteria. The first full run also
crashed the engine (`try_replace_with_deletion` indexed `current_nodes` with a stale index after
an accepted mid-pass candidate), fixed with a bounds guard and pinned red-green by
`bind_deletion_survives_an_adopted_candidate_shorter_than_the_probe_index`, the one production
change phase 11 made.

## 009b: composed-rules re-verification

**Question.** Do 009a's operating points hold once the composed rules (min-fails 4, 20-run
seeding at both sites, the derived floor, gamma 1.0 at or above the retention high-water)
replace the shipped arithmetic, and what does false accept measure at q = 0.02? The in-engine
half of 009b is the spot check appended to the 008 notes.

**Method.** `/experiments/watermark` gained a `composed` subcommand (009a's code path
untouched), re-running the 009a episode protocol with the same bodies, cells, and seeds against
the engine at the phase-12 commit, then replaying the composed arithmetic over re-measured
weights, plus a false-accept replay at q = 0.02. About two hours, byte-identical.

**Results.** Reported and with-blob counts match 009a exactly, because those are decided at or
before the first bar accept, which phase 12 does not touch. The in-engine cost of phase 12 is
1.56–2.08x measurement replays at p <= 0.3 and 4.01–6.03x at p = 0.9, fail-heavy top-ups against
near-deterministic evidence, matching 008's prediction of where cost concentrates. The
escalation signal again does not fire (anchors 0.108/0.131 at true p = 0.1).

At p = 0.9 the extension replaces the stopping rule's pinned 0.510 anchors with 0.764/0.779,
below the 0.8 high-water, so genuinely racy p = 0.9 bodies keep gamma 0.8 and only zero-miss
evidence crosses to gamma 1.0, as decision 55 intends. Fluke rejection cost is clone 21–22 and
machine 27–28, the same body-property arithmetic decision 57 accepted. The gauntlet at p = 0.9
anchors reaches 100% proof-rejects at median 10–13, passing the <= 15 and > 50%-proof targets.
At low anchors, m = 4's new fluke rejects ride the 30-run cap with proof share zero, which is
arithmetic rather than defect: 008's DP already priced E[runs | fail] = 29.9 there, and what the
30 replays buy against 009a's accept-share of 1.00 is that the fluke is not accepted. False
accept measures 3.7e-4 (clone) and 4.0e-4 (machine) per proposal at p = 0.1 anchors, at or under
008's DP row. The G10 recheck returns identical counts cell for cell, since the misses live in
never-flipped episodes that execute no phase-12 code. No new decision number was assigned. 009b
validates decisions 54–58 as composed.

## 010: what the data tree buys

**Question.** Test the hypothesis "on real workloads the data tree is rarely buying us anything
and is significant overhead", the first move of gate G20's option (d). It ran on main at 770970b8
(the production engine, with no ND-branch machinery), on local branch
`claude/experiment-010-tree-value`, harness commit 19a47940. The harness patches main's engine,
so it is not frozen under `/experiments`.

**Method.** An environment knob, `HEGEL_EXPERIMENT_NO_TREE=1`, disables the tree's four roles of
recording (which also carries mismatch detection), serving from `cached_test_function`,
novel-prefix generation, and exhaustion, with default-off verified as a zero-behaviour change. A
stats knob dumps per-run counters including duplicate executions by an order-sensitive value
fingerprint. Eight workloads run from tiny boolean spaces through filtered, shrinking, stateful,
and regex bodies, 20 fixed seeds per cell, tree-on versus tree-off, database disabled.

**Results** (per-run means, with executions counting body executions):

| cell | execs tree | execs no-tree | serves/run | dups/run (no-tree) | wall tree (ms) | wall no-tree |
| --- | --: | --: | --: | --: | --: | --: |
| tiny_bools tc=1000 | 4 | 1000 | 0 | 996 | 0.05 | 1.43 |
| filtered_mod3 tc=1000 | 1000 | 3076.8 | 0 | 2252.5 | 10.35 | 18.25 |
| shrink_sum tc=100 | 200.4 | 1308.0 | 1145.5 | 1134.0 | 2.71 | 6.04 |
| stateful_fail tc=100 | 7778.9 | 7137.6 | 831.1 | 2349.8 | 134.16 | 96.59 |
| regex_email tc=1000 | 1000 | 1000 | 0 | 0.4 | 5.34 | 3.71 |

Every failing cell found its bug in both arms and every seed shrank identically. Reading by
role: serving's one large win is non-stateful shrinking (shrink_sum runs 6.5x fewer bodies, with
85% of shrink probes served), but it inverts on the stateful shrink (serve rate 10%, tree-on
running 9% more bodies and 39% more wall for identical shrink quality), and the serve count
approximately equals the no-tree duplicate count (1146 vs 1134), so nearly all serves are exact
repeats and a flat fingerprint cache captures the win. Novel prefix eliminates duplicates but no
cell showed a discovery or shrink-quality difference. Exhaustion is decisive only on tiny or
filtered spaces and never changed a verdict. Recording is pure cost where the other roles are
inert: 40–80% wall overhead on passing cells with zero counter movement (regex builds 15k nodes
for 0 serves).

**Consequence.** The recommendation (serving replaced by a flat fingerprint cache, exhaustion by
a duplicate-counter stop, novel prefix dropped, recording dropped with the mismatch check
re-homed on the cache) was adopted as the tree's removal (decision 60: the execution cache, with
the shrink-heavy count guard holding at 1510 versus 1661, and one recorded regression, where
chain-only recursive generators lose novelty forcing and P(depth >= 10) falls from 0.30 to
0.14), the duplicate stop scoped to the all-invalid grind (decision 61), kind drift re-homed on a
within-run ledger under `error` strictness only (decision 62), and three frontend test
casualties resolved without engine changes (decision 63). The caveats recorded were that
near-free bodies price engine overhead only, that easy bugs leave novel-prefix value on rare
bugs unmeasured, and that one stateful machine shape was tested.

## 011: instrumented seam spot check

**Question.** Decompose the seam losses (flip site and time, caveat-only outcomes split into
correct fluke rejections versus power misses) on the pre-change engine, then accept or reject
the seam-plan engine against the letters table in `seam-plan.md`. The table's letters are the
contract.

**Method.** The frozen `/experiments/gauntlet-calibration` crate was amended with a sequential
`seam` subcommand consuming the engine's `__bench` seam dump (`nd::seam_dump`): every flip's
detection site, call index, and interesting map, and every reject-eviction's evicted incumbent
mapped back to landscape p, with a D0 deterministic control and blob-kind counts added. The 008
subcommands still reproduce the 008 output, so the amendment did not perturb the engine. Each
cell ran 100 seeds at a 500-case budget, the baseline half at ad3ff0c1 (the dump commit, phase
14, before any seam fix) and the comparison half at 532be034 plus the phase-16 coverage/docs
commit.

**Baseline.** This half produced the decomposition that justified the plan. L4b's 49%
caveat-only is 6 correct fluke rejections plus 43 power misses: the bar correctly targets
genuine p = 0.1 incumbents and rejects them at its 45%-per-attempt power with no recycle, so
mechanism 2 is 88% of the loss and the backtrack's many-attempts design aims at exactly this.
L4's 15% is the opposite: 15/15 are correct fluke rejections. L1's incumbent-p at flip is 0.26
at every percentile: displacement has already walked the incumbent to the minimal-bug floor before
any detector fires (flip call median 1004), so everything the workflow must recover exists only
pre-flip. The tree's kind-mismatch flip channel fired zero times in 600 trials, the number
decisions 62 and 64 both cite. Never-flipped runs emit v1 blobs (L4 66, L3 22, L1 5). The D0
control measured 528 median executions.

**Comparison against the letters.** L3, L4, L4b, and D0 pass everywhere: caveat-only falls from
49% and 15% to zero, bug kept is 100/100, and aborted and no-bug are zero in every cell, so the
duplicate stop does not interfere. L1 passes every letter except executions and was escalated:
the final-p median rises from 0.34 to 0.74 against the >= 0.50 letter, but median executions are
17,583 = 1.54x against the <= 1.5x letter (17,139), a 2.6% overshoot with p90 improved
(47,296 vs 52,267). The mechanism on both sides of the trade is the early flip (median call 1004
to 136): displacement
stops walking the incumbent down, and the whole shrink runs gauntleted. D2 was escalated at
68/100 deterministic finals against the 100/100 letter: 30 of 80 flipped trials flip before any
core-bearing sighting exists, and after the flip decision 20's displacement freeze plus the
shrinker's value-lowering lattice cannot reach the core from a three-bug-atom incumbent. None
that held the core lost it, and the baseline's 100/100 was fake, with 0/100 flips and free
displacement. The new engine reports a confirmed p = 0.7 example with a v2 blob instead of the
prettier deterministic core, which is honest reporting at a priced cost but still a miss against
the letter as written.

**Consequence.** Both misses were escalated to DRM with their decompositions and accepted,
closing gate G20. The L1 letters price one of decision 67's two residuals, pre-flip single-run
trust inside a checked origin's shrink. The other, the never-flip share that passes an honest
check, is priced by experiment 012. The baseline half also fed decisions 62 and 64 (the 0-in-600
tree-channel measurement).

## 012: detection-escape recheck

**Question.** Do the phase-16 first-interesting check and the phase-14 v1 continuation fix close
009a's never-flip corner? The criteria were clone p = 0.9 never-flip at most 1/200 (the first
check's escape probability is (p·s)^4, ~3e-4 at the top of 009a's estimate), blob reproduction at
least 199/200 on both bodies at p = 0.9 (baselines 180 and 191), the p <= 0.3 cells holding
009a's >= 98% reuse and blob rates, and the deterministic control recording zero flips and
exactly k x origins measurement replays.

**Method.** A new frozen crate, `/experiments/detection-escape`, clones
`/experiments/watermark`'s episode protocol on the post-seam engine (5d3aadc3), bodies verbatim
from the watermark crate so 009a's baselines carry, with 200 episodes per cell plus a
deterministic control (eight draws, unconditional failure). Each episode runs discovery with
database, blob, and statistics, then a database-reuse run, then a blob-reproduction run. The
`__bench` seam dump records the first flip site.

**Results.** Every criterion passes. Never-flip is 0/200 at clone p = 0.9 (was 23/200) and 0/200
on machine. Blob reproduction is 200/200 on both bodies at p = 0.9 (was 180 and 191), with every
blob v2. Reuse is 100% everywhere, and blob replay at clone p = 0.1 is 189/190 (denominators at
p = 0.1 are the confirmed episodes: 10 clone and 7 machine reported caveat-only, the expected
bar power). Every flip in every ND cell happened at the first-interesting check: the hidden
schedule almost surely perturbs structure within four exact replays. The control pays exactly
four measurement replays per episode (`FIRST_CHECK_REPLAYS = 4`), so the first check is a
deterministic run's whole ND cost.

The surprise finding was a gauntlet cost lottery at high p, escalated rather than fixed. The
ND cells' measurement-replay counts run 240k–3.8M per 100-case episode. A probe decomposition of
clone p = 0.9 episode 0 (1,198,879 replays) found all but one are `nd_replay_once` calls from
the gauntlet's evidence loop, spread over 42,125 distinct candidate timelines at a median of 29
replays each. The mechanism is that above `RETENTION_HIGH_WATER = 0.8` gamma is 1.0, the anchor
ratchet converges to roughly the all-fails cap-length bound (~0.88 at p = 0.9), and from there only
another all-fails batch can accept (probability 0.9^30 ≈ 4%), so nearly every genuine shrink
step rejects at the 30-run cap and is re-proposed later. Shrinking still reaches correct minima
(the blob column proves it), but 011's L4 cell on the same constant p = 0.9 sat at 1,774 median
executions where these bodies pay a million or more, and on a 10 ms body
`MAX_SHRINKING_SECONDS = 300` would truncate the shrink instead. The interaction predates the
seam work, but the universal first check makes every high-p episode pay it from discovery, and
012 is its first measurement. Any fix trades against the no-probability-loss constraint, so it
was escalated to DRM. Decision 67 records it as a follow-up outside G20's loss accounting.

## The program's outcome

000-plan.md's closing entry (2026-09-02, "All six experiments done") records what the
implementation inherited from the original sequence:

- **Statistics** (001, 005A): the gauntlet accepts at LCB >= max(0.8 x anchor, 0.05) against a
  monotone anchor never fed by pinned-regime evidence, with confirmed-dry stopping, no
  checkpointing, and no decay. The discovery bar is gate 1/10 then 4/40 (decision 23).
- **Mechanism** (002, 003, 005B): `serve_replays` is the whole resampling seam, confirmation
  gates origin admission on every path (decisions 20, 21, 24), and the lifecycle runs end to end
  in-engine with 100% cross-run reproduction and caveated reporting.
- **Representation** (004, 006B): a whole-timeline pool at cap 10, first-fit, with a small
  continuation budget. Replay order is pool, then splices, then fresh (decision 25), and the
  trie and per-position anchoring are closed.
- **Boost** (006A): works, cheap, and holdout-gated, shipping behind reporting policy.
- **Deliberately left for implementation**: the blob v2 format, the strictness setting surface,
  demote-to-secondary, span-anchored split points, the `FAILED_NONDETERMINISTIC` ABI semantics,
  and the generation strategy under ND (novel-prefix replacement, and small budgets starving
  narrow structural bugs of discoveries).

Per decision 26 the experiment-grade scaffolding (`nd_experiment`, `nd_boost`, the `nd_*` fields
in `test_runner.rs`) was the seed of the real implementation: the branch was brought to
production quality in place, with extraction and pruning deferred to later work from the
production-grade branch.

The later six experiments revised that inheritance in three ways. 008 with 009a/009b
recalibrated the statistics the first six had calibrated, after the shipped arithmetic turned
out to reproduce 001's naive policy. 010 removed the structure 002 had studied, keeping only its
seam finding. 011 and 012 priced and then closed the deterministic-to-ND seam that 008's own
in-engine spot check exposed, leaving decision 67's two priced residuals and the escalated cost
lottery as the program's open ends (see [the seam plan](seam-plan.md)).
