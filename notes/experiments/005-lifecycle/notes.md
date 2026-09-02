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

(spec next: capture-at-confirmation -> pool, blob v2 persistence, pool-first-fit DB replay
with 004's extend, strictness, caveated reporting; two-run harness)

## Results

(part A above; part B pending)
