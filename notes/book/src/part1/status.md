# Where the plan stands

Every phase of the three plans has landed. The production plan (phases 1–8, gates G1–G4) took
the experiment-era scaffolding to production grade. The remediation plan (phases 9–13, gates
G5–G20) fixed the 41 findings in the as-built register and recalibrated the statistics. The
seam plan (phases 14–17, gates G21–G26) resolved G20, the deterministic-to-ND seam. The last gate closed
as decision 67 on 2026-09-04 (commit 43b2eb35). Four review-fix commits from a final
adversarial review of the whole branch sit on top (through 48894dc4), and the branch has
continued past them: decisions 68 and 69 (2026-09-07) restored targeting under ND handling
as a measured race, with experiment 013 deriving its constants (see
[shrinking](shrinking.md)); decisions 70 and 71 (2026-09-07) removed the concurrency
declaration channel, so detection is by observation only, and the verbatim-watermark
weighting, so evidence is plain (fails, runs); decision 72 (2026-09-07, experiment
014 and `research/fcr-analysis.md`) added multiplicity control — per-origin bar and
backtrack attempt budgets, per-proposal gauntlet alpha spending with an escalating
failure minimum, and a bar on the pooled review's reproducing run in place of the
any-failure rule (see [the lifecycle](lifecycle.md), [shrinking](shrinking.md), and
[the final replay](final-replay.md)); and decision 73 (2026-09-10) gave the failing
test case one representation, `Counterexample` (`counterexample.rs`), replacing the
engine's interesting map, `OriginLifecycle`, and the separate history, alpha-budget, and
first-check containers (see [the lifecycle](lifecycle.md)). On 2026-09-10 `main` (at
799451cc) was merged into the branch: the pieces already extracted to main (the execution
cache, the persister's save discipline, the blob decode bound, the `bind_deletion` guard,
the targeting mismatch propagation) reconciled against the branch's versions, and main's
other changes taken — fallible choice serialization (`MAX_CLONE_DEPTH`), the frontend's
`ffi::sys` loader, worker attribution and blocks in the printer, `Unsatisfiable`, the
Antithesis settings, and the removal of single-test-case mode. Part II
tells the story ([production](../part2/production.md), [remediation](../part2/remediation.md),
[seam plan](../part2/seam-plan.md)). This chapter records the state as of the plans'
close plus that addition.

The phases form one continuous series across the three plan documents, each landing as
ordinary commits green under `just check` and `just check-coverage`. The production plan's
exit was "the branch is the artefact decision 26 describes". The same day, the adversarial
as-built review produced the finding register and the branch continued into remediation.
Remediation's exit audit (verified 2026-09-03) traced all 41 findings to landed fixes with
pinning tests or recorded decision entries. The deliberate retentions (z = 1.96, the
asymmetric miss weighting, `FINAL_REPLAY_FRESH` chosen not derived) are decisions 53/54;
the miss weighting was later superseded by plain counting (decision 71). The
audit carried exactly one item open: G20, raised by phase 12's in-engine spot check. The seam
plan closed it. The phase-17 closing sweep's full gate run (check, c-test, check-docs, miri,
minimal-versions, package) is green.

## Gate register

Gates G5–G19 were resolved in one sitting, wholesale as recommended (decision 34), and the
detailed entries landed with their fixes. G20 was the only gate to spawn a plan of its own.

| Gate | What it gated | How it closed |
| --- | --- | --- |
| G1 | ABI shape for ND failures | FAILED plus `hegel_failure_caveat`, with status 3 retired and reserved (decisions 27/43) |
| G2 | Boost default | Reliability-floor heuristic, no public setting (decision 28); floor re-derived to 0.30 (decision 56) |
| G3 | Data tree under ND | Disabled (decision 29); superseded by removal, the invariant surviving on the execution cache (decision 60) |
| G4 | Strictness surface | `quiet \| warn \| error` confirmed, default quiet (decision 30); `error` aborts earlier after decisions 60/64 |
| G5 | Anchor estimand | Reproduction rate under pinned replay, raised only at validated events (decision 46) |
| G6 | Retention shape | Gamma schedule: 0.8 below the retention high-water, 1.0 at or above (decision 55) |
| G7 | Boost floor units | 0.30 in 20-run-batch LCB units, holdout = `ANCHOR_SEED_RUNS` (decision 56) |
| G8 | Watermark landing order | Recursive clone-descending watermark in phase 10, validated by experiment 009a (decision 45); the watermark itself is superseded by plain counting (decision 71) |
| G9 | Physical backstop (`PHYS_GATE`) | Closed no-change: the escalation signal never fired (decision 57); moot since decision 71 removed weighting |
| G10 | Off-ceiling reproduction rates | Closed keeping decision 31: reuse/blob ≥ 98% at p ≤ 0.3 (decision 58) |
| G11 | Stamping the unconfirmed report | Generation cases stamped once `nd_active` (decision 49), amended by G26 |
| G12 | Trusted-shrink anchor | Evidence-batch LCB; trusted origins exempt from the bar's verdict, not spared replay (decision 47) |
| G13 | Stored pool at trusted promotion | Merged fresh-first, deduplicated, capped at `POOL_CAP` (decision 48) |
| G14 | Same-run supersession | Save-then-delete; a superseded same-run save is deleted, never demoted (decision 44) |
| G15 | Secondary corpus bound | `SECONDARY_CORPUS_CAP = 50` per key, shortlex-largest evicted at reconciliation (decision 44) |
| G16 | Capture-predicate rename | `hegel_test_case_should_capture` with no shim, as a compile-time migration signal (decision 50) |
| G17 | ND line under `show_statistics` | One line: measurement replays and failures (decision 51), amended by G26 |
| G18 | Splice budget | `REPRODUCE_SPLICES` restored to 10, citing 006B's cap-10 measurement (decision 52) |
| G19 | `FINAL_REPLAY_FRESH` disposition | Documented as chosen, not derived (decision 53) |
| G20 | The deterministic-to-ND seam | Option (d), the seam plan's four-step workflow; closed with two priced residuals (decision 67) |
| G21 | Execution-cache scope and bound | Two-tier cache, byte-bounded serving tier, flushed and off under ND (decision 60) |
| G22 | Duplicate-stop semantics | N = 10 = `RANDOM_GENERATION_BATCH`, scoped as built to the all-invalid grind (decision 61) |
| G23 | First-check budget | k = 4 exact replays, stop on first miss; a miss flips the run and seeds the bar (decision 64) |
| G24 | History retention | Keep everything, deduplicated, no eviction; dropped at confirmation or run end (decision 65) |
| G25 | Backtrack scan and budgets | Geometric scan plus binary refinement, ≤ 40 scan replays, ≤ 3 bar attempts (decision 66) |
| G26 | Contract amendments for the check | Decisions 30/49/51 amended: position-naming `error` diagnostics, stamped check replays, counted on the statistics line (decision 64) |

## Evaluation verdicts

`notes/evaluation.md` is the standing audit of the decision log, covering every entry 1–33
with where it lives in the implementation. It was written 2026-09-03 against the phase-7 tree
as production phase 8's exit, then re-verdicted in place as the remediation and seam phases
landed, so its rows now cite decisions through 66. The verdicts fall into four shapes.

Most rows read implemented as decided: the quiet default (1), caveated unreproduced failures
(3), per-origin identity (4), self-identifying persistence (8), within-run detection only (9),
capture at confirmation (10), two-strike hygiene (11), confirmed-dry stopping (18), the
discovery bar (23), blob replay-until-failure (33).

A second group is implemented after recalibration by experiment 008. The gauntlet gains a
4-failure minimum and 20-run seeded anchors (rows 2/7/19, decisions 54/55), the boost floor
moves from 0.5 to 0.30 in the new estimator's units (row 28, decision 56), and
`REPRODUCE_SPLICES` returns to 10 after decision 25's transcription error (decision 52).

A third group is closed or superseded. The data-tree rows (6/29) close with the tree's
removal, and the "never serve cached conclusions under ND" invariant survives verbatim on the
execution cache (decision 60). Per-position anchoring (14) closes on experiment 007's ceiling
rates and stays closed off-ceiling (decisions 31/57/58). The branch-process row (15) is
superseded by decision 26.

The last group is annotated by the seam work:

- No-checkpoint/rollback (17) now excludes decision 66's detection-triggered, bar-gated
  backtrack.
- The displacement freeze (20) gains the history and the reuse-replay exemption
  (decisions 65/66).
- Discovery confirmation (21) extends to every run via the first-interesting check
  (decision 64).
- The bar row (23) gains seeded batches and backtrack bar attempts (decisions 64/66).
- Trusted admission (24) is reworded by decision 47.
- The strictness row (30) notes that `error` now aborts earlier than the engine it emulates
  (decisions 60/64).

## The review fixes

After phase 17, a final adversarial review of the whole branch produced four commits on
2026-09-05:

- b41de5a6 (engine ND handling and shrink scheduling): A flip during a successful
  deterministic final replay re-enters the pooled review. A dry pooled review backtracks over
  origin history before rejecting. A bar accept requires an in-batch reproducing replay, so a
  first-check seed cannot carry the whole quota. Superseding a reused run-start entry demotes
  it to the secondary key instead of deleting it (decision 11). The shrink stall guard is off
  during confirmation sweeps. The fast reject consults the ledger before rejecting a
  conclusively accepted timeline.
- 67653632 (blob encoding and decode hardening): Encoders keep the raw form for payloads past
  `MAX_DECOMPRESSED_LEN`, so the decoder's inflation bound never rejects the encoder's own
  output. `decode_nd_state` rejects trailing bytes after the last timeline.
- 0f504c52 (ND origin lifecycle): A write-only Unconfirmed rejection counter is removed, the
  actual Unconfirmed-to-Confirmed admission paths are named in the state-machine docs, and
  lifecycle tests are pinned to what they claim.
- 48894dc4 (frontend, ABI docs, and notes): Capture/blob documentation is aligned across both
  crates with the header regenerated, two report-site `expect`s are replaced with
  internal-error framing, a new C-ABI test replays an ND blob through `hegel_run_start_blob`,
  and the notes are corrected, including the C2/C9/C10 annotations and the audit
  count 40 → 41.

Two of these reverse remediation-era choices: superseding a reused run-start entry now
demotes rather than deletes, and the decode bound no longer rejects the encoder's own output.

## What remains

### Extraction

Under decision 26 this branch is the artefact, not the destination: extraction and pruning of
the final implementation happen later from the production-grade branch, and `notes/README.md`
records the same intent. Extraction is deliberately outside all three plans and is the
project's remaining step.

### Priced residuals

Decision 67 closes G20 leaving two residuals, each priced rather than fixed:

- Pre-flip single-run trust inside a checked origin's shrink, priced by experiment 011's L1
  letters: final-p median 0.74 (baseline 0.34) and executions 1.54× baseline against the
  ≤ 1.5× letter.
- The never-flip share that passes an honest first-interesting check, priced by experiment
  012 at 0/200 episodes per cell (baseline 23/200), with blob reproduction 200/200 at
  p = 0.9.

Both escalated 011 letters were accepted with decompositions. Alongside L1's execution
overshoot, D2 reports 68/100 deterministic finals against a 100/100 letter whose baseline was
fake (0/100 flips, free displacement): kept 37, gained 13, lost 0, never-flipped 30. A missed
run reports a confirmed p = 0.7 example with a v2 blob where free displacement would have
found a smaller, deterministic core.

Decision 72's multiplicity budgets add three more priced costs (experiment 014): a
sub-target p = 0.05 bug confirms in 42% of runs instead of near-certainly given a long
one, recycling across runs; a mixed bug-plus-fluke origin's bug confirm falls
0.95/0.72/0.45 at fluke share 0/0.5/0.75; and the pooled review's power at p = 0.1
falls 0.97 → 0.44 before the backtrack rescue, in exchange for a ~170x cut in fluke
confirms and a bounded per-origin false-accept spend.

### Escalated follow-up

Experiment 012 also measured the gauntlet's cost lottery above the retention high-water: ~1M
measurement replays per constant-p = 0.9 episode, because at gamma 1.0 only another all-fails
batch (probability ≈ 0.9³⁰ ≈ 4%) can accept a genuine shrink step, so nearly every step
rejects at the 30-run cap and is re-proposed. Shrinking still reaches correct minima, and on
slow bodies `MAX_SHRINKING_SECONDS` truncates instead. It was escalated with no fix chosen,
since every candidate fix trades against decision 2, and recorded outside G20's loss
accounting (decision 67).

### Deferred items

- hegel-cpp's compile-time migration for retired run status 3, on its next header sync.
- The bytes-increment shrinker hole (decision 63): both eras stall on ~1 in 20 random starts,
  and closing it waits on a probe-based increment variant that executes the bumped proposal.
- `replay_aligned` under ND stays as accepted re-shrinking every run for structurally-ND
  bodies (005B priced it), to be revisited if slow real bodies bite.
- The recursive depth-spread regression from the tree removal (decision 60): chain-only
  recursive generators lose the tree's novelty forcing, P(depth ≥ 10) 0.30 → 0.14 at the
  pinned seed, with a recursive-pricing follow-up pinned in `test_distributions.rs`.
- Decision 31's reopen condition stands: per-position anchoring returns only if a real
  workload shows pool plus splices missing at meaningful rates.
- Experiment 010's caveats were never re-tested: novel-prefix value on hard or rare bugs is
  unmeasured, and only one stateful machine shape was priced.

### Dangling citations

The remediation plan cites three register ids that do not exist in
`research/critique-asbuilt.md`, whose register has no C class: C10 on phase-9 documentation
text that avoids constants owned by phase 12, C2 on the anchor-seed extension applying to
trusted promotions, and C9 on the `constants_match_their_documented_values` test. Decision 54
carries the C2 marker twice. All are annotated in place as "no such register id, unresolved"
(the annotations landed in 48894dc4) rather than repaired without a record. The register they
point at was never located in the notes.
