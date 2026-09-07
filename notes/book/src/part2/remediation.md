# Remediation: the as-built review

Production-plan phase 8 closed on 2026-09-03 with the branch declared "the artefact
decision 26 describes". The same day, an adversarial review of the as-built branch at
head 9c800e8e produced `notes/research/critique-asbuilt.md`, a register of 41
findings, and the branch continued straight into `notes/remediation-plan.md`
(480c3970): phases 9–13 and gates G5–G19, continuing
[the production plan](production.md)'s numbering.

## What the critique found

Nine subsystem reviewers and four design-foundations auditors went over the full
branch diff. Every finding was adversarially verified by independent sceptics. Six
died in verification and were omitted, and the high-severity survivors were
re-verified by hand, including the Wilson arithmetic. Findings carry stable IDs and
a status of confirmed (code-verified) or plausible (the sceptics split on
S7, L2, R5).

The verdict was "the architecture holds, but the statistics don't compose." Each constant
had been derived in an isolated experiment. The composition (bar to anchor to
gauntlet and boost, watermark to evidence) was never measured end to end, and it broke
exactly at the p ≥ 0.1 target (decision 16) and on the flagship concurrent workload
(decision 12), on top of two persistence-destroying bugs, one reporting hole violating
decision 24, and a documentation layer that had drifted from the code.

The 41 findings fell into seven classes: S1–S8 (statistical foundations), W1
(divergence weighting), R1–R5 (report path), L1–L6 (trusted-origin lifecycle), P1–P3
(persistence hygiene), D1–D13 (documentation drift), M1–M5 (constants and test gaps).
The worst of them:

- **S1 (high).** The gauntlet, the mechanism built to kill unguarded single-run
  accepts, degenerated back to naive single-run accepts across the entire target
  regime. Wilson LCB(1 fail / 1 run) = 0.2065 against a threshold of
  max(0.8·anchor, 0.05) with no minimum evidence, the recruiting failure counted, and
  the verdict checked before any rerun: any anchor ≤ 0.258 accepted every candidate on
  its single recruiting failure, and after the first such accept the anchor
  equilibrated at 0.2065. This is the P0 policy decision 7 rejected, which
  experiment 001 had measured losing the bug in 34% of noise-floor trials, and a test
  pinned the degenerate case as intended. S2 explained it: the shipped anchor seeding
  was never simulated, and design.md's drift-protection numbers came from 001/003's
  20-run confirmation batches, a stronger mechanism. S4 found that
  `GAUNTLET_FLOOR = 0.05` had no recorded derivation.
- **W1 (high).** `verbatim_weight` treated an entire clone stream as one element with
  whole-record equality, so on concurrent machines any intra-stream divergence zeroed
  the miss weight: evidence degenerated to a fail counter, reject-by-proof never fired,
  and a fluke cost 37 replays instead of the documented ~15.
- **R1–R3 (high).** Unconfirmed origins were reported with v2 blobs alongside
  confirmed failures, violating decision 24. The decision-3 report for a
  never-reproduced failure contained no counterexample at all. Unstamped gauntlet and
  boost probes clobbered the per-origin capture, so a confirmed-but-dry failure
  printed an empty block.
- **P1, P2 (high).** The pre-shrink secondary drain deleted v2 entries it never
  replayed (zero-strike destruction, against decision 11), and the secondary corpus
  grew without bound while an ND failure stayed live.
- **D4 and the D family (thirteen drift findings).** design.md and both RELEASE.md
  files still claimed shrinking never lowers reproduction probability, when the
  machinery enforced LCB ≥ 0.8·anchor at best.
- **S3, M2.** `BOOST_RELIABILITY_FLOOR = 0.5` gated on an LCB whose ceiling at
  confirmation time was 0.5101, so boost was always-on in practice, and `REPRODUCE_SPLICES = 6`
  shipped experiment 006's mean cost per miss as if it were the attempt budget.

The critique also recorded what held up: the v2 blob format, the bar's DP-re-derived
operating points, `error` strictness's byte-identical diagnostics, the accounting
split, and the monotone anchor, which prevented multiplicative threshold decay and was
named "the reason S1/S2 are a recalibration, not a redesign".

## The plan

The register's verdict shaped the plan: "the architecture stands, so nothing here is a
redesign." Statistical constants would be re-derived with the composition measured end
to end (phases 11–12). Correctness bugs were ordinary fixes, mostly gate-free
(phase 9), structural fixes waited on their gates (phase 10), and the documentation
got a two-pass truth sweep ending in a closing audit (phase 13). The fifteen gates
were designed to be resolvable in one sitting between phases 9 and 10, and DRM
resolved them all as recommended on 2026-09-03 (ffa83adf), recorded as decision 34
("resolve them all as recommended and if you run into any problems we can revise
later"), with the detailed entries landing alongside their fixes.

## Phase 9: gate-free defect fixes

Phase 9 fixed every verified defect that waited on no gate, each pinned by a test
verified red on the pre-fix tree, recorded as decisions 35–43 (27c13cee, 01232f71,
3aa085de). On the report path, `build_report` now partitions on `needs_confirmation`
before sort and truncation so unconfirmed origins never reach the blob path (R1),
capture replacement is rank-gated (R3), and a flip during the shrink verify or shrink
probes routes the origin through the discovery bar with one requeue (R4). In the
shrink loop, a gauntlet accept moves state only at shrinker adoption, through the new
`ShrinkProbe::candidate_adopted` seam (S7, verified confirmed from plausible), and
targeting is fully off under ND handling (R5). In persistence, the pre-shrink
secondary drain is scoped to v1 entries under deterministic handling (P1), zlib
payloads decode under a 16 MiB bound (P3), `POOL_CAP` becomes a total of 10 built by
`pooled_timelines` alone (M1), and run status 3 is reserved in the ABI rustdoc.
Documentation corrections (D2–D4, D7–D9, D11–D13) landed with the code that made them
true, and experiment 008's simulation harness started producing tables in parallel.

## Phase 10: gated structural fixes

Phase 10 landed the structural fixes the gate sitting had unblocked, recorded as
decisions 44–53 (62bbeda0, 2390c7ec, bdcc0076):

- **Lifecycle (L1–L6, gates G12/G13).** `Trusted` now carries evidence seeded by
  `trust()`, and `nd_confirm` became `nd_evidence_batch`, whose bar arithmetic acts as
  a stopping rule only for trusted origins, the honest rewording of decision 24's
  "the bar is not re-run", which was never true (decision 47). A failing batch
  promotes with the batch LCB as anchor, merging the stored pool fresh-first,
  deduplicated, capped at `POOL_CAP` (decision 48).
- **Persistence (P2, gates G14/G15).** Same-run supersession is save-then-delete, so
  the primary key always carries the most recent validated incumbent and Ctrl-C
  mid-shrink loses nothing. Superseded same-run saves are deleted, never demoted, and
  `SECONDARY_CORPUS_CAP = 50` per key evicts the shortlex-largest at reconciliation
  (decision 44).
- **Watermark (W1, gate G8).** `verbatim_weight` became flat-length-weighted with
  recursive clone descent (a diverged clone pair earns `1 + credit(children)`), so a
  replay falling off late inside a clone stream keeps credit for the reproduced prefix
  (decision 45).
- **Reporting and surface (R2, gates G11/G16/G17).** Generation-phase executions are
  stamped for capture once ND handling is active, so the report for a never-reproduced
  failure carries the discovering case's draw lines (decision 49). The ABI stamp was renamed
  `hegel_test_case_should_capture` with no shim (decision 50), and `show_statistics`
  gained the measurement-replay line (decision 51).
- **Constants (gates G18/G19).** `REPRODUCE_SPLICES` was restored to 10, amending
  decision 25's mistranscribed parenthetical (decision 52), and
  `FINAL_REPLAY_FRESH = 4` was documented as chosen, not derived (decision 53).

## Experiment 008: the recalibration

The plan's statistical core was experiment 008, run on the extended
`experiments/shrink-sim` harness with an exact-DP module built on 005A's method. It
modelled the shipped rules from `hegel-c/src/native/nd/mod.rs` exactly, down to the
verdict being checked before any rerun, and swept a factorial over anchor seeding
{bar-batch, extended-20, extended-40} × accept rule {shipped, min-fails 2/3/4,
min-fails-3 recruit-excluded} × floor {0.035, 0.05, 0.08, 0.10} × gamma {flat 0.8, high-water 0.7,
high-water 0.8} × miss weight {1.0, 0.2, 0}. To 001's L1–L5 landscapes it added L4b
(bug p = 0.1 over a 0.02 noise floor, the decision-16 target regime) and D1/D2
(deterministic cores with flaky regions at 0.3 and 0.7, S6's displacement case).

**What the simulation showed.** H1 confirmed S1 quantitatively: the shipped policy is
degenerate below anchor 0.258. Bar-batch median anchors sat inside that zone through
the whole low-to-mid regime (0.061 at p = 0.1, 0.138 at 0.3, 0.250 at 0.5), the DP put
P(accept) at 1.000 for a q = 0.02 fluke, and the simulation lost the L4b bug to noise
accepts in 33% of trials, matching 001's P0 policy, which lost 34%.

**Which constants moved.** All landed in `hegel-c/src/native/nd/mod.rs` at phase 12
(c68a89eb), recorded as decisions 54–56:

| Constant | Shipped | Recalibrated | Why |
| --- | --- | --- | --- |
| `GAUNTLET_MIN_FAILS` | none (1 fail could accept) | 4 | Accept-rule ladder on L4b (bug kept / cost vs shipped): shipped 51%, m2 73%/1.37×, m3 97%/2.60×, m4 100%/3.01×. m4 costs 1.00× on 003's landscapes, so its whole cost and whole value live in the target regime. m3 ruled out: no floor passes both floor criteria at m3 |
| `GAUNTLET_FLOOR` | 0.05, underived | 0.05, derived | 0.05 < LCB(4/30) = 0.0531, the min-fails acceptance boundary at the 30-run cap, so the floor costs zero power at m4 (at 0.08 power halves). False accept 4.0e-4 unconditional per proposal, resolving S4 |
| Anchor seeding | the bar's accept batch | `ANCHOR_SEED_RUNS = 20` at both sites | 20 is what 001/003 actually simulated, and the largest size whose all-fail LCB a candidate can still match within the gauntlet cap. e40 structurally excluded: LCB(40/40) = 0.912 > LCB(30/30) = 0.887, so under gamma 1 shrinking stalls outright |
| Retention gamma | flat 0.8 | 0.8 below `RETENTION_HIGH_WATER = 0.8`, 1.0 at/above | The high water is a zero-miss detector, not a tuning dial: with 20-run seeding only LCB(20/20) = 0.839 reaches it. It converts D2's 33% deterministic-core displacement to zero and costs L1 1.26× replays, a G6 letter miss accepted as decision 2's intended behaviour (decision 55) |
| `BOOST_RELIABILITY_FLOOR` | 0.5 | 0.30 in 20-run-batch LCB units | Decision 28's literal 0.5 was written for the old estimator and over-triggered (59% of true-0.7 incumbents). 0.30 is the boundary image LCB(10/20) of a true-0.5 incumbent (recall 0.991, precision 1.000). `BOOST_HOLDOUT` rose to `ANCHOR_SEED_RUNS` (decision 56) |

Two retentions were deliberate: the recruiting run stays counted (exclusion's 7× DP
advantage does not survive ledger retention across retries), and z stays 1.96
(realized worst-case false accept 4.0e-4 per proposal against the ~1e-3 design
target). The DP rows became the specification, pinned by
`gauntlet_matches_the_008_operating_points`.

The composition finding is recorded in decision 54 as "neither piece works alone":
extension without min-fails is worse than shipped (51% vs 67% L4b retention), because
honest 20-run ledgers are what stop the anchor ratcheting to 0.2065 flukes. The chosen
rule's drift envelope (L1 final-p median 0.82, 100% bug kept on every landscape, 100%
deterministic finals on D1/D2) became the numbers design.md's goals quote.

One coupling was left open: the weighting columns showed w = 0.2 preserves every
headline but w = 0 breaks min-fails itself (L4b 89%, D2 39%), so the constants were
marked PRELIMINARY until 009a measured the real weight distribution.

## Phase 11: experiment 009a

009a measured the phase-10 watermark off the ceiling 007 had run at: a frozen
`experiments/watermark` crate drove the real engine with clone-stream and
state-machine bodies whose hidden seeded schedules injected structural divergence at
known rates, 200 episodes per {body} × p ∈ {0.1, 0.3, 0.9} cell (d409c04b, details in
[the experiments chapter](experiments.md)). The new watermark's median weights came
out at 0.28–0.44 with exactly zero mass at weight 0, while the old weighting from
before decision 45, recomputed offline on the same replay pairs, put 78–97% of misses
at exactly zero. The old estimator had been running 008's w = 0 breakage column in
practice. The measured distribution sat strictly between 008's w = 0.2 and w = 1.0
columns, so the phase-12 constants froze, resolving 54's PRELIMINARY marker. The
escalation signal did not fire (median confirmed anchors 0.106 and 0.130 at true
p = 0.1, against the 0.2 line), so no physical gate was added and decision-14
machinery stayed closed (decision 57, gate G9). Off-ceiling DB reuse and blob replay
held 98–100% at p ≤ 0.3, keeping decision 31 closed (decision 58, gate G10).

Two loose ends came out of 009a. The first full run crashed the engine:
`try_replace_with_deletion` indexed `current_nodes` with a stale index after an
adopted mid-pass candidate. A bounds guard fixed it, the one production change of
phase 11. And 23 of 200 clone episodes at p = 0.9 never flipped
into ND handling and emitted v1 exact-choice blobs, which reproduced at 13% where
every v2 blob reproduced. Decision 58 recorded it under gate G20's
seam family: "a failure's blob quality currently depends on whether the run noticed its own
nondeterminism."

None of the plan's conditional experiments ran: a splice-budget 010 (resolved without
running by decision 52), a stamp-overhead 011, and a trusted-anchor 012. That is why
the seam plan later reused those numbers for different experiments.

## Phase 12: the spot check, gate G20, and 009b

Phase 12 landed the 008 constants (c68a89eb), then checked them in the engine: a
frozen `experiments/gauntlet-calibration` crate re-ran 003's bodies through the public
C ABI with no `nd_force`, so runs started deterministic and flipped on production
detection, itself part of what was measured (fa657947). The recalibrated
mechanics reproduced their simulated envelope wherever a confirmed origin
entered shrinking. But three headline numbers missed, all in the same place: L1
final-p median 0.34 against the 0.82 envelope, caveat-only rates of 15% (L4) and 49%
(L4b), and D2 passing trivially because it never flipped at all. Every miss lived in
production's lazy entry into ND handling: pre-flip `update_interesting` displacement
walking the incumbent down before decision 20's guard existed, and a flip arriving at
shrink-verify with no generation budget left to re-hunt. The 008 notes place the seam
outside 008's model and its fix beyond these constants, and it was raised as gate G20
(fdc30860).

009b then re-verified the composed rules on the shipped engine (00651a9e, 8d4ca4f8):
the escalation signal did not fire (anchors 0.108/0.131 at p = 0.1), false accepts sat
at or under the DP's 4.0e-4 per proposal, and reuse and blob reproduction held ≥ 98%
at p ≤ 0.3, so decisions 57 and 58 stood as composed. The measured cost was 1.6–2.1×
measurement replays at p ≤ 0.3 and 4–6× at p = 0.9, concentrated where 008 predicted:
fail-heavy top-ups against near-deterministic evidence.

## Phase 13: closing audit

Phase 13 re-ran the documentation inventory as a full design.md as-built sweep, which
confirmed 21 of its 22 findings under adversarial refutation and corrected them in
place. The largest were flip-source attribution, persistence and reporting scoped to
confirmed *or* trusted, and decision 19's replay-evidence overclaim. evaluation.md
rows were re-verdicted, D4/D5's final wording came from 008's drift envelope ("never
lower" did not return, and bounded loss stands), and workload-#1 claims were restated
from 009a/009b's off-ceiling numbers instead of 007's ceiling.

The exit audit, verified 2026-09-03, traced all 41 register findings to a landed fix
with its pinning test, a decision entry, or a recorded deliberate retention
(decisions 53/54: z = 1.96, the asymmetric miss weighting, `FINAL_REPLAY_FRESH` chosen
not derived), with none untraceable. The full gate run came back green on
e79994f4. One anomaly stands in the plan itself: three cited register IDs (C2, C9,
C10) exist in no register and were annotated "no such register id, unresolved" in
place rather than repaired without a record (see
[where the plan stands](../part1/status.md)).

## What closed and what stayed open

All fifteen remediation gates closed as recommended and were recorded as decisions:

| Gate | Subject | Outcome |
| --- | --- | --- |
| G5 | Anchor estimand | Reproduction rate under pinned replay, raised only at validated events (decision 46) |
| G6 | Retention shape | Gamma schedule: 0.8 below the high water, 1.0 at/above (decision 55) |
| G7 | Boost floor units | 0.30 in 20-run-batch LCB units (decision 56) |
| G8 | Watermark landing | Recursive clone-descending watermark in phase 10, 009a validates (decision 45) |
| G9 | Physical backstop | Closed no-change: 009a's escalation signal did not fire (decision 57) |
| G10 | Off-ceiling exposure | Decision 31 kept: reuse/blob ≥ 98% at p ≤ 0.3 (decision 58) |
| G11 | Unconfirmed-report stamping | Generation executions stamped once ND handling is active (decision 49) |
| G12 | Trusted-shrink anchor | Evidence-batch LCB, with trusted origins exempt from the bar's verdict (decision 47) |
| G13 | Stored pool at promotion | Merge fresh-first, deduplicated, capped (decision 48) |
| G14 | Same-run supersession | Save-then-delete, superseded saves deleted not demoted (decision 44) |
| G15 | Secondary corpus | `SECONDARY_CORPUS_CAP = 50` per key (decision 44) |
| G16 | ABI rename | `hegel_test_case_should_capture`, no shim (decision 50) |
| G17 | Statistics line | One line: measurement replays and failures (decision 51) |
| G18 | Splice budget | `REPRODUCE_SPLICES = 10`, amending decision 25 (decision 52) |
| G19 | `FINAL_REPLAY_FRESH` | Documented as chosen, not derived (decision 53) |

G20, the deterministic-to-ND seam, was the one finding the plan raised on itself and
could not close within its own frame. It was not a constant to recalibrate but a
structural property of the quiet-flip lifecycle, priced at a 49% caveat-only rate in
the target regime. It was carried through the phase-13 audit as open, and resolved the next day
when DRM's four-step workflow was accepted as option (d) and became
[the seam plan](seam-plan.md).
