# Remediation plan

Fix plan for the findings in `research/critique-asbuilt.md` (the 2026-09-03 adversarial
review of the as-built branch). Finding IDs below (S1…, W1, R1…, L1…, P1…, D1…, M1…) refer
to that register. The process follows `production-plan.md`: phases continue from 8,
decision gates from G4, experiments from 007; each phase lands green under `just check` +
`just check-coverage`, code with its tests in the same commit-group, notes and design.md
kept as-built throughout. Line references are as of 9c800e8e and will drift; anchor on the
named functions.

The register's verdict shapes the plan: the architecture stands, so nothing here is a
redesign. The statistical constants get re-derived with the composition measured end to
end (phases 11-12); the correctness bugs are ordinary fixes, most of them gate-free
(phase 9); the documentation layer gets a truth sweep in two passes (corrections with the
code that makes them true, then a closing as-built audit).

## Decision gates (DRM input needed)

Fifteen gates, resolvable in one sitting between phases 9 and 10 — nothing in phase 9
waits on any of them. Recommendations are the workstream designs' strongest proposals.

*Resolved 2026-09-03: all fifteen as recommended (decision 34), to be revisited if
implementation hits problems. The per-gate detailed decision entries land with their
fixes.*

**G5. Anchor estimand — decision-19 clarification** (blocks the decision entry and
design.md wording; not experiment 008). The anchor tracks the incumbent's reproduction
rate under the engine's own pinned-replay procedure, raised only at validated events —
bar accept, adopted gauntlet first-accept (once per realized timeline), boost holdout
pass; post-accept re-measurement of the standing incumbent never feeds it. That is what
001/006 measured and what the `raised` set implements; candidate and incumbent then sit
on one estimand. Alternative: decision 19's literal reading (only fresh-generation
evidence may raise the anchor), which forecloses every shipped anchor source and needs a
new experiment series. Recommendation: the clarification.

**G6. Deterministic-core retention shape** (blocks the phase-12 constant, not the
mechanism). Options: (a) gamma schedule in `nd::gauntlet` — flat 0.8 below a retention
high-water, 1.0 at or above it, parameters from 008; (b) zero-miss matching, which needs
incumbent evidence inside the verdict function; (c) re-open decision 2.
Recommendation: (a).

**G7. Boost reliability floor units** (clarifies decision 28). With anchors moving to
extended-batch LCBs (phase 12), keep 0.5 by 28's letter (triggers on observed rates
≲ 0.75) or move to the 008-derived value (~0.30 in LCB-at-n=20 units) matching its
rescue intent. Recommendation: the derived value.

**G8. Watermark landing order.** (a) Land the recursive clone-descending watermark in
phase 10 and let 009a validate — it is behavior-identical on the bodies 004 calibrated
and strictly dominates the current weight on clone streams; (b) gate on 009 with a
flat-length interim. Recommendation: (a).

**G9. Discovery-gate physical backstop** (decided by 009a's numbers). If the
racy workload's median miss weight after the fix is ≥ 0.3, no change; if < 0.1, add a
`PHYS_GATE` clause to the zero-fail gate with its constant from a DP re-run under the
measured weight distribution; in between, DRM sees the tables. Decision 23's bar is not
touched without new DP numbers.

**G10. Decision-31 exposure disposition** (report-only). 009a produces the first
off-ceiling pool+splice reproduction rates on workload #1 — the data decision 31's reopen
clause names. If reproduction falls below the thresholds in 009's spec, DRM chooses:
schedule per-stream work or ticket it. Recommendation: ticket unless the shortfall breaks
decision 11's budget arithmetic (reproduction < 90% of the 95% design point).

**G11. Stamping policy for the unconfirmed report** (blocks R2 only). Decision 3's report
for a never-reproduced failure currently has no counterexample. Options: (a) stamp
generation-phase executions once `nd_active` — draw lines and diagnostic captured for at
most the generation budget, in runs already paying multi-replay confirmation costs;
shrink/gauntlet/boost probes stay unstamped (decision 10 untouched); (b) leave the report
caveat-only, documented gap; (c) render raw choice values engine-side (no generator
context, new ABI surface). Recommendation: (a); run experiment 011 first only if the
overhead number is wanted (< 5% threshold), sliding R2 to phase 12.

**G12. Trusted-shrink anchor, and the decision-24 rewording** (blocks L1). A trusted
origin promoted at shrink time anchors on: (a) its evidence batch's LCB, floor-gated by
the existing gauntlet arithmetic — extended to `ANCHOR_SEED_RUNS` once phase 12 lands;
(b) a constant prior at `TARGET_FAILURE_RATE`; (c) floor-only. Recommendation: (a);
experiment 012 only if contested. This gate also signs off rewording decision 24's
protection as "trusted origins are exempt from the bar's verdict" (never evicted), since
"the bar is not re-run" was never true — the shrink-time batch is real and intended.

**G13. Stored-pool disposition at trusted promotion** (blocks L2). (a) Merge the stored
v2 pool into the promotion pool — fresh captures first, dedup, cap at `POOL_CAP` counting
the incumbent; (b) document the drop as intended. Recommendation: (a).

**G14. Same-run supersession policy for the Persister** (blocks P2a). (a) Delete
superseded same-run saves uniformly, demotion reserved for the run-start primary,
save-before-delete ordering preserving the Ctrl-C property; (b) ND-only fork of the rule;
(c) status quo plus the G15 cap. Recommendation: (a) — a same-run intermediate never
ended a run as anyone's best example, so it earned no cross-run staleness strike.

**G15. Secondary corpus cap** (blocks P2b). (a) No cap; (b) cap 50 per database key,
shortlex-largest evicted at end-of-run reconciliation. Recommendation: (b); confirm the
constant (50 = 5× the reuse phase's default secondary sampling ceiling).

**G16. Rename `hegel_test_case_is_nondeterministic`** (blocks D6's naming only). The
stamp now means "capture this case" — it fires on deterministic final replays and blob
replays. (a) Rename to `hegel_test_case_should_capture`, no shim; (b) rename plus a
doc-deprecated alias export; (c) re-document only. Recommendation: (a) — this release
already forces every binding to rewrite its capture logic, and a rename turns a silently
changed contract into a compile-time signal, the same reasoning decision 27 used for
status 3.

**G17. ND line under `show_statistics`** (interacts with decision 1). (a) One line after
the statistics render when the run is ND: measurement replay count and failures — the
only sub-Debug surface revealing the flip and its cost; (b) risks entry only.
Recommendation: (a) — `show_statistics` is requested diagnostics, not the unsolicited
notice decision 1 forbids.

**G18. Splice budget** (blocks M2). (a) Restore `REPRODUCE_SPLICES` to 10, citing 006B's
cap-10 measurement directly; (b) run experiment 010 at 6 vs 10 first. Recommendation:
(a); the decision entry must explicitly amend decision 25's "(~6 replays/miss)"
parenthetical, the source of the misreading.

**G19. FINAL_REPLAY_FRESH disposition** (blocks M3). (a) Derive it — rejected: no
estimable fresh-hit rate exists; (b) measure in 010; (c) document as chosen, not derived:
a bounded last chance to capture fresh failing output after the stored state's ~29-replay
budget is spent. Recommendation: (c).

**G20. The deterministic-to-ND seam** (new, from phase 12's in-engine spot check — see
the 008 notes). Production enters ND handling lazily, and a run that flips late has
already spent its deterministic window: pre-flip `update_interesting` displacement walks
the incumbent down the landscape before decision 20's guard exists, and a bar rejection
at shrink-verify leaves no generation budget to re-hunt. Measured on 003's bodies: L1
final-p median 0.34 against the 0.82 envelope, and 49% of target-regime (L4b) trials
report caveat-only — the failure still fails the run, but unshrunk, unconfirmed, and
unpersisted. The shrink mechanics hold wherever a confirmed origin entered shrinking.
Options: (a) accept and document — detection lag is inherent, declared concurrency does
not pay it, and a rerun that finds persisted ND state skips the seam (though a
caveat-only run persists nothing and re-races it); (b)
on a late flip, re-enter generation with the remaining budget to re-hunt and confirm; (c)
guard displacement pre-flip (costs deterministic-run behavior). Undecided — needs DRM;
carried into the phase-13 audit as an open item. Same seam, opposite direction (009a):
at p = 0.9, 11.5% of clone episodes never flip at all and emit v1 exact-choice blobs
that reproduce at 13% where v2 blobs hit 100% — blob quality currently depends on
whether the run noticed its own nondeterminism (decision 58's audit note).

## Experiments

Numbered continuing the series; specs and results in `notes/experiments/<n>-<name>/`,
harness crates under `/experiments` frozen with a README note after their run.

**008 — gauntlet calibration under the shipped rules** (drives phase 12; sim starts
during phase 9). Extends `experiments/shrink-sim` with the shipped policy as a baseline
and a factorial over the fixes: anchor seeding {bar-batch, extended-20, extended-40} ×
accept rule {shipped, min-fails 2/3/4, min-fails + recruit-excluded} × floor {0.035,
0.05, 0.08, 0.10} × gamma {flat 0.8, high-water 0.7, high-water 0.8} × miss weighting
{shipped watermark, floored 0.2, off} (the 009 coupling), on 001's landscapes plus a
deterministic core, incumbents at p ∈ {0.1, 0.3, 0.9}; plus an exact-DP module for
per-candidate operating points (005A method). Decides: `GAUNTLET_MIN_FAILS` (smallest m
holding L4 bug-retention ≥ 99% and L1 final-p median ≥ 0.50 at ≤ 1.5× 003's gauntlet
cost), the recruit disposition, the floor (largest value with p = 0.02 per-shrink false
accept ≤ 1%), `ANCHOR_SEED_RUNS` (smallest n with seeded anchor within 0.05 of a 40-run
reference; expected 20), the retention high-water (≥ 27/30 deterministic finals without
boost, 30/30 with, at ≤ 20% L1 cost increase), the boost floor in corrected units (G7
input), the z disposition for S5, and the measured drift envelope design.md's goal will
quote. Final constants freeze after 009a fixes the weighting column. In-engine
spot-check: a new frozen crate `experiments/gauntlet-calibration` re-runs the 003 table
cells on the fixed engine (phase 12).

**009a — off-ceiling watermark measurement** (drives G9/G10; runs in phase 11 on the
phase-10 tree). New frozen crate `experiments/watermark`: 007-style bodies with
injectable race/failure rates (p ∈ {0.1, 0.3, 0.9}), a `#[cfg(feature = "__bench")]`
hook in `nd_replay_once` dumping (stored, realized, failed) per measurement replay, the
old and new weightings recomputed offline, bar/gauntlet arithmetic replayed via
`experiments/confirm-bar`'s DP generalized to weighted misses. Deciding numbers per
(body, p), N ≥ 200 episodes: median miss weight W50 (≥ 0.3 closes G9 as no-change; < 0.1
selects PHYS_GATE); fluke rejection cost ≤ 20 physical (vs today's 37); gauntlet reject
cost median ≤ 15 with > 50% proof-rejects (vs 30-always, proof-never); median confirmed
anchor at true p = 0.1 ≤ 0.2 — failing that after the fix escalates to DRM as the signal
that weighting alone cannot describe clone bodies and decision-14 territory is genuinely
in play. Side measurement for G10: off-ceiling DB-reuse and blob-replay reproduction
rates (flag if < 90% at p = 0.3 or < 60% at p = 0.1).

**009b — composed-rules re-verification** (phase 12, no new harness): the 009a operating
points recomputed offline under the post-008 composed rules, plus an in-engine spot
check.

**010 — splice budget** (only if G18 → (b)): rerun `experiments/replay-semantics` with
`SPLICE_TRIES ∈ {6, 10}`, 006B bodies and seeds; B5 rescue at budget 6 decides.

**011 — stamp overhead** (only if G11 wants the number first): the frozen 007 harness,
wall-clock and per-case render cost of stamped vs unstamped generation on workload #1;
< 5% overhead accepts G11(a).

**012 — trusted anchor** (only if G12 contested): 003's mixture-landscape sim over
anchor ∈ {batch LCB, 0.1 prior, floor-only}; an option is out if final true p ends below
the incumbent's in > 5% of trials; among survivors, lowest replay cost wins.

## Phases

### Phase 9: gate-free verified-defect fixes

Everything that restores a settled decision with no DRM input. Order within the phase
follows the shared seams (shrink loop, run tail, then frontend).

- **R5 — targeting fully off under ND** (completes decision 29). Guard the recording
  (`record_run`: `!self.nd_active` joins the target-observations condition) and stop
  in-flight climbs (`Optimiser::budget_exhausted` gains `|| self.engine.nd_active`).
  Pre-flip observations stay in the map, unused.
- **S7 — adoption-gated anchor raises and persistence** (verified confirmed:
  never-adoptable candidates reach the gauntlet via mutation probes and
  divergence-observing replays). `ShrinkProbe` gains a defaulted no-op
  `candidate_adopted()`; `Shrinker::accept_improvement` — the single adoption point —
  calls it. `EngineShrinkProbe::run`'s Accept arm stops raising and persisting; it
  stashes a `PendingAccept` (ledger key, lower bound, nodes), cleared at the top of every
  `run()`; `candidate_adopted` consumes the stash — raises the anchor (respecting the
  `raised` first-accept set) and calls `record_nd_incumbent`. `NestedCloneProbe` forwards
  the method (tested — the `set_sweep_mode` hazard). This makes "all acceptance paths
  gate on the same validated-accept event" literally true: gauntlet accept *and*
  adoption.
- **R4 — a flip mid-verify or mid-shrink routes through the bar.** Verify seam: after
  the verify run, `deterministic_verify` requires `!self.nd_handling()` — a flipped
  verify falls into the existing bar arm. Shrink seam: after `shrinker.shrink()`, if the
  probe ran ungauntleted but the run is now ND, requeue the origin from its
  verify-validated pre-shrink nodes (discard untrusted single-run progress) and skip
  `shrunk_origins`; terminates because the second pass constructs the probe gauntleted
  and `nd_active` never clears.
- **R1 — unconfirmed origins never reach the blob path** (restores decisions 3/24).
  Three seams: (1) `final_replay`'s dry branch honors `reject()`'s evict signal;
  (2) report assembly moves to `fn build_report(&mut self)` and partitions on
  `needs_confirmation` — the same predicate as the persistence filter — before the sort
  and the `report_multiple_failures` truncation, so a leaked unconfirmed origin can never
  displace a confirmed one; the caveat-only fallback keeps its shape, now meaning
  "nothing confirmed or trusted"; (3) `OriginLifecycle::unconfirmed()` drops its
  `rejections > 0` filter and its count payload (composing R1 with L5), so never-replayed
  origins reach the fallback; `caveat()` gains a `replays == 0` branch ("observed once,
  never replayed…"). Deliberate non-fix, recorded as a decision entry: no
  post-final-replay bar — confirming report-time admissions can admit further origins
  without bound; they report caveat-only and recycle via rediscovery, the trade
  decision 23 already accepted.
- **R3 — capture precedence in `drive_run`.** Rank-gated replacement (diagnostic = 2,
  lines-only = 1, bare = 0); an interesting case replaces its origin's `CapturedReport`
  only at rank ≥ stored. Under quiet everything is rank 0 (today's behavior); at normal
  verbosity a confirmation capture survives shrink probes and a reproducing final replay
  supersedes it. Honest limit, recorded in design.md: a dry final replay prints the
  freshest stamped failing execution (usually pre-shrink values) while the blob carries
  the shrunk incumbent.
- **P1 — the pre-shrink secondary drain stops destroying entries** (restores
  decision 11). The drain runs only under deterministic handling (under ND every entry
  class is wrong to drain), decodes each entry three ways — v1: replay once then delete
  (unchanged); v2 (`decode_nd_state` succeeds): retain untouched, its hygiene lives
  entirely in the reuse phase's budgeted strikes; garbage: delete — and breaks on a
  mid-drain detection flip. Deliberate non-fix, recorded: no drain replay of v2 entries —
  under decisions 20/24 a pre-shrink reproduction can change no outcome, so it is pure
  cost (~35 executions per dry entry).
- **P3 — zlib decode limit.** Both blob.rs call sites move to
  `decompress_to_vec_zlib_with_limit` at 16 MiB, derived from
  `ND_STATE_MAX_TIMELINES × BUFFER_SIZE ×` the serializer's per-choice sizing, doubled
  for content-carrying choices.
- **M1 — POOL_CAP invariant**: 10 total stored timelines per origin, incumbent included.
  A `pooled_timelines` helper replaces the four hand-written builds (the off-by-one came
  from writing one comparison five times); `trust()`/`confirm()` truncate incoming pools
  (a v2 entry can decode up to 64); `nd_confirm`'s capture bound stays inline (it caps
  capture, not storage); `ND_STATE_MAX_TIMELINES` stays 64 as a decode-side sanity bound,
  documented as deliberately looser.
- **M4 — conventions**: the three `.expect("static")` → `.unwrap()` in new tests; the
  three `let _ = tc.draw(…)` → bare calls; the `nd/mod_tests.rs:1` header path fix.
  Audited non-changes: `run_lifecycle.rs`'s production `.expect`s (convention is
  test-scoped) and `expect_err` (established idiom).
- **M5a/M5b/M5e — test gaps**: `failures_count_in_full_at_zero_weight` (pins the
  `record(true, 0.0)` reliance at the fresh tier); `reproduce_blob_never_runs_a_fresh_
  generation` (pins decision 33 — a one-timeline ND blob over a huge integer domain, body
  panics on any nonzero draw); `bar_physical_cap_rejects_diverged_zero_fail_evidence`
  (the one missing bar boundary).
- **Docs that describe unchanged behavior**: D3 (the persisted-format flip source, third
  bullet), D7 (confirmation gates shrinking/persistence, not reporting — runner.rs,
  stateful.rs, RELEASE.md), D8 (reproduce_failure and macro docs to until-failure
  semantics), D9 (the stale-blob message names both hypotheses, with the 5%-miss number),
  D11 (the mechanical rustdoc sweep; header regenerated), D12 (evaluation.md rows 35/45
  updated to "survey done, recorded in production-plan.md phase 6"; `hegel_run_status_t`
  rustdoc reserves value 3 against reuse — the reserved-vs-removed call decision 27
  required), D13's risk entries (estimator bias, stalled-vs-finished shrink, quiet-flip
  visibility — the last replaced by G17's line if accepted), and D2/D4 in constant-free
  form (the boost sentence corrected; "never lower" reworded to bounded-loss now,
  strengthened later only if 008 justifies it — per C10, phase-9 doc text avoids pinning
  constants phase 12 changes).
- **In parallel**: start 008's sim sweep (standalone, no code dependency).

Tests (each named test fails on the pre-fix code): `final_replay_evicts_a_dry_
unconfirmed_origin`, `report_blobs_only_confirmed_origins`, `an_origin_admitted_during_
the_final_replay_is_not_blobbed`, `unconfirmed_origins_report_caveat_only_when_nothing_
confirmed`, `report_multiple_false_truncates_after_the_confirmed_filter`, `a_flip_during_
the_shrink_verify_routes_the_origin_through_the_bar`, `a_flip_during_shrink_probes_
requeues_the_origin_for_a_gauntleted_shrink`, `nd_runs_record_no_targeting_observations`,
`the_optimiser_stops_when_the_run_flips_nondeterministic`, `gauntlet_accept_without_
adoption_moves_nothing`, `anchor_raises_only_on_adoption_and_once_per_timeline`,
`accept_improvement_notifies_the_probe`, `nested_clone_probe_forwards_candidate_adopted`,
`capture_rank_orders_diagnostic_lines_bare`, `a_bare_capture_never_replaces_a_ranked_one`,
`a_confirmed_but_dry_failure_prints_its_confirmation_capture`, `shrink_drain_retains_v2_
secondary_entries_unreplayed`, `nd_run_skips_the_pre_shrink_secondary_drain`,
`shrink_drain_deletes_undecodable_secondary_entries`, `drain_stops_at_mid_drain_nd_flip`,
`decode_blob_rejects_zlib_bomb_v1`/`_nd`, `zlib_decode_limit_admits_large_legitimate_
blobs`, `nd_state_for_caps_stored_timelines_at_pool_cap`, `pooled_timelines_caps_at_pool_
cap_and_dedupes`, `trust_truncates_an_oversized_pool_to_pool_cap`,
`confirm_truncates_an_oversized_pool_to_pool_cap`, plus the M5 trio above and the D9/D6
doc-pinning tests (`test_stale_blob_message_names_both_hypotheses`,
`test_deterministic_final_replay_is_stamped`, `test_blob_replay_cases_are_stamped`).

Exit: every test above green; no gate touched; 008's sim producing tables.

### Gate checkpoint

One DRM sitting resolving G5-G19. Gate outcomes recorded as decisions.md entries
(numbered in landing order — see "Decision entries").

### Phase 10: gated structural fixes, no experiment dependencies

- **Lifecycle L1-L6** (G12, G13). `OriginState::Trusted` becomes evidence-carrying
  (`pool, fails, replays, report_fails, report_replays`); `trust()` seeds/folds reuse
  evidence (DB path, blob path, deterministic replay path). `nd_confirm` renamed
  `nd_evidence_batch` (`NdConfirm` → `NdBatch`, `accepted` → `bar_accepted`), documented
  as two uses: the discovery bar's driver for admission, and an evidence-gathering batch
  for trusted origins where the bar arithmetic is only the stopping rule. The shrink loop
  gets an explicit three-way branch after `take_witness` misses: trusted → batch; any
  failure promotes (now intended and named) with anchor = batch LCB (G12), pool merged
  with the stored v2 pool fresh-first, deduped, capped (G13, absorbing M1's truncation);
  zero failures → `record_trusted_batch`, stay Trusted, skip shrinking, still reported
  and persisted. Unconfirmed-mid-shrink → today's full bar verbatim. R4's flipped-verify
  arm rebases into this branch (flipped origins are Unconfirmed → full bar, which is what
  R4 wants). `record_final_replay` matches `Trusted | Confirmed`, writing the `report_*`
  fields; `confirm()` returns `Result` with an internal-error assert replacing the dead
  Confirmed-prior arm (L4); caveats split confirmation-time from report-time counts (L6)
  and gain trusted-live/trusted-dry wording ("reproduced from stored timelines…"),
  folding phase 9's `replays == 0` branch into the final wording set (lifecycle owns
  `caveat()`).
- **Persistence P2a/P2b** (G14, G15). `Persister::record_bytes`: save-then-**delete** for
  same-run supersession (the Ctrl-C property holds — at every instant primary carries the
  most recent validated incumbent); a `saved_this_run` set lets end-of-run reconciliation
  delete same-run leftovers while still demoting the run-start primary (the freshest
  cross-run backup, decision 11's strike one). Net steady state: one secondary deposit
  per origin per run. Then the cap: `SECONDARY_CORPUS_CAP = 50` per key,
  shortlex-largest evicted at reconciliation — a resource bound outside decision 11's
  two-strike scheme, which is why it is gated.
- **Watermark W1** (G8). `verbatim_weight` becomes flat-length-weighted with recursive
  clone descent: credit = matched flat weight walking paired elements, descending into a
  diverged clone pair (`1 + credit(children)`) via the existing `CloneValues`
  equality/indexing, over `flattened_values_len(stored)`. Behavior-identical on
  clone-free timelines (the 004 calibration and 005A operating points stand); a monotone
  improvement on clone streams. Signature and single caller unchanged; every consumer is
  fixed by the one function. Rejected in the decision entry: physical fallback (inverts
  decision 22 where divergence is common), epsilon floor (unprincipled, keeps the
  degeneracy muted), per-stream ledgers (reopens decision 14/31 machinery a weighting fix
  doesn't need). The cfg-gated 009a dump hook lands with it.
- **R2 — stamped discoveries** (G11(a)): new `capture_discoveries` engine field set
  around the generation phase; the stamp condition becomes `capture_replays ||
  (nd_active && capture_discoveries && !measurement)`. The unconfirmed report now prints
  the discovering case's draws and diagnostic. Residuals documented: gauntlet-discovered
  zero-reproduction origins, and the single case that itself flips the run. The pinned
  frontend test flips from asserting zero draw lines to asserting them.
- **G16 rename** (`hegel_test_case_should_capture`) + `just c-header` + both RELEASE.md
  files + design.md ABI summary, one commit-group with D6's three doc sites written
  against the winning name. **G17** statistics line (`measurement_calls`/
  `measurement_fails` counters in `record_run`'s measurement path, one render line) with
  its test.
- **M2/M3** (G18, G19): `REPRODUCE_SPLICES` → 10 with the corrected doc comment
  (attributing 65-100% rescue to the 10-splice cap, 1.6-6.3 mean replays per rescue);
  `FINAL_REPLAY_FRESH` documented as chosen-not-derived. Landing before 009a so the
  experiment measures the shipped budget.
- **Docs riding their fixes**: D1 (trusted story), D10 (lifecycle doc falsehoods), D5's
  design.md line in interim form, evaluation.md row 24 re-verdict.

Tests: the lifecycle unit set (`trust_seeds_and_folds_reuse_evidence`,
`confirm_merges_a_trusted_pool_deduped_and_capped`, `confirm_on_a_confirmed_origin_is_an_
internal_error`, `record_final_replay_records_report_counts_on_trusted_and_confirmed`,
`caveat_separates_confirmation_from_report_time_counts`, …), the integration set
(`nd_trusted_promotion_repersists_the_stored_pool` — written first, red on pre-fix code —
`nd_trusted_zero_fail_shrink_batch_keeps_trusted_and_reports_honestly`,
`nd_trusted_weak_batch_promotes_and_shrinks_under_the_floor`, `nd_confirmed_dry_caveat_
does_not_fold_report_replays_into_confirmation_counts`), the persistence set
(`persister_deletes_superseded_same_run_saves`, `persister_saves_new_bytes_before_
deleting_superseded` on an op-logging DB fake, `nd_secondary_corpus_stays_bounded_across_
runs`, `end_of_run_reconciliation_demotes_only_the_run_start_primary`,
`secondary_corpus_cap_evicts_shortlex_largest`), the watermark unit set (tracked-prefix
inside a diverged stream, flat-length weighting across elements, elongated-stream-as-
diverged, kind-mismatch zero credit, values/realized interchangeability, the existing
clone-free test unchanged, bar-cost and gauntlet-proof-reject under fractional weights)
and engine set (`a_diverged_clone_replay_records_a_fractional_miss_weight`,
`nd_reproduce_terminates_by_weighted_budget_on_diverged_clone_replays`), and the updated
`an_unconfirmed_one_shot_failure_reports_only_its_caveat` asserting the discovering
case's material.

Exit: all of the above green; header-drift green; one secondary deposit per origin per
run demonstrated.

### Phase 11: experiments

009a on the phase-10 tree; 008's sim finalization (weighting column selected by 009a);
010/011/012 only where their gates asked for numbers. Notes written up per the series;
harness crates frozen. No production code changes, so the phase boundary is trivially
green. G9/G10 resolve here from 009a's tables.

### Phase 12: statistical recalibration

All constants from 008; every change inside `nd::gauntlet`, `nd_evidence_batch`, and the
probe, spec'd by DP fixtures:

- **Accept rule**: `GAUNTLET_MIN_FAILS` (provisional 3) — accept also requires
  the fail count; short-of-evidence verdicts are Continue, never Reject on that ground;
  UCB-proof and cap rejections unchanged. Recruiting run stays in the ledger unless 008's
  DP said otherwise. `GAUNTLET_FLOOR` re-derived and cited.
- **Anchor seeding**: `nd_evidence_batch` extends past a bar accept to `ANCHOR_SEED_RUNS`
  (provisional 20) before seeding the anchor — applied to every anchor-seeding batch,
  trusted promotions included (C2); the gauntlet Accept arm tops the accepted timeline's
  ledger up to the same size before stashing the accept, restoring 006's core-retention
  anchor (~0.84 at LCB 20/20). Bar accept/reject semantics (decision 23) untouched.
- **Retention** (G6): gamma schedule — 0.8 below `RETENTION_HIGH_WATER` (provisional
  0.8), 1.0 at or above. **Boost** (G7): floor in corrected units; `BOOST_HOLDOUT` raised
  to `ANCHOR_SEED_RUNS`.
- **PHYS_GATE** if G9 selected it. **S5 disposition**: the DP over the shipped decision
  procedure becomes the recorded operating points; bias-direction note in the module doc;
  z widens only if the realized false-accept missed target.
- **M5d** `constants_match_their_documented_values` asserting the frozen post-008 values
  (per C9, it waits for them). 009b's re-verification. The in-engine
  `experiments/gauntlet-calibration` spot check.

Tests: `gauntlet_never_accepts_below_minimum_evidence` replaces the deleted
`gauntlet_floor_accepts_a_single_failure_at_zero_anchor` (which pins the S1 defect);
`gauntlet_matches_the_008_operating_points` (exact DP, so any future parameter change
forces re-derivation — the S2-class regression); `gauntlet_floor_matches_its_derivation`;
`gauntlet_gamma_is_unity_above_the_retention_high_water`;
`anchor_seed_extension_reaches_the_reference_batch`; `boost_skips_a_reliable_incumbent` /
`boost_runs_below_the_floor` (counted via the counting ctx — catches S3, where the
shipped code boosts a deterministic body too); `shrink_does_not_drift_on_a_rising_
landscape` (end-to-end S1 regression: lands at the P0 value under shipped rules);
`deterministic_core_is_retained`.

Exit: the DP fixtures and both end-to-end tests green; 009b within its pass thresholds;
D4/D5's final wording unblocked by the measured drift envelope.

Spot-check outcome (in-engine, appended to the 008 notes): the recalibrated mechanics
reproduce the simulated envelope wherever a confirmed origin entered shrinking; every
headline miss lives in production's lazy ND entry, raised as gate G20.

009b (composed-rules re-verification) closes the phase: on the shipped engine the
escalation signal stays quiet (anchor medians 0.108/0.131 at p = 0.1 against the 0.2
line), false accept sits at or under the DP's 4.0e-4 per proposal, and reuse/blob hold
>= 98% at p <= 0.3, so decisions 57/58 stand. The two cost letters formally missed —
fluke rejection 21-22 clone / 27-28 machine against the plan's <= 20, and low-anchor
gauntlet fluke rejects riding the 30-run cap with no proof share — are the
10/mean-weight and evidence-before-reject arithmetic decisions 57 and 54 accepted.
Measured cost of the composed rules: 1.6-2.1x measurement replays at p <= 0.3, 4-6x at
p = 0.9, concentrated in top-up work against near-deterministic evidence as 008
predicted.

### Phase 13: closing audit

- The docs inventory (D1-D13) re-run as a full design.md as-built sweep — every quoted
  passage verified against code, the same standard as production-plan phase 8.
- evaluation.md re-verdicts: rows 2 and 19 (after phase 12), row 7 (after phases 9/12 —
  the gauntlet-loop findings), row 24 (done in phase 10), row 27 (done in phase 9);
  appended as a dated correction section.
- D4/D5 final wording from 008's drift envelope ("never lower" returns only if the
  numbers say so; otherwise bounded-loss stands). Both RELEASE.md files reworded once,
  final.
- decisions.md final numbering assigned in landing order (see below); design.md's
  closed-decisions table annotated with 009a's decision-14/31 disposition (G10).
- Workload-#1 claims re-validated: the 007-derived concurrent-behavior claims currently
  rest on at-ceiling evidence; they are restated from 009a/009b's off-ceiling numbers.
- Full gate run per production-plan phase 8's list; /self-review over all prose added by
  phases 9-13.

Sweep outcome: 21 of 22 findings confirmed under adversarial refutation and corrected in
place. The largest: flip-source attribution (four of six `nd_flip` sites live outside
`test_function_tagged`), persistence and reporting scoped to confirmed *or* trusted,
decision 19's replay-evidence overclaim (also in the gauntlet rustdoc), and the
`hegel_test_case_should_capture` contract doc — the deterministic final replay and blob
replays are stamped too; header regenerated.

Exit: every register finding traceable to a landed fix, a recorded deliberate non-fix, or
a DRM-accepted risk entry.

Exit verified 2026-09-03: an independent audit traced all 40 register findings to landed
fixes with their pinning tests or decision entries (deliberate retentions — z = 1.96, the
asymmetric miss weighting, FINAL_REPLAY_FRESH chosen not derived — are recorded in
decisions 53/54). None untraceable.

Full gate run green on e79994f4 (production-plan phase 8's list): `just check`,
`check-coverage` (no ratchet increase, no new nocov), `c-test`, `c-test-abort`,
`c-test-runtime`, `check-docs`, `check-tests-minimal-versions` (under nightly, as CI runs
it), `miri`, and `cargo package --workspace` — the last after clearing a stale build-cache
rlib that shadowed the packaged engine locally; a clean run, like CI's, is unaffected.

## Decision entries to record

Numbers assigned in landing order at each phase boundary; content fixed here. Phase 9:
report-assembly seam and the no-post-final-replay-bar trade (R1); adoption as part of the
validated-accept event (S7); capture precedence (R3); flip-mid-shrink routing (R4);
targeting under ND (R5, completes 29); drain scoped to v1-under-deterministic (P1,
restores 11); the 16 MiB decode bound (P3); the POOL_CAP invariant (M1); decision 27
closed — status 3 reserved, survey recorded (D12). Gate checkpoint / phase 10: the
decision-19 clarification (G5, merging the stats and docs drafts — stats' estimand
wording, docs' rejected-alternative line); trusted evidence batch + decision-24 rewording
(G12); pool merge at promotion (G13); same-run supersession (G14); the secondary cap
(G15); the watermark rule with its three rejected alternatives (G8); stamped discoveries
(G11); the rename (G16); the statistics line (G17); splices restored to 10 amending
decision 25's parenthetical (G18); FINAL_REPLAY_FRESH chosen-not-derived (G19). Phase 12:
the gauntlet accept rule recording that the shipped rule was decision 7's rejected P0 and
how it got there; retention shape (G6); boost floor in corrected units (G7, clarifies
28); PHYS_GATE if taken (G9). Phase 13: the decision-31 disposition (G10).

## Risks

- **Composition risk, again.** Phases 11-12 exist because constants derived in isolation
  composed badly once. The mitigations are structural this time: 008 sweeps the composed
  pipeline (bar → anchor → gauntlet → boost, weighting included), 009b re-checks the
  watermark's operating points under the final rules, and the DP fixture tests make any
  future constant change fail until re-derived.
- **Shrink wall clock.** Min-fails accepts (~3× replays per accepted step) plus
  accept-time seeding extensions (~20 replays per accept) push against
  `MAX_SHRINKING_SECONDS`, already a known risk for slow concurrent bodies. 008 records
  the cost multiplier; if it lands above ~1.5× on the reference landscapes, a shrink
  budget setting becomes a follow-up gate rather than a silent regression.
- **Requeue termination (R4).** The mid-shrink requeue re-runs one origin's shrink once.
  The bound is structural (`nd_active` never clears, the second probe is gauntleted), and
  the requeue test pins it, but any future third mode would need the same one-requeue
  argument re-made.
- **Two renames in one release** (G16 + the retired status 3). Both are deliberate
  compile-time migration signals, but the bindings survey should be re-checked for the
  stamp symbol before the header ships, the same way decision 27 required for status 3.
- **009a inconclusive.** If the racy workload's miss weights stay near zero after the
  watermark fix, evidence weighting alone cannot describe clone bodies; that escalates to
  DRM under G9/G10 with decision-14 territory genuinely reopened — the one branch of this
  plan that could grow a new design phase.
