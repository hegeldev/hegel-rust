#set page(paper: "a4", margin: (x: 2.4cm, y: 2.6cm), numbering: "1 of 1")
#set text(size: 10.5pt)
#set par(justify: true, leading: 0.62em)
#set heading(numbering: "1.1")
#show heading: set block(above: 1.4em, below: 0.7em)
#show raw: set text(size: 9.5pt)
#set table(stroke: 0.4pt)
#show table: set text(size: 9pt)

#align(center)[
  #text(size: 16pt)[The nondeterminism branch: a review brief]

  #text(size: 9.5pt)[
    Compiled by Claude from the branch's book (`notes/book/`), notes, and code,
    2026-09-07. \
    Assumes pre-branch Hegel and the decisions you directed. Assumes none of the
    experiment logs. \
    Primary sources remain authoritative: `notes/design.md`, `notes/decisions.md`,
    `notes/experiments/`.
  ]
]

= The run, before and after the flip

A run starts deterministic and stays that way until something proves otherwise.
The proof sets `Engine.nd_active`, a sticky run-level flag, and there are seven
things that can set it: declared concurrency (a state machine created with
`max_concurrency > 1`), a verdict mismatch in the execution cache, a miss at the
first-interesting check, a miss at the shrink verify, a miss at the final
replay, a version-2 database entry, and a version-2 blob. The last two flip the
run before any replay happens, because stored ND state is self-identifying
(decision 8). Everything in this brief that says "under ND handling" means
"while `nd_active` is set".

The flip itself is small. It clears the execution cache and the kind ledger
(nothing recorded pre-flip may be served or compared again), resets the
duplicate counter, and says whatever `nondeterminism_strictness` tells it to
say: `quiet` (the default) says nothing, `warn` prints one notice per run, and
`error` keeps the old aborts with byte-identical diagnostics for suites that use
determinism as a lint (decisions 1, 30). Declared concurrency is the exception
under `error`: the user asked for real threads, so the run enters handling
rather than aborting on what the threads then do.

What changes while the flag is set:

- The execution cache neither records nor serves, so every replay executes the
  body. Serving the first recorded verdict is exactly the bias the multi-run
  machinery exists to avoid.
- The duplicate stop is off and the kind ledger goes unfed.
- A raw interesting run fills a vacant origin but never displaces an occupied
  one (decision 20). Everything else about admission belongs to the origin
  lifecycle (@lifecycle).
- Targeting switches from the hill climber to a measured race (@targeting).
  Span mutation stays on unchanged.
- Reporting and persistence switch to the v2 machinery with caveats
  (@finalreplay, @persistence).

Two accounting rules keep the arithmetic meaningful. Executions the ND
machinery makes to measure reproduction (confirmation batches, gauntlet runs,
boost and targeting races, replay-until-failure) go through `measure()`, which
moves none of the counters that describe generation: valid-case counts, the
invalid budget, health checks, event statistics, recorded target observations,
bug-window markers. And the statistics block, when enabled, prints one line
counting measurement replays and their failures (decision 51), which is the
number to look at when a run feels expensive.

= Timelines and evidence <evidence>

Two different problems needed two different tools, and most of the machinery is
one of these two tools applied at some seam.

Generation nondeterminism (the same replayed prefix produces different draw
structure) is a representation problem. The branch stops pretending one choice
sequence describes the test and stores whole realized timelines: the realized
choice sequence of one execution. An origin carries a bounded pool of failing
timelines (`POOL_CAP` = 10 in total, incumbent included, from experiment 004's
measurement that 5 is near-ceiling and 10 the plateau). Replay of a timeline
uses `for_probe` with a continuation budget of `len + max(4, len/8)` total
draws, `max(4, len/8)` of them fresh past the stored timeline, so a replay
that runs slightly long draws fresh values instead of overrunning
(experiment 004: a flat budget of 4 absorbs all net elongation on plateau
bodies). When the whole pool misses, positional splices of random timeline
pairs are the rescue tier.

Outcome nondeterminism (the same realized timeline produces a different
verdict) is a statistics problem. Every replay outcome lands in an `Evidence`
ledger: failures count 1.0, and a non-failure counts a weight in [0, 1] given
by the verbatim watermark, the flat-length-weighted fraction of the stored
timeline the replay tracked before first diverging (decisions 22, 45). A
diverged run says little about the timeline it abandoned, so its miss is weak
evidence. The watermark descends into clone streams recursively, so a replay
that falls off late inside a clone stream keeps credit for the tracked prefix.
Physical run counts are tracked separately, and every cost cap is physical, so
weighting can slow a decision down but never make it cheaper than it looks.

Decisions over evidence are Wilson confidence bounds at z = 1.96 on the
estimated failure rate, with the weighted total as the denominator. The
arithmetic is sized by decision 16: handle tests that fail at least 10% of the
time they run (`TARGET_FAILURE_RATE` = 0.1). The generic budget rule is
`replay_budget(rate, tolerance)`, the number of replays after which a bug
failing at `rate` slips through with probability at most `tolerance`. At the
standing target and a 5% tolerance that is 29 replays, the database-reuse
budget. Callers stop at the first failure, so the expected cost on a live bug
is about 1/rate.

The one principle under all of it: a single failing run is selection, not
evidence (decision 21). On a noisy test the first interesting sighting is a
background fluke more often than a real bug, so nothing is believed until it
reproduces, and nothing is disbelieved on one passing replay either.

= Detection without the data tree <detection>

The data tree is gone (825 lines, deleted in the seam plan's phase 15, closing
decisions 6 and 29). Experiment 010 priced its four roles on production main:
recording alone cost 40 to 80% wall overhead on passing bodies, novel-prefix
generation and exhaustion bought nothing measurable outside tiny or filtered
spaces, and serving's one real win (6.5x on the non-stateful shrink) turned out
to be almost entirely exact repeats. Two small replacements carry the live
roles, both in `exec_cache.rs`.

The execution cache keys every executed conclusion on its serialized realized
values (floats by bit pattern, clones by child values, overruns enter nothing).
A digest tier keeps a 128-bit fingerprint per conclusion. A repeat inside the
generation window advances the consecutive-duplicate counter, and
`DUPLICATE_STOP` = 10 of them end generation, but only while no valid case
exists yet, which makes it the exhausted-space FilterTooMuch trigger and
nothing more (decision 61, after the unconditional version ended a 32-way
`one_of` early). A repeat that concludes with a different status or origin is a
verdict flip, which is outcome-nondeterminism evidence the tree structurally
could not see (it overwrote differently-concluding leaves). A full
tier serves exact repeats outside the generation window, byte-bounded with
oldest-first eviction, which recovers experiment 010's 85% shrink-serve win.

The kind ledger is `error` strictness's generation-nondeterminism detector: a
map from a rolling value-prefix hash to the choice kind drawn at the next
position, compared within one run, producing the tree's old diagnostic
verbatim. It is never fed between runs, because between-run divergence
overwhelmingly means a code change, not nondeterminism (decision 9). Quiet and
warn have no generation-level detector at all. Their detection is the verdict
channel plus the replay checks, which is the deliberate trade behind the first
check below.

Detection by verdict alone is sparse, and the seam work (phases 14 to 17)
established that the expensive failures were not detection failures but
pre-flip actions taken irreversibly on single observations. Three mechanisms
close that seam:

*The first-interesting check* (decision 64, `FIRST_CHECK_REPLAYS` = 4). Before
anything consumes a generation-discovered origin, its incumbent sighting
replays four times exactly, stopping at the first miss. A miss flips the run
and the check's observations seed the origin's discovery bar, so the bar starts
partially filled and no replay is paid for twice. All-reproduce marks the
origin checked. Detection power is 1 − (p·s)⁴ for a bug failing at p with
structural stability s. A deterministically failing origin pays exactly four
extra replays, and a run that finds no bug pays nothing, which is the whole
measurement cost of a deterministic run.

*The per-origin history* (decision 65). Pre-flip, every interesting execution
of an origin (raw sightings and shrink accepts alike) is appended to an
unbounded per-origin history, deduplicated by serialized nodes, dropped on
confirmation or run end. Pre-flip displacement can walk an incumbent far down
the landscape before any detector fires. The history keeps what displacement
would otherwise destroy. Keep-everything is deliberate: evict-oldest deletes
the reproduction boundary exactly when shrinking went nondeterministic early,
and the memory the bound would have defended against left with the tree.

*The backtrack* (decision 66, @finalreplay) consumes the history when a late
flip arrives.

= The origin lifecycle <lifecycle>

An origin is failure identity: the panic site as a `file:line:col` string
(decision 4). Under ND handling an origin is in one of three states, and
confirmation gates admission on every path (decision 24). Nothing consumes an
origin (shrinking, persistence, blob emission) until it is confirmed or
trusted.

*Unconfirmed.* A raw sighting filled a vacant origin. To confirm on the
discovery and backtrack paths, the origin must clear the discovery bar: a
gate-then-extend replay batch over the incumbent (decision 23, sized by
experiment 005A's exact DP). The one bar-less admission is the final replay's
pooled review, which confirms on any reproducing replay (@finalreplay). The
gate rejects
on zero failures in the first `GATE_RUNS` = 10 weighted misses. Otherwise
replays continue to a physical cap of `CONFIRM_CAP` = 40, accepting on the
`CONFIRM_MIN_FAILS` = 4th failure, rejecting early when the remaining budget
cannot reach four. Operating points: 0.6% false accepts per p = 0.02 fluke,
45% per-discovery power at the p = 0.1 target (rejected discoveries recycle
through rediscovery while generation is alive), roughly 15 replays per
rejected fluke.

*Confirmed.* The origin now carries a witness (the batch's first reproducing
run), an anchor, and a pool of failing timelines harvested at confirmation.
Everything downstream prices against the anchor: a Wilson lower confidence
bound on the
incumbent's reproduction rate under the engine's own pinned-replay procedure
(decision 46). It is monotone and rises only at validated events (decision 19).
Anchor-seeding batches always extend to `ANCHOR_SEED_RUNS` = 20 physical runs
past their accept, because a batch that stops at its accepting failure
estimates the stopping rule, not the rate (a four-straight-fail bar batch
would seed 0.51 whatever the truth).

*Trusted.* An origin reproduced from stored state (database or blob) skips the
bar's verdict. At shrink entry it runs an evidence batch with the bar as a
stopping rule only, and any failure promotes it to confirmed with the batch's
lower bound as anchor (decision 47), merging the stored pool fresh-first,
deduplicated, capped (decision 48). A zero-failure batch skips shrinking but
the origin is still reported.

A bar rejection evicts an unconfirmed origin on the spot, whether it came at
the discovery sweep, at shrink admission, or at the final replay, and a
never-confirmed origin that still misses everything reports caveat-only rather
than disappearing (@finalreplay). The caveat wording is honest about what the
run knows: a confirmed failure quotes its own counts ("failed k of n replays
this run"), and the environment-modification hypothesis is worded to admit it
is indistinguishable from a very rare failure (decision 3).

= Shrinking <shrinking>

The shrink passes themselves are untouched. What changed is what "the test
still fails"
means, and the rule is: charge accepts, not rejects (decision 7). A candidate
whose first run passes is rejected at a cost of one replay, with the 0/1
outcome recorded in a ledger keyed by the candidate's realized timeline so
that evidence accumulates when the
shrinker re-proposes the same timeline later. A candidate whose first run fails
is the dangerous case, because one lucky failing run used to teleport the
incumbent, so it pays the gauntlet before displacing anything.

The gauntlet accepts when the candidate's ledger holds at least
`GAUNTLET_MIN_FAILS` = 4 failures and its Wilson lower bound clears
`max(gamma × anchor, 0.05)`. It rejects when the upper bound proves the
threshold unreachable or the physical cap (`GAUNTLET_CAP` = 30) is spent.
Short of four failures the verdict is never reject, only continue. Gamma is
0.8 below the retention high-water and 1.0 at `RETENTION_HIGH_WATER` = 0.8 and
above: with 20-run seeding only zero-miss evidence reaches 0.8, so the
high-water marks incumbents indistinguishable from deterministic and refuses to
trade their reliability down. The floor is derived, not chosen: 0.05 sits just
under LCB(4/30) = 0.0531, the acceptance boundary at the cap, so it costs no
power (experiment 008). Worst-case false accept is 4.0e-4 per proposal against
a q = 0.02 fluke, inside the 1e-3 design target with the check-per-run stopping
bias absorbed, which is why z stays at 1.96.

Nearly every number in that paragraph was set or confirmed by experiment 008
(the 30-run cap and the 0.8 gamma carry over from experiment 003's rule, which
008 recalibrated around), and the composition matters more than any constant:
seeding without min-fails is worse than what it replaced (51% against 67% L4b
retention), and min-fails without 20-run seeding still fails L1's drift
protection in every cell, so neither piece works alone (decision 54). The
one composed safeguard that survived the original constants was the monotone
anchor, which is why 008 was a recalibration rather than a redesign.

An accept moves nothing by itself. State moves only at the shrinker's adoption,
through the `candidate_adopted` seam (decision 36): a gauntlet-accepted
candidate the shrinker then discards (a punned realization, a sort-key-larger
mutation probe) raises no anchor and persists nothing. On adoption the anchor
rises from the candidate's topped-up ledger, and the new incumbent is persisted
save-then-delete (@persistence), so an interrupt at any moment loses nothing.

Stopping is confirmed-dry (decision 18). After one sweep with no adopted
accept, a confirmation sweep re-proposes everything and drives each
candidate's cumulative ledger to a bound decision, with the fast reject
disabled, so
stopping carries a certificate rather than a count. Two review-era rules keep
the certificate honest: the stall guard is off during confirmation sweeps
(the certificate only holds if every candidate executes), and a bar accept
requires an in-batch reproducing replay as witness, so a seeded quota cannot
satisfy the bar by itself.

Entry into shrinking is per origin. A deterministic run does one exact verify
replay (reproduce means shrink with anchor 0 and no gauntlet, exactly the old
behaviour). A miss at that verify aborts as flaky under `error` and otherwise
flips the run, and a flipped verify is never treated as a deterministic verify
(decision 38). A flip mid-shrink restores the origin's pre-shrink nodes and
requeues one gauntleted re-pass, discarding untrusted single-run progress.

Boost runs before shrinking when a confirmed anchor sits below
`BOOST_RELIABILITY_FLOOR` = 0.30 (gate G2): successive halving over the
incumbent, its pool, and prefix-cut mutants (up to `BOOST_POOL` = 16
candidates) scored by raw in-race failure rate, with the winner re-measured on
a fresh `BOOST_HOLDOUT` = 20 holdout before it can raise the anchor, because
the in-race rate of a halving winner is selection-biased upward. The floor is
in 20-run-batch LCB units: it is the boundary image of decision 28's "true
rate below 0.5" class (LCB(10/20) ≈ 0.30). The literal 0.5 was written for the
old estimator and over-triggered on 59% of true-0.7 incumbents (decision 56).
Changing an estimator quietly re-prices every threshold written against it,
which is a lesson this branch paid for twice.

= Targeting under ND handling <targeting>

Restored on 2026-09-07 (decisions 68, 69), after the review question of why it
was off at all. The honest answer was that nothing had paid for it: decision 39
had disabled it wholesale, and the statistics critique had established that the
deterministic climber applied to a noisy score is unsound in the same ways the
old shrink loop was. Experiment 013 measured that climber keeping a single-run
maximum that sits about 1.7 standard deviations above truth on normal noise,
then freezing against its own inflated bar within roughly ten runs, in 92 to
100% of trials.

The replacement is boost's design applied to user scores. Each label holds a
reference timeline and a monotone reference score, the median of a fresh
20-run batch (never the recorded per-label maximum, which is a max of noisy
draws and is demoted to seed material). Per firing of the target phase, up to
`TARGET_ND_RACES` = 4 races run. A race builds a pool of 16 perturbations of
the reference (single-node steps by power-of-two deltas, plus prefix-cut
mutants for structure the stepper cannot reach, such as clone streams),
successive-halves it on mean observed score, and puts the winner to a sign
test on a fresh 20-run holdout: adopt only if the Wilson lower bound of
"strictly beats the reference" clears 0.5, which at 20 runs means 15 beats.
Ties and unobserved runs count against. Adoption re-estimates the reference on
yet another fresh batch and only ever raises it.

Operating points from 013: the gate passes a true 75%-beat improvement 62% of
the time per race at a 2.1% false-adopt rate, the race reaches the landscape
maximum everywhere it can move for about 950 replays per run, and a false
adopt costs a lateral move and one holdout batch (no bug is lost and no anchor
moves). Race replays are measurement executions, and everything yields to a
discovery: once any origin exists, the run's replay budget belongs to
confirmation and shrinking.

= The final replay, backtracking, and reporting <finalreplay>

The engine owns the final replay. Every failure about to be reported
re-executes first, and there are two regimes.

A deterministic origin gets one exact replay. A miss does not silence the
report the way the old flakiness abort did: it flips the run (or aborts under
`error`) and the origin re-enters through the ND path.

An ND origin gets replay-until-failure (`nd_reproduce`): the pool first-fit
under a weighted per-timeline budget with a physical cap at twice that, then
`REPRODUCE_SPLICES` = 10 positional splices of random pool pairs, then
`FINAL_REPLAY_FRESH` = 4 fresh generations. The splice cap is experiment 006's
measured operating range (65 to 100% rescue rates), corrected from a shipped
transcription error (decision 52). The fresh tail is chosen, not derived
(decision 53). A confirmed origin that stays dry after all of that is still
reported, with its caveat switched to say so: decision 3's contract is that an
unreproduced failure still fails the run, and decision 24 makes confirmation,
not the final replay's luck, the gate.

A never-confirmed origin that misses its shrink verify or final replay is
where the history pays off (decision 66). The backtrack scans the origin's
history for the reproduction boundary, capped at `BACKTRACK_SCAN_REPLAYS` = 40,
probing at geometric offsets with binary refinement, biased old under
uncertainty (a linear newest-first walk would spend the budget on the degraded
tail and re-ratify the loss, and decision 2 makes the old bias safe). The
settled
candidate faces the full discovery bar with up to `BACKTRACK_BAR_ATTEMPTS` = 3
batches, composing to roughly 83% power in the target regime. A cleared bar
confirms the origin, force-persists the restored incumbent, and re-enters
shrinking under the gauntlet.

Report assembly enforces the lifecycle at the seam. `build_report` partitions
on confirmation before sorting and truncation, so an unconfirmed origin never
reaches the blob path. Capture replacement is rank-gated (a diagnostic beats
draw lines beats a bare record), so an unstamped probe cannot clobber a good
capture. Generation-phase executions are stamped for capture once ND handling
is active (decision 49, extended to first-check replays), which is what makes
the never-reproduced report carry the discovering case's draw lines instead of
printing a values-less block. The reported failure is plain `FAILED` with a
per-failure caveat quoting the run's own counts, and the reproducer line
carries a v2 blob.

= Persistence and reproduction <persistence>

Persistence stores the representation, never estimates (decision 8). A v2
database entry or blob is `NdReproState`: the pooled timelines plus replay
parameters, self-identifying, so the next run flips before any replay and
every run stands alone. The ND state encoding, `ND_STATE_MAGIC` (four 0xFF
bytes, an impossible choice count) then a version byte then the timelines, is
the v2 database entry verbatim and the payload behind the blob's ND prefixes
(a blob wraps its payload in a raw-or-zlib prefix byte and base64). Decode is
hardened: a 64-timeline cap deliberately looser than the write side's
`POOL_CAP`, and a 16 MiB decompression bound sized with headroom over the
largest state the decoder would accept.

Reuse of a v2 entry replays the stored pool until a failure under the standing
budget (29 replays at the p = 0.1 target with 5% tolerance), and a
reproduction trusts the origin (@lifecycle). Hygiene is two strikes (decision
11): a primary-key miss demotes the entry to the secondary corpus, and a
secondary miss deletes it, so one unlucky run cannot destroy a live entry. The
pre-shrink secondary drain is scoped to v1 entries under deterministic
handling only, because a v1 single-replay delete under ND would be a
zero-strike deletion (decision 40).

Within a run, supersession is save-then-delete (decision 44): the new
incumbent is written before what it supersedes is removed, so the primary key
carries the most recent validated example at every instant and Ctrl-C
mid-shrink loses nothing. Superseded same-run saves are deleted outright
(they never ended a run as anyone's best example), and the secondary corpus is
capped at 50 per key, evicting the shortlex-largest at reconciliation.

Old v1 exact-choice blobs still replay: `V1_BLOB_REPLAYS` = 4 attempts, each
with the standard continuation budget. One bare exact replay reproduced
never-flipped runs' blobs at 13% where the reuse path held 99% on identical
bytes, so the fragility was alignment, not example quality (decision 59).

= The ABI and the frontend

The ABI break is deliberate and small (gate G1):

- Run status 3 (`FAILED_NONDETERMINISTIC`) is retired and reserved, never
  reused (decisions 27, 43). An ND failure is plain `FAILED` plus
  `hegel_failure_caveat`, one nullable string per failure.
- `hegel_test_case_is_nondeterministic` is renamed
  `hegel_test_case_should_capture` with no shim (decision 50), because the
  contract changed from "which runs are doomed" to "which executions should
  capture", and a silent semantic change under the old name would have been
  worse than a compile error. The engine stamps capture-enabled executions.
  Frontends capture when told to and keep the newest ranked capture.
- `hegel_run_start_blob` replays a blob as a run (decision 33): a v2 blob
  replays its stored pool through the same first-fit-then-splice primitive as
  database reuse until a replay fails, trusting the origin and re-reporting
  the failure with its caveat, rather than one bare exact re-execution.
- `hegel_settings_set_nondeterminism_strictness` and the statistics line's
  measurement-replay count (decision 51) round it out.

The Rust frontend's `run_lifecycle` builds reports from stamped captures, adds
the caveat as a `note:` line, and prints the reproducer with the v2 blob. The
pre-branch concurrent regime (the sacrificed first case, the single-slot
stash, at most one reported failure) is deleted: concurrent failures confirm,
shrink, persist, and report like any other ND failure (experiment 007, phase
7). Clones are values-only at the replay layer (decision 32), and splices
structurally cannot tear a clone record, because a clone stream is one
timeline element.

= What the experiments established <experiments>

You have not read these, so here is what each one established that the design
now stands on. The harnesses are frozen under `/experiments`, the write-ups
under `notes/experiments/`.

#table(
  columns: (auto, 1fr),
  align: (left, left),
  [*001*], [Pure shrink simulation. Charging accepts (not rejects) is the only
    policy that survived the noise floor: the naive P0 policy lost the bug in
    34% of noise-floor trials and per-candidate fixed-N lost it in 100%.
    Confirmed-dry stopping replaced fixed dry sweeps at equal cost and half
    the recoverable missed reductions (decision 18). Checkpoint rollback and
    anchor decay both died here (decisions 17, 19).],
  [*003*], [The same rules in the real shrinker. Flukes displaced a
    20/20-confirmed discovery in about 80% of noise-floor trials, hence
    never-displace-an-occupied-origin (decision 20) and
    the-discovery-is-selection (decision 21).],
  [*004*], [Replay semantics. Pool of 5 to 10 first-fit timelines is the
    plateau, continuation budget 4 absorbs elongation, and the trie encoding
    was rejected because prefix sharing anticorrelates with pool need
    (decision 22).],
  [*005*], [Lifecycle. 005A's exact DP priced the provisional 2-in-20 scaffold
    at a 6% false accept per fluke and derived the gate-then-extend bar
    (decision 23). 005B showed run-triggered confirmation letting span-mutation
    executions confirm flukes in 26 of 30 pure-noise runs, hence confirmation
    gates admission on every path (decision 24).],
  [*006*], [Grafting and boost. Splices rescue 65 to 100% of whole-pool misses
    (decision 25). Boost by Optimiser hill-climbing had already died in the
    statistics critique on three verified contradictions, and 006 validated
    its replacement, the holdout-gated halving race (decision 28).],
  [*007*], [Concurrency. Whole-timeline pool replay reproduces concurrent
    stateful and clone-flaky failures at ceiling (20/20 discovery and reuse,
    60/60 blob replays per workload), closing per-position anchoring
    (decision 31) and unifying the concurrent regime into the ND path.],
  [*008*], [The recalibration. The shipped gauntlet had degenerated to
    single-run accepts across the whole target regime (any anchor at or below
    0.258 accepted every candidate on its recruiting failure, losing the
    p = 0.1 bug 33% of the time). Each constant had been derived alone and the
    composition never measured. Fixed by min-fails 4 plus 20-run anchor
    seeding plus the derived floor plus the high-water gamma (decisions 54 to
    56), whose envelope (median final failure probability 0.82 on the rising
    landscape, 100% bug retention everywhere) is what design.md now quotes.],
  [*009a/b*], [Watermark validation off the ceiling. The old element-granular
    weighting put 78 to 97% of misses at exactly zero on structurally
    divergent bodies (it was running 008's known-broken w = 0 column), where
    the recursive flat-length watermark leaves no mass at zero (decisions 45,
    57). 009b re-verified the composed rules on the shipped engine: false
    accepts at or under the DP's 4.0e-4, reuse and blob reproduction at or
    above 98% for p at or below 0.3, measurement cost over the phase-11
    engine 1.6 to 2.1x there and 4 to 6x at p = 0.9. 009a also found the never-flip corner: 23 of 200 clone
    episodes at p = 0.9 never flipped and emitted v1 blobs reproducing at
    13%.],
  [*010*], [What the tree buys, on production main. Recording cost 40 to 80%
    wall on passing bodies, serving was almost all exact repeats, novel
    prefix and exhaustion were inert outside tiny spaces. Unblocked the
    removal (decisions 60 to 63).],
  [*011*], [The seam, instrumented. Decomposed the phase-12 misses: 88% of the
    target-regime caveat-only rate was bar power meeting a spent budget, L1's
    loss was entirely pre-flip displacement (incumbent already at the
    minimal-bug floor by median call 1004), and the tree's own detection
    channel had fired zero times in 600 trials. After the seam work:
    caveat-only 49% to 0 and 15% to 0, bug kept 100/100 everywhere but D2
    (99/100), two letters missed and priced (@residuals).],
  [*012*], [Detection escape, re-run on the post-seam engine. Never-flip 0/200
    in every ND cell against the 23/200 baseline, every flip at the first
    check, blob reproduction 200/200 at p = 0.9, and the deterministic
    control pays exactly 4 measurement replays per episode. Also surfaced the
    gauntlet cost lottery (@residuals).],
  [*013*], [Targeting. The deterministic climber freezes on noisy scores in 92
    to 100% of trials against a winner's-curse maximum. The race and its
    constants (@targeting).],
)

The methodological lesson the series kept re-teaching: an estimate contaminated
by the selection process that produced it will bite, and the fix is always to
move the accounting to a validated event. The gauntlet accept, the anchor's
estimand, the boost holdout, the targeting reference, and the in-batch witness
rule are all the same fix at different seams.

= Costs and residuals <residuals>

What handling costs. The composed rules cost 1.6 to 2.1x total measurement
replays at p at or below 0.3, measured against the pre-composition phase-11
engine (009b over 009a) and concentrated in fail-heavy evidence top-ups, and 4
to 6x at p = 0.9 for the ordinary path. A deterministic run pays exactly four
extra measurement replays per discovered origin (the first check) and nothing
else on that ledger.

The one known pathological regime is the gauntlet cost lottery above the
retention high-water at high p (experiment 012): the anchor ratchets to about
0.88 at p = 0.9, gamma is 1.0 there, and from that point only an all-fails
30-run batch accepts (probability about 4%), so nearly every real reduction
rejects at the cap and re-proposes. ND cells paid 240k to 3.8M measurement
replays per 100-case episode. Shrinking still reaches correct minima, but a
10 ms body would hit `MAX_SHRINKING_SECONDS` = 300 first and stall part-way.
Any fix trades against the no-probability-loss constraint, so it is escalated
to you rather than fixed (decision 67's follow-up note).

Two residuals were left deliberately, each priced (decision 67):

- Pre-flip single-run trust inside a checked origin's shrink. An origin that
  passes an honest first check still shrinks on single-run trust until
  something flips the run. Price: L1 final-p median 0.74 against the 0.82
  simulated envelope, executions 1.54x against the 1.5x acceptance letter.
- The never-flip share that passes an honest check. The check's escape rate
  (p·s)⁴ is small, not zero. What escapes now carries a v1 blob whose four
  continuation replays keep it reproducible (012: blob reproduction 200/200 at
  p = 0.9 including escapes, of which there were none in 200 episodes per
  cell).

Smaller open items, from the status chapter: the bytes-increment shrinker hole
(both eras stall on about 1 in 20 random starts, decision 63), the recursive
depth-spread regression from the tree removal (chain-only recursive generators
lose novelty forcing, decision 60), `replay_aligned` under ND accepted as
re-shrinking every run for structurally-ND bodies, experiment 010's untested
caveats (novel-prefix value on rare bugs), decision 31's reopen condition, and
hegel-cpp's compile-time migration for the retired status 3. Targeting's
composed false-adopt rate on a flat score landscape is about 4% per race,
accepted because the cost is a lateral move.

Extraction of the final implementation from this branch, the step decision 26
deliberately left outside all the plans, remains open.

#pagebreak()

= Appendix: the constants

#table(
  columns: (auto, auto, 1fr),
  align: (left, left, left),
  [*Constant*], [*Value*], [*Derivation*],
  [`TARGET_FAILURE_RATE`], [0.1], [Decision 16, the design target. Everything
    else is sized against it.],
  [`GATE_RUNS`], [10], [Discovery bar gate (005A exact DP).],
  [`CONFIRM_CAP`], [40], [Discovery bar physical cap (005A).],
  [`CONFIRM_MIN_FAILS`], [4], [Discovery bar accept (005A).],
  [`ANCHOR_SEED_RUNS`], [20], [Batches extend past their accept so anchors
    estimate the rate, not the stopping rule. 20 is what 001/003 simulated
    and the largest size whose all-fail LCB a candidate can match within the
    gauntlet cap. 40-run seeding stalls shrinking outright (008).],
  [`GAUNTLET_MIN_FAILS`], [4], [Without it, any anchor at or below 0.2065
    accepts on the recruiting failure. m4 keeps 100% of target-regime bugs at
    3.01x the shipped cost, and costs 1.00x on 003's landscapes (008).],
  [`GAUNTLET_CAP`], [30], [Physical cap per proposal (008 rows).],
  [`GAUNTLET_GAMMA`], [0.8], [Bounded-loss retention below the high-water
    (decision 2's budget).],
  [`RETENTION_HIGH_WATER`], [0.8], [Zero-miss detector: only LCB(20/20) =
    0.839 reaches it under 20-run seeding. Converts D2's 33% displacement to
    zero for 1.26x replays on L1 (008, decision 55).],
  [`GAUNTLET_FLOOR`], [0.05], [Derived: just under LCB(4/30) = 0.0531, so it
    costs no power at min-fails 4 (008).],
  [`POOL_CAP`], [10], [Timelines per origin, incumbent included. 5 is
    near-ceiling, 10 is the plateau (004).],
  [`REPRODUCE_SPLICES`], [10], [65 to 100% rescue measured at a 10-splice cap
    (006). The shipped 6 was a transcription error (decision 52).],
  [`FINAL_REPLAY_FRESH`], [4], [Chosen, not derived (decision 53).],
  [`V1_BLOB_REPLAYS`], [4], [Bounds the joint escape-then-miss at 1.2e-3
    (seam plan, decision 59).],
  [`FIRST_CHECK_REPLAYS`], [4], [Detection 1 − (p·s)⁴, cost +4 on a
    deterministically failing origin (decision 64).],
  [`BACKTRACK_SCAN_REPLAYS`], [40], [Set equal to `CONFIRM_CAP`
    (decision 66).],
  [`BACKTRACK_BAR_ATTEMPTS`], [3], [Composes to about 83% target-regime power
    (decision 66).],
  [`BOOST_RELIABILITY_FLOOR`], [0.30], [LCB(10/20), the 20-run image of "true
    rate below 0.5". Recall 0.991, precision 1.000 on the G7 population
    (008, decision 56).],
  [`BOOST_POOL`], [16], [Halving race width (006).],
  [`BOOST_HOLDOUT`], [20], [Set equal to `ANCHOR_SEED_RUNS`, so boosted
    anchors are estimated on seeded batch size (decision 56).],
  [`TARGET_ND_POOL`], [16], [Set equal to `BOOST_POOL` (decision 69).],
  [`TARGET_ND_HOLDOUT`], [20], [15/20 beats is the sign-test boundary. 62%
    power on a true 75%-beat improvement at 2.1% false adoption. 10 stalls on
    ties, 30 buys nothing (013).],
  [`TARGET_ND_RACES`], [4], [Full progress at about 950 replays per run. 8
    doubles cost and flat-landscape false adoption for nothing (013).],
  [`DUPLICATE_STOP`], [10], [Exhausted-space trigger only while no valid case
    exists (decision 61).],
  [`SECONDARY_CORPUS_CAP`], [50], [Per key, shortlex-largest evicted at
    reconciliation (decision 44).],
  [`ND_STATE_MAX_TIMELINES`], [64], [Decode-side bound, deliberately looser
    than `POOL_CAP`.],
  [`MAX_DECOMPRESSED_LEN`], [16 MiB], [Decode hardening. The final review
    found the first version rejecting the encoder's own output on large
    payloads.],
  [Reuse replay budget], [29], [`replay_budget(0.1, 0.05)`: a flat 10 misses
    a p = 0.1 bug 35% of the time (decisions 11, 16).],
  [Continuation budget], [len + max(4, len/8)], [Flat 4 absorbs all measured
    elongation, the len/8 term scales long timelines (004).],
  [z], [1.96], [Deliberate retention: realized worst-case false accept
    4.0e-4 per proposal against the 1e-3 design target (008).],
)
