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
    fifteen gates; DRM accepted every recommendation without detailed review, to be
    revisited if implementation hits problems. Outcomes: anchor estimand clarified —
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

44. *(Reserved for the phase-10 persistence entry, landed by cherry-pick.)*

45. *(Reserved for the phase-10 watermark entry, landed by cherry-pick.)*

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
