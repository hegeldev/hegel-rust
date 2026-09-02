# 004: replay semantics

Question (from `000-plan.md`): how should ND mode replay stored timelines — pool fallback,
continuation budgets, structural divergence — measured on bodies whose *structure* changes
run to run? Feeds deferred decision 14 (per-position anchoring / trie) with fall-off and
prefix-sharing data. 003 covered outcome ND (fixed structure, flaky verdict); this covers
the complement: deterministic verdict, flaky structure.

## Today's replay semantics (read from `resolve_choice`, state.rs)

1. Alignment is positional: draw `i` is served `prefix[i]`. No re-anchoring after a
   divergence — a count change at position k shifts everything after k.
2. Misfit inside the prefix (kind flip, or stored value outside the requested constraints)
   silently serves the unit value (or simplest, if the stored value was that node's
   simplest). No divergence signal, no random draw.
3. Past the prefix: random continuation up to `max_size` (`for_probe`) or overrun
   (`for_choices`, where `max_size == choices.len()`).

So "pool fallback + continuation budgets" needs no new `resolve_choice` mechanism to
measure: bare replay = `for_choices`, budgeted continuation = `for_probe` with
`max_size = len + extend`. The experiment measures how far those semantics carry.

## Setup

Driver: `__bench::replay_once(choices, extend, seed, body)` — one execution against a
replayed sequence (bare at `extend == 0`, else budgeted continuation; empty prefix =
fresh generation). Returns interestingness + the realized `ChoiceValue` timeline.

Harness (`/experiments/replay-semantics`): bodies draw integer atoms in 0..=100 through
the engine; hidden per-execution coins (splitmix64 stream per trial) change the *draw
structure*, never the verdict. Fails deterministically iff >= 3 atoms >= 90.

| body | structure | hidden ND |
| --- | --- | --- |
| B0 det | n in 0..=12, then n atoms | none (control) |
| B1 late-coin | as B0 | p=0.3: one trailing boolean draw |
| B2 step-coins | n in 0..=10 steps, 1 atom each | p=0.2/step: a second atom (count shifts) |
| B3 kind-flip | n in 0..=10 draws | p=0.15/draw: boolean instead of integer (kind puns, no shift) |
| B4 stable-prefix | 4 fixed atoms, then B2 tail (n in 0..=8) | tail-only count shifts |

Per trial (40 trials/body): discover a failing timeline T0 by fresh generation (cap 400
attempts); build a pool per capture-at-confirmation — 20 replays of T0 (extend 16), each
failing replay's realized timeline into the pool (dedup, T0 first, cap 10); then 50 cold
attempts per strategy:

- T0 replay at extend 0 / 4 / 16 / 64 (continuation-budget sweep)
- pool first-fit at extend 16 (try pool entries in order, stop at first failure)
- fresh generation (control — the floor any replay strategy must beat)

Instrumented: verbatim watermark (longest common prefix of T0 vs realized, as a fraction
of T0's length) on confirmation replays; pool size; mean pairwise LCP fraction within the
pool (the decision-14 prefix-sharing signal); replays consumed per pool attempt.

Pool build: 40 extra confirmation-style replays after the 20 measured ones (dedup, cap 20),
so the K-sweep has material to truncate.

## Results

40 trials/body, 50 cold attempts/strategy, ~0.6s total.

| body | disc med | fresh % | confirm med /20 | watermark p50 (p10) | pool med | pair-lcp mean |
| --- | --- | --- | --- | --- | --- | --- |
| B0 det | 27 | 4.1 | 20 | 1.00 (1.00) | 1 | n/a |
| B1 late-coin | 27 | 4.1 | 20 | 1.00 (0.93) | 3 | 0.98 |
| B2 step-coins | 23 | 4.6 | 20 | 1.00 (0.80) | 18 | 0.94 |
| B3 kind-flip | 39 | 1.5 | 13 | 0.40 (0.11) | 13 | 0.32 |
| B4 stable-prefix | 11 | 6.6 | 20 | 1.00 (0.88) | 20 | 0.93 |
| B5 het-shift | 19 | 2.3 | 5 | 0.35 (0.14) | 7 | 0.48 |

Single stored timeline, reproduction % by extend budget:

| body | e=0 | e=4 | e=16 | e=64 |
| --- | --- | --- | --- | --- |
| B0 det | 100 | 100 | 100 | 100 |
| B1 late-coin | 78 | 100 | 100 | 100 |
| B2 step-coins | 53 | 85 | 86 | 85 |
| B3 kind-flip | 66 | 66 | 66 | 64 |
| B4 stable-prefix | 61 | 98 | 99 | 99 |
| B5 het-shift | 19 | 28 | 27 | 28 |

Pool first-fit, reproduction % by pool cap K (extend 16; replays consumed per attempt in
parens):

| body | K=1 | K=2 | K=5 | K=10 | K=20 |
| --- | --- | --- | --- | --- | --- |
| B0 det | 100 (1.0) | 100 (1.0) | 100 (1.0) | 100 (1.0) | 100 (1.0) |
| B1 late-coin | 100 (1.0) | 100 (1.0) | 100 (1.0) | 100 (1.0) | 100 (1.0) |
| B2 step-coins | 85 (1.0) | 93 (1.1) | 97 (1.3) | 98 (1.3) | 99 (1.3) |
| B3 kind-flip | 68 (1.0) | 88 (1.3) | 99 (1.5) | 100 (1.6) | 99 (1.5) |
| B4 stable-prefix | 99 (1.0) | 99 (1.0) | 100 (1.0) | 100 (1.0) | 100 (1.0) |
| B5 het-shift | 28 (1.0) | 44 (1.7) | 65 (2.8) | 72 (3.2) | 73 (3.2) |

(K=1 matches the e=16 single-timeline column — the sweep's internal sanity check.)

## What we learned

1. **The continuation budget only needs to absorb net elongation.** e=4 captures the entire
   single-timeline benefit on every body; e=64 adds nothing and costs nothing (a diverged
   long tail reproduces at ~fresh rate, which is negligible). Bare replay's losses (B1 78,
   B2 53, B4 61) are end-of-sequence overruns, not value loss. A small constant plus a small
   fraction of stored length is plenty.
2. **Positional punning is robust exactly when constraints are homogeneous.** Count shifts
   barely hurt B2/B4 (85-99% with a budget) because every stored atom fits every requested
   range — values get re-purposed, not lost. Make draws heterogeneous (B5: wide values
   landing on 0..=1 requests pun to unit and vice versa) and single-timeline replay craters
   to 28% with no help from any budget. Kind flips (B3) lose exactly the flipped positions:
   66% ~= 0.85^3, extend-independent.
3. **The pool is the recovery mechanism, and K=5-10 is the right bound.** K=5 gets B3 from
   68 to 99 and B5 from 28 to 65; K=10 reaches the plateau (72-73% on B5); K=20 adds
   nothing. Cost stays low — worst case 3.2 replays/attempt. Capture-at-confirmation
   harvests enough diversity on its own (B3: 13 distinct timelines from 60 replays; B5: 7).
4. **Prefix sharing is anticorrelated with pool need** — the decision-14 instrumentation
   answer. Bodies where the pool matters have low sharing (B3 0.32, B5 0.48: fall-off is
   early and unpredictable, watermark p10 0.11-0.14); bodies with high sharing (0.93-0.98)
   are the ones K=1 already handles. A merged trie would compress timelines that don't need
   pooling and fail to compress the ones that do. Per-position anchoring keeps a measured
   ceiling to aim at: the 27% residue on B5 that whole timelines can't reach — 006's span
   grafting is the natural probe for it.
5. **Watermark understates value survival.** B3's verbatim LCP falls off at 0.40 of the
   stored length while 66% of replays still fail: punning damages one position and stays
   aligned after it. Divergence detection should therefore not treat first-divergence as
   "replay dead" — its value is evidence weighting (a structurally diverged non-failure says
   little about the stored timeline) rather than early abort.

Caveats: bug predicate is a global atom count, insensitive to position — bodies whose
failure depends on *which* draw carries the value would be harsher on punning; hidden ND is
per-execution-independent coins, not stateful cross-run drift; no spans/clones (006);
discovery here is fresh-generation-until-interesting rather than the engine's novel-prefix
walk.
