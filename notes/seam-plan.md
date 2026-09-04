# Seam plan

Implementation plan for gate G20's resolution: DRM's four-step workflow (2026-09-04),
recorded as option (d) in `remediation-plan.md`. It replaces the deterministic-to-ND cliff
with detection that is near-free while nothing is interesting, bounded when something is,
and recoverable when evidence arrives late. Inputs: `research/g20-seam-analysis.md` (the
mechanism analysis) and experiment 010 (the data tree's measured value). The process
continues `remediation-plan.md`: phases from 14, decision gates from G21, experiments from
011 (remediation-plan's conditional 011/012 never ran; the numbers are reused), decisions
from 59; each phase lands green under `just check` + `just check-coverage`, code with its
tests in the same commit-group, notes and design.md kept as-built with the phase that
changes them. Everything lands on this branch per decision 26 (production grade in place,
extraction later); the tree removal is not split out to main first. Line references are as
of de906f01 and will drift; anchor on the named functions. Engine paths are
`hegel-c/src/native/` and bare line refs are `test_runner.rs`.

## The workflow, mapped onto the engine

DRM's four steps, with the accepted refinements:

1. **Ditch the data tree.** Experiment 010: recording, novel prefix, and exhaustion buy
   nothing measurable on large-space workloads; serving is a 6.5x execution win on
   non-stateful shrinking, almost entirely exact repeats. Two replacements recover the
   wins: a flat fingerprint(choices) -> outcome cache (the shrink serving win, plus the
   stateful repeats the tree declined) and a consecutive-duplicate stop (the
   tiny/filtered-space exhaustion win). The cache doubles as a passive detector: the same
   fingerprint concluding differently is ND evidence — a channel the tree never had (it
   flagged kind changes only; verdict flips silently overwrote the leaf,
   `data_tree.rs:307-315`). The tree's other detection role — kind drift at shared
   prefixes across distinct stored sequences — moves to the reuse phase's
   realized-vs-stored comparison (:390-397), today a silent `replay_aligned` clear.
2. **Check each origin's first interesting case.** Per-origin because origin is the
   identity unit (decision 4): one run can hold a deterministic assertion and a racy
   timeout. `record_run` is sync, so the check runs in the post-batch sweep on the
   origin's newest recorded sighting — the discovery may already have been displaced,
   which is one reason history (step 3) lands first. The check replays the case k times
   with exact `for_choices` semantics (the shrink verify's instrument, :698, not
   `for_probe` — pre-flip, divergence must surface, not get repaired by continuation
   draws). "Looks deterministic" = every replay is structurally aligned (the :390-397
   comparison shape) *and* fails. Any miss flips the run and the check's observations
   seed that origin's ledger, so the bar starts partially filled. All-reproduce costs +k
   executions per origin, deterministic runs have 0-2 origins, and passing runs pay
   nothing. Scope: generation-discovered origins; an origin first admitted at shrink
   verify or final replay keeps decision 35's path.
3. **Deterministic-henceforth, with history.** Pre-flip rules stay cheap — raw shortlex
   displacement continues (G20 option (c) stays rejected) — but every pre-flip interesting
   execution (raw sightings and shrink accepts alike, all through `record_run`'s
   interesting arm, :1949-1953) enters a per-origin in-memory history, deduped by
   serialized choices. Nothing about persistence timing changes: backtracking works from
   memory, so decision 44's save-then-delete and the Ctrl-C property stand as shipped.
4. **Final check; backtrack on a miss.** The engine-owned final replay already replays the
   shrunk case once (:1549-1566) — this plan adds no k_f there; a miss — or any earlier
   ND evidence on a never-flipped run: cache mismatch, reuse-comparison miss, first-check
   miss on a later origin, shrink-verify miss (:694-715) — now backtracks instead of
   taking one bar attempt with spent budgets. The backtrack scans the history for the
   reproduction boundary — the newest entry that still reproduces — with cheap
   continuation-tolerant replays (`nd_replay_once`, :1432 — post-flip the bar itself
   accepts via continuation, and exact replays would skip the structurally racy entries
   the scan exists to find), geometrically rather than linearly (G25 has the algorithm);
   the candidate it settles on gets the full bar (`nd_evidence_batch` takes an arbitrary
   timeline, :1613); a bar-cleared entry is restored as incumbent, seeds the anchor
   through the 20-run extension (decision 54), pools the scan's other reproducing
   entries, and gauntleted shrinking resumes under the remaining deadline. A bar reject
   resumes the scan on the older side; an exhausted scan falls back to today's
   caveat-only report. The boundary framing needs no boundary to exist: on an always-ND
   origin (no slip-in point) any reproducing probe is a candidate and the bar
   adjudicates, and both scan errors self-correct — a too-new candidate anchors low or
   gets rejected, a too-old one costs re-shrinking the gauntlet makes safe (decision 2)
   — which is why the scan biases old under uncertainty. On a gradient landscape the
   restored entry sits near the top of the reproducing region, not necessarily at the
   pre-displacement sighting; experiment 011's restored-vs-history-best column prices
   the residual gap. ND mode itself (bar, gauntlet, anchors, pools, v2 blobs, caveated
   reporting) is unchanged, and backtracking is scoped to never-confirmed origins: a
   confirmed origin's final-replay miss keeps today's `nd_reproduce` pool path
   (:1567-1578).

Against the three measured loss mechanisms (`research/g20-seam-analysis.md`): mechanism 1
(n = 1 displace-and-persist) keeps its cheap pre-flip behavior, but history retains what
displacement discarded; mechanism 2 (one late bar attempt) becomes many — each probed
entry reaches the bar at roughly its true reproduction rate and each bar attempt holds
45% target-regime power, so three attempts compose to ~83%; mechanism 3 (never-flip v1
blobs) is attacked from both ends — the first check catches structurally racy bodies at
discovery (escape ~(p·s)^k, and s is smallest on raw cases, before shrinking shortens the
timeline), and the v1 blob continuation fix repairs the blobs that still escape.

## Decision gates (DRM input needed)

Six gates: constants and policy shapes, resolvable in one sitting. Nothing in phase 14
waits on any of them.

**G21. Execution-cache scope, bound, and serve policy.** The entry a served conclusion
needs is (status, origin, realized nodes, spans) — `EngineShrinkProbe::run` reads all
four, keys the gauntlet ledger on the serialized realized values, and returns the full
nodes to the shrinker (:2151-2172); span mutation reads status only (:2309). Key on
realized choice values with `ChoiceValueKey` semantics (`data_tree.rs:37-96`: floats by
bit pattern, clones by child values); conclusions only, overruns never cached. Policy
split to pin: generation-phase duplicates *execute* (they are the duplicate-stop signal
and free mismatch probes); shrink-phase repeats are *served* (the measured 85% win).
Options: (a) two-tier — during generation a digest map fingerprint -> (status, origin,
realized kinds; the kinds are what a hit compares for drift, and folding them into the
key would silently drop the check); full serving entries kept only during shrinking,
byte-bounded with eviction (010's memory caveat: the stateful tree held ~77k nodes); (b)
one whole-run full-entry cache, capped. Recommendation: (a). Under ND the cache is
disabled entirely — flushed at the flip, no serving and no inserts (post-flip, identical
timelines concluding differently are expected); that satisfies decision 6's surviving
clause. This gate also signs the detection trade: the tree checked kinds at any shared
prefix and fired as early as run 2; the cache checks exact-value repeats only, the reuse
comparison covers stored-entry replays, and in exchange verdict flips — invisible to the
tree — are detected.

**G22. Duplicate-stop constant and exhaustion semantics.** Stop generation after N
consecutive generation-phase duplicates: a counter on the engine, reset on novel, gated
to the generation window (the `collect_statistics` flag marks it, set :431 / cleared
:614), skipping measurement runs (`record_run`'s flag — the first check would otherwise
inject k consecutive duplicates after every discovery), staying active under health-check
suppression (or tiny suppressed spaces grind to the full budget), and suspended under ND
handling. Recommendation: N = 10 = `RANDOM_GENERATION_BATCH`. False-stop bound: the
window below coverage c lasts on the order of the space size S, so a run stops before
coverage c with probability of order S·c^N — negligible on large spaces (the per-case
duplicate chance never approaches c) and small-space stops are the intended behavior.
The health-check tests pinning exact execution counts on large spaces (`test_health_
check.rs`) hold as-is. The exhausted-space FilterTooMuch variant (:592-608) swaps its
`is_exhausted` conjunct for stopped-by-duplicates — the other conjuncts (no interesting,
not trivial, valid == 0, invalid > 0, not suppressed) unchanged — or a
bool+assume(false) body becomes a vacuous pass. Statistics keep reporting the stop as
exhaustion.

**G23. First-check budget.** (a) k = 4 exact replays, stop-on-first-miss; detection
1 - (p·s)^4, cost +4 per origin (experiment 011's arithmetic); (b) k = 3, counting the
discovering sighting as the bar's first failure (+3, the seam analysis's arithmetic).
Recommendation: (a) — the discovering case is selection, not evidence (decision 21's
reasoning), and the round exponent prices the spec. Mechanics signed with the constant:
check replays fold into the origin's ledger on a miss (a new evidence-seeding entry
point on `OriginLifecycle`; today only `reject()` folds, and it counts a rejection,
`lifecycle.rs:267`); `nd_force` skips the check (:1396 starts flipped); check replays
must not displace or persist (:1949-1953 takes no `measurement` guard today; the guard
must exempt the reuse path's replays — the Error+v2 interaction phase 16 pins).

**G24. History retention.** Entries are realized `Vec<ChoiceNode>` (what restore and
`Persister::record` need), deduped by serialized choices, recorded in `record_run`'s
interesting arm; spans are not stored (the restore's bar batch supplies a fresh witness).
Recommendation: keep everything — every raw sighting and every shrink accept, no
eviction. A recency bound evicts the entries an early slip-in needs most (the boundary
sits at the oldest accepts there), and forcing the scan back onto a raw sighting costs a
full gauntleted re-shrink — thousands of replays to save kilobytes. The memory argument
runs the other way now the tree is gone: the tree interned every execution (~77k nodes
on 010's stateful workload) where history holds only interesting cases, and accepts
shrink monotonically, so the entry sizes telescope. The `__bench` dump records history
bytes per origin; if a real workload bites, the recorded fallback is middle decimation —
drop every other interior entry, so both ends survive and the geometric scan loses one
probe of resolution — never end-eviction. An origin's history is dropped when it
confirms (the pool takes over) or at run end; dropping at confirmation also keeps the
accept segment shortlex-sorted — accepts strictly shrink the incumbent and no
post-restore accept is ever recorded — the sorted domain G25's scan searches. ND-mode
origins keep pools, not history.

**G25. Backtrack scan and budgets.** The scan hunts the reproduction boundary rather
than walking linearly: newest-first, a linear walk burns its budget on the degraded tail
before reaching anything worth restoring, and barring the first entry that happens to
reproduce re-runs mechanism 1 in miniature — the bar is permissive by design (it targets
p >= 0.1), so it admits a degraded-but-genuine entry and the anchor then ratifies the
loss. Algorithm: probe the accept segment at geometric offsets from the newest (1, 2, 4,
...) plus every raw sighting, one `nd_replay_once` replay each — a coarse reproduction
profile in ~log2(accepts) + |raw| replays; binary-refine between the newest reproducing probe
and its nearest newer non-reproducing one, budget permitting, else take the
known-reproducing position (the old-biased error: a too-old restore re-shrinks under the
gauntlet, priced by decision 2's machinery, while a too-new one anchors on degraded p);
bar the candidate; a reject resumes the scan on the older side; no reproducing probe
spends the remaining replay budget on a second pass before caveat-only. On the slip-in
landscape this is binary search for the slip-in point — the old side reproduces at ~1,
so the only noise is racy-side false positives at rate p, which the bar adjudicates;
elsewhere the probes are Bernoulli samples and the same adjudication applies. With G24's
unbounded retention the accept segment runs to hundreds of entries, so the geometric
scan is what keeps the profile at ~2 log2(m) replays instead of linear in the chain.
Recommendation: cap scan replays at `CONFIRM_CAP` (40) and bar attempts at 3 — a probed
entry reaches the bar at roughly its true rate, each attempt holds 45% target-regime
power, and three compose to ~83% — everything counted as measurement, with
`capture_replays` save/restore around nested batches (phase 14 fixes the clobber).
Resume: on mid-run
evidence the existing loop mechanics suffice (restore the incumbent, `confirm` with the
batch witness, skip `shrunk_origins` — the R4 requeue pattern, :805-810); at
final-replay time the scan calls the per-origin shrink method phase 14 extracts from
`run()`. Termination re-uses R4's argument: the scan is replay-capped, `nd_active` never
clears, and the resumed shrink is gauntleted.

**G26. Contract amendments.** Three prior decisions get amended and the changes are this
gate's to sign, separately from the constants above. Decision 30 (`error` diagnostics):
the check's structural miss gets a NonDeterministic-class diagnostic naming the
divergence position (richer than the tree's kind message); aligned-but-outcome-changed,
and a cache mismatch, get `flaky_diagnostic()` verbatim (the tree's kind mismatch said
NonDeterministic; the cache mismatch is literally "different outcomes on the same
generated data", so Flaky is the honest class). Error-mode suites will abort on races the
old engine missed until shrink-verify or never — the lint improving, but a behavior
change to record. Decision 49 (stamping): the stamp is decided before execution
(:1866), so the discovering case cannot be stamped retroactively; instead the check's
replays run stamped (`capture_replays` around the check batch) — a stamped failing
replay of the same choices serves as the captured discovery, and an origin whose k
replays all pass has no stamped failing case until verify or final replay, the recorded
residual. Decision 51 (the statistics line): its "while `nd_active`" scope no longer
covers all measurement — the line also counts pre-flip check replays (the scan runs
post-flip and is already counted).

## Experiments

**011 — instrumented seam spot check** (acceptance for the whole plan; baseline half in
phase 14, comparison half in phase 17). The frozen `experiments/gauntlet-calibration`
crate needs zero harness-side changes to *run* — it drives the stable public ABI — but
gets the instrumentation columns the seam analysis names, via a `__bench`-gated event
dump in the engine (the `nd::watermark_dump` pattern): flip call index, flip site
(tree kind-mismatch [baseline only] / first-check / cache mismatch / reuse comparison /
verify / final replay / v2 / concurrency), the incumbent's realized values at flip, the
evicted incumbent's values on every reject-evict (decomposing caveat-only into correct
fluke rejections vs power misses), the restored entry's values against the history's best
on every backtrack, per-replay first-check outcomes, and blob kind. The amendment to the
frozen crate is recorded as a dated section in its README and the 011 notes. Baseline
(phase 14, before any engine change beyond the dump itself): the same cells with the new
columns — the decomposition the seam analysis says is missing. Comparison (phase 17, new
engine) acceptance:

| cell | criterion | baseline |
| --- | --- | --- |
| L1 | final-p median >= 0.50 and p10 >= 0.26; distance to the 0.82 envelope reported with the restored-vs-best column; caveat-only <= 4/100; execs <= 1.5x baseline | 0.34, p10 0.26; 4; 11.4k |
| L3 | 0.50 finals, caveat-only 0 (regression only) | pass |
| L4 | caveat-only <= 2%; bug kept among shrunk >= 99% | 15%; 83/85 |
| L4b | caveat-only <= 10%, decomposition column shows the remainder is fluke rejections and bar power, not seam loss; bug kept >= 99% | 49%; 50/51 |
| D2 | deterministic finals 100/100; execs <= 8x baseline (009b prices the p = 0.9 measurement multiplier at 4-6x; the letter sits above the priced band) | 100/100; 1047 |
| D0 (new deterministic control) | 0 flips, 0 caveats, execs = bug-free arithmetic + k x origins + final replay | — |
| all | aborted = 0; no-bug = 0 (duplicate-stop non-interference; every body's space is huge) | pass |

Plus a model-validation column: per-cell flipped-at-first-check share must match
1 - p^4 for the checked case (the incumbent at sweep time, not necessarily the
discovery): L3 ~94%, L4b ~100%, L4 ~34%, D2 ~76%. Note 0.82 stays the envelope, not the
criterion: 0.9^4 = 66% of high-p L1 origins legitimately pass an outcome-only first
check and shrink deterministically until backtracking, so L1's letter is the phase-12
0.50 plus the p10 floor, with the decomposition columns proving the rest is priced.

**012 — detection-escape recheck** (phase 17). A new frozen crate cloning
`experiments/watermark`'s episode protocol: {clone, machine} x p in {0.1, 0.3, 0.9} plus
a deterministic control, 200 episodes each, with 011's event columns. Criteria: clone
p = 0.9 never-flip <= 1/200 (baseline 23/200 = 11.5%; the first check's escape is
(p·s)^4 with p·s estimated at 0.085-0.13 from 009a's schedule arithmetic and v1-blob
rate, so ~3e-4 at the top of the range); blob reproduction >= 199/200 on both bodies at
p = 0.9 (baselines 180 and 191 — the v1 continuation fix carries the episodes that still
escape, since with continuation the per-attempt rate matches the reuse path's 99%);
p <= 0.3 cells hold 009a's >= 98% reuse/blob rates; deterministic control: 0 flips and
exactly k x origins measurement replays.

## Phases

### Phase 14: instrumentation, independent fixes, and the extraction

Everything valuable regardless of how G21-G26 resolve, in landing order:

- **The `__bench` event dump** (011's engine half): the column list above, including the
  tree kind-mismatch flip site that exists only until phase 15.
- **Experiment 011 baseline**, run at the dump commit before anything else lands: the
  caveat-only decomposition and flip-site table as a dated 011 notes section.
- **v1 blob continuation and retry** (seam-analysis option 6, mechanism 3's cleanest
  fix). `reproduce_blob`'s Choices arm replays `for_choices(..., None, None)` once
  (:208-219); it gets the continuation budget the reuse path already has and a retry
  budget of 4. The plan's replay chain is k = 4 check replays + one verify + one final
  replay, so the worst-case joint escape-then-miss over all (p, s) is max x^6(1-x)^B:
  5.7% at B = 1, 1.2e-3 at B = 4 — and with continuation the per-attempt rate matches
  the reuse path's 99% on 009a's bodies, so the realistic residual is far smaller. The
  stale report keeps naming both hypotheses. Two neighbors stay put deliberately: the
  reuse phase's single-miss-demote (decision 11) and the pre-shrink drain's exact
  replays (:651-656) — a hit on either is the origin's first sighting and gets the
  step-2 check, which supplies the continuation-tolerant follow-up. Tests:
  `a_v1_blob_replays_with_the_continuation_budget` (red today),
  `a_v1_blob_retries_up_to_its_budget`,
  `a_truly_stale_v1_blob_still_reports_stale_within_budget`.
- **The `capture_replays` clobber**: `nd_evidence_batch` clears it to false on exit
  (:1621, :1649) instead of restoring — harmless today, wrong the moment a batch runs
  inside the final replay's capture window — fixed with a pin.
- **The shrink extraction**: the per-origin shrink body (:693-810) moves out of `run()`
  into a method threading `shrink_deadline`, `shrunk_origins`, and the output closures.
  Behavior-identical, gate-independent, and the plan's one refactor with real regression
  surface — it lands here, alone, with the existing shrink tests as the harness, so
  phase 16 composes onto settled ground.

Exit: baseline table recorded; the fixes and the extraction green with the existing
suite.

Landed 2026-09-04 (ad3ff0c1..82f83966, decision 59): the dump, the baseline (its
headline: L4b's 49% caveat-only is 43 power misses to 6 correct fluke rejections, and
L1's incumbent sits at the 0.26 floor by flip time at every percentile — see the 011
notes), the v1 continuation/retry fix with its three red-first pins, the capture-flag
restore, and the extraction. Gates green.

### Phase 15: tree removal

The removal inventory, from the groundwork sweep:

- `data_tree.rs` (835 lines) and `data_tree_tests.rs` (758 lines) deleted; `sub_key`
  relocated first (it is a database-key helper, `database.rs` is its home; call sites
  :305, :629, :834 re-pointed). The span-event plumbing exists only to feed
  `record_tree_full` and goes with it: `RunResult::span_events`,
  `NativeDataSource::take_span_events`, `RealizedStream::span_events`
  (`choices.rs:731-772`). Orphaned `ChoiceKind` APIs (`random_value`, `enumerate`, the
  `max_children` family) deleted with their tests — the ratchet forbids dead code.
- Generation loses the novel-prefix arm (:496-504): `new_random_with_params` everywhere,
  `for_probe_with_params` deleted. The `hegel.h` doc line mentioning the choice tree is
  reworded at its source (`hegel-c/src/lib.rs:170`) and the header regenerated.
- **The flat cache** (shape per G21): serving replaces `simulate_full` in
  `cached_test_function` (:2038-2054) for exact full-sequence repeats under
  `!nd_active`; mismatch detection replaces `record_tree_full` in `record_run`
  (:1893-1916) through the same returned-diagnostic plumbing (`error` keeps it,
  quiet/warn flip); kind drift compared on generation-phase hits per G21. Known
  capability losses, accepted per 010: trailing-unread proposals, predicted overruns for
  truncated proposals, pun/forced prediction (serves ~= exact repeats, 1146 vs 1134 on
  the shrink workload).
- **The reuse comparison becomes a detection channel**: the realized-vs-stored
  comparison (:390-397) stops being a silent `replay_aligned` clear — a structural
  divergence on a stored-entry replay is ND evidence, routed like a cache mismatch
  (`error` aborts, quiet/warn flip). This is the tree's cross-sequence kind-drift
  coverage moving to the one place stored and realized timelines still meet; without it,
  phase 15 would leave `error`-mode generation-level detection with no source at all.
  Decision entry recorded here.
- **The duplicate-stop** (constant and gating per G22): counter beside the cache insert
  in `record_run`; the two loop reads (:462, :477) become counter < N; the FilterTooMuch
  conjunct swap per G22. It lands in the same commit as the novel-prefix removal — with
  the prefix arm gone duplicates actually occur, and without the counter nothing stops
  tiny spaces.
- **Test rework** per the inventory: the three tree-serving tests rewritten against the
  cache; the truncated-overrun prediction test inverted; the
  kind-flip-across-stored-sequences tests and `test_flaky_global_state` re-homed onto
  the reuse channel (their fingerprints never repeat, so no cache trigger exists);
  `tree_exhausted_filter_too_much...` rewritten against the G22 variant. The
  structure-flip quiet/warn tests and `a_double_flip_in_one_verify...` move to phase 16
  with the first-check channel that replaces their trigger; until then their bodies'
  flips arrive at shrink verify, and the tests assert that interim honestly. Red-first
  new pins: `a_repeated_stateful_probe_is_served` (red today — the tree declines it),
  `a_reexecuted_fingerprint_with_a_different_outcome_flips_the_run` (red today — no
  verdict-flip channel exists), `cache_mismatch_compares_status_and_origin`,
  `a_diverged_stored_replay_flips_the_run` (the reuse channel),
  `generation_stops_after_consecutive_duplicates_on_a_tiny_space` (red today in the
  execs >= 4 + N direction),
  `filter_too_much_fires_via_the_threshold_variant_on_an_exhausted_space` (red today on
  the message), `the_execution_cache_is_flushed_and_serving_stops_at_the_flip`,
  `duplicate_counter_resets_on_a_novel_case`,
  `duplicate_stop_never_fires_on_a_large_space`,
  `duplicate_stop_stays_active_under_health_check_suppression`,
  `duplicate_stop_is_disabled_under_nd_handling`, `the_execution_cache_is_bounded`.
- **Cost guards**: a seed-pinned exact execution-count parity test on a passing body
  (recording removal must change wall time only), and a seed-pinned shrink-execution
  guard on a shrink-heavy deterministic body at <= ~1.1x the tree-era count (the 85%
  serve win, red on a no-cache build).
- **Docs with the change**: design.md's data-tree section replaced (cache +
  duplicate-stop + reuse channel), the flip-source list and conceptual-model vocabulary
  updated.

Exit: suite green with the reworked inventory; both cost guards green; `just c-header`
clean.

### Phase 16: the workflow

Steps 2-4, in dependency order:

- **History first** (retention per G24) — the check and the backtrack both read it. Recording
  hooks `record_run`'s interesting arm (:1949-1953), not `update_interesting`'s
  mutations: after a fluke displaces the incumbent, later genuine sightings are
  shortlex-larger and never insert or replace, yet they are exactly what the scan needs.
  The same commit scopes that arm's missing `measurement` guard (per G23): check and
  scan replays must not displace or persist, while the reuse path's `nd_reproduce`
  replays must keep doing both — under `error` strictness a v2 entry reproduces with
  `nd_active` still false, and that arm populating `interesting` is what makes
  `found_in_reuse` true. Pinned both ways.
- **The first-interesting check** (budget per G23, contracts per G26): a pre-flip sweep
  sibling of `nd_discovery_sweep` (:1748-1791, same call sites), keyed on a checked-set,
  running k exact `for_choices` replays of the origin's newest history entry with the
  structural comparison; a miss flips, seeds the ledger through the new evidence-seeding
  entry point, and routes `error` to G26's diagnostics; all-reproduce marks the origin
  checked. Accounting through `measure()` plus the deterministic-check counter feeding
  decision 51's amended statistics line; check replays stamped per G26.
- **Backtracking** (scan and budgets per G25): the boundary scan + bar + restore +
  resume, mid-run first (existing loop mechanics), then the final-replay site calling
  the phase-14 extraction. Two subtleties from the groundwork: `final_replay`'s deterministic
  branch `continue`s on an outcome match (:1560), so a mismatch-triggered flip mid-loop
  is invisible to its control flow — an explicit post-iteration `nd_handling()` check
  routes already-replayed origins into review; and the Persister needs a supersede
  operation — `needs_save` is monotone on sort_key (:1257-1263) and would refuse the
  restored, shortlex-larger incumbent, leaving the barred shrunk bytes as primary —
  save-the-restored-entry-then-delete preserves decision 44's ordering. Scoping:
  never-confirmed origins only; confirmed origins keep the existing ND final-replay
  path, so no second-confirm lifecycle transition is needed.
- **Test rework carried from phase 15**: the structure-flip quiet/warn tests and
  `a_double_flip_in_one_verify...` rewritten against the first-check channel.
- **Red-first set** (each pinned red before its mechanism lands, the phase-12
  seed-pinned-scramble idiom): `a_first_check_outcome_miss_flips_the_run_before_
  displacement`, `a_first_check_realized_timeline_miss_flips_the_run`,
  `first_check_evidence_seeds_the_origins_ledger`, `each_origin_gets_its_own_first_
  check`, `an_all_reproduce_first_check_keeps_the_run_deterministic` (exact +k count),
  `history_records_raw_displacements_and_shrink_accepts`,
  `history_dedupes_repeated_timelines`, `history_is_kept_only_while_deterministic`,
  `a_displaced_incumbent_is_recoverable_after_a_late_flip` (the L1 loss as one test;
  its seed pins fluke displacement — the gradient case is 011's restored-vs-best
  column, not a unit pin),
  `a_final_replay_miss_backtracks_to_the_reproduction_boundary`,
  `the_backtrack_scan_probes_geometrically`,
  `a_flip_during_final_replay_reviews_already_replayed_origins`,
  `the_scan_continues_past_a_bar_rejected_candidate`, `backtrack_pools_the_other_
  reproducing_entries`, `a_backtracked_incumbent_anchors_from_its_bar_batch`,
  `an_exhausted_backtrack_reports_caveat_only`, `backtrack_replays_are_capped`,
  `backtrack_resumes_gauntleted_shrinking_under_remaining_budget`,
  `a_passing_run_replays_nothing`, `a_deterministic_failing_run_pays_exactly_the_first_
  check_per_origin`.
- **Docs with the change**: design.md's shrinking (the decision-38 paragraph), origin
  lifecycle (evidence seeding, history, restore), reporting (final-replay behavior,
  stamped sites), accounting, and statistics-constants sections.

Exit: the red-first set green; the two cost guards from phase 15 still green; decision
entries recorded.

### Phase 17: validation and the closing sweep

- Experiment 011's comparison half against the acceptance table; experiment 012. Misses
  escalate to DRM with the decomposition columns — the table's letters are the contract.
- The closing sweep: design.md's three affected risk entries and the concurrency
  section's residual-misses sentence; evaluation.md rows 6, 9, 21, 29, 30 re-verdicted,
  rows 10, 17, 20, 23 annotated, and the dead kind-set-tolerance residual replaced;
  README.md's file list gains this plan; changelogs record the user-visible changes
  (exhaustion semantics, the FilterTooMuch message on exhausted spaces, `error` mode
  detecting earlier, v1 blob replay adopting continuation and retries).
- /self-review over all prose added by phases 14-17; full gate run per production-plan
  phase 8's list.

Exit: acceptance table green or escalated; every amended decision's new wording landed;
gate run green.

## Decision entries to record

Numbers assigned in landing order. Phase 14: the v1 continuation/retry budgets with the
drain and reuse-path retentions; the capture_replays restore as a recorded defect
correction. Phase 15: tree removal closing decisions 6 and 29 (gate G3) — recording what
survives verbatim (ND never serves cached conclusions, now hosted on the cache) and
G21's detection trade; the reuse-comparison detection channel; the duplicate-stop
constant with its derivation; the FilterTooMuch conjunct swap. Phase 16: the universal
first-interesting check (extends decision 21's principle to every run — new entry, not
an amendment); the history and backtrack mechanism superseding decision 38 (its
termination argument re-made) and amending decision 17's title clause (the backtrack is
detection-triggered and bar-gated, outside 17's rejected statistically-triggered
rollback) and decision 35's final-replay clause (reject triggers the backtrack; evict is
the fallback); G26's amendments to decisions 30, 49, and 51; the measurement-guard
scoping with the Error+v2 interaction; the Persister supersede operation (decision 44's
ordering preserved). Phase 17: G20 closed — the entry records what option (d) subsumes
from the seam analysis's graded options (1, 2, 3's intent, and 6, implemented in phase
14) and the two residuals it deliberately leaves (pre-flip single-run trust inside a
checked origin's shrink, priced by 011's L1 letters; and the never-flip share that
passes an honest check, priced by 012).

## Risks

- **The cache is not the tree.** 010's serve counts are a slight upper bound on
  flat-cache hits (the tree also served fresh-generation prefixes). The phase-15 shrink
  guard measures the realized rate; if the win does not materialize the guard fails,
  and the fallback is widening the cache to prefixes, not restoring the tree.
- **The duplicate-stop is a heuristic where exhaustion was a proof.** The tree knew the
  space was exhausted; the counter infers it. The exposure is asymmetric: a false stop
  truncates generation on a small-but-unexhausted space (bounded by G22's S·c^N
  argument), a missed stop costs budget, not correctness. The health-check tests pin
  exact counts on large spaces, and 011's no-bug column pins non-interference.
- **The gradient landscape.** The scan restores near the top of the reproducing region,
  but on a smooth gradient single-replay probes still land stochastically below the
  history's best entry. The scan's old bias bounds the error's direction, and 011's
  restored-vs-best column measures the residual gap instead of the plan claiming full
  recovery.
- **History memory.** Unbounded retention on a long shrink chain over a large stateful
  case is real memory, though less than the tree it replaces held for the same run. The
  `__bench` history-bytes column measures it; the fallback is G24's middle decimation,
  and no bound gets reintroduced silently.
- **Error-mode earlier detection.** Suites that passed under `error` because the old
  engine never re-executed their racy case will now abort at first discovery. Correct as
  a lint, but a behavior change the changelog and decision 30's amendment must own.
- **Wall clock.** The check is +k per origin and backtracking is bounded, but D2-shaped
  deterministic-looking landscapes that flip late pay check + scan + bar + reseeded
  gauntlet; 011's D2 letter (execs <= 8x, above 009b's priced 4-6x band) is the hard
  bound.
- **Frozen-crate amendment.** 011 amends a frozen harness. The freeze exists to keep
  results comparable; the amendment is additive columns plus a dated README note, and
  the baseline rerun on the pre-change engine re-anchors comparability.
