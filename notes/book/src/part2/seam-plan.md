# G20 and the seam plan

Gate G20, the deterministic-to-ND seam, was the one finding the remediation plan could
not close in place. Raised on 2026-09-03 by phase 12's in-engine spot check
(fa657947, recorded as the gate in fdc30860), it closed on 2026-09-04 as decision 67,
after a mechanism analysis, experiments 010–012, and phases 14–17. The as-built
mechanisms are described in [detection](../part1/detection.md), [the origin
lifecycle](../part1/lifecycle.md), and [the final replay](../part1/final-replay.md). The
recalibration that exposed the seam is in [remediation](remediation.md).

## The finding

Phase 12's spot check re-ran experiment 003's landscapes through the real engine over the
public C ABI, with no `nd_force`: runs started deterministic and flipped on production
detection, itself part of what was measured. The recalibrated shrink mechanics reproduced
their simulated envelope wherever a confirmed origin entered shrinking, but every
headline miss lived elsewhere: on the rising landscape L1 the final failure probability
median was 0.34 against the simulated 0.82 envelope, and caveat-only rates were 49% on
the target-regime landscape (L4b) and 15% on L4. Caveat-only failures were reported
but unshrunk, unconfirmed, and unpersisted.

The gate named the mechanism: production enters ND handling lazily, and a run that
flips late has already spent its deterministic window. Pre-flip `update_interesting`
displacement walks the incumbent down the landscape before decision 20's guard exists,
and a discovery-bar rejection at the shrink verify has no generation budget left to
re-hunt. Experiment 009a had added the same seam from the opposite direction: at p = 0.9,
11.5% of clone episodes never flipped at all, persisted v1 exact-choice state, and their
blobs reproduced at 13% where every v2 blob reproduced. Decision 58 filed it under G20's
family: blob quality depended on whether the run noticed its own nondeterminism.

The remediation plan recorded three options, (a) accept and document, (b) re-enter
generation on a late flip, and (c) guard displacement pre-flip at deterministic-run cost,
and left the gate undecided through the phase-13 exit audit as the one open item.

## The analysis: not the data tree

`notes/research/g20-seam-analysis.md` (2026-09-04, 0d639eb3) answered DRM's framing
question: what actually drives the seam, and is it just the data tree? The answer was
no. The tree was net protective on the seam workloads, supplying one of the run's two behavioural
detectors, with no measured loss attributed to it. The cliff was single-run trust, not
caching: each deterministic-branch decision rule treats one observation as proof and
takes an irreversible action on it, and the ND guards exist only behind `nd_active`, a
boolean set by sparse detection events.

It ranked three loss mechanisms:

1. **n = 1 displace-and-persist.** Pre-flip, every interesting run displaces an occupied
   origin on shortlex alone and immediately rewrites the database. This was the whole L1
   loss. The anchor then seeds from the degraded incumbent and monotonically ratifies it.
2. **A late flip meets spent budgets.** Decision 23 priced the bar's 45% per-attempt
   power on the premise that rejects recycle through rediscovery (decision 21), which
   holds only while generation is alive. A flip at the shrink verify gets one attempt
   with zero re-hunt budget, which produced the 49% caveat-only rate.
3. **The never-flip corner.** Episodes with no detection event persisted v1 state whose
   blobs reproduced at 13% while their DB reuse held 99% on identical bytes. The cause
   was replay semantics (a single attempt with zero continuation) rather than example
   quality.

Detection is inherently sparse. For verdict-only nondeterminism there was no channel
before the verify at all: the tree flagged only choice-kind changes at shared prefixes
and overwrote a differently-concluding leaf without recording anything, and detection
power vanishes as p → 1. Continuity costing found two cliffs. On trust, pre-flip rules
act irreversibly on n = 1, though most pre-flip protections would be free to run
unconditionally. On cost, every post-flip accept pays ~20 physical runs, which is the
reason a mode split exists at all.
"`nd_active` swaps roughly seven behaviors at once with nothing in between."

It then priced eight graded-middle options:

1. a bounded history of displaced incumbents, consulted on flip
2. provisional pre-flip pool capture
3. verify-as-bar-run-1, where the already-paid shrink-entry verify becomes the batch's
   first evidence instead of a discarded boolean
4. trusted admission from k ≥ 2 pre-flip sightings
5. a bounded re-hunt on a late flip
6. v1 blob continuation and retry
7. provisional persistence from caveat-only runs
8. routing measurement replays through the tree as evidence conditional on determinism

Its own summary was that options 1–3 plus 6 close most of the measured loss at zero
deterministic-run cost.

## Option (d)

DRM proposed a fourth option the same day (3db56b15), accepted the same day: a four-step
workflow. Ditch the data tree, with experiment 010 measuring what it buys. Check each
origin's first interesting case with k exact replays for structural alignment and
outcome, flipping on a miss with the observations seeding the bar. Otherwise proceed
deterministically, keeping a per-origin history of every interesting execution. Finally,
check the shrunk case and, on a miss or any later ND evidence, backtrack the history to
the newest entry that clears the discovery bar and resume ND-mode shrinking. It replaced
options (b) and (c): history preserves what displacement destroyed, and the backtrack
gives the bar many attempts instead of one.

## Experiment 010: what the tree buys

Experiment 010 ran on main at 770970b8, the production engine with no ND-branch
machinery, its harness on a separate local branch rather than frozen under `/experiments`
because it patches the engine. An env knob disabled all four of the tree's roles at once.
Eight workloads ran 20 fixed seeds per cell, tree-on against tree-off.

| role | measured | disposition |
| --- | --- | --- |
| serving | 6.5x fewer bodies on the non-stateful shrink (200 vs 1308, with 85% of shrink probes served), but inverted on the stateful shrink (10% serve rate, 9% more bodies, 39% more wall); serve count ≈ no-tree duplicate count (1146 vs 1134), so serves are almost all exact repeats | replace with a flat fingerprint(choices) → outcome cache |
| novel prefix | eliminates duplicates, which matters only on small spaces, and no cell showed a discovery or shrink-quality difference | drop |
| exhaustion | decisive on tiny and filtered spaces (4 vs 1000 executions, and the filtered cell stops at the 334 valid cases that exist instead of 3077 executions), inert elsewhere, and never changed a verdict | replace with a duplicate-counter stop |
| recording | pure cost where the other roles are inert: 40–80% wall overhead on passing cells with zero counter movement | drop with the tree and re-home the mismatch check on the cache |

Every failing cell found its bug in both arms and every seed shrank identically, and the
recommendation unblocked option (d): "Ditching the tree is supported with those two
replacements" (remediation-plan.md). The flat cache also gains a channel the tree never
had: the same fingerprint concluding with a different status or origin is a verdict
flip, exactly the outcome-ND evidence the tree silently discarded.

## The plan and its revisions

`notes/seam-plan.md` (163026d8) mapped the workflow onto phases 14–17 and gates G21–G26,
continuing the remediation plan's numbering. It was adversarially reviewed before commit
and twice revised under DRM review:

- The backtrack walk became a geometric boundary scan (cc8a0c1f): a linear newest-first
  walk burns the scan budget on the degraded tail and re-runs mechanism 1 in miniature,
  since the permissive bar admits a degraded-but-genuine entry and the anchor ratifies
  the loss. The scan instead probes at geometric offsets and binary-refines towards the
  reproduction boundary, biased old under uncertainty.
- History became keep-everything (4b997a54). Evict-oldest deletes the boundary exactly
  when shrinking went nondeterministic early, and the memory the bound defended against
  left with the tree. The recorded fallback is middle decimation rather than end-eviction.

## Phase 14: instrumentation and independent fixes

Phase 14 made five landings (ad3ff0c1..82f83966, closed as decision 59). The `__bench`
seam dump (`nd::seam_dump`, hegel-c/src/native/nd/mod.rs) recorded every flip's detection
site, call index, and interesting map, and every reject-eviction's evicted incumbent.
Experiment 011's baseline half then ran at the dump commit, before any fix, and supplied
the decomposition the seam analysis said was missing:

- L4b's 49% caveat-only was 43 bar power misses and 6 correct fluke rejections, so
  mechanism 2 was 88% of the loss. L4's 15% was 15/15 correct fluke rejections, which is
  the bar working as designed on p = 0.02 noise.
- L1's incumbent-p at flip was 0.26 at every percentile (median flip call 1004):
  displacement had walked the incumbent to the minimal-bug floor before any detector
  fired, so everything the workflow had to recover existed only pre-flip.
- The tree's kind-mismatch flip channel fired zero times in 600 trials. On outcome-only
  ND every flip came from the shrink verify or the final replay, a number decisions 62
  and 64 both cite.
- Never-flipped runs emitted v1 blobs: 66 on L4, 22 on L3, 5 on L1.

The other three landings were independent fixes. The v1 blob fix (5936016a, the
analysis's option 6) replaced one bare `for_choices` replay with up to
`V1_BLOB_REPLAYS = 4` `for_probe` attempts under the standard continuation budget,
bounding the worst-case joint escape-then-miss at 1.2e-3. `nd_evidence_batch` was fixed
to restore `capture_replays` on exit instead of clearing it (03b47e98). The per-origin
shrink was extracted, behaviour-identical, into `shrink_origin` (82f83966) so report-time
backtracking could later re-enter a shrink.

## Phase 15: the tree removal

Commit 41ed08c6 deleted `hegel-c/src/native/data_tree.rs` (825 lines) with its tests,
span-event feeds, and the novel-prefix generation arm. Its live roles went to the
two-tier execution cache (`hegel-c/src/native/exec_cache.rs`, 244 lines) and the
duplicate stop, whose as-built mechanics are in [detection](../part1/detection.md).
Decision 60 closed decisions 6 and 29, and the invariant that ND handling never serves
cached conclusions survives verbatim on the cache, which the flip flushes.

Three as-built deviations from the plan:

1. The planned reuse-comparison flip channel was not built: between-run divergence is
   routinely staleness, and flipping on it would punish every legitimate generator
   refactor, exactly what decision 9 forbids. The replacement is the kind ledger, an
   `error`-only within-run detector (decision 62). Quiet/warn lost their
   generation-level channel until phase 16's check.
2. The duplicate stop was scoped to the all-invalid grind (decision 61) after the
   unconditional G22 version ended a 32-way `one_of` early: `DUPLICATE_STOP = 10` fires
   only while no valid case exists.
3. G21's kinds-on-hit comparison was subsumed by the ledger.

Both cost guards held: passing-body parity was exact at 50 executions on the pinned seed, and
the shrink-heavy body came in at 1510 executions against the 1661 bound, realising 010's serve
win. Two regressions were priced rather than fixed: chain-only recursive depth spread
(decision 60) and three frontend casualties resolved without engine changes (decision 63).

## Phase 16: history, check, backtrack

Steps 2–4 landed in dependency order (532be034, closed at 5d3aadc3 as decisions 64–66),
history first because the check and the backtrack both read it.

- **History** (decision 65, gate G24): `record_run`'s interesting arm appends every
  pre-flip interesting execution to an unbounded per-origin history: raw sightings and
  shrink accepts alike, deduplicated by serialized nodes and dropped on confirmation or
  run end. The hook is the arm rather than `update_interesting`: after a fluke displaces the
  incumbent, genuine later sightings are shortlex-larger and never displace, yet they are
  what the scan needs.
- **The first-interesting check** (decision 64, gates G23/G26): each
  generation-discovered origin's incumbent sighting at sweep time replays
  `FIRST_CHECK_REPLAYS = 4` times exactly before anything consumes it, stopping on the
  first miss. A miss flips the run
  and seeds the origin's discovery bar. Detection is 1 − (p·s)⁴, a
  deterministically-failing origin pays exactly +4 executions, and a passing run pays
  nothing. It extends decision 21's principle (the discovering case is selection, not
  evidence) to every run.
- **Backtracking** (decision 66, gate G25): a never-confirmed origin that misses its
  shrink verify or final replay scans its history for the reproduction boundary, capped
  at `BACKTRACK_SCAN_REPLAYS = CONFIRM_CAP = 40`. The settled candidate faces the full
  discovery bar, up to `BACKTRACK_BAR_ATTEMPTS = 3` batches composing to ~83%
  target-regime power. A cleared bar confirms the origin, force-persists the restored
  incumbent (`supersede_nd`), and re-shrinks under the gauntlet. Decision 66 amends
  decision 17's title clause: detection-triggered, bar-gated recovery of recorded state
  is not the statistical rollback 17 rejected.

Gate G26 signed the contract amendments: `error`-mode diagnostics for check misses (an
aligned-outcome miss aborts as flaky, an as-built deviation), first-check replays running
stamped (amending decision 49), and decision 51's statistics line counting pre-flip check
replays. Step 3 deviated from red-first: the backtrack landed before its tests, which
were then sensitivity-checked by mutation.

## Phase 17: validation

Experiment 011's comparison half ran the same cells on the phase-16 engine against the
plan's acceptance table, whose letters the plan had declared the contract. L3, L4, L4b,
D0, and the all-cells rows passed outright. Caveat-only fell from 49% to 0 on L4b (the
seeded bar plus backtrack recycling) and from 15% to 0 on L4 (fluke incumbents flip at
the check before the bar sees them at shrink time). The bug was kept 100/100 everywhere
but D2 (99/100), the aborted and no-bug rows were 0, pinning duplicate-stop non-interference, and the
model-validation column matched (flipped-at-first-check shares tracked 1 − p⁴, less
cache-mismatch preemptions).

Two letters missed and were escalated with decompositions:

- **L1 execs 1.54x against the ≤ 1.5x letter** (median 17583 vs 17139), a 2.6% overshoot
  with the p90 improved. The early flip (median call 136 rather than 1004) is the
  mechanism on both sides of the trade: displacement stops walking the incumbent down, so
  the final-p median doubled from 0.34 to 0.74, and the whole shrink runs gauntleted,
  which is the cost.
- **D2 deterministic finals 68/100 against the 100/100 letter.** Thirty of the 80 flipped
  trials flip before any core-bearing sighting has displaced the incumbent, and post-flip
  the displacement freeze plus the shrinker's value-lowering lattice cannot reach the core from a
  3-bug-atom incumbent. None that held the core lost it. The baseline's 100/100 came
  from 0/100 flips and free displacement. The new engine reports a confirmed p = 0.7
  example with a v2 blob instead of the prettier deterministic core. The honesty was
  priced, but it is still a letter miss as written.

Experiment 012 closed the never-flip corner. A new frozen crate,
`experiments/detection-escape`, re-ran 009a's episode protocol, its bodies verbatim, on
the post-seam engine, and every criterion passed. Never-flip was 0/200 in every ND cell
(baseline 23/200 on clone at p = 0.9), with every flip landing at the first-interesting
check. Blob reproduction was 200/200 on both bodies at p = 0.9 (baselines 180 and 191),
all blobs v2, and the p ≤ 0.3 cells held 009a's ≥ 98% reuse and blob rates. The
deterministic control paid exactly 4 measurement replays per episode: the first check is
a deterministic run's whole ND cost.

012 also surfaced the shrink gauntlet's cost lottery at high p: ND cells paid 240k–3.8M
measurement replays per 100-case episode, nearly all gauntlet evidence. One clone
p = 0.9 episode decomposed into 42,125 candidate timelines at a median of 29 replays
each. Above the retention high-water gamma is 1.0, the anchor ratchet converges to
roughly the all-fails cap-length bound (~0.88 at p = 0.9), and from there only another
all-fails batch accepts (probability 0.9³⁰ ≈ 4%), so nearly every
genuine step rejects at the 30-cap and is re-proposed later. Shrinking still reaches
correct minima, though a 10 ms body would hit `MAX_SHRINKING_SECONDS = 300` first and
stall part-way. The interaction predates the seam work, and the universal first check
just makes every high-p episode pay it from discovery. Any fix trades against the
no-probability-loss constraint, so it was escalated to DRM rather than fixed, outside
G20's loss accounting.

## Decision 67: closure and priced residuals

Decision 67 closed G20 by mapping option (d) back onto the analysis's graded options.
Option 1, the displaced-incumbent history, became decision 65's unbounded history plus
decision 66's bar-gated backtrack, unbounded and scanned rather than a newest-first
ring, per DRM. Option 2, provisional pool capture, is the same history, since the backtrack pools
its other reproducing entries. Option 3's intent landed at decision 64's first check,
while the shrink-entry verify itself stays a boolean. Option 6 landed in phase 14.
Options 4, 5, and 7 stayed untaken as bounded-cost trades the measured loss no longer
justified, and 8 stayed out of scope.

Two residuals were left deliberately, each with a price attached:

1. **Pre-flip single-run trust inside a checked origin's shrink**, priced by 011's L1
   letters: final-p p50 came in at 0.74 rather than the 0.82 envelope, and execs at 1.54x
   against the 1.5x letter. An origin that passes an honest first check still shrinks on single-run trust
   until something flips the run.
2. **The never-flip share that passes an honest check**, priced by 012 at 0/200 episodes
   per cell against the 23/200 baseline, with blob reproduction 200/200 at p = 0.9. The
   check's escape rate (p·s)⁴ is small but not zero, and what escapes now carries a v1 blob
   whose four budgeted continuation replays keep it reproducible.

The phase-17 closing sweep re-verdicted evaluation.md, updated design.md and the
changelogs, and ran the full gate list green. Commit 43b2eb35, the branch's last phase
commit before the [review fixes](../part1/status.md), landed experiment 012, decision 67,
and the sweep.
