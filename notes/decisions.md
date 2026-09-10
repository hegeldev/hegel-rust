# Decision log

Append-only. Each entry: the decision, rejected alternatives, rationale. "DRM" = David,
"review" = the adversarial design review (`research/critique-*.md`), "agreed" = discussion.

## 2026-09-02

1. **Strictness defaults to quiet.** New setting `nondeterminism_strictness = quiet | warn | error`,
   default quiet — no notice at all when a run flips to ND handling. `error` keeps today's aborts
   for people using them as an accidental-global-state lint. (DRM. Rejected: notice-by-default.)

2. **Shrinking must not lower failure probability.** Raise it when possible (boost before
   shrinking); if shrinking reaches a deterministically-failing region, stay there. Guard against
   small-sample overconfidence. (DRM.)

3. **Unreproduced failures still fail the run**, reporting the observed failure with a caveat
   naming both hypotheses (rare failure vs. environment modification), evidence-weighted wording.
   (Agreed. Rejected: today's abort-with-flaky-error.)

4. **Failure identity stays per-origin** (panic site string). (DRM. Rejected: coalescing origins
   observed from one sequence.)

5. **Representation: timeline pool first.** Semantics are branch-points-with-per-value-suffixes;
   storage is a flat failing incumbent + bounded pool of whole realized timelines per origin.
   The merged trie encoding is deferred: no stable trunk during shrinking (every accept
   invalidates folded branches), anchoring is unreliable, and it costs a new ChoiceValue variant,
   CloneRecord equality, sort-key extension, and recursive serialization. Revisit if the pool
   shows heavy prefix sharing. (Agreed, from review. DRM's earlier preference was tree-shaped
   suffixes; conceded as storage, kept as semantics.)

6. **Data tree**: restore for ND runs if feasible; caching is low priority; disabling is
   acceptable. ND mode never serves cached conclusions. (DRM + review.)

7. **Shrink predicate: charge accepts, not rejects.** Single-run rejects; accepts accumulate
   ledger evidence past an LCB threshold scaled off the incumbent anchor. Rejected candidates
   must be retryable when a pass completes without shrinking (pass repetition), with evidence
   accumulating across retries. (Agreed; DRM added the reject-retry requirement. Rejected:
   per-candidate fixed-N for everything — uniform N-times cost; naive single-run accepts —
   monotone probability drift with no recovery.)

8. **Persist only the representation, never estimates.** ND-ness is carried by the entry/blob
   format itself (self-identifying); nondeterminism-rate estimates are computed afresh each run.
   (DRM, correcting an earlier "persist ND status" answer that had been read as a status flag +
   metadata.) Consequence: every run stands alone; CI with the database disabled is fully served;
   no flag clearing/flapping problem exists.

9. **Detection uses within-run evidence only.** A stored DB entry that stops reproducing is
   staleness (usually a code change), never ND evidence. (Review.)

10. **Capture at confirmation**, not at discovery: the engine marks confirmation replays
    capture-enabled; shrink probes stay cheap; the caveated fallback report shows the shrunk
    incumbent; the sacrificed-first-case trick and NondetStash go away. (Review; DRM directed
    that the current implementation is a bandaid to be replaced, not extended.)

11. **DB hygiene: demote then delete.** Replay budgets derived from the p >= 0.1 target with
    early exit (low-to-high 20s cap, ~1/p expected for real bugs); primary miss -> demote to
    secondary, secondary miss -> delete. No persisted miss counters. (Agreed.)

12. **Workload priorities**: concurrent stateful #1, plain clone-based concurrency #2; external
    randomness and timing dependence should fall out of a good implementation automatically. (DRM.)

13. **Scope**: ignore the parallel-tests branch until it lands; ignore Antithesis (separate
    determinator-based path inside Antithesis). (DRM.)

14. **Per-position anchoring (incl. inside clone streams): deferred, simplest-first.** Start
    with whole-timeline machinery only; instrument where replays fall off stored timelines. DRM
    is suspicious of the "no anchors in clone streams" argument — heavy nondeterminism may need
    *more* alternative tracking, not less — so this is decided by data, not now. (DRM.)

15. **Branch process**: this branch is working state. Commit notes and experiments liberally;
    history cleanliness doesn't matter; the final implementation will be extracted with pruning
    and history rewriting. (DRM.)

16. **Target**: handle tests failing >= 10% of the time they are run; budgets and confidence
    arithmetic are derived from that target rather than fixed small constants (a flat N = 10
    misses a p = 0.1 failure 35% of the time). (DRM set the target; review set the arithmetic.)

17. **No checkpoint/rollback in the shrink loop.** Experiment 1 follow-up, mixture landscape:
    rollback-on-uncertainty (LCB below bar after 10 validation runs) rescues mispinned
    incumbents but fires constantly on stable landscapes — L1 cost 2.3x, missed reductions
    9% -> 52% from poisoning good candidates; rollback-on-proof (UCB below bar, up to 40 runs)
    is free but never fires, because the mispinned rate sits within Wilson noise of any bar
    derived from the accept-time anchor. Capture-at-confirmation removes ~95% of the hazard at
    source (pin-failing vs pin-random tables); the residue is a report-time annotation via
    final validation, not a search-time rollback. (Experiment result.)

18. **Stopping rule: confirmed-dry.** After a dry sweep, run one confirmation sweep where every
    proposal skips the single-run fast reject and drives cumulative ledger evidence to a bound
    decision; stop only if it accepts nothing. Same cost as three fixed dry sweeps, half the
    missed-reduction rate where misses are recoverable (L3 18% -> 10%), and stopping carries a
    certificate. Replaces the fixed dry-sweep count. (Experiment result.)

19. **Anchor decay rejected; post-accept evidence never feeds the anchor.** Decay bought 1-2
    length units on L1 at +20-40% cost and made stopping incoherent (missed 51-75%: threshold
    still falling at stop). Separately, evidence gathered under timeline replay must not raise
    the anchor, or a pinned incumbent prices fresh-generation candidates out and stalls the
    shrink. (Experiment result; confirms DRM's monotone-anchor stance.)

20. **In ND mode, raw interesting runs never displace an occupied origin.** A raw interesting
    execution may fill a vacant origin (first discovery, pending confirmation); displacement
    happens only through validated accepts (the shrinker's gauntleted result). Experiment 003
    showed the alternative live: post-discovery generation flukes at the noise floor displaced
    a 20/20-confirmed discovery in ~80% of noise-floor trials. (Experiment result; instance of
    the standing "gate all acceptance paths on validated accepts" decision.)

21. **Discovery-time confirmation is a prerequisite, not a nicety.** First-interesting on a
    noisy test is a background fluke more often than a real bug (2:1 in experiment 003's
    noise-floor). Today's Flaky abort accidentally filters those by refusing to proceed when
    the verify replay doesn't reproduce; ND mode removes that abort and must replace it with
    the confirmation batch — an unconfirmed origin is dropped and generation keeps hunting.
    The confirmation bar must respect the noise-floor caution (a flat >= 2-fails-in-20 passes
    p = 0.02 noise ~5% of the time); deriving it is 005's job. (Experiment result.)

22. **Replay: pool cap 5-10, first-fit, small continuation budget; trie stays rejected;
    divergence signals weight evidence rather than abort.** Experiment 004 on structurally-ND
    bodies: K=5 captures nearly all recoverable reproduction, K=10 reaches the plateau, K=20
    adds nothing, worst-case cost ~3 replays/attempt; extend 4 absorbs all net elongation and
    larger budgets buy nothing. Prefix sharing is anticorrelated with pool need, so the merged
    trie's "revisit if heavy sharing" trigger fires only where pooling is unneeded. First
    divergence is not replay death (punned replays stay aligned after the damaged position and
    still reproduce), so divergence detection downweights diverged non-failures instead of
    bailing early. Per-position anchoring (decision 14) stays deferred with a measured target:
    the ~27% residue whole timelines can't reach on heterogeneous bodies; 006's span grafting
    probes it. (Experiment result.)

23. **Discovery-confirmation bar: gate then extend — 10 replays, reject on zero failures;
    otherwise continue to 40 total, accept early on the 4th failure.** Exact-DP comparison
    (experiment 005A) against flat k-of-B, SPRT, and Wilson-threshold rules: 0.6% false
    accept per p = 0.02 fluke, 45% per-discovery power at the p = 0.1 target, 15 replays per
    rejected fluke, 4.4 per p = 0.9 confirmation. Rationale: false accepts are sticky (they
    occupy the origin behind the displacement gate) while false rejects recycle through
    re-discovery, so per-discovery power is the cheap thing to trade away. Wilson-LCB-over-
    noise-floor rejected for confirmation (26% false accept); SPRT rejected as paying 50+
    replays for power that recycling provides free. Replaces 003's placeholder >= 2-in-20.
    (Experiment result; completes the derivation decision 21 assigned to 005.)

24. **Confirmation gates origin admission, not one code path.** Hooking discovery
    confirmation on the generation run's own status let span-mutation executions fill vacant
    origins unconfirmed; on pure noise those slipped to shrink and produced false confirms in
    26/30 runs. The engine sweeps every unconfirmed interesting origin after each generation
    step, and an untrusted origin reaching shrink (discovered mid-shrink) faces the full bar
    there. Origins reproduced from the DB are trusted on reproduction — the prior run only
    persisted confirmed origins, and re-running the bar would drop real p ~ 0.1 reused bugs
    ~55% of the time. Caveated `[unconfirmed]` failures are reported only when nothing
    confirmed (caveat fatigue otherwise). Generalizes decision 20 from displacement to
    admission. (Experiment 005B result.)

25. **Replay-until-failure order: pool first-fit, then positional splices, then fresh
    generation; boost ships as machinery, defaulting off outside deterministic-core
    rescue.** Experiment 006: cheap positional splicing of pool pairs recovers 65-100% of
    full-pool replay misses (~6 replays/miss), so the 004 residue needs recombination of
    stored content, not per-position anchoring or a trie — span-anchored grafting is an
    optimization for implementation, not a prerequisite. Successive-halving boost (~250
    replays, holdout-gated) closes the deterministic-core tail (27/30 -> 30/30 deterministic
    finals) and is harmless on flat landscapes, but on coreless rising landscapes it trades
    size and cost for reliability (p 0.26 -> 0.42, len 3 -> 5, +46% execs) — that trade is
    reporting policy, deferred to implementation (a reliability-floor heuristic or setting).
    The gauntlet's monotone anchor alone already finds deterministic cores in 90% of runs.
    (Experiment result.)

## 2026-09-02 (continued)

26. **Clarifying decision 15: this branch itself goes to production grade.** "Extract with
    pruning" had been read as "the scaffolding is throwaway; a fresh implementation gets
    extracted". Correct reading (DRM): the branch is brought to production quality in place —
    experiments and messy history included — and *that* production-grade branch is the
    artefact later extraction/pruning/history-rewriting works from. Consequence: the nd_*
    scaffolding is the seed of the real implementation, to be refactored and hardened on this
    branch, not discarded. (DRM, correcting a misreading.)

27. **ABI: ND failures report as FAILED plus a per-failure caveat accessor** (plan gate G1).
    `FAILED_NONDETERMINISTIC = 3` is retired — the caveat is per-origin information a
    run-level status can't carry; frontends that never call the accessor keep working.
    Before the header change lands: survey hegel-go/-ocaml/-typescript/-cpp for status-3
    references and decide reserved-vs-removed. (DRM accepted recommendation. Rejected:
    reusing value 3 with changed semantics; appending a v2 status.)

28. **Boost ships as a reliability-floor heuristic** (plan gate G2): boost runs only when
    the confirmed incumbent's LCB is below the floor (0.5), accepts only holdout-passing
    improvements; no public setting until demanded. (DRM accepted recommendation. Rejected:
    default-off behind a setting; always-on, foreclosed by decision 25.)

29. **Data tree stays disabled under ND on this branch** (plan gate G3); kind-set tolerance
    (positions that ever flipped kind become unexpandable) is the follow-up if generation
    cost shows up. Experiment 007 measures the cost on workload #1. (DRM accepted
    recommendation. Rejected for now: per-(kind,value) child edges.)

30. **Strictness surface confirmed** (plan gate G4): `nondeterminism_strictness = quiet |
    warn | error`, default quiet; `error` reproduces today's abort diagnostics verbatim;
    `warn` prints once per run. (DRM accepted recommendation.)

## 2026-09-03

31. **Decision 14 closed: no per-position or per-stream anchoring.** Experiment 007's full
    campaign: whole-timeline pool replay reproduces concurrent-stateful and clone-flaky
    failures at ceiling (20/20 discovery and DB reuse, 60/60 blob replays, each workload)
    with positional splices as the rescue tier, and splices structurally cannot tear clone
    records (a clone stream is one timeline element). Reopen only if a real workload shows
    pool + splices missing at meaningful rates. (Experiment result.)

32. **Clone serialization stays values-only.** The round-trip drops realized kinds; the only
    consumer is `resolve_choice`'s is-simplest check, which fires solely on constraint drift,
    so verbatim replay is unaffected — measured cross-run in 007 (replays re-raise the stored
    shrunk value exactly). Cost: a stale stored clone value puns to `unit()` rather than
    `simplest()`. (Experiment result; closes the phase-4 open item.)

33. **`reproduce_failure` replays a nondeterministic blob until a replay fails.**
    `hegel_run_start_blob` runs the blob through the same replay primitive as database reuse
    (pool first-fit, then splices; no fresh tier — a fresh case could fail for an unrelated
    reason). Measured need: single-shot incumbent replay reproduced a racy machine's shrunk
    failure 4/30. Completes decision 25's one-replay-primitive rule for the blob path;
    `hegel_test_case_from_blob` stays for embedders as a documented single attempt.
    (Experiment 007 result.)

34. **Remediation gates G5-G19 resolved as recommended.** The as-built review
    (`research/critique-asbuilt.md`) and its fix plan (`remediation-plan.md`) raised
    fifteen gates; DRM accepted every recommendation without detailed review. Outcomes: anchor estimand clarified —
    pinned-replay reproduction rate, raised only at validated-accept events (G5);
    retention gamma schedule with parameters from 008 (G6); boost floor re-derived in
    corrected units (G7); the clone-descending watermark lands before 009 validates (G8);
    G9/G10 stay data-decided by 009a; generation-phase executions stamped once the run is
    nondeterministic (G11); trusted-shrink anchor is the batch LCB, and decision 24's
    protection is reworded to verdict-exemption (G12); stored pools merge at promotion
    (G13); same-run supersession deletes, demotion for the run-start primary only (G14);
    secondary corpus capped at 50 per key (G15); `hegel_test_case_is_nondeterministic`
    renamed `hegel_test_case_should_capture`, no shim (G16); ND measurement line under
    `show_statistics` (G17); splices restored to 10 (G18); `FINAL_REPLAY_FRESH`
    documented as chosen, not derived (G19). Detailed entries land with their fixes per
    the plan. (DRM, wholesale: "resolve them all as recommended and if you run into any
    problems we can revise later".)

35. **Report assembly enforces decision 24 at the seam, with no post-final-replay bar.**
    `build_report` partitions on `needs_confirmation` (the predicate the persistence filter
    already uses) before the sort and the single-failure truncation, so blobs and
    replay-state caveats go to confirmed and trusted origins only and a leaked unconfirmed
    origin can never displace a confirmed one. Unconfirmed origins (bar rejects and
    never-replayed sightings alike: `unconfirmed()` drops its `rejections > 0` filter and
    its count payload) report caveat-only when nothing confirmed, with a dedicated wording
    for the never-replayed case. The final replay honors `reject()`'s evict signal.
    Deliberate non-fix: origins first observed by a report-time measurement run are never
    barred — confirming them can admit further origins without bound — so they report
    caveat-only and recycle via rediscovery next run, the trade decision 23 already accepted.
    (Restores decisions 3/24; critique R1.)

36. **A gauntlet accept moves state only at adoption.** `ShrinkProbe` gains a defaulted
    no-op `candidate_adopted()`, called from `Shrinker::accept_improvement` (the single
    adoption point) and forwarded by `NestedCloneProbe`. The engine probe stashes an accept
    (ledger key, lower bound, nodes) and only adoption consumes it: anchor raise (respecting
    the once-per-timeline set) plus incumbent persistence. Never-adoptable candidates reach
    the gauntlet via mutation probes with upward offsets, divergence-observing replays, and
    sort-key-larger ND realizations; before this fix a legitimate accept among them lifted
    the anchor and could price every real reduction out for the rest of the shrink, violating
    the anchor's definition as a bound on the *incumbent's* rate. Makes "all acceptance paths
    gate on the same validated-accept event" literally true: the event is gauntlet accept
    *and* adoption. (Critique S7.)

37. **Per-origin capture precedence is ranked.** Diagnostic beats draw lines beats bare;
    newest at the best rank; the panic payload travels with its capture, so the re-raised
    panic always matches the printed diagnostic. Under quiet everything is rank 0 and
    replacement stays unconditional. A dry final replay prints the freshest stamped failing
    execution — usually confirmation-time, pre-shrink values — while the blob carries the
    shrunk incumbent. Capturing shrunk values would need stamped gauntlet accepts, which
    decision 10 rules out. (Critique R3.)

38. **A nondeterministic flip during the shrink verify or shrink probes routes the origin
    through the bar.** A flipped verify that still fails at the origin is not taken as a
    deterministic verify: it falls into the bar arm, so an untrusted origin faces the full
    bar before boost, gauntlet, or persistence (decision 24's shrink-seam wording). A flip
    during shrink probes requeues the origin once, from its verify-validated pre-shrink
    nodes, discarding untrusted single-run progress (conservative under decision 2);
    terminates because the second pass is gauntleted and `nd_active` never clears.
    (Critique R4; decisions 20/24 at the phase boundary.)

39. **Targeting is fully off under ND handling.** `record_run` records target observations
    only while deterministic, and `Optimiser::budget_exhausted` treats `nd_active` as
    exhaustion, stopping an in-flight climb at the flip. Observations recorded before a
    mid-run flip stay in the map, unused. (Completes decision 29; critique R5.)

40. **The pre-shrink secondary drain is v1-only and deterministic-only.** Under ND handling
    the whole drain is skipped: a v1 single-replay delete contradicts decision 11's budget
    derivation, and deleting v2 entries it cannot replay was the defect. Under deterministic
    handling: v1 entries replay once then delete (Hypothesis semantics), v2 entries are
    retained untouched, undecodable entries delete, and a mid-drain detection flip stops the
    remaining deletes. Rejected: replaying v2 entries in the drain — under decisions 20/24 a
    pre-shrink reproduction can change no outcome, so it is pure cost (~35 executions per
    dry entry). Their hygiene lives in the reuse phase's budgeted strikes.
    (Restores decision 11; critique P1.)

41. **Blob and entry zlib payloads decode under a 16 MiB bound.**
    `decompress_to_vec_zlib_with_limit` at both decode sites. The bound derives from
    `ND_STATE_MAX_TIMELINES` x `BUFFER_SIZE` x the serializer's per-choice sizing (~8.5 MiB
    for the largest choice-only state the decoder would accept), with headroom for
    content-carrying choices. Rejected: no limit; a tighter limit needing format knowledge
    at call sites. (Critique P3; the format's existing defensive posture, decision 8.)

42. **POOL_CAP is a total: 10 stored timelines per origin, incumbent included.**
    `pooled_timelines` builds every stored, persisted, or replayed pool (the off-by-one came
    from writing one comparison five times by hand); `trust()` and `confirm()` truncate
    incoming pools, since a decoded v2 entry can carry up to the format bound.
    `ND_STATE_MAX_TIMELINES` stays 64 as a deliberately looser decode-side sanity bound, so
    raising the pool cap later does not invalidate stored corpora; oversized entries written
    by the buggy build remain decodable and are re-capped on trust. Inside decision 22's
    measured plateau. (Critique M1.)

43. **Decision 27 closed: run status 3 is reserved, never reused.** The bindings survey is
    recorded in production-plan.md phase 6 (ts/ocaml never adopted it, go's handling is dead
    but harmless, cpp vendors the header and gets a compile-time migration signal).
    evaluation.md now points there, and the `hegel_run_status_t` rustdoc reserves value 3
    against reuse. (Critique D12.)

44. **Same-run supersession is save-then-delete, and the secondary corpus caps at 50 per
    key.** `Persister::record_bytes` writes the new incumbent before deleting the bytes it
    supersedes, so at every instant the primary key carries the most recent validated
    incumbent and Ctrl-C mid-shrink loses nothing. A superseded same-run save is deleted,
    never demoted: it never ended a run as anyone's best example, so it earned no cross-run
    staleness strike. End-of-run reconciliation deletes same-run leftovers (a
    `saved_this_run` set tells them apart) while still demoting the run-start primary entry
    (decision 11's strike one, the freshest cross-run backup), leaving one secondary
    deposit per origin per run. `SECONDARY_CORPUS_CAP = 50` per key (5x the reuse phase's
    default secondary sampling ceiling) evicts the shortlex-largest entries at
    reconciliation — a resource bound outside decision 11's two-strike hygiene, which still
    decides which entries demote and which delete. Rejected: an ND-only fork of the
    supersession rule; the status quo plus the cap alone; no cap. (G14, G15; critique
    P2a/P2b.)

45. **The verbatim watermark is flat-length-weighted with recursive clone descent.**
    `verbatim_weight` credits a tracked element its flattened length, over
    `flattened_values_len(stored)`. The first mismatch ends the walk, but a diverged clone
    pair (elongation included) first earns `1 + credit(children)` by the same rule, through
    `CloneValues` equality/indexing, so values-only and realized records weigh
    interchangeably. Identical on clone-free timelines (the 004 calibration and 005A
    operating points stand), while a diverged clone stream now earns its tracked prefix
    instead of nothing. 009a re-checks the operating points on clone streams. Rejected:
    physical fallback (inverts decision 22 where divergence is common); an epsilon floor
    (unprincipled, keeps the degeneracy muted); per-stream ledgers (reopens decision 14/31
    machinery a weighting fix doesn't need). (Critique W1; gate G8.)

46. **Decision 19 clarified: the anchor estimates the incumbent's reproduction rate under
    the engine's own pinned-replay procedure.** It rises only at validated events — bar
    accept, adopted gauntlet first-accept (once per realized timeline, the `raised` set),
    boost holdout pass — and post-accept re-measurement of the standing incumbent never
    feeds it. That is what experiments 001 and 006 measured, and it puts candidate and
    incumbent on one estimand. Rejected: decision 19's literal reading (only
    fresh-generation evidence raises the anchor), which forecloses every shipped anchor
    source and would need a new experiment series. (Gate G5.)

47. **A trusted origin runs an evidence batch at shrink time, and any failure promotes
    it.** The batch is `nd_evidence_batch` (`nd_confirm` renamed: one function, two uses)
    with the discovery bar as its stopping rule only — trusted origins are exempt from the
    bar's verdict, the honest rewording of decision 24's "the bar is not re-run", which was
    never true. A failing batch promotes with the batch's LCB as anchor, priced by the
    existing gauntlet arithmetic. A zero-fail batch folds its evidence into the trusted
    counts (`record_trusted_batch`), skips shrinking, and the origin is still reported and
    persisted. `Trusted` now carries evidence (fails, replays, and report-time counts),
    seeded by `trust()` from the reproducing batch on the database, blob, and deterministic
    replay paths. `confirm()` folds trusted evidence at promotion and errors on a Confirmed
    prior (the arm was dead: every caller sits behind a `needs_confirmation` or
    `take_witness` check). Caveats quote report-time counts apart from confirmation or
    reuse counts, with trusted-live and trusted-dry wordings. (Gate G12; critique L1,
    L3-L6.)

48. **Promotion merges the stored v2 pool into the promotion pool, fresh-first.** Fresh
    captures first, then the stored timelines, deduplicated and capped at `POOL_CAP`
    counting the incumbent (`pooled_timelines`, absorbing decision 42's truncation).
    Rejected: documenting the drop as intended — the trusted pool is the previous run's
    validated replay state, and the promotion path was forgetting exactly what had just
    reproduced the failure. (Gate G13; critique L2.)

49. **Generation-phase executions are stamped for capture once ND handling is active.**
    A new `capture_discoveries` flag is set around the generation loop; the stamp condition
    becomes `capture_replays || (nd_active && capture_discoveries && !measurement)`.
    Decision 3's report for a never-reproduced failure now carries the discovering case's
    draw lines and diagnostic instead of a bare caveat, at capture cost bounded by the
    generation budget in runs already paying multi-replay confirmation. Documented gaps: an
    origin first observed by a gauntlet probe (measurement runs stay unstamped outside
    `capture_replays`), and the case that itself flips the run (its stamp decision predates
    the flip). Shrink, gauntlet, and boost probes stay unstamped, so decision 10's cost
    profile is untouched. (Gate G11; critique R2.)

50. **`hegel_test_case_is_nondeterministic` is renamed `hegel_test_case_should_capture`,
    with no shim.** The stamp means "capture this case" — it fires on deterministic final
    replays, blob replays, and now generation cases — and the old name said something
    false. This release already forces every binding to rewrite its capture logic, so the
    rename turns a silently changed contract into a compile-time signal, the reasoning
    decision 27 applied to status 3. The `DataSource` trait method and the frontend wrapper
    follow the new name. Rejected: a doc-deprecated alias export; re-documenting only.
    (Gate G16; critique D6.)

51. **`show_statistics` reports what ND handling cost.** `record_run`'s measurement path
    counts replays and their failures while `nd_active` (the deterministic final replay is
    not counted), rendered as one line after the statistics block: "nondeterministic
    handling: measurement replays N, failing M". The only sub-Debug surface revealing the
    flip and its cost. Within decision 1's letter: `show_statistics` is requested
    diagnostics, not the unsolicited notice quiet strictness forbids. (Gate G17.)

52. **`REPRODUCE_SPLICES` is 10, amending decision 25.** Experiment 006B measured the
    65-100% rescue rate at a cap of 10 splice candidates costing 1.6-6.3 replays per
    rescue. Decision 25's "(~6 replays/miss)" parenthetical recorded the cost per rescue,
    and the constant was mistranscribed from it. (Gate G18; critique M2.)

53. **`FINAL_REPLAY_FRESH = 4` is chosen, not derived.** A bounded last chance to capture
    fresh failing output after the stored state's ~29-replay budget is spent; no estimable
    fresh-hit rate exists to derive it from. Rejected: deriving it (nothing to derive
    from); pricing it in a dedicated experiment (not worth an experiment slot for a
    4-replay tail). (Gate G19; critique M3.)

54. **The gauntlet requires four failures, and anchors seed from 20-run batches.**
    Experiment 008 confirmed finding S1: a fresh ledger's single failure has Wilson LCB
    0.2065, so every threshold below that — the whole bar-seeded anchor regime up to
    p ~ 0.5 — accepted candidates on their recruiting run, losing 33% of target-regime
    bugs to p = 0.02 noise, 001's P0 arithmetic. The composed fix: `GAUNTLET_MIN_FAILS
    = 4` (short of it the verdict is Continue, never Reject; m = 3 has no floor passing
    both floor criteria), `ANCHOR_SEED_RUNS = 20` at both seeding sites — the discovery
    bar's batch extends past its accept, and a gauntlet accept tops the candidate's
    ledger up before the anchor can move (trusted promotions ride the same batch, C2 —
    no such register id, unresolved) —
    and `GAUNTLET_FLOOR = 0.05` now derived: 0.05 < LCB(4/30) = 0.0531, the min-fails
    acceptance boundary at the cap, so the floor costs zero power (S4). Neither piece
    works alone: extension without min-fails is *worse* than shipped (51% vs 67% L4b
    retention — honest anchors keep the whole shrink in the degenerate zone), and 40-run
    seeding stalls shrinking outright (LCB(40/40) = 0.912 exceeds the cap-reachable
    0.887). The recruiting run stays counted: exclusion's 7x fresh-ledger DP advantage
    does not survive ledger retention, and m = 4 dominates it on every composed measure.
    z stays 1.96 — the realized worst-case false accept (4.0e-4 per proposal, exact DP)
    sits under the ~1e-3 design target with the check-per-run stopping bias included, and
    the DP rows are now the recorded operating points, pinned by
    `gauntlet_matches_the_008_operating_points` (S5). Cost 1.00x on 003's landscapes —
    accepts against informative anchors already carry four fails; 3x in the target
    regime, where the reference loses the bug half the time. PRELIMINARY until 009a
    prices the miss-weighting column: safe under {shipped watermark, floored 0.2}, and
    w ~ 0 re-opens a noise channel min-fails cannot close (the G9/G10 escalation path).
    (Findings S1, S2, S4, S5, C2 — no such register id, unresolved; experiment 008.)

55. **Retention gamma is 1.0 at anchors of 0.8 and above.** `RETENTION_HIGH_WATER = 0.8`
    is a zero-miss detector, not a tuning dial: with 20-run seeding the only reachable
    anchor at or above it is LCB(20/20) = 0.839 (19/20 gives 0.764), so gamma = 1 fires
    exactly for incumbents indistinguishable from deterministic, converting the S6
    displacement (a 0.7-rate candidate displacing a deterministic incumbent 33% of the
    time under flat 0.8) to zero. The G6 cost letter is missed by 6 points — L1 replay
    cost rises 26%, concentrated in trials whose incumbent evidence is itself zero-miss
    at 20, where final p rises 0.58 to 0.82 — accepted as decision 2's intended behavior
    rather than overhead. hw = 0.7 adds cost without adding retention; "off" was
    rejected as accepting the 33% displacement. (Gate G6; finding S6; experiment 008.)

56. **The boost floor is 0.30 in 20-run-batch LCB units, and its holdout matches the
    seed size.** Decision 28's literal 0.5 was written for the old estimator; against
    honest 20-run anchors it over-triggers — 59% of true-0.7 incumbents, whose boost
    race buys nothing. The trigger intent ("true rate below 0.5") maps through the
    estimator to its boundary image LCB(10/20) ~= 0.30: recall 0.991 and precision
    1.000 on the G7 population, 5% trigger rate at true 0.7, and values above 0.30 only
    erode boundary robustness. `BOOST_HOLDOUT` rises to `ANCHOR_SEED_RUNS` so a
    boost-raised anchor is estimated on the same batch size as a seeded one. Boost
    entry now logs one Debug line, making skip-versus-decline observable. (Gate G7;
    finding S3; experiment 008.)

57. **G9 closed: the shipped watermark stands, and no physical gate is added.** 009a
    measured the flat-length clone-descending watermark on genuinely racy clone and
    machine bodies at p in {0.1, 0.3, 0.9}: W50 sits at 0.28-0.44 in every cell with
    zero mass at weight 0, against 0.03-0.13 means and 78-97% zero-share for the
    pre-decision-45 weighting — the old estimator was running 008's w = 0 breakage
    column in practice, and the fix lands the distribution strictly between 008's
    w = 0.2 and w = 1.0 columns, both of which preserve every 008 headline. The
    phase-12 constants therefore freeze as landed, resolving decision 54's preliminary
    marker. The escalation signal did not fire (median confirmed anchor at true
    p = 0.1: 0.106 clone, 0.130 machine, against the 0.2 line), so decision-14
    territory stays closed. Two letters are accepted as misses rather than acted on:
    the machine body's W50 cells at 0.278/0.294 sit under the gate's 0.3 no-change
    line but nowhere near the 0.1 PHYS_GATE clause, and its fluke rejection cost
    (26-27 physical, target 20) is 10/mean-weight — a body property, with the
    CONFIRM_CAP arm bounding the worst case at 37. Rejected: PHYS_GATE (guards a
    regime the measurement says is empty); tightening bar constants for the machine
    body (a cost letter, not a correctness one). (Gate G9; experiment 009a.)

58. **G10 closed: off-ceiling persistence holds decision 31's design points.** DB reuse
    and blob replay reproduce at 98-100% in every p in {0.1, 0.3} cell (thresholds were
    90%/60%), so whole-timeline pools plus splices stand off the ceiling 007 measured
    and no per-position anchoring is revisited. The one dip — clone blob replay 90% at
    p = 0.9 — is not a pool failure: 11.5% of those episodes never flipped into ND
    handling (at p = 0.9 the verify replay almost always reproduces), emitted v1
    exact-choice blobs, and those reproduce at 13% where every v2 blob reproduced.
    Recorded under gate G20's seam family for the phase-13 decision-3 audit: a
    failure's blob quality currently depends on whether the run noticed its own
    nondeterminism. (Gate G10; experiment 009a.)

## 2026-09-04

59. **v1 blobs replay with continuation and retries.** A v1 exact-choice blob got one
    `for_choices` replay with no continuation budget — the fragility decision 58
    recorded: never-flipped runs' blobs reproduced at 13% against v2's 100% at
    p = 0.9, on the same episodes whose DB reuse held 99% because the reuse path
    allows continuation. The Choices arm now runs up to `V1_BLOB_REPLAYS` = 4
    `for_probe` attempts under the standard continuation budget, stopping at the
    first failure, every attempt stamped. Four attempts bound the worst-case joint
    escape-then-miss (a run that passes the seam plan's k = 4 first check, the
    verify, and the final replay, then misses every blob attempt) at
    max x^6(1-x)^4 = 1.2e-3; experiment 012 measures the realized rates. Two
    neighbors deliberately keep exact semantics: the reuse path's single-miss-demote
    (decision 11's hygiene) and the pre-shrink drain — a hit on either is the
    origin's first sighting once the seam plan's step-2 check lands. Alongside, a
    defect correction: `nd_evidence_batch` cleared `capture_replays` on exit instead
    of restoring it, which would have silently unclamped stamping for any batch run
    inside the final replay's capture window. (Seam plan, phase 14; experiments 009a
    and 012.)

60. **The data tree is removed; the execution cache replaces its live roles.** Closes
    decisions 6 and 29 (gate G3): "ND handling never serves cached conclusions" survives
    verbatim, now hosted on the flat cache — the flip flushes it, and nothing is served
    or recorded while `nd_active`. Shape per G21 option (a): every executed conclusion
    keys on its serialized realized values (`serialize_choices` — floats by bits, clones
    by child values; overruns enter nothing); a digest tier (128-bit FNV) holds every
    verdict, a full serving tier (status, origin, nodes, spans) is kept outside the
    generation window, byte-bounded at 8 MiB with oldest-first eviction, and
    `cached_test_function` serves exact repeats from it. The detection trade G21 signed:
    kind drift is now checked only under `error` strictness (decision 62) and only
    within-run, in exchange for verdict flips — the same realized values concluding with
    a different status or origin — which the tree could never see; a verdict flake flips
    quiet/warn runs and aborts `error` runs with decision 30's flaky diagnostic
    verbatim. Costs measured at the seam: the passing-body execution count is unchanged
    (seed-pinned parity guard), the shrink-heavy count held 1510 against the 1661 guard
    (1.1x tree-era; 010's 85% serve win realized), and one distribution regression is
    recorded: chain-only recursive generators lose the tree's novelty forcing
    (P(depth >= 10) 0.30 -> 0.14, P(depth >= 25) 0.08 -> 0.04 at the pinned seed), a
    recursive-pricing follow-up pinned at its new floor in `test_distributions.rs`.
    (Seam plan, phase 15; experiment 010.)

61. **The duplicate stop is scoped to the all-invalid grind.** Generation ends after
    `DUPLICATE_STOP` = 10 = `RANDOM_GENERATION_BATCH` consecutive generation-window
    duplicates *only while no valid case exists*, and the exhausted-space FilterTooMuch
    swaps its `is_exhausted` conjunct for that stop (other conjuncts unchanged), so a
    tiny fully-filtered space reports instead of grinding out the invalid budget — and
    keeps doing so under health-check suppression. The counter ignores measurement runs,
    resets on novelty, and is suspended under ND handling. The valid-case scope is an
    as-built revision of G22, which would have stopped any low-novelty window: at k of
    S values seen a duplicate streak of N has probability (k/S)^N, near 1 late in coupon
    collection, so the unconditional stop ended a 32-way `one_of` before reaching every
    alternative (`test_one_of_every_arity_reaches_every_alternative` caught it). Valid
    spaces are budget-bounded already; the tree's early exit on tiny passing spaces is
    given up as worthless. (Seam plan, phase 15, revising G22; experiment 010.)

62. **Generation kind drift is detected by a within-run ledger, `error` strictness
    only; the planned reuse-comparison channel is rejected as a decision-9 violation.**
    The plan routed realized-vs-stored divergence on stored-entry replays into the flip
    plumbing; but between-run divergence is routinely staleness — every legitimate
    generator refactor would flip (or abort) the next run against its old entries,
    exactly what decision 9 forbids. What the tree actually enforced was within-run
    consistency (it lived and died with one run), so its replacement does the same: a
    ledger from rolling value-prefix hash to the choice kind drawn at the next position
    (constraints included — a `min_value` shift is a kind change), compared across
    executions within the run, aborting with the tree's diagnostic verbatim,
    entry-capped, cleared at the flip. Reuse replays feed it like any execution, so
    `test_flaky_global_state` and both reuse kind-flip pins survive on it unchanged,
    while a stale stored entry is deleted as staleness under `error` strictness without
    a word. Quiet/warn don't maintain the ledger: their generation-level flip channel
    is gone until the phase-16 first-interesting check (011 measured the tree's version
    firing 0 in 600 trials), and the three tests that relied on it now pin the interim
    honestly. (Seam plan, phase 15, revising the reuse-channel item; decision 9.)

63. **The removal's frontend fallout, resolved without engine changes.** A no-fail-fast
    sweep (fail-fast had been masking whole binaries) surfaced three casualties. (a)
    `test_lowering_together_{positive,negative}` search for the single pair satisfying
    `a + gap == b` at `gap = ±20` in a 21x21 space; the tree's novelty forcing covered
    that inside 500 attempts, the random stream does not — budget raised to 5000. (b)
    The nondeterministic reproduce-failure fixture flipped tree-era through generation
    kind drift, a channel quiet no longer has (decision 62); reshaped to a verdict
    flake — the same fingerprint passing once then failing — the channel phase 15 gives
    quiet. The displaced behaviour is permanent, not an interim: generation drift whose
    failure reproduces from its choices reports plainly at quiet/warn (no caveat, v1
    blob) even after phase 16, whose check replays the interesting choices and never
    sees pre-discovery drift. (c) `test_bytes_increment_shortens_sequence` pinned a
    shrink the shrinker cannot guarantee: reaching the 20-byte/empty-dict minimum from
    a 19-byte/one-entry start needs an equal-length proposal with a larger sort key
    (grow the bytes node, delete the entry), and `consider` rejects those before
    executing — the increment pass survives that pre-check only when zeroing a suffix
    drops `flattened_len` (clones). Both eras stall on roughly 1 in 20 random starts;
    the old pin held because its seed generated a 2-choice interesting case outright.
    Re-pinned to the two reachable minima; closing the hole needs a probe-based
    increment variant (execute the bumped proposal, accept on the realized early exit),
    flagged as a shrinker follow-up. (Seam plan, phase 15.)

64. **Every generation-discovered origin passes a first-interesting check before
    anything consumes it, extending decision 21's principle to every run.** Before each
    `nd_discovery_sweep` (same call sites, pre-flip only), each unchecked origin's
    incumbent sighting at sweep time (in-batch displacement may already have replaced
    the discovery) replays `FIRST_CHECK_REPLAYS` = 4 times exactly, stopping at the
    first miss; a reproduction concludes interesting at the same origin with the
    same realized values. A miss flips the run (its own `FirstCheck` seam site) and
    seeds the origin's discovery bar with the check's evidence through a new lifecycle
    seed slot, consumed by the next `nd_evidence_batch`, so the observations are not
    paid for twice. Under `error` strictness a structural miss aborts with a
    position-naming diagnostic (kind-shaped drift on a shared prefix still aborts
    through decision 62's ledger first, with the tree's wording) and an aligned outcome
    change aborts as flaky — the verdict the cache-mismatch channel produces, kept
    consistent because a zero-choice discovery (a strategy that fails before drawing)
    never enters the cache and only the check sees it. As G26 recommended: check
    replays are stamped (amending decision 49's stamp condition) and counted on
    decision 51's statistics line via a check-window flag; reuse reproductions are
    exempt (decision 20 already replayed them); `nd_force` starts flipped and skips
    the check. Cost is +4 exact replays per deterministically-failing origin, pinned,
    with the passing-run count untouched; in exchange quiet/warn regain a
    generation-level detection channel — 011 measured the tree's version firing 0 in
    600 trials, where this one replays the recorded sighting. (Seam plan phase 16,
    gates G23/G26.)

65. **Every pre-flip interesting execution is kept per origin, and measurement runs
    neither displace nor persist.** `record_run`'s interesting arm appends each
    sighting to an unbounded per-origin history — raw sightings and accepts alike,
    deduplicated by serialized nodes — dropped when the origin confirms or the run
    ends. The hook is the arm, not `update_interesting`: after a fluke displaces the
    incumbent, genuine sightings are shortlex-larger and never displace, yet they are
    exactly what the backtrack scan needs. Bounding rejected (DRM): with the tree
    gone, whole-history retention is strictly less memory than what it replaced, and
    eviction risks the entries a late flip needs most. The same arm gains its missing
    measurement guard, with one exemption: the reuse phase's `nd_reproduce` replays
    (a `reuse_replays` flag) must keep displacing and persisting — under `error`
    strictness a v2 entry reproduces with `nd_active` still false, and that arm
    populating `interesting` is what makes `found_in_reuse` true. Pinned both ways.
    (Seam plan phase 16, gates G23/G24.)

66. **A never-confirmed origin that misses its shrink verify or final replay
    backtracks over its history to the reproduction boundary.** The scan probes the
    accept segment at geometric offsets from the newest (1, 2, 4, ...), plus the
    oldest accept and every raw sighting, one continuation-tolerant replay each, then
    binary-refines between the newest reproducing probe and the nearest newer
    non-reproducing one — capped at `BACKTRACK_SCAN_REPLAYS` = `CONFIRM_CAP` = 40,
    with a candidate-less first pass spending the remainder on a second sweep. The
    candidate faces the full discovery bar, up to `BACKTRACK_BAR_ATTEMPTS` = 3
    batches, a reject resuming the scan on the older side. A cleared bar confirms the
    origin — witness and anchor from the batch, the scan's other reproducing entries
    pooled — and the restored incumbent supersedes the barred save through a forced
    Persister write: `needs_save` is monotone on sort key and would refuse the
    shortlex-larger restore, and save-then-delete keeps decision 44's ordering.
    Mid-run, a restore requeues the origin for a gauntleted pass — superseding
    decision 38's requeue-from-pre-shrink-nodes when history exists (empty history
    keeps 38's path), its termination argument intact: the restored origin is
    confirmed, `nd_active` never clears, so the second pass is gauntleted and marks
    shrunk. At the final replay a restore re-shrinks under the gauntlet on the shrink
    deadline's remaining budget before the pooled review, and origins exactly
    replayed before a later origin's flip re-enter the queue for that review.
    Exhaustion keeps decision 35's reject/evict caveat-only fallback. Amends decision
    17's title clause: this is detection-triggered, bar-gated recovery of recorded
    state, not 17's rejected statistically-triggered rollback — it fires only on a
    flip, and nothing is restored without clearing the same bar discovery pays. Scan
    errors bias old, which decision 2 makes safe: a too-old restore re-shrinks under
    the gauntlet, a too-new one anchors low or gets bar-rejected. Scoped to
    never-confirmed origins; confirmed origins keep the pooled final-replay path.
    (Seam plan phase 16, gate G25; DRM's scan-not-newest and keep-everything
    directions.)

67. **G20 closed: the seam plan's option (d) is the deterministic-to-ND seam's
    resolution, subsuming the seam analysis's graded options and leaving two priced
    residuals.** Of `research/g20-seam-analysis.md`'s options: (1)
    displaced-incumbent history is decision 65's unbounded per-origin history plus
    decision 66's bar-gated backtrack (unbounded and scanned, not a newest-first
    ring, per DRM); (2) provisional pre-flip pool capture is the same history — the
    backtrack pools its other reproducing entries; (3)'s intent — a paid replay
    becomes evidence instead of a discarded boolean — lands at decision 64's first
    check, whose miss seeds the discovery bar's batch (the shrink-entry verify
    itself stays a boolean); (6) v1 blob continuation and retry landed in phase 14.
    Options 4, 5, and 7 stay untaken (bounded-cost trades the measured loss no
    longer justifies) and 8 stays out of scope. The two residuals: pre-flip
    single-run trust inside a checked origin's shrink, priced by 011's L1 letters
    (final-p p50 0.74, execs 1.54x against the 1.5x letter); and the never-flip
    share that passes an honest check, priced by 012 at 0/200 episodes per cell
    (baseline 23/200) with blob reproduction 200/200 at p = 0.9. 012 also measured
    the shrink gauntlet's cost lottery above the retention high-water (~1M replays
    per episode on constant p = 0.9 bodies; `012-detection-escape/notes.md`) —
    escalated as a follow-up, not part of G20's loss accounting. (Seam plan phase
    17.)

## 2026-09-07

68. **Targeting runs under ND handling as a measured race** (`optimise_targets_nd`),
    superseding decision 39's full disablement with the statistics critique's own
    prescription for noisy-score search (confidence-bound scoring, fresh-holdout
    re-estimation, race allocation): every single-run trust point becomes
    measurement. Per label: a reference timeline with a monotone reference score
    estimated only from fresh unselected batches (median of a `TARGET_ND_HOLDOUT`
    batch; a batch observing no score marks the label dead); per firing, up to
    `TARGET_ND_RACES` races of `TARGET_ND_POOL` perturbations (single-node
    power-of-two steps plus boost's prefix-cut mutants, the lever on clone streams
    the stepper lacks, plus the recorded best while its raw score exceeds the
    reference), successive-halved on mean observed score; the winner adopts only when
    a fresh holdout clears the sign test (`target_adopt`: Wilson LCB of
    strictly-beats-the-reference above 0.5, ties and unobserved runs counting
    against), and adoption re-estimates the reference on another fresh batch, raised
    only. Observations record again under `nd_active` as seed material — harmless
    once nothing treats the recorded maximum as an estimate. Race replays are
    `measure()` executions (statistics-line counted, generation-excluded, never
    recorded as observations), firing still requires an empty interesting map, and
    every replay yields to a discovery. The deterministic climber is untouched, and a
    mid-climb flip still stops it (decision 39's stop, retained). (DRM directed:
    restore targeting under ND. Experiment 013 measured the shipped climber on noisy
    scores losing to a ~1.7 sd winner's-curse maximum after ~10 runs and freezing in
    92-100% of trials.)

69. **ND targeting constants** (experiment 013): `TARGET_ND_HOLDOUT = 20`
    (= `ANCHOR_SEED_RUNS`; the 15/20-beat gate passes a true 75%-beat improvement
    62% of the time at 2.1% false adoption — 10 runs stall on tie-heavy scores, 30
    adds ~15% cost for nothing), `TARGET_ND_RACES = 4` (full progress on every
    gradient landscape at ~950 replays per run, ~410 per adopted step; 8 doubles
    cost and flat-landscape false adoption for no progress),
    `TARGET_ND_POOL = 16` (= `BOOST_POOL`). Composed false adoption on a flat
    landscape is ~4% per race against the gate's 2.1% because the reference is
    itself estimated; accepted as priced — a false adopt is a lateral move and one
    holdout batch, losing no bug and moving no anchor. Unobserved runs counting
    against the beat quota is deliberate: missingness tightens the gate (at 30%
    no-shows, false adoption fell to zero and the climb still reached 99.5).
    (Experiment result.)

70. **Concurrency is not a declaration; `error` always errors.** Creating a state
    machine with `max_concurrency > 1` no longer flips the run into ND handling, and
    the `error`-strictness concurrency exception is gone with it: a properly
    serialized concurrent machine can fail deterministically, so concurrency is
    handled like any other potential nondeterminism source — by observing verdict
    flips and replay misses — and under `error` strictness every detection aborts,
    threads or not. `FamilyCore::concurrent_machine`, `Engine.concurrent`, and the
    declaration paragraph in `hegel_new_state_machine`'s contract are removed; the
    declared-nondeterminism mechanism the old ABI carried (the
    `is_nondeterministic` stamp, later the max_concurrency declaration) was a
    plaster and none of it remains. Consequences accepted: a concurrent failure
    that reproduces exactly reports as a plain deterministic failure (measured in
    `a_worker_panic_is_reported_with_its_real_origin_and_buffered_output`), and a
    concurrent one-shot failure on a not-yet-flipped run reports caveat-only
    without the discovering case's draws, because pre-flip generation cases are
    not stamped for capture and nothing reproduces to re-capture (the old
    behavior relied on the declaration stamping every case from the start; if
    values-less unconfirmed reports bite, the fix is stamping generation
    executions unconditionally, a capture-cost trade not taken here). (DRM
    directed.)

71. **Evidence counts plain trials of the test case, not weighted trials of one
    timeline.** `Evidence` is (fails, runs): every measurement replay of a stored
    or candidate state is one Bernoulli trial of the test case under the standing
    replay procedure (`for_probe` plus continuation), whatever timeline it
    realizes, and a miss counts in full. Statistics are about the test
    case, which can have many timelines; tracking how far a replay followed one
    realized timeline patched a per-timeline estimand instead of fixing it. The
    verbatim watermark (decision 22's weighting clause, decisions 45 and 57) is
    superseded and deleted (`verbatim_weight`, `tracked_credit`,
    `watermark_dump`), and with it the weighted/physical split: `nd_reproduce`
    takes a per-timeline budget of `ceil(reuse_replay_budget()/n)` runs
    with no separate physical cap, and its fresh tier records only failures
    (a fresh generation is a rescue, not a trial of the stored state). The
    gauntlet ledger stays keyed by serialized realized choices — the realized
    run is the test case an accept would adopt — but records plain matches.
    Plain counting is exactly the setting the deriving experiments modelled
    (005A's DP is pure Bernoulli; 008's headline envelope is its w = 1.0
    column), so the bar and gauntlet operating points hold without
    recalibration. Residual: the on-engine measurements in 009a/009b/011/012
    describe the old estimator, and divergence-heavy bodies now measure at the
    lower rate a user replaying the stored state actually sees; re-measurement
    is a candidate follow-up. (DRM directed.)

72. **Multiplicity control: repeated statistical tests spend bounded per-origin
    budgets** (experiment 014; design in `research/fcr-analysis.md`). The
    per-test operating points were sound but composed without bound: the sweep
    re-barred every re-sighting (21% false confirm per q = 0.02 fluke by 200
    epochs), the gauntlet charged nothing per proposal while a body can realize
    unbounded candidate counts (33% exposure per thousand floor-threshold
    proposals, and a confirmation-sweep drive carries 2.9e-3 — seven times the
    Fast alpha decision 54's arithmetic composed), and the final replay's
    pooled review confirmed on any failure with no bar at all (49-59% per
    fluke). Control is sequential and online, not Benjamini-Hochberg: verdicts
    act immediately and irreversibly, so there is no batch of p-values to
    rank — budgets bound the per-origin error rate instead. Three mechanisms:
    `BAR_ATTEMPTS_PER_RUN = 5` bar batches per origin per run shared by the
    sweep, shrink admission, and the pooled review (composed false confirm
    2.9%, >95% target-regime power); at the cap the origin is rejected with
    evidence (0, 0) and evicted, keeping the full unconfirmed treatment, and
    the backtrack keeps a separate `BACKTRACK_BAR_ATTEMPTS = 3` budget, now
    per origin per run rather than per call (history skews toward the real
    bug's pre-flip sightings; ceiling F = 8, 4.6%).
    `GAUNTLET_ALPHA_BUDGET = 0.02` per origin per run, held on the engine so
    re-shrink probe rebuilds keep spending from it: every proposal on an
    unbound ledger is charged its exact unconditional false-accept mass (DP at
    q0 = 0.02 — fast recruit-then-drive 4.0e-4 at the floor, confirmation
    drive 2.9e-3, unreachable thresholds zero, so the 012 lottery is free);
    when a new ledger's charge is unaffordable the failure minimum escalates
    4 → `GAUNTLET_MIN_FAILS_CEILING` = 8 (tail ≤ 1e-7 per proposal). A
    ledger's minimum pins at its first charge and its bound verdict latches —
    a stopping rule never changes mid-test, and the latched accept preserves
    the nested-clone-splice guard. The pooled review's any-failure rule
    retires: a reproducing review run is a sighting handed to a standard
    evidence batch on the origin's remaining bar attempts (fluke confirm
    0.487 → 0.003; power at p = 0.1 falls 0.97 → 0.44 before the backtrack
    rescue), with the shrink deadline bounding the batch — an expired
    deadline rejects, since a cut-short batch proves nothing. Anchors keep
    z = 1.96 with no haircut: selection miscoverage concentrates at the
    accept boundary while mean anchors sit at or below truth, the
    conservative direction under decision 2, absorbed by the gamma slack.
    Amendments: decision 35's "no post-final-replay bar" headline is
    superseded for pending origins (its review-discovered-origins scope
    stands); decision 18's stopping certificate gets easier to obtain at
    escalated minima, stopping sooner and missing recoverable reductions
    (recruited-accept at p = 0.1 against its realistic 0.053 threshold:
    0.57/0.33/0.16 at minima 4/5/6); decision 54's per-shrink composition
    claim is superseded by the budget bound; decision 66's backtrack budget
    becomes per-origin-per-run; decision 23's within-run recycling is capped,
    so sub-target bugs lean on cross-run recycling (p = 0.05 confirms 42%
    per run instead of near-certainly given a long run, p ≥ 0.1 loses ≤ 5
    points, and mixed bug-plus-fluke origins pay most: bug confirm
    0.951/0.719/0.450 at fluke share 0/0.5/0.75). Rejected: BH over batched
    p-values (nothing to batch), alpha-investing with accept payouts (bounds
    mFDR, not the per-origin rate, and needs its own calibration), a
    count-based doubling schedule (terminal stage still leaks unboundedly and
    it overcharges mid-anchor proposals). (DRM directed; review-refined.)

## 2026-09-10

73. **One type for a failing test case: `Counterexample`** (`native/counterexample.rs`).
    The abstract algorithm's object — an origin's example is a pool of realized
    executions plus the evidence about it — had no type: the incumbent lived in
    `Engine.interesting` (as nodes, for the shrinker), the pool, standing, and
    evidence in `OriginLifecycle` (`nd/lifecycle.rs`, as values), the pre-flip
    history in `Engine.history`, the alpha budget in `Engine.gauntlet_spend`, the
    first-check flag in `Engine.first_checked`, and `nd_state_for` glued incumbent
    and pool together at every persist point (25 node→value conversions in
    `test_runner.rs`). All of it is now one `Counterexample` per origin in
    `Engine.origins: Counterexamples`: `incumbent: Option<Vec<ChoiceNode>>` (None
    after a bar rejection evicted it; the record keeps standing, evidence, and
    budgets so a re-sighting resumes and the caveat-only report can quote them),
    `pool` (as captured at confirm/trust, confirm-time incumbent first),
    `standing: Unconfirmed | Trusted | Confirmed { anchor, witness }`, the
    evidence counters, `history`, `seed`, `first_checked`, and the three budgets.
    The type owns the admission rules (`adopt` founds or shortlex-displaces;
    `replace` installs a validated result; `reject` evicts unconfirmed origins
    only; `confirm`/`trust` are the pool's only writers and `confirm` drops the
    history) and builds the stored form (`repro_state` → `NdReproState`), which
    stays the wire format unchanged. The execution-level representation
    (`Vec<ChoiceNode>` / `Vec<ChoiceValue>`) is untouched. Behaviour-preserving by
    intent: the engine tests are the oracle; the one semantic difference is that
    per-origin iteration (first-check and discovery sweeps, persistence) now runs in
    origin order rather than hash order. Deliberately not done here: making the
    shrinker write through the record during a shrink (it still owns
    `current_nodes` and hands the result back at the end; `record_nd_incumbent`
    takes explicit nodes for that reason), folding the Persister's `last_saved`
    into the record, and span-aware splicing. (DRM directed: "come up with a clean
    test case representation and move this branch over to it.")
