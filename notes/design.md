# Handling nondeterministic tests

Working design for extending the engine to handle tests whose behaviour varies across runs.
Agreed 2026-09-02, pre-implementation. `decisions.md` is the decision log; `research/` holds the
code maps and the adversarial review this design is grounded in; `experiments/` tracks the
experiment sequence. This branch is working state: the final implementation will be extracted
from it with pruning and history rewriting.

## Problem

Hegel inherits Hypothesis's central invariant: no nondeterminism outside the system's control.
Concurrent stateful testing (PR #378) intrinsically breaks it, and the current handling is
wholesale surrender: a sticky run-level flag disables the data tree, novel-prefix generation,
span mutation, targeting, shrinking, database persistence/reuse, and reproduce blobs, then
reports at most one failure per run from a capture-at-discovery stash. Two problems:

1. Concurrent machines are not the only nondeterminism source. Plain clone-based concurrency,
   external randomness, and timing dependence get no handling: they surface as
   `RunError::Flaky` / `RunError::NonDeterministic`, which abort the run with no failure report.
2. It gives up far too much — most importantly shrinking.

The existing implementation is a temporary bandaid, not a canonical pattern to extend.

## Goals

- Handle test cases that fail at least 10% of the times they are run.
- Detect nondeterminism rather than only accepting declarations. Declared sources (concurrent
  machines) keep working.
- Restore shrinking, multi-failure reporting, database persistence, and reproduce blobs for
  nondeterministic tests.
- Shrinking must not lower the failure probability of the reported example; raise it when
  possible, and if shrinking reaches a deterministically-failing region, stay there.
- Keep warning about environment modification (a test that mutates global state so it fails once
  and then always passes), with wording that admits we cannot reliably distinguish it from a
  very rare failure.

Non-goals:

- Antithesis. Inside Antithesis the environment is deterministic and a separate
  determinator-based path will exist.
- The unmerged parallel-tests branch; ignore until it lands.
- Controlling or enumerating thread schedules. We sample schedules; we never replay one.

## Conceptual model

Two axes needing different machinery:

- **Generation nondeterminism**: the same replayed prefix produces a different draw structure —
  the code path depends on scheduling or external state, so the test asks different questions.
  A representation problem.
- **Outcome nondeterminism**: the same realized choice sequence produces a different verdict.
  A statistics problem.

Failure probability becomes first-class. Every decision that today assumes "interesting is a
pure function of the choice sequence" — shrinker acceptance, database reuse, final replay, the
flakiness errors — becomes a decision about an estimated probability with explicit budgets.

Vocabulary:

- **Timeline**: the realized choice sequence (nodes + spans) of one execution.
- **Incumbent**: the flat failing timeline currently held as the best example for an origin —
  what the shrinker minimizes and what persists as the primary DB entry.
- **Timeline pool**: bounded per-origin set of other realized timelines, failing-first; replay
  fallback and donor material for cross-timeline passes.
- **Evidence ledger**: per-candidate `(runs, failures)` counts accumulated across proposals,
  passes, and retries within a run. Never persisted; recomputed fresh each run.
- **Class / anchor**: a confidence lower bound (Wilson/Jeffreys LCB) on the incumbent's failure
  rate, derived from the ledger. Never a raw streak: confirmation stops at the first failure, so
  an incumbent can enter shrinking with zero observed passes even at p = 0.7, and a streak-based
  "deterministic" class would paralyze shrinking.
- **Gauntlet**: the multi-failure evidence a candidate must accumulate to be accepted as the new
  incumbent.

## Design

### Mode lifecycle and strictness

A run flips deterministic -> nondeterministic (sticky, run-level) when:

- **declared**: a state machine is created with `max_concurrency > 1` (as today); or
- **detected**, from within-run evidence only: a data-tree kind mismatch at any checked site
  (reuse/probe/generation/verify), a verify status/origin flake, a confirmation-replay flip, or
  a final-replay flake (which needs a new channel — today it fires frontend-side after the
  engine future completed; either move the final replay engine-side or add a post-run path).
  Mid-shrink mismatches are currently discarded (`test_runner.rs:1222`) and need plumbing: a
  flip during shrinking converts or restarts that origin's shrink in ND mode rather than
  aborting via `ShrinkHalt::Error`.

Cross-run divergence — a stored DB entry that no longer reproduces — is **not** evidence: it
overwhelmingly means the test or generators changed. It keeps today's semantics (staleness),
softened by the retry/demotion policy below.

New setting `nondeterminism_strictness = quiet | warn | error`, default quiet. `error`
preserves today's aborts (the accidental-global-state lint); `warn` flips and prints a notice
once; `quiet` just flips. Lives in frontend `Settings`, forwarded over a new
`hegel_settings_set_*`, honored on both sides of the ABI.

No case stamping, no sacrificed first case: capture-at-confirmation (below) removes the reason
cases had to be marked nondeterministic before they start.

### Representation: timeline pool

Per origin: a flat failing incumbent plus a bounded pool of other realized timelines,
failing-first. Replay of a stored ND failure tries stored timelines in order (incumbent first);
a replay that falls off a timeline continues with fresh generation under a capped continuation
budget (which requires an entropy source and an extension budget in blob replay — today's blob
replay has neither). The pool is semantically the branch-point-with-per-value-suffixes model;
storing whole sequences instead of a merged trie avoids a new `ChoiceValue` variant,
`CloneRecord` equality extension, sort-key extension, and recursive serialization, and — the
deciding argument — the merged artifact has no stable trunk: everything engine-side operates on
the flat realized sequence of one execution, and every accepted shrink would invalidate all
folded branches.

Persistence: DB entries and blobs get a new encoding (new blob prefix byte; blob decode already
rejects unknown prefixes loudly) storing incumbent + pool. The format self-identifying as ND is
the **only** ND persistence. No flags, no rate estimates, no sample counts are ever persisted;
estimates are computed afresh each run. Consequences: every run stands alone (CI with
`Database::Disabled` gets the full detect-from-scratch experience), and there is no stale-flag
or flag-flapping problem.

Deferred (see table below): per-position divergence anchors, and any anchoring inside clone
streams. Start with whole-timeline machinery only. Suspicion to test with data: heavy
nondeterminism may demand tracking *more* alternatives, not fewer — instrument where replays
fall off stored timelines and how much prefix sharing the pool exhibits.

### Detection signals

Kind mismatch (the existing detector) sees only a minority of divergence: booleans validate
unconditionally, most integer draws contain all recorded heads, and pool draws repair rather
than reject, so structure can shift with identical kinds (e.g. a collection `reject()` firing
in one timeline only). Add a structural comparison — draw counts and span events between the
recording and the replay — as the primary detected-divergence signal, plus a verbatim watermark
(the position where replay stopped being verbatim: first pun, repair, or forced recompute) so
downstream machinery knows how far positional claims are valid.

### Confirmation and budgets

Confirming a failure in ND mode = replay up to B times, stopping at the first failure. B is
derived from the target, not fixed: B = ln(delta) / ln(1 - p) with the p = 0.1 target and
delta = 0.05..0.1 gives B in the low-to-high 20s. Expected cost is min(1/p, B), so the cap is
only paid for failures that don't reproduce. Once in-run evidence exists, budgets adapt to the
fresh estimate. If nothing reproduces, the run still fails, reporting the observed failure with
the caveated wording (below).

### Shrinking

One principle: **charge accepts, not rejects.**

- Rejects are single-run: a candidate whose first run passes is rejected with evidence 0/1
  retained in the ledger.
- Accepts pay a gauntlet: a candidate whose first run fails keeps running until its ledger LCB
  clears the acceptance threshold (accept), its UCB falls below it (reject), or a run cap hits.
  The threshold is gamma * anchor: a tolerance factor times the incumbent anchor, so acceptance
  cost scales with how reliably the incumbent fails — a few runs against a flaky incumbent,
  ~10+ against a near-deterministic one. This is where "must not lower failure probability" is
  enforced, at the only place probability can be lost. gamma is an experiment 1 output.
- The anchor is monotone: set from initial confirmation evidence, raised when validated
  accept-time evidence shows a higher LCB, never re-baselined downward (decay measured and
  rejected in experiment 1: marginal size gains, and stopping becomes incoherent against a
  falling threshold). Post-accept evidence gathered under timeline replay must not feed the
  anchor — a pinned incumbent's replay rate would price fresh-generation candidates out.
- There is no checkpoint/rollback in the shrink loop (settled by the experiment 1 follow-up:
  rollback-on-uncertainty poisons good candidates and multiplies cost on stable landscapes;
  rollback-on-proof can't separate a mispinned incumbent from its accept-time LCB within
  affordable run counts). The pinning hazard is handled at source by capture-at-confirmation,
  plus a final validation at report time that can annotate a residual mispin — a reporting
  concern, not a search concern.
- Rejected candidates are retried via pass repetition — the existing stochastic-pass budget
  mechanism (`STOCHASTIC_MAX_FAILURES`) generalized, budgets scaled by the incumbent class —
  and their ledger evidence accumulates across retries, so retries add power instead of
  starting over. Stopping is confirmed-dry (experiment 1 follow-up): after a dry sweep, one
  confirmation sweep drives every proposal's cumulative evidence to a bound decision instead
  of the single-run fast reject; stop only if it accepts nothing. Costs about the same as
  three fixed dry sweeps and halves the missed-reduction rate where misses are recoverable,
  and it terminates with a certificate.
- All three acceptance paths gate on the same validated-accept event: `consider()`,
  `update_interesting`, and `Persister::record`. Today the latter two fire on every raw
  interesting execution, so one lucky failure of a p = 0.02 candidate displaces the good
  primary DB entry mid-pass. The Persister buffers during shrinking and commits validated
  incumbents; Ctrl-C keeps the last validated example.
- Accounting units: the stall guard and improvement cap count logical candidates (first run of
  a candidate); wall clock counts physical runs. Adaptive searches (`FindInteger`,
  `BinSearchDown`) either get resumable state across pass repetitions or we accept restart
  cost — measure first. `BinSearchDown` probing the simplest value first is the single worst
  teleport hazard under single-run accepts; the gauntlet is what defuses it.

Boost phase (raise p before minimizing): successive halving / bandit allocation over
mutation-generated variants (span duplication, donor splices — machinery that exists), scored
by ledger LCB, winner re-estimated on a holdout sample before it seeds the anchor, on its own
wall-clock budget. Not the Optimiser: its budget gate exits once any failure exists, failure
rate is not an observable it can score, and `is_climbable` excludes Clone nodes — the structure
of the top workload.

Cross-timeline passes: donor splicing of failing-timeline content onto candidates, at span
granularity initially (reusing `try_span_mutation` / `pass_to_descendant` /
`mutate_and_shrink`'s divergence-repair shapes). Finer, anchor-addressed grafting is deferred
with the representation question.

### Data tree in ND mode

Detection and novel-prefix generation only. Conclusions are never served: `cached_test_function`
must execute in ND mode, since serving the first recorded verdict is exactly the bias the
multi-run machinery exists to avoid; the ledger replaces the tree's dedup role. If restoring
the tree is too costly initially, disabling it stays acceptable (decided). If restored:
widened nodes need per-(kind, value) child keys, an interior-conclusion "continued past"
marker (a node concluded at depth 5 in one run and continued to depth 8 in another must not
read as exhausted), per-branch forced flags, and transactional recording so a mismatch under
quiet-flip doesn't leave the tree half-updated.

### Reporting

Unified on replay-until-failure:

- ND failures carry blobs (new format). The final replay runs the blob up to B; the first
  failing execution, rendered fresh (emit + backtrace on), is the report. `report_multiple_failures`
  works; per-origin identity stays.
- If nothing reproduces within B: caveated report from capture-at-confirmation — the engine
  marks confirmation replays capture-enabled (shrink probes stay cheap; the capture corresponds
  to the shrunk incumbent, not the unshrunk discovery). Wording quotes in-run measurements only
  ("failed 3 of 20 replays this run") and names both hypotheses, weighting environment
  modification only when non-reproduction is genuinely surprising given the in-run evidence.
- `NondetStash`, capture-at-discovery, and the one-failure-per-run limit are replaced.
  Whether `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` keeps its value with new semantics or ND
  failures report as `FAILED`-with-blobs is an implementation-time ABI decision; either way it
  is a coordinated ABI-semantics break for bindings (hegel.h documents the stamp contract and a
  C-ABI test pins it) and needs changelog callouts in both crates.

### Database

- Reuse replays an ND-format entry up to B with early exit.
- Hygiene without persisted counters, expressed in the existing corpora: a primary entry that
  doesn't reproduce within budget is demoted to secondary; a secondary entry that misses again
  is deleted. Two strikes across two runs.
- `replay_aligned` (skip shrinking on exact replay) essentially never holds under ND; accept
  re-shrinking for now and revisit if local-run cost bites.

### Accounting

`test_function` gets a replay/measurement flag excluding confirmation replays, gauntlet runs,
checkpoint validations, and boost measurements from: `valid_test_cases` and the invalid budget,
health-check counters, event statistics (whose changelog promise is per-generation-case
fractions), targeting records, and first/last-bug markers. Without it every quantitative
runner behavior silently changes meaning.

### ABI surface (sketch)

New: strictness setter; blob prefix byte(s) for the ND format plus a blob-kind query (or
out-param on `hegel_test_case_from_blob`); entropy + continuation budget for ND blob replay;
a capture-enabled stamp on confirmation replays (reusing or extending the
`hegel_test_case_is_nondeterministic` slot); the replay/measurement distinction if confirmation
is ever frontend-driven. Changed semantics: run statuses and the stamping contract, per above.
`RunError` variants stay flattened to one string over the ABI — tolerable because only `error`
mode still aborts.

## Deferred decisions

| Decision | Default for now | Revisit when |
| --- | --- | --- |
| Per-position divergence anchors; anchoring inside clone streams | None anywhere; whole-timeline machinery only | Instrumentation shows stable fall-off points / heavy prefix sharing (experiments 3-4) |
| Merged trie (ND-node) encoding | Timeline pool | Pool shows heavy prefix sharing worth deduplicating |
| extend=0 vs continuation budget on shrink Full replays | Undecided | Experiment 3/4: deterministic-realization invariant (deficit repair, divergence-observing passes) vs retry-shaped divergence that lengthens paths |
| Strict never-lower-p vs tolerance floor (gamma) | gamma < 1 | Experiment 1's deceptive landscape quantifies the size-vs-reliability tradeoff |
| Checkpoint/rollback on top of gauntleted accepts | Dropped | Experiment 1 follow-up (mixture landscape): rollback-on-uncertainty rescues mispins but poisons good candidates (L1 missed 9% -> 52%, 2.3x cost); rollback-on-proof never fires (mispinned rate sits inside Wilson noise of the bar). Capture-at-confirmation kills the hazard at source; final validation at report time annotates the residue |
| `replay_aligned` replacement | Accept re-shrinking | Measured local-run cost |
| FAILED vs FAILED_NONDETERMINISTIC semantics | Undecided | ABI implementation, with binding-compat notes |

## Known risks (accepted, not solved)

- **Invisible divergence**: kind-compatible structural divergence that even span/draw-count
  comparison can miss in principle; whole-timeline machinery is the backstop.
- **Origin instability**: the same sequence can panic at different sites; panics from unjoined
  threads collapse to `Panic at <unknown>`. Per-origin identity stays; the collapse fix is
  deferred to structured concurrency support.
- **Shrink wall clock**: multi-run accounting makes `MAX_SHRINKING_SECONDS = 300` the binding
  constraint for slow concurrent bodies. Measure; possibly a budget setting later.
- **Caveat fatigue**: at p near the 0.1 target, non-reproduction within B is common enough that
  over-eager environment-modification warnings would train users to ignore them; hence
  evidence-weighted wording.
- **Bindings**: the ABI-semantics changes need a coordinated rollout and loud changelogs.

## Experiment sequence

Detail and status in `experiments/000-plan.md`.

1. **Simulation harness** (`experiments/shrink-sim/`): the gauntlet/ledger/pass-repetition
   loop against synthetic failure landscapes; calibrates gamma, budgets, stopping rules;
   settles pass-repetition vs per-candidate-N cost and whether checkpointing earns its keep.
   Becomes the regression suite for the real statistics.
2. **Cache seam**: resampling seam at `cached_test_function`; measure a fixate iteration with
   dedup off on a real ~50-node target.
3. **Flat-timeline shrink in-engine** on synthetic flaky tests; measure final true p, size, cost.
4. **Replay semantics**: pool fallback + continuation budgets in `resolve_choice`; structural
   divergence detector; fall-off instrumentation.
5. **Lifecycle**: confirmation, capture-at-confirmation, persistence gating, unified reporting,
   blob v2, strictness setting.
6. **Cross-timeline grafting and the boost phase.**
