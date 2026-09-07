#set page(paper: "a4", margin: (x: 2.4cm, y: 2.6cm), numbering: "1 of 1")
#set text(size: 10.5pt)
#set par(justify: true, leading: 0.62em)
#set heading(numbering: "1.1")
#show heading: set block(above: 1.4em, below: 0.7em)
#show raw: set text(size: 9.5pt)
#set table(stroke: 0.4pt)
#show table: set text(size: 9pt)

#align(center)[
  #text(size: 16pt)[One nondeterministic test, end to end]

  #text(size: 9.5pt)[
    A worked example: one test with nondeterministic generation, followed through the
    engine run by run. Companion to the review brief (`brief.pdf`), 2026-09-07. \
    The test, its timings, and each replay's outcome are stipulated for the trace. Every
    rule, constant, and number computed from them is the shipped one.
  ]
]

= The test

A job queue with a background drainer thread. The test pushes a drawn number of drawn
jobs, backing off with a drawn jitter whenever the queue is full, and asserts at the end
that the drainer saw every job:

```rust
use hegel::generators as gs;

#[hegel::test]
fn no_job_is_dropped(tc: &TestCase) {
    let queue = JobQueue::with_capacity(2); // spawns a drainer thread
    let n = tc.draw(gs::integers::<u8>().min_value(1).max_value(6));
    for _ in 0..n {
        let job = tc.draw(gs::integers::<u32>().max_value(999));
        if !queue.try_push(job) {
            // full: back off for a drawn jitter, then hand over directly
            let jitter = tc.draw(gs::integers::<u64>().min_value(1).max_value(10));
            std::thread::sleep(Duration::from_millis(jitter));
            queue.push_blocking(job);
        }
    }
    assert_eq!(queue.finish(), u32::from(n)); // joins the drainer, returns its count
}
```

Two things about this body are outside the choice sequence. Whether `try_push` finds the
queue full depends on how fast the drainer ran, so the *number and position of the jitter
draws* vary between executions of the same choices: that is generation nondeterminism, the
subject of this document. And `push_blocking` has a race with the drainer's buffer swap
that loses the handed-over job now and then, so even two executions that realize identical
choices can differ in verdict: outcome nondeterminism, along for the ride. Stipulate that
given the failing pattern below, the race loses about a third of the time.

= What the choices no longer describe

Call the realized choice sequence of one execution a *timeline*. Two executions of this
body under identical conditions can realize:

```text
T0 = [4, 611, 802, 129, 5, 774]    the queue was full at the third job: n, three jobs,
                                   a jitter of 5, the fourth job
T' = [4, 611, 802, 129, 774]       the drainer kept up: no jitter draw at all
```

Neither sequence describes the test. Replay T0 when the drainer keeps up: the body makes
five draws — the count and four jobs — and the fifth is served the stored value at that
position, the jitter 5. The last stored value, 774, is never read, and the replay realizes
`[4, 611, 802, 129, 5]`, a different test case in which 5 is a job. Value for value that
is a clean prefix of T0, and both draws are integers, so no served value betrays the
reinterpretation — only the number of draws does. Replay T' when the queue fills at the
last job: after 774 the body asks for a jitter draw the stored sequence never had, and the
replay runs off the end.

The pre-branch engine treated both outcomes as fatal. A replay that concluded differently
from what was recorded aborted the whole run as `Flaky`, with no failure report, so a bug
in this test died on the first misaligned replay. The branch's machinery exists to keep
going instead. Two of its tools appear immediately: a replay is given a *continuation
budget* of `len + max(4, len/8)` total draws (for T0, its 6 values plus up to 4 fresh
ones), so running off the end draws fresh values instead of aborting, and every claim
about the failure will be bought with repeated replays rather than read off a single one.

= Discovery

Generation runs normally. The 23rd generated case realizes T0, the race loses, and the
final assertion panics: `left: 3, right: 4`. The failure's identity is its origin, the
panic site rendered as a string — `Panic at tests/queue.rs:16:5` — and the sighting fills
that origin's vacant slot in the interesting map with T0 as its incumbent.

The run is still deterministic at this point, and the engine believes nothing yet. This
execution was noticed *because* it failed: over a long generation phase, low-probability
flukes get many chances to fire once, so first sightings systematically over-represent the
failures least likely to reproduce. A sighting is selection, not evidence. It contributes
nothing to any rate estimate.

= The first check and the flip

Before anything consumes the origin, and before any confirmation attempt, the engine
replays T0's exact choices up to four times, stopping at the first miss. An exact replay reproduces only if it concludes interesting at the same origin
with the *same realized values*, which is what makes structural divergence visible even
though every draw here is kind-compatible.

- Replay 1: the drainer stalls at the same point, the sequence realizes exactly T0, the
  race loses again. Reproduced.
- Replay 2: the drainer keeps up. The replay realizes `[4, 611, 802, 129, 5]` — the
  jitter reinterpreted as a job, the sixth stored choice never drawn. Five realized
  choices against six stored: a miss.

The miss flips the run: `Engine.nd_active` sets, sticky for the rest of the run. Under the
default `quiet` strictness nothing is printed. From here the execution cache neither
records nor serves (every replay must execute the body — serving a remembered verdict is
exactly the bias multi-run statistics exist to remove), raw interesting runs can fill
vacant origins but never displace occupied ones, and reporting and persistence switch to
the nondeterminism-aware machinery. The check's two observations are not wasted: they are
deposited as the origin's *seed*, one failure in two replays, and pre-fill its first
confirmation batch.

Had strictness been `error`, the run would have aborted here instead, with a diagnostic
naming the divergence position and reporting that six choices were recorded where the
replay realized five — this suite has opted to use determinism as a lint, and the engine
obliges at the first detection.

= Confirmation: the discovery bar

The origin must now clear the discovery bar before shrinking, persistence, or a blob will
touch it: reject if the first 10 replays hold zero failures, otherwise extend to a cap of
40, accepting on the 4th failure. The batch replays the incumbent T0
continuation-tolerantly (fresh RNG, the continuation budget above), and "failure" now
means concluding interesting at this origin *whatever the replay realized* — each replay
is one Bernoulli trial of the test case, not of one timeline. The trace:

#table(
  columns: (auto, 1fr, auto, auto),
  align: (left, left, left, left),
  [*Replay*], [*Realized*], [*Outcome*], [*Evidence*],
  [seed], [carried from the first check], [1 failure, 1 miss], [(1, 2)],
  [1], [T0 exactly — full at job 3, race lost], [fail], [(2, 3)],
  [2], [`[4, 611, 802, 129, 774]` — never full], [pass], [(2, 4)],
  [3], [T0's structure, race won, drains clean], [pass], [(2, 5)],
  [4], [`[4, 611, 802, 129, 5, 774, 3]` — full twice; the second jitter ran off the
    stored end and drew fresh (3) from the continuation budget; race lost], [fail],
    [(3, 6)],
  [5], [`[4, 611, 802, 129, 774]`], [pass], [(3, 7)],
  [6], [T0 exactly, race lost], [fail], [(4, 8) — accept],
)

The 4th failure accepts, but the batch does not stop: an accepting batch always extends to
20 physical runs, because a batch stopped at its accepting failure estimates the stopping
rule rather than the rate (four straight failures would seed 0.51 whatever the truth).
The extension adds 12 more replays, 4 of them failing. Final evidence (8, 20).

The origin is now *Confirmed*, carrying three things. The *anchor*: the Wilson lower
confidence bound on 8/20, 0.219, a validated floor under the incumbent's reproduction
rate, monotone from here on. The *witness*: batch replay 1, the first in-batch
reproduction, which will be the shrinker's starting point (the seeded failure came from
the check and cannot serve — an accept must rest on a reproduction the batch itself
holds). The *pool*: the failing replays' realized timelines, deduplicated — here T0 and
T1 = `[4, 611, 802, 129, 5, 774, 3]`. Confirmation is a validated event, so the incumbent
persists to the failure database immediately as a version-2 entry (the pooled timelines
plus replay parameters), written before anything it supersedes is deleted, so an interrupt
from here on loses nothing.

One piece of bookkeeping: this batch spent 1 of the origin's 5
per-run bar attempts. Repetition of statistical tests is budgeted, because each batch
carries a small false-accept rate and a fluke re-sighted forever would eventually recycle
its way past the bar.

= Boost declines

The anchor, 0.219, sits below the reliability floor of 0.30, so before shrinking starts
the engine tries to buy a steadier starting point. Successive halving races the incumbent,
its pool, and prefix-cut mutants against each other on raw in-race failure rate. Say T1
wins its rounds at 3 failures in 6 replays. In-race rates are selection-biased upward —
the winner of a race is partly the luckiest — so the winner faces a fresh 20-run holdout
before anything moves: T1 comes back 7 of 20, lower bound 0.181, below the anchor. Boost
declines, and the shrink starts from the witness with the anchor unchanged. On a body like
this one, whose noise is intrinsic rather than incidental to one unlucky timeline, that
refusal is the designed outcome.

= Shrinking under the gauntlet

The shrink passes are the deterministic engine's, unchanged. What changed is the meaning
of "the test still fails". A candidate that *passes* its first run is rejected at the cost
of that one replay, with the 0-of-1 outcome remembered in a ledger keyed by the
candidate's realized timeline, so later re-proposals accumulate evidence instead of
starting over. A candidate that *fails* its first run is the dangerous case — one lucky
failing run used to move the incumbent — so it must clear the gauntlet: at least 4
observed failures, and a Wilson lower bound clearing

#align(center)[`threshold = max(0.8 × anchor, 0.05) = max(0.8 × 0.219, 0.05) = 0.175`]

before it may displace the incumbent. The 0.8 permits a bounded reliability trade per
step, and the floor, 0.05, is where pricing gives way to a flat minimum. The drive is
bounded too: a candidate rejects when even its evidence's upper bound cannot reach the
threshold, or at a cap of 30 runs. Three candidates from the first sweep:

*Candidate 1, delete the last job.* The proposal lowers the count draw to 3 and cuts 774:
`[3, 611, 802, 129, 5]`. Its first run realizes exactly that — still full at the third
job — and the race loses: interesting, at the right origin. A ledger opens for that
realized timeline. Before the outcome is even recorded, the proposal is charged the exact
false-accept mass a hypothetical 2%-fluke candidate would carry at this threshold — about
1e-5 — debited from the origin's per-run alpha budget of 0.02. Then the gauntlet drives
the ledger:

#table(
  columns: (auto, auto, auto, 1fr),
  align: (left, left, left, left),
  [*Run*], [*Outcome*], [*Ledger*], [*Verdict*],
  [1 (proposal)], [fail], [(1, 1)], [continue — short of 4 failures],
  [2], [fail], [(2, 2)], [continue],
  [3], [pass], [(2, 3)], [continue],
  [4], [fail], [(3, 4)], [continue],
  [5], [fail], [(4, 5)], [LCB 0.376 ≥ 0.175 with 4 failures — accept],
)

As at the bar, the accept is not the stopping point: the ledger tops up to 20 runs,
landing at (9, 20), lower bound 0.258. And an accept still moves nothing by itself. The
shrinker adopts the candidate because its sort key is strictly smaller, and only that
adoption commits the state: the incumbent becomes `[3, 611, 802, 129, 5]`, the anchor
rises to 0.258 (the accept's own bound, higher than 0.219 — it would have stayed put
otherwise), and the new incumbent persists to the database. One logical shrink step cost
twenty physical replays. The logical budget (500 candidates) is untouched by the twenty,
and physical cost is bounded by the shrink deadline alone.

*Candidate 2, jitter 5 → 1.* The proposal is `[3, 611, 802, 129, 1]`, and its first run
happens to realize a schedule where the queue never fills: the trailing value goes unread,
the run passes, and the candidate is rejected for the price of one replay, ledger (0, 1).
Pass repetition will re-propose it across sweeps and the ledger will accumulate, (0, 2),
(0, 3), …

*Candidate 3, the one the gauntlet exists for.* Suppose some candidate can only fail
through an unrelated flake at 2%, and it gets lucky on its recruiting run. To be accepted
it still needs three more failures and a lower bound over the threshold before the 30-run
cap, roughly a 4-in-10,000 event at a 2% rate. Weighted by the 2% chance of the lucky
recruiting run in the first place, that is the 1-in-100,000 mass the proposal was charged
before its first outcome was recorded. Without the 4-failure minimum this protection
collapses: a single failure's lower bound is 0.2065, so any threshold below that would
accept every candidate on the run that recruited it. That degeneration was measured before
it was fixed — it lost target-regime bugs a third of the time.

Later sweeps repeat the shape: job values walk to 0, the jitter to 1, each displacement
paying its own gauntlet against a threshold that ratchets with the anchor (0.8 × 0.258 =
0.207 after the first adoption). Say the incumbent ends at `[3, 0, 0, 0, 1]` with the
anchor at 0.34 and the threshold at 0.27.

= Stopping

A fast sweep eventually proposes everything and adopts nothing. Under noise that alone
certifies nothing: candidate 2 was rejected on single unlucky runs, and one of those
rejects might be a real reduction. So one *confirmation sweep* re-proposes every
candidate with the single-run fast reject disabled and drives each cumulative ledger to a
bound verdict. Candidate 2's ledger, all misses, is driven until even its upper bound
falls below the threshold — with the threshold at 0.27 that takes 11 straight misses —
and bound-rejects. The verdict latches: re-proposals of that timeline cost one replay and
no further drive. When a confirmation sweep adopts nothing, the shrink stops with a
certificate: every reachable proposal was decided by evidence, not by luck. Fixed
dry-sweep counting, the alternative, missed 18--46% of reachable reductions in simulation
where confirmed-dry missed 10%.

= The final replay, and the report

Every failure about to be reported re-executes first, inside the engine. This origin is
confirmed, so it gets the pooled review: replay until failure over the incumbent plus the
pool — `[3, 0, 0, 0, 1]`, then T0, then T1 — with a budget derived from the design's
target of handling bugs that fail at least 10% of the time: 29 replays, the count at which
a one-in-ten failure escapes with probability at most 5%, split as ceil(29/3) = 10 per
timeline. The incumbent's first replay realizes `[3, 0, 0, 0]` (never full,
the trailing jitter unread) and passes. The second realizes the full structure, wins the
race, and passes. The third loses the race: reproduced. The review stops there, evidence
(1, 3), folded into the origin's report-time counts.

Had all three timelines stayed dry, two rescue tiers wait behind them: 10 positional
splices of random pairs from that same replay list — cut both at position 2 and
`[4, 611 | 802, 129, 5, 774]` crossed with `[3, 0 | 0, 0, 1]` glues to `[4, 611, 0, 0, 1]`
— and then 4 fresh
generations, failures pinned to this origin. And a confirmed origin that stays dry after
all of that is still reported: the caveat switches to say so rather than the failure
disappearing, because an unreproduced failure still failed this run.

What the user sees, schematically:

```text
  n = 3
  job = 0
  job = 0
  job = 0
  jitter = 1
thread 'no_job_is_dropped' panicked at tests/queue.rs:16:5:
assertion `left == right` failed
  left: 2
  right: 3
note: nondeterministic failure, confirmed: failed 8 of 20 replays at
      confirmation and 1 of 3 at report time
reproduce with: #[hegel::reproduce_failure("<v2 blob>")]
```

Plain `FAILED` — no special status — with the uncertainty carried by the `note:`, which
quotes only this run's own replay counts, because no rates are ever persisted. The printed
draw lines come from the freshest captured failing execution (the review's reproducing
replay — gauntlet probes are never captured, since capturing every discarded probe is the
dominant cost of failing-heavy runs). The blob carries the same version-2 state the
database holds at the end of the run: the shrunk incumbent and the pool.

= The next run

The next run fetches the database entry, and the entry itself says what it is: version-2
state opens with an impossible choice count, so the run flips into ND handling *before*
replaying anything. The stored timelines replay until failure under the same split budget.
Say the incumbent reproduces on its second replay: the origin is *Trusted* — the previous
run already paid the bar for it, and re-barring a p ≈ 0.35 bug every run would throw it
away needlessly often — and the first check is skipped, since the reproduction just
replayed it. The reproducing replay realized the stored incumbent node-for-node, so the
shrink phase is skipped too, and the run re-reports and re-persists the failure.

A run where the whole budget comes up dry demotes the entry to the secondary corpus. A
second dry run deletes it there: two strikes, spread across two runs, so one unlucky run
cannot destroy a live entry. `#[hegel::reproduce_failure("…")]` replays the blob through
the same machinery (minus the fresh tier — a fresh generation's failure could be
unrelated to the blob), reporting reproduction or staleness.

= Variations

*If the first check had passed.* Four exact replays all reproducing has probability
roughly (0.35 × s)⁴ here, s being the chance the schedule realizes the same structure —
small, but not zero. The run would have stayed deterministic and shrunk this origin on
single-run trust. The safety net is the origin *history*: while
a run is deterministic, every interesting execution of an origin (raw sightings and shrink
accepts alike) is retained. When the shrink verify or the final replay eventually missed
and flipped the run, the engine would backtrack over that history for the reproduction
boundary — probing at geometric offsets, refining by bisection, biased towards older
entries since a too-old restore merely re-shrinks while a too-new one re-ratifies the
damage — and the settled candidate would face the full discovery bar (on a separate
budget of 3 attempts) before being restored, re-persisted, and re-shrunk under the
gauntlet.

*If the bar had rejected.* Had the failure been a genuine rarity — say the race loses 2%
of the time — the seeded batch would almost certainly have found nothing more. A batch
already carrying the seed's failure cannot trip the zero-failure gate at 10 replays (that
gate serves origins that arrive with no seed, such as ones first sighted after the flip),
so it grinds on until the four-failure quota is unreachable within the 40-replay cap and
rejects at (1, 38). Rejection evicts the origin from the interesting map (generation keeps
hunting, and a rediscovery faces the bar afresh, up to the 5-attempt budget), but the
evidence is kept: if nothing else confirms this run, the failure is still reported,
caveat-only — "unconfirmed failure: failed 1 of 38 replays this run, below the
confirmation bar — likely rare" — with no blob.

*Under `error` strictness.* The run aborts at the first detection: here, the first
check's structural miss, with a diagnostic naming the divergence position. An
outcome-only flip (same realized values, different verdict) aborts as `Flaky` with the
pre-branch wording. The one thing `error` does not do is refuse stored version-2 state: a
prior run's entry replays with handling off, because a stored entry that stops reproducing
is staleness, not evidence of anything about this run.

*With real threads.* A concurrent state machine is this same story with the schedule as
the hidden state. There, each worker thread draws through its own handle, and the engine
records a worker's whole draw stream as a *single element* of the parent timeline — so
pooling, splicing, and replay treat it as one value, a splice can never tear it apart, and
the thread interleaving itself is sampled fresh on every execution, never replayed. That
is why reproduction rests on the pool and the statistics rather than on exact replay.

= What this example leaves out

The example never exercised outcome-only nondeterminism (a body whose structure is stable
but whose verdict is noisy — same machinery, easier case), targeting under ND handling
(the hill climber is replaced by a holdout-gated race of the same design as boost), the
gauntlet's alpha-budget escalation (this shrink spent a few hundred-thousandths of its
0.02; a shrink that realizes thousands of floor-threshold candidates escalates the failure
minimum from 4 towards 8 instead of running unbounded risk), or the sizing arguments
behind every constant used above. Those live in the review brief and, authoritatively, in
`notes/design.md` and `notes/decisions.md`.
