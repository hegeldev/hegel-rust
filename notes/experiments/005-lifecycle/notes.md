# 005: lifecycle

Question (from `000-plan.md`): confirmation, capture-at-confirmation, persistence gating,
unified reporting, blob v2, strictness setting — the failure lifecycle end to end. Two parts:

- **A — confirmation bar** (assigned by decision 21): derive the discovery-confirmation rule
  from the noise-floor caution instead of the scaffold's flat >= 2-fails-in-20.
- **B — lifecycle prototype**: capture-at-confirmation feeding the timeline pool, ND blob
  persistence (v2 prefix), pool-first-fit DB replay with 004's budgets, strictness setting,
  and the caveated unreproduced-failure report, wired through the ND scaffold and driven by
  a two-run harness (run 1 discovers/shrinks/persists, run 2 replays from the DB).

## A: confirmation bar

Setting: a raw interesting execution proposed a new origin; confirmation replays the
timeline and decides real-vs-fluke. The discovery run itself is selection, not evidence —
only fresh replays count. Design targets: handle p >= 0.1 (decision 16); noise-floor
landscapes fire background flukes at p ~ 0.02 (003).

The loss function is asymmetric. A false **accept** is sticky: it occupies the origin,
anchors the gauntlet on garbage, and the displacement gate then protects it. A false
**reject** recycles: the origin is dropped, generation keeps hunting, and the next
discovery of the same bug gets a fresh batch — run-level power compounds across
re-discoveries while run-level false-accept compounds against us (roughly 2 fluke
discoveries precede the real one on 003's noise floor, and each unconfirmed fluke is
retried by later flukes). So: minimize P(accept | noise) and E[cost | noise] hard, accept
moderate per-discovery power at p = 0.1, keep E[cost] at p = 0.9 near the early-accept
minimum.

Rules evaluated by exact DP over (runs, fails) states (`/experiments/confirm-bar`):

- Flat k-of-B (early accept at k fails, early reject when k is unreachable):
  the scaffold's 2/20, plus 3/20, 3/30, 4/30, 3/50, 4/50.
- Wald SPRT p0 = 0.02 vs p1 = 0.1 at (alpha, beta) in {(.05,.05), (.01,.05), (.01,.3)},
  capped at 50/80 runs (accept at cap iff LLR > 0).
- Wilson pair: accept at LCB >= 0.02, reject at UCB < 0.1, cap 40 (accept at cap iff
  fails/runs >= 0.05) — the gauntlet-shaped variant.

Report P(accept) and E[runs] at p in {0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 0.9}; pick the bar.

### A results

Full tables from `cargo run --release` in `/experiments/confirm-bar`. The decisive rows
(P(accept) per true p; E[replays] at p = 0.02 / 0.1 / 0.9; run-level false-accept at
F = 5 fluke exposures):

| rule | p=.02 | p=.05 | p=.1 | p=.2 | E .02 | E .1 | E .9 | false acc F=5 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| flat 2/20 (scaffold) | .060 | .264 | .608 | .931 | 18.9 | 14.7 | 2.2 | .266 |
| flat 3/20 | .007 | .075 | .323 | .794 | 18.3 | 17.5 | 3.3 | .035 |
| flat 4/30 | .003 | .061 | .353 | .877 | 27.5 | 26.4 | 4.4 | .014 |
| gate 1/10 then 3/30 | .016 | .143 | .476 | .871 | 13.4 | 17.3 | 3.3 | .077 |
| **gate 1/10 then 4/40** | **.006** | **.102** | **.454** | **.877** | **15.2** | **23.0** | **4.4** | **.029** |
| sprt .01/.05 cap 80 | .022 | .359 | .899 | 1.00 | 51.9 | 48.6 | 3.3 | .104 |
| wilson .02/.1 cap 40 | .261 | .651 | .931 | .999 | 31.3 | 13.9 | 1.1 | .780 |

What settled it:

1. **The scaffold's 2-in-20 is confirmed unsafe**: 6% per-discovery false accept, 27%
   run-level at F = 5 — 003's one lost L4 trial was this rate arriving on schedule.
2. **Wilson-LCB-above-noise is the wrong shape for confirmation** (26% false accept): a
   couple of early fails push the LCB over a small floor long before the evidence
   distinguishes 0.02 from 0.1. The gauntlet's Wilson machinery is for comparing against an
   anchor, not for fluke rejection.
3. **SPRT buys power only by spending 50+ replays at the boundary** — the recycling
   asymmetry makes that a bad trade: per-discovery power is cheap to forgo (a p >= 0.1 bug
   is rediscovered and re-confirmed within the generation window; power compounds as
   1-(1-.45)^attempts) while per-discovery cost is paid on every fluke.
4. **The two-stage gate dominates flat rules**: rejecting on 0-fails-in-10 dismisses 82% of
   p = 0.02 flukes for 10 replays, and re-spending the savings on a 40-run ceiling buys
   more power than flat 4/30 at comparable false-accept.

**Bar adopted (decision 23): replay 10x; no failure -> reject. Any failure -> continue to
40 total, accepting early on the 4th failure, rejecting when 4 is unreachable.** Operating
point: 0.6% false accept per fluke (~3% per run at heavy exposure), 45% per-discovery power
at the p = 0.1 target compounding to >95% by the 5th discovery, 15 wasted replays per
fluke, 4.4 replays for a p = 0.9 bug.

## B: lifecycle prototype

Scaffold changes (all gated on `NdExperiment != Off`):

1. **Discovery confirmation adopts the 005A bar** via a shared `nd_confirm` helper: replays
   are budgeted (`for_probe`, extend max(4, len/8) per 004), the discovery run itself is
   selection and counts nothing, gate 1/10 then 4/40 with early accept/reject. Rejected
   origins are removed from the interesting map and remembered with observation counts.
2. **Capture at confirmation**: failing confirmation replays' realized timelines feed a
   per-origin pool (incumbent first, dedup, cap 10 per decision 22), the anchor is seeded
   from the confirmation evidence, and the witness run is stored — the pre-shrink verify
   step consumes it instead of running its own 20-replay batch (the double-work the design
   said capture-at-confirmation removes). Origins that arrive without discovery-time stats
   (DB reuse) fall back to a fresh `nd_confirm` at verify time.
3. **Persistence under ND**: the end-of-run save keeps the shrunk incumbent and adds the
   origin's pool entries as further primary entries. No blob v2 needed for the DB path —
   entries are already one timeline each, so incumbent + pool is just more entries; the v2
   *reproduce-blob* format (prefix byte, pool, entropy budget) is deferred to
   implementation as pure format work.
4. **Reuse under ND**: each stored entry gets up to 10 replay attempts (early exit on
   reproduction, ~1/p expected per decision 11) before deletion. Demote-to-secondary is
   deferred with the format work.
5. **Caveated reporting**: when a run observed interesting executions but confirmed
   nothing, it fails with `[unconfirmed] <origin>` failures instead of passing silently
   (decision 3). Unconfirmed observations are reported only when no confirmed failure
   exists — a confirmed failure plus fluke chatter would be caveat fatigue.
6. **Strictness setting**: not prototyped — it is presentation-layer mapping onto existing
   scaffolding (`error` = today's aborts, `quiet`/`warn` = ND handling with/without a
   notice) with no open statistical or mechanism question.

Harness (`/experiments/nd-lifecycle`): per (body, seed), run 1 with a fresh in-memory DB
copied into run 2 — run 1 generates/confirms/shrinks/persists, run 2 (Reuse phase, no
generation once reproduced) measures cross-run reproduction. Bodies: 003's outcome-ND
landscapes (L1/L3/L4) plus 004-style structural ND (step-coins, het-shift) and a pure-noise
body (p = 0.02 everywhere, no real bug) for the caveated-report path.

## B results

Two-run lifecycle, gauntlet mode, 30 seeds/body. At the default 300-case budget and at
1000 (`ND_CASES`):

| body | budget | r1 confirmed | r1 caveated | r2 reproduced | r2 execs med |
| --- | --- | --- | --- | --- | --- |
| L1 rising | 300 | 30/30 | 0 | 30/30 | 4 |
| L3 constant | 300 | 30/30 | 0 | 30/30 | 4 |
| L4 noise-floor | 300 | 30/30 | 0 | 30/30 | 2 |
| S2 step-coins | 300 | 30/30 | 0 | 30/30 | 1950 |
| S5 het-shift | 300 | 20/30 | 4 | 20/20 | 8285 |
| N0 noise-only | 300 | 0/30 | 30 | — | — |
| S5 het-shift | 1000 | 29/30 | 1 | 29/29 | 8429 |
| N0 noise-only | 1000 | 0/30 | 30 | — | — |

Zero run errors anywhere; every confirmed run-1 failure reproduced in run 2 (139/139 across
both budgets).

## What we learned

1. **The lifecycle holds together end to end.** Discover -> confirm (gate 1/10 then 4/40,
   capture, anchor) -> gauntlet shrink -> persist incumbent + pool -> reproduce from the DB
   next run, with no Flaky/NonDeterministic aborts anywhere and the DB needing no format
   change (the pool is just more primary entries under the same key). Blob v2 remains pure
   format work for the reproduce-blob path.
2. **Confirmation is a property of origin admission, not of one execution path.** The first
   cut hooked confirmation on the generation run's own interesting status; span-mutation
   (and in principle targeting) executions also fill vacant origins, and those slipped to
   shrink unconfirmed — on pure noise, 26/30 runs "confirmed" a fluke through the
   witness-only pre-shrink fallback. Fixed by sweeping every unconfirmed interesting origin
   after each generation iteration (and once after the loop), and holding untrusted origins
   reaching shrink (novel origins discovered mid-shrink) to the full bar. This generalizes
   decision 20: every path into the interesting map needs the same gate, and every origin
   in it needs the same confirmation.
3. **Caveated reporting works as designed** (decision 3): N0 fails all 60 runs with
   `[unconfirmed after N observation(s)] bug` instead of passing silently or confidently,
   persists nothing, and holds zero false confirms across ~hundreds of fluke confirmations
   (the 005A alpha on display).
4. **Cross-run cost splits exactly on structure.** Outcome-ND bodies reproduce from the DB
   in 2-4 executions: the stored timeline realizes identically, `replay_aligned` holds, and
   shrink is skipped. Structurally-ND bodies misalign, so every run re-shrinks (1.9k/8.4k
   median executions) — the deferred `replay_aligned`-replacement decision now has its
   price tag.
5. **Reused entries are trusted on reproduction** (up to 10 replay attempts each): the
   prior run only persisted confirmed origins, so re-running the full bar would drop real
   p ~ 0.1 reused bugs ~55% of the time at verify. Residual hazard — noise persisted by a
   prior run's false accept surviving via reuse — is bounded by the ~3%-per-run false-accept
   rate and that entry's own ~18%-per-run reproduction chance.
6. **Small budgets starve narrow structural bugs of discoveries**, not of confirmations:
   S5's 300-budget shortfall (20/30) was runs with zero or one discovery — span-mutation
   probes of small passing cases dominate the execution budget and rarely hit a narrow
   deterministic predicate. At 1000 cases discovery recovers (29/30). A generation-strategy
   note for implementation (ND mode currently disables the novel-prefix walk), not a
   lifecycle defect.

Caveats: strictness setting not prototyped (presentation-layer; `error` = today's aborts);
demote-to-secondary not prototyped (single-step delete after 10 misses); blob v2 untouched;
single origin per body; reuse trust is prototype policy, revisit with the demote work.
