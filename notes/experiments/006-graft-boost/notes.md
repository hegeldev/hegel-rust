# 006: cross-timeline grafting + boost phase

Questions (from `000-plan.md` and design.md): does a boost phase (successive halving over
variants, decision 2's "raise p when possible") deliver higher-reliability incumbents at
acceptable cost before shrinking; and does cross-timeline content (donor splicing) recover
any of the ~27% replay residue whole-timeline pools plateau under (004, decision 22)?

## A: boost (in-engine)

New `Settings::nd_boost` flag (prototype). After confirmation seeds the anchor and before
the shrinker starts, run successive halving:

- Candidates: incumbent + pool entries, topped up to 16 with probe mutants (replay the
  incumbent's prefix cut at a random point, random continuation, small extension).
- Rounds: score every candidate with r replays (budgeted probe), keep the top half by
  cumulative failure rate, double r. Cost 16x2 + 8x4 + 4x8 + 2x16 = 128 replays + a 10-run
  holdout on the winner.
- The winner replaces the incumbent as the shrink start and raises the anchor only if its
  holdout LCB beats the confirmation anchor (holdout because the halving selection is
  biased upward — the winner's in-race rate overstates its p).

Bodies (harness `/experiments/nd-boost`, outcome-ND like 003): **deterministic-core** (any
atom >= 95 -> p = 1.0; else >= 3 atoms >= 10 -> p = 0.3; else 0) — the user-visible payoff
case where boost should find the deterministic region and shrinking should then stay in it;
**L1 rising** (boost trades size for reliability); **L3 constant 0.5** (control: nothing to
find, boost should not hurt). Gauntlet vs gauntlet+boost, 30 seeds: final true p, length,
executions.

## B: grafting (harness)

Extend the 004 methodology on the structurally-ND bodies where the pool plateaus (B5
het-shift; B3 kind-flip as contrast). Per trial, build the pool as in 004, then on R cold
attempts compare: pool first-fit (K = 10, extend 16 — the 004 baseline) vs pool + splice
fallback — when every pool entry misses, try candidates spliced from random pool pairs at a
random position (prefix of one, suffix of another), positional granularity. Positional
splicing is a cheap lower bound on span-anchored grafting: if it recovers part of the
residue, span machinery is worth building; if it recovers nothing, the residue needs live
re-execution anchoring rather than stored cross-timeline content.

## A results

30 seeds/cell, 500-case budget, gauntlet mode with and without boost:

| landscape | boost | shrunk | final p med (p10-p90) | det frac | len med | execs med (p90) |
| --- | --- | --- | --- | --- | --- | --- |
| D1 deterministic-core | off | 30/30 | 1.00 (1.00-1.00) | 27/30 | 1 | 1792 (2736) |
| D1 deterministic-core | on | 30/30 | 1.00 (1.00-1.00) | 30/30 | 1 | 2044 (2975) |
| L1 rising | off | 30/30 | 0.26 (0.26-0.34) | 0/30 | 3 | 7953 (13160) |
| L1 rising | on | 30/30 | 0.42 (0.26-0.50) | 0/30 | 5 | 11589 (24779) |
| L3 constant | off | 30/30 | 0.50 (0.50-0.50) | 0/30 | 1 | 1553 (2082) |
| L3 constant | on | 30/30 | 0.50 (0.50-0.50) | 0/30 | 1 | 1733 (2655) |

## B results

Splice fallback appended to the 004 harness: after all K = 10 pool entries miss on a cold
attempt, up to 10 positional splices of random pool pairs (extend 16):

| body | pool misses | rescued | rescue % | splice replays/miss |
| --- | --- | --- | --- | --- |
| B2 step-coins | 31 | 26 | 84 | 4.8 |
| B3 kind-flip | 15 | 15 | 100 | 1.6 |
| B5 het-shift | 541 | 350 | 65 | 6.3 |

(B0/B1/B4 never miss the pool; earlier tables unchanged.)

## What we learned

1. **The gauntlet's monotone anchor already does most of the boost's job on landscapes with
   a deterministic core**: plain gauntlet shrinking lands deterministic in 27/30 D1 runs,
   because any accepted candidate that touches the core raises the anchor to ~0.78 and
   prices the flaky region out for the rest of the shrink. Boost closes the tail (30/30) for
   +14% cost — a guarantee, not a discovery mechanism, on this landscape.
2. **On landscapes with no deterministic core, boost is a size-for-reliability trade**: L1
   final p 0.26 -> 0.42 median (p90 0.50) at len 3 -> 5 and +46% cost. Whether the shipped
   default takes that trade is a reporting-policy question, not a statistical one — the
   machinery works and is cheap (~250 replays); it can ship behind a setting or heuristic
   (e.g. boost only when the anchor is below some reliability floor).
3. **Boost is harmless where there is nothing to find** (L3 unchanged, +12% cost) — the
   holdout gate correctly refuses to raise the anchor on noise.
4. **Cheap positional splicing recovers most of the whole-timeline replay residue**: 65% of
   B5's full-pool misses (72% -> ~90% overall reproduction), 100% of B3's, at ~6 extra
   replays per miss. The 004 conclusion "the residue is the per-position-anchoring target"
   sharpens: the residue does NOT require live re-execution anchoring or a trie — stored
   cross-timeline content recombined at arbitrary positions already reaches most of it, so
   span-anchored grafting (better split points) is an optimization over splicing, not a
   prerequisite. Replay-until-failure should be: pool first-fit, then a handful of splices,
   then fresh generation.
5. Splicing doubles as candidate generation: the boost's mutant generator and the splice
   construction are the same shape (prefix of one timeline + suffix of another/random) —
   one mechanism serves both replay repair and boost variants.

Caveats: positional splices only (span-anchored not built — justified by 4: it is now an
optimization, so it belongs in implementation, not another experiment); boost variants are
prefix-cut probe mutants + pool entries, no span-duplication mutants; D1's core is reachable
by single-atom shrinking, which flatters the no-boost gauntlet — cores requiring coordinated
multi-position moves would show a bigger boost gap.
