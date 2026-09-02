# Production plan

Goal (decision 26): bring this branch to production grade in place. Experiments, notes, and
history stay; the `nd_*` scaffolding is refactored into the real implementation, not
discarded. Extraction/pruning happens later, from the production-grade branch.

Production grade means:

- Nondeterministic tests are handled by default per decisions 1-25: quiet detection-driven
  flip, confirmation-gated origins, gauntleted shrinking that never lowers failure
  probability, pool/splice/fresh replay, caveated reporting, self-identifying persistence.
- `NdExperiment` is gone as a mode entry. Behavior is driven by detection plus the public
  `nondeterminism_strictness` setting; tests force ND handling through a test-only knob.
- All CI gates green: `just check`, `just check-coverage` (100% line coverage on new code,
  no ratchet increase, no new `nocov`), `just c-test` + `c-test-abort` + `c-test-runtime`,
  `just miri`, `just check-docs`, `check-tests-minimal-versions`, `cargo package`, header
  drift, and `check-release` (RELEASE.md files present where required).
- ABI and frontend expose the new semantics; header regenerated; RELEASE.md entries written
  (version bumps, pins, and CHANGELOG.md are release automation's job, never hand-edited);
  rustdoc on every new public item; `design.md` updated to as-built.
- The bandaids the design replaces are deleted: NondetStash, the sacrificed-first-case
  trick, the flaky abort on ND tests, `reject_concurrent_machine` as sole concurrency
  handling.

Line references are as of a1d1b6d2 and will drift; anchor on the named functions.

## Known reds today

- Coverage: the `nd_*` code in `test_runner.rs` is unconditionally compiled, exercised only
  by `__bench` experiment binaries outside the workspace, and therefore measured and
  uncovered (neither of `check-coverage.py`'s llvm-cov passes enables `__bench`). The
  coverage job almost certainly fails on this branch now. Phase 1-2 tests fix it — phase 2
  explicitly includes smoke tests for the scaffolding paths whose real tests come later
  (boost driver, ND reuse) so `just check-coverage` is green from the phase 2 boundary on.
- `variant Resample never constructed` warning under default features.
- The `experiments/` crates pin engine internals via `__bench` and will stop compiling as
  refactors land. They are outside the workspace and invisible to CI; freeze them (note in
  each README that they compile against a1d1b6d2) rather than maintain them.

## Decision gates (DRM input needed)

Everything else in this plan has a settled answer or a stated recommendation; these four
change user-visible surface and need a call before their phase starts.

**G1. ABI shape for ND failures** (blocks phase 6). `FAILED_NONDETERMINISTIC = 3` today
means the concurrent-state-machine path: no blob, no shrinking, NondetStash capture. The
new semantics (decision 3): ND failures fail the run with shrunk incumbent, blob, and an
evidence-weighted caveat. Options: (a) reuse value 3 with the new semantics — no new enum
value, but existing consumers' expectations change silently; (b) append
`FAILED_NONDETERMINISTIC_V2 = 4` and retire 3; (c) collapse to `FAILED` plus a per-failure
caveat accessor (e.g. `hegel_failure_caveat`), since decision 8 makes ND-ness a property of
the entry, not the run. Recommendation: (c) with status 3 retired — the caveat is
per-origin information and a run-level status can't carry it; frontends that never call the
accessor keep working. Before choosing (c): survey the binding repos (hegel-go, -ocaml,
-typescript, -cpp — the bump-libhegel job auto-notifies all four) for references to
status 3, and decide whether the retired value stays in the header as reserved/documented
never-returned or is removed.

**G2. Boost default policy** (blocks phase 5 reporting; machinery lands regardless).
Experiment 006: boost closes the deterministic-core tail cheaply but on coreless rising
landscapes trades size and cost for reliability (p 0.26→0.42, len 3→5, +46% execs).
Decision 25 already forecloses always-on ("defaulting off outside deterministic-core
rescue"); what's open is the shape: (a) default off, setting only; (b) reliability-floor
heuristic — boost only when the incumbent's LCB is below some floor (e.g. 0.5) and accept
only holdout-passing improvements. Recommendation: (b) — it's exactly the "rescue" case
where 006 showed pure win, and the holdout gate already rejects the bad trades.

**G3. Data tree under ND** (blocks phase 3 generation strategy). Decision 6: restore if
feasible, disabling acceptable, never serve cached conclusions. `DataTreeNode` keys
children by value with one `kind` per node, so a kind flip at a recorded position is
currently a hard mismatch; `record_tree_full` partially records before bailing. Options:
(a) stay disabled under ND (the `clone_subtree_disabled` precedent), fresh generation only
— simplest, loses novel-prefix steering exactly where duplicate generation is likeliest;
(b) per-(kind,value) child edges + compare-then-commit recording — real tree surgery;
(c) kind-set tolerance: keep the tree for prefix novelty but treat any position that has
ever flipped kind as unexpandable. Recommendation: (a) for this branch with (c) as a
follow-up; the tree is an optimization and phase 7 measures what its absence costs on the
priority workload.

**G4. Strictness surface** (blocks phase 3/6 naming only). Decision 1 fixes
`nondeterminism_strictness = quiet | warn | error`, default quiet. Confirm: `error`
reproduces today's abort diagnostics verbatim (people using it as a lint keep their
output), and `warn` prints once per run, not per flip. Also confirm the setting name.

## Phases

Each phase lands as ordinary commits on this branch, green under `just check` and
`just check-coverage` before the next starts (phase 1 may leave pre-existing reds; phase 2
closes them).

### Phase 1: statistics module

Extract the decision arithmetic into `hegel-c/src/native/nd/` (or `test_runner`-adjacent
module; naming free) with no engine dependencies: `wilson_bound`, the discovery bar
(gate 1/10 then 4/40, decision 23), the gauntlet accept/reject/continue rule
(LCB ≥ max(0.8·anchor, 0.05), cap 30, monotone anchor, decision 17/19), successive-halving
boost schedule + holdout rule (006), and the replay budget arithmetic from decision 11.
Replace the ledger's `HashMap<Vec<u8>, (u64, u64)>` with a named `Evidence { runs, fails }`
— the bare tuple ordering is an accident waiting to happen. Evidence counters take a
weight: decision 22 says structurally diverged non-failures count less than verbatim ones
toward confirmation misses and demotion/deletion; the weighting rule lives here, fed by
phase 3's watermark. The crate is `#![no_std]` + alloc with a runtime CI build
(`c-test-runtime`, clippy `-D warnings`): float math through libm, maps through
alloc/hashbrown, no panics outside `hegel_internal_assert!`.

Everything here is pure: `fn(evidence, anchor) -> Verdict`. Tests in
`hegel-c/tests/embedded/native/nd/` against fixtures exported from the experiment
programs: the 005A exact-DP operating points (false-accept 0.6% at p=0.02, power 45% at
p=0.1, expected replay counts), gauntlet accept/reject traces from 001, a
divergence-weighting fixture from 004's kind-flip shape (falls off verbatim at 0.4 of
length yet reproduces 66%), boundary cases (zero runs, all fails, anchor at floor).

Exit: module exists, `test_runner.rs` calls it, constants live in one place, DP fixtures
pass, no behavior change (the 005/006 `__bench` experiments still reproduce their numbers
if re-run by hand).

### Phase 2: origin lifecycle state machine

Replace the five parallel maps (`nd_confirmed`, `nd_pool`, `nd_anchor`, `nd_witness`,
`nd_unconfirmed`, test_runner.rs:1114-1136) with one `BTreeMap<Origin, OriginState>` where
`OriginState` is an explicit enum: `Unconfirmed { evidence }` →
`Confirmed { anchor, witness, pool }`. One admission function is the only writer, enforcing
decisions 20 and 24: raw interesting fills vacant origins only; occupied origins change
only via validated accepts; confirmation is the sole Unconfirmed→Confirmed edge.
`nd_discovery_sweep` (test_runner.rs:1318-1364) and the mid-shrink full-bar fallback become
methods on it. All three acceptance paths gate on the same validated-accept event
(design "Shrinking"): `consider()`, `update_interesting`, and `Persister::record` — the
Persister currently commits every raw interesting execution immediately
(test_runner.rs:1453), so under ND it buffers during shrinking and commits only validated
incumbents; Ctrl-C keeps the last validated example. Grep for every direct write to
`self.interesting` under ND and route it through admission.

Tests: regression tests for the two demonstrated leaks — 003's noise-floor displacement
(raw interesting displacing a confirmed origin) and 005B's span-mutation slip (mutation run
filling a vacant origin without confirmation) — as embedded tests using the
`run_main_sync`/`with_counting_ctx` harness with deterministic flaky bodies (the
`rbool`/hidden-counter pattern from the existing `concurrent_machine` test). Persister
tests: a raw interesting mid-shrink does not overwrite the primary DB entry; interrupt
keeps the last validated example. Unit tests on the state machine itself: illegal
transitions unrepresentable or rejected. Plus coverage-driving smoke tests for the
scaffolding paths whose real tests land later: one run with `nd_boost` set (embedded tests
set private settings directly) covering the boost driver (test_runner.rs:1236-1311), one
DB-reuse-under-ND run via the `reuse_run` harness covering the ND reuse branch
(test_runner.rs:240-281). Phase 4/5 tests supersede these; they exist so the coverage gate
holds at every boundary — `check-coverage.py` has no per-module mode, it is repo-wide or
red.

Exit: five maps gone, admission single-pathed (Persister included), leak regressions
locked, `just check-coverage` green repo-wide.

### Phase 3: detection, mode entry, strictness

The largest semantic change: ND handling becomes detection-driven instead of
settings-gated.

- **Signals.** Two detectors, sharing the same plumbing: (1) kind-punning in
  `resolve_choice` (today it puns silently) and (2) the structural comparison the design
  names as the primary signal — draw counts and span events diverging between recording
  and replay, since kind mismatch alone sees only a minority of divergence — plus a
  verbatim watermark (how far the replay tracked the recording) so positional claims and
  evidence weighting (phase 1) know their validity range. Recorded as position-tagged
  events on `NativeTestCase` parallel to `span_events: Vec<(usize, SpanEvent)>`
  (state.rs:1594), drained via the existing `NativeDataSource::take_*` pattern into
  `RunResult`, and through `RealizedStream` for clone streams. Within-run evidence only
  (decision 9): replay-vs-recorded divergence on a DB entry is staleness, not ND.
- **Flip triggers.** Any of (design "Mode lifecycle"): a divergence event from the
  detectors above; a verify status/origin flake (today `RunError::Flaky` at
  test_runner.rs:579-583); a confirmation-replay flip; a final-replay flake (channel
  decided in phase 6 — today it fires frontend-side after the engine future completes);
  the existing concurrent-machine flip (test_runner.rs:1395-1399). A flip during shrinking
  converts that origin's shrink to ND handling rather than aborting — the shrink-probe
  path currently discards the mismatch signal (test_runner.rs:1561). Quiet by default.
  Unify with `Engine::nondeterministic` — one flag, set by detection, never cleared within
  a run, its ~12 existing gate sites reviewed one by one: each either subsumed by the new
  machinery or explicitly kept (e.g. targeting).
- **Interim scope.** The flip enables ND handling (confirmation-gated admission,
  replay-based verify, gauntleted shrinking) for non-clone runs. Concurrent-machine flips
  set the same flag, but those failures keep today's FAILED_NONDETERMINISTIC/NondetStash
  reporting until phase 6 and stay unshrunk until phase 7 removes the shrink gate
  (test_runner.rs:509).
- **Setting.** `nondeterminism_strictness` through the whole stack: engine `Settings`
  field → u32-validated C setter on the verbosity model (lib.rs:1174-1199) → cbindgen
  `[export]` include + `just c-header` → `ffi.rs` wrapper → frontend `Settings` +
  builder method (runner.rs:129-146, re-export at src/lib.rs:695) → works in
  `#[hegel::test(nondeterminism_strictness = ...)]` with no macro change. `error` = today's
  Flaky/NonDeterministic aborts; `warn` = one-line notice; `quiet` = nothing.
- **Mode entry cleanup.** Delete `NdExperiment` (settings.rs:147-159): `Resample` dies
  outright (its experiments concluded); `Gauntlet` behavior becomes the ND path. The
  `__bench` functions naming the enum die with it — `nd_shrink_experiment`,
  `nd_lifecycle_experiment`, `nd_boost_experiment`, `nd_shrink_settings_experiment`,
  `NdShrinkMode` (hegel-c/src/lib.rs:209/278/287) — `just check` compiles
  `--all-features`, so they cannot outlive the enum. Keep a `pub(crate)` test-only force
  flag (`nd_force: bool` or similar) so embedded tests can enter ND handling
  deterministically without engineering a detectable divergence.

Tests: divergence recording unit tests (pun at position k → event at position k; a
kind-identical structural shift, e.g. a collection reject firing in one timeline only;
clones included); flip end-to-end for both axes — a body that flips kind mid-run, and an
outcome-only body (stable structure, verdict flips) that must flip via the verify-flake
trigger and complete under quiet with a correct failure; strictness at all three levels
including `error` matching today's diagnostics; ffi round-trip of the setter including
invalid u32; a frontend-level `#[hegel::test]` using the setting.

Exit: `nd_experiment` gone from `Settings` and `__bench`, detection drives the flip for
both structural and outcome nondeterminism, setting reachable from the macro, header
regenerated, header-drift test green.

### Phase 4: replay stack and persistence

- **Replay order** (decision 25): pool first-fit (cap 10) → positional splices of pool
  pairs → fresh generation, with continuation budget max(4, len/8) (004/006 numbers, via
  the phase 1 module). This replaces the experiment-only replay logic in
  `nd_confirm`/`EngineShrinkProbe` with one engine-owned "replay until failure" primitive
  used by confirmation, shrink verify, gauntlet reruns, and DB reproduction. Splices are
  whole-timeline positional for now; span-anchored grafting is a noted optimization, and
  clone-stream splice granularity is phase 7's question.
- **Run provenance.** The primitive tags each execution replay-sourced vs fresh and
  measurement vs generation. Two consumers: (1) decision 19's second clause — evidence
  gathered under timeline replay never raises the anchor (today `EngineShrinkProbe` raises
  it unconditionally, test_runner.rs:1667-1669); (2) the design's Accounting section —
  confirmation replays, gauntlet runs, and boost measurements are excluded from
  `valid_test_cases`, the invalid budget, health-check counters, event statistics,
  targeting records, and the first/last-bug markers (`record_run`,
  test_runner.rs:1429-1450, currently counts every execution), or every quantitative
  runner behavior silently changes meaning.
- **Capture at confirmation** (decision 10): confirmation replays run capture-enabled and
  feed the pool; discovery runs stay cheap. Pool admission is first-fit with the cap;
  displacement only alongside validated accepts (phase 2 owns the rule).
- **Blob v2 / DB entry v2.** New prefix byte (blob.rs: PREFIX_RAW=0, PREFIX_ZLIB=1;
  unknown → None, so old readers safely reject) carrying incumbent + pool + entropy seed +
  extension budget — the design's "storing incumbent + pool"; without the pool, cross-run
  replay degenerates to incumbent-plus-fresh and the splice tier has no pair material. The
  format is self-identifying (decision 8). `data_source_for_blob` → `for_choices` today
  has no RNG or budget, so any ND blob overruns to EarlyStop — v2 replays route through
  the replay primitive like DB reuse (`for_probe`). v1 blobs keep working for
  deterministic failures.
- **DB hygiene** (decision 11): budgets from the p ≥ 0.1 target with early exit; primary
  miss → demote to the existing secondary entry (`data_tree::sub_key(key, b"secondary")`,
  used from test_runner.rs:211/522/684 with `move_value`), secondary miss → delete. Misses
  are divergence-weighted (decision 22, via the phase 1 rule and the watermark). No
  persisted counters. Only confirmed origins persist (decision 24: reproduced DB entries
  are trusted without re-running the bar).
- **Serialization.** `serialize_choices`'s Clone encoding (tag 5, values only) loses
  realized clone kinds — round-trip puns to unit rather than simplest. Fix or explicitly
  scope to phase 7; a pool that can't round-trip clone timelines can't serve workload #1.

Tests: replay-order integration tests with bodies from 004/006 shapes (pool hit, splice
rescue, fresh fallback); blob v2 round-trip incl. pool contents, v1 compat, and
truncated/corrupt inputs; demote-then-delete incl. a weighted-miss case; provenance tests
(no anchor raise from replay evidence; confirmation/gauntlet/boost runs move none of the
runner counters, health checks, event statistics, targeting, or bug-window markers);
overrun-with-budget replay.

Exit: one replay primitive with provenance; ND failures reproduce cross-run from both DB
and blob (005B's 139/139 standard as an automated test, smaller N); hygiene and accounting
behavior locked.

### Phase 5: shrinker integration

- **Confirmed-dry stopping** (decision 18): the fixpoint loop in `fixate_shrink_passes`
  (scheduling.rs:117-187) gets a confirmation sweep after a dry sweep — every proposal
  skips the single-run fast reject (test_runner.rs:1659-1661) and drives ledger evidence to
  a bound decision; stop only if it accepts nothing. Mode plumbing: add a defaulted method
  to `ShrinkProbe` (`fn set_sweep_mode(&mut self, _: SweepMode) {}`) rather than widening
  `ShrinkRun` — the blanket `FnMut` impl and ~20 embedded test call sites stay untouched.
  `NestedCloneProbe` (clones.rs) wraps the outer probe and must forward the method
  explicitly, or the default no-op silently exempts nested clone shrinkers; test the
  forwarding.
- **Accounting** (decision 7): gauntlet reruns are physical, `calls`/stall/`MAX_SHRINKS`
  are logical. Charge one logical call per candidate regardless of gauntlet depth; the
  300s deadline (checked in `run_test_fn`) remains the physical backstop. Make the
  logical/physical split explicit in the shrinker counters instead of implicit in who
  increments what. Measure the adaptive-search restart cost under pass repetition
  (FindInteger/BinSearchDown hold no cross-invocation state; design says measure before
  deciding resumable state vs accepting the cost) and record the disposition.
- **Ledger scope**: per-origin-shrink ledger (already) so pass repetition accumulates
  evidence across retried rejects (decision 7); keyed by serialized realized nodes —
  document that punned realizations merge evidence, which is intended. Anchor updates go
  through the phase 2 admission API and respect phase 4's provenance rule.
- **Boost** (G2 policy): runs between confirmation and shrink (test_runner.rs:571-633
  today), holdout-gated, anchor/witness updates through the admission API.

Tests: confirmed-dry stopping on a synthetic landscape where fixed dry sweeps stop early
(from 001's L3 shape); never-lower-p regression at the 003 L4 standard (shrink a p≈0.99
region, assert final true p, using a hidden-counter body); stall/deadline accounting under
gauntlet depth; sweep-mode forwarding through `NestedCloneProbe`; nested clone shrink
under ND as a direct-`Shrinker` unit test with a synthetic gauntleted probe (the
end-to-end version needs the 509 gate gone and is phase 7's).

Exit: shrinker meets decision 2 ("must not lower failure probability") under test, stopping
rule is confirmed-dry, accounting documented and tested.

### Phase 6: reporting, ABI, frontend

- **Engine result surface** per G1: failures carry origin, shrunk nodes, blob (v2 when ND),
  and a caveat: confirmed / unconfirmed-rare / unconfirmed-environmental, evidence-weighted
  wording (decision 3). Caveated unconfirmed failures reported only when nothing confirmed
  (decision 24). Multi-failure reporting works under ND: distinct origins each report with
  their own caveat and blob (design: the one-failure-per-run limit is replaced).
- **Final replay.** Decide the channel the design left open: today the frontend replays
  each blob exactly once after the engine future completes (run_lifecycle.rs:616-624);
  the natural fit is engine-side via phase 4's replay primitive — up to the budget B with
  early exit, first failing execution rendered fresh (emit + backtrace on) as the report,
  a flake here feeding the phase 3 flip. `#[reproduce_failure]` and explicit blob replay
  accept v2 blobs through the same path.
- **Frontend `drive`** (run_lifecycle.rs): the FAILED branch's
  `hegel_internal_error`-on-missing-blob (612-615) and FLAKY_DIAGNOSTIC panic on
  non-reproducing replay (622-624) both misfire on ND failures — replace with the caveat
  path. The run-status match is exhaustive, so any new C variant is a compile error until
  handled; RunError string-flattening sites (lib.rs:1553, 1561) updated for the new
  diagnostics.
- **Retire NondetStash** and the sacrificed-first-case trick (decision 10);
  concurrent-machine failures flow through the ordinary ND pipeline. First-case-Invalid
  (`reject_concurrent_machine`) is phase 7's to remove.
- **Mechanics**: cbindgen include list, `just c-header`, header-drift test; write
  RELEASE.md and hegel-c/RELEASE.md with RELEASE_TYPE headers (the `check-release` PR gate
  requires them once src/ and hegel-c/src/ change; version bumps, the `=x.y.z` pin, and
  CHANGELOG.md entries are applied by release automation on merge — never by hand);
  rustdoc for the strictness setting and caveat accessor.

Tests: C-ABI tests for the new status/caveat (c-test examples, incl. abort build);
frontend integration test that an ND failure produces a failing run with caveat text and a
working repro blob; a two-origin ND run reports both failures, each with its own caveat
and blob; `#[reproduce_failure]` with a v2 blob; `error` strictness produces today's
abort; miri over the new ABI surface (extend c_abi_miri.rs).

Exit: a flaky test failing ≥10% of the time fails a frontend run with a shrunk, caveated,
blob-reproducible failure, quietly, end to end.

### Phase 7: experiment 007 — clone streams and concurrent stateful

Workload #1 (decision 12) and the machinery has never run on it: the shrink phase is gated
on `!self.nondeterministic` (test_runner.rs:509), so clone-bearing tests bypass everything
built above. This phase is part experiment, part implementation; spec and results in
`notes/experiments/007-concurrent/notes.md` like the others.

Questions: does whole-timeline pool replay reproduce concurrent-stateful failures at
useful rates, or do worker-round stream reassignments under positional splice shifts
demand per-stream (span-anchored) splicing — the decision-14 data finally arrives; does
boost need to descend into clone streams; what does G3(a)'s no-tree cost on generation
look like here.

Implementation: remove the 509 gate and first-case-Invalid; clone kind round-trip from
phase 4; splice shifts across clone-stream boundaries either made structurally safe or
constrained to within-stream.

Early smoke: right after phase 3, run the machinery on a clone-bearing flaky test behind
the test-only force flag — if something structural breaks (e.g. pool capture of realized
streams), better to learn it before phases 4-6 harden the wrong shape. The full experiment
still runs here.

Exit: concurrent stateful and plain clone-based flaky tests get confirmed, shrunk,
reproducible, caveat-correct failures; measured reproduction rates recorded in the notes;
any deferred anchoring work has a data-backed disposition (close or ticket decision 14).

### Phase 8: production bar sweep

- Delete residual experiment plumbing (the `NdExperiment`-dependent `__bench` functions
  died in phase 3): `fixate_cost_experiment` and `serve_replays` as an
  independently-mutated knob (fold into the replay primitive, hegel-c/src/lib.rs:91); any
  remaining `nd_`-prefixed naming that no longer describes the production shape.
- Freeze `experiments/` (README note per crate: built against a1d1b6d2). Add `/notes` to
  the root Cargo.toml package exclude list — `cargo package --workspace` currently ships
  every notes/*.md in the published hegeltest crate.
- Full-gate run: `just check`, `just check-coverage` (100% on all branch-added lines, no
  ratchet increase, no new nocov), `just c-test`, `just c-test-abort`,
  `just c-test-runtime`, `just miri`, `just check-docs`, `check-tests-minimal-versions`,
  `cargo package --workspace`; zero warnings under default and all-features builds.
- Docs: `design.md` rewritten as-built (it currently mixes design and experiment log);
  `decisions.md` gains entries for G1-G4 outcomes; rustdoc pass; /self-review over all
  prose added on the branch.
- Final honest evaluation against the decision log: every decision 1-26 either implemented
  (cite where) or explicitly re-opened with DRM.

Exit: the branch is the artefact decision 26 describes.

## Sequencing

1 → 2 → 3 are strictly ordered (each refactor feeds the next). 4 and 5 both depend on 2/3
and are largely independent of each other; 5's replay verify uses 4's primitive, so 4
first. 6 depends on 4/5 for what it reports. 7's smoke runs after 3; its full form after
6. 8 last. G1 (including the bindings survey) needed before 6, G2 before 5's policy
wiring, G3 before 3's generation work, G4 before 3.

Coverage discipline throughout: new code lands with its tests in the same commit-group;
`just check-coverage` at each phase boundary (the script is repo-wide — phase 2's smoke
tests exist to make that gate meaningful early), since `cargo test` at the root does not
build the hegel-c embedded tests.

## Risks

- **Clone streams late** (mitigated by the phase 7 smoke): if whole-timeline replay is
  structurally wrong for workload #1, decision 5's representation gets revisited and
  phase 4 reworked. The smoke bounds the exposure to phases 4-6 rework, not 1-3.
- **The ~12 `nondeterministic` gate sites**: each is a behavior change when unified in
  phase 3. Review individually; the concurrent-machine test corpus is thin, so phase 3
  adds tests before changing gates.
- **Coverage on error paths**: 100% line coverage on replay/persistence failure handling
  will force either testable seams (Persister fake, corrupt-blob fixtures — planned) or a
  nocov conversation with DRM. Plan assumes seams; no nocov without permission.
- **Deadline realism**: gauntlet multiplies physical runs; slow test bodies burn the 300s
  shared shrink budget faster. Phase 5 keeps the deadline as backstop but reports
  `slow_shrink_warning` accuracy under ND; if real-world shrinks routinely time out, budget
  policy becomes a follow-up decision.
