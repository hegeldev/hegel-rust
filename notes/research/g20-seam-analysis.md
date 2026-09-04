# The deterministic-to-ND seam (G20): mechanism analysis

Written 2026-09-04 as the decision input for gate G20, answering DRM's question: what
actually drives the seam — is it just the data tree? Shouldn't a nearly-deterministic
test behave very close to a deterministic one? Sources: an enumeration of every
`nd_active`/`nd_handling` branch site in the engine, the 008 in-engine spot check, 009a's
detection-escape audit, 009b, and the constants in `nd/mod.rs`. File references are
`hegel-c/src/native/test_runner.rs` unless noted; line numbers as of 8d4ca4f8.

## Answer

Not the data tree. The tree ranks behind three other mechanisms, and on the seam
workloads it is net protective: it supplies one of the run's two behavioral detectors
(the kind-mismatch signal), 002 already audited its serving seam ("every other
replay-shaped path already executes unconditionally"), and no measured G20 loss is
attributed to it. The seam is driven by the deterministic branches' decision rules. Each
treats one observation as proof and takes an irreversible action on it — DB overwrite, DB
delete, origin eviction, budget exhaustion — and the ND guards exist only behind
`nd_active`, a boolean set by sparse detection events. The cliff is single-run trust, not
caching.

## The three loss mechanisms, ranked

1. **n = 1 displace-and-persist** (`record_run`'s deterministic branch, :1951-1953;
   `update_interesting` :1160-1175; the Persister save-then-delete :1268-1276). Pre-flip,
   every interesting run displaces an occupied origin on shortlex alone and immediately
   rewrites the database, deleting the superseded bytes. Decision 20's guard engages only
   at the flip; the mid-shrink restore (:805-810) rolls back to a start-of-shrink value
   that already embeds the generation-phase walk. This is the whole L1 loss (final-p
   median 0.34 vs the 0.82 envelope; p10 0.26 is the minimal-bug floor, p90 0.82 the
   envelope — trials that flipped early keep everything). The anchor then seeds from the
   degraded incumbent and monotonically ratifies the loss.
2. **A late flip meets spent budgets** (pre-shrink verify :694-715; bar reject evicts
   from `interesting` :744-753; caveat-only fallback :940-949). Decision 23 priced the
   bar's 45%-per-attempt target-regime power on the premise that rejects recycle through
   rediscovery (decision 21). That premise holds only while generation is alive. A flip
   at shrink-verify gets exactly one attempt with zero re-hunt budget, and part of the
   time the bar is *correctly* rejecting a p = 0.02 fluke that mechanism 1 left standing.
   Result: 49% of L4b trials caveat-only — reported, but unshrunk, unconfirmed,
   unpersisted.
3. **The never-flip corner** (v1 reuse single-replay delete :405-408; v1 blob replay
   `for_choices(..., None, None)` with no continuation :208-219). At p = 0.9, 11.5% of
   009a's clone episodes end without any detection event and persist v1 exact-choice
   state. Their blobs reproduce at 13% vs v2's 100% — and 009a's construction
   (value-independent failure) proves this is *not* degraded example quality: it is
   zero-continuation alignment fragility plus single-attempt semantics. The isolation is
   the same episodes' DB-reuse column: the reuse path allows continuation (`for_probe`)
   and holds 99% on identical stored bytes.

## Why detection is sparse — and why the tree is not the story

The flip channels are: declared concurrency (:1874-1877), the tree's kind mismatch
(:1878-1884, `data_tree.rs:268-276`), the pre-shrink verify miss (:712), the final-replay
miss (:1565), and stored v2 state (:348, :224). For verdict-only nondeterminism there is
no channel before the verify at all: `record_tree_full` flags only a choice-*kind* change
at a shared prefix; a re-executed path that concludes differently silently overwrites the
leaf (`data_tree.rs:307-315`), and clone contradictions deliberately disable the subtree
rather than report (`data_tree.rs:148-154`). So even with serving and novel-prefix
steering removed, generation-phase duplicates of a verdict-ND body carry zero detection
signal. Detection power vanishes as p -> 1 (both designated replays reproduce), which is
why the never-flip share converges to a constant for structurally racy bodies instead of
going away. Two genuine but narrow tree carry-acrosses exist: `is_exhausted` and the
FilterTooMuch check read pre-flip tree state post-flip (:462, :477, :592), and pre-flip
serving recycles n = 1 evidence into mechanism 1 — an amplifier, not a source.

## What continuity would cost a deterministic test

Per protection, run unconditionally with the shipped constants (deterministic origin:
every replay of stored choices fails):

- Decision 20's displacement guard, provisional pools, v2 persistence: **free** —
  bookkeeping, zero extra executions.
- Discovery-bar admission: accepts on the 4th failure — **+3 executions per origin**, or
  +19 with the 20-run anchor-seed extension (which could be deferred until a flip).
  Deterministic runs have 0-2 origins.
- Replay-until-failure budgets: **0 on live entries** (first-fit exits on the first
  failure); +28 per *stale* entry per run until hygiene evicts — the only cost that lands
  on passing runs.
- The shrink gauntlet with seeded anchors: **+19 executions per accepted candidate**
  (LCB(19/19) = 0.832 misses the 0.839 high-water threshold; n = 20 is also the top-up
  floor). At hundreds of accepts this is the real cliff — a 5-20x shrink multiplier —
  and it is why a mode split exists at all. 009b measured the flipped-run version of this
  as 4-6x measurement replays at p = 0.9.

So the statistics themselves have no cliff — the rules degrade gracefully (a
deterministic origin confirms in 4 runs, anchors at 0.839, runs gamma 1.0, and dry probes
proof-reject in 1 run since UCB(0/1) = 0.794 < 0.839). The cliffs are (a) trust: pre-flip
rules act irreversibly on n = 1, and (b) cost: post-flip every accept pays 20 physical
runs however deterministic the test really is. `nd_active` swaps roughly seven behaviors
at once with nothing in between.

## Graded-middle options (deterministic cost ~zero unless noted)

1. **Displaced-incumbent history**: a bounded per-origin ring of raw incumbents that
   `update_interesting` displaced, consulted only on flip — bar the history
   newest-to-oldest and restore what the flukes displaced. Attacks mechanism 1 directly;
   also hands mechanism 2's bar a better target. Post-flip cost only.
2. **Provisional pre-flip pool capture**: record distinct raw failing timelines per
   origin; on a late (or final-replay) flip they are splice material and v2-quality blob
   pools. Attacks mechanism 3's flipped-late half.
3. **Verify-as-bar-run-1**: the shrink-entry verify (already paid) becomes the first
   `Evidence::record` of the batch instead of a discarded boolean — the seam becomes a
   gradient at exactly the site G20 names. Free reinterpretation.
4. **Trusted admission from repeated sightings**: an origin raw-hit k >= 2 times pre-flip
   enters post-flip shrink as trusted (decision 24's existing path — bar as stopping rule,
   any failure promotes) rather than needs-confirmation. Cuts the caveat-only rate without
   re-paying discovery power. First sighting excluded (selection, not evidence).
5. **Re-hunt on a late flip** (G20 option b): re-enter generation with remaining budget
   after a shrink-verify bar rejection; the existing rejection counter bounds fluke
   loops.
6. **v1 blob continuation/retry**: replay a v1 blob with the continuation budget (and
   optionally a small retry) like the reuse path already does. Converges v1 semantics
   toward v2 and removes most of the p -> 1 artifact bimodality without touching
   detection. The cleanest single fix for mechanism 3.
7. **Provisional persistence from caveat-only runs**: marked v2 entries so the next run
   reuses instead of re-racing the seam. Not free: persisted flukes cost the stale-entry
   budget on later runs; needs a shorter provisional budget.
8. The radical version: route measurement replays through the tree and count served
   echoes as evidence *conditional on determinism*, audited by a budgeted fraction of
   physical replays. Makes the whole apparatus ~free on deterministic tests and scales
   physical cost with observed divergence — but it is a real change to `Evidence`'s
   semantics (which already tracks `physical` separately) and to what a Wilson bound
   means. Not a phase-13 move.

Options 1-3 plus 6 close most of the measured G20 loss at zero deterministic-run cost;
options 4, 5 and 7 trade small bounded costs for the caveat-only rate and the re-race.

## Instrumentation gaps

The spot check does not record flip time or the incumbent's p at flip, cannot separate
mechanism 1 from pre-flip single-run shrink accepts (both live "before ND handling
exists"; the notes name displacement, and the p10/p90 signature supports it), and does
not decompose the 49% caveat-only between correct fluke rejections and true-origin power
misses (caveat-only reports are blobless, so the incumbent is unscoreable). 009a does not
record which replay site fired the flip in the 88.5% of episodes that did flip. Any fix
work should add these columns first.
