# Experiment 010: what the data tree buys

Hypothesis under test: "on real workloads the data tree is rarely buying us
anything and is significant overhead."

Base: `main` at 770970b8 (production engine, no nondeterminism-branch
machinery). Apple M5 Pro, rustc 1.98.0, release build.

## The tree's four roles

On this revision (`hegel-c/src/native/`):

1. **Recording** — `Engine::record_run` folds every execution into the tree
   via `data_tree::record_tree_full` (`test_runner.rs`). Recording is also
   what detects choice-tree mismatches (nondeterminism).
2. **Serving** — `Engine::cached_test_function` serves any fully recorded
   path from `data_tree::simulate_full` without running the body. Consumers:
   shrinker probes and span mutation. Targeting (`targeting.rs::Optimiser`)
   calls `test_function` directly and never gets served.
3. **Novel-prefix generation** — the generate loop walks the tree
   (`generate_novel_prefix`) to steer each case away from seen paths.
4. **Exhaustion** — `tree_root.is_exhausted` stops generation when the space
   is fully explored, and gates the exhausted-space FilterTooMuch variant.

## Knob and instrumentation

`HEGEL_EXPERIMENT_NO_TREE=1` (read once in `Engine::new`) disables all four
roles: no recording, `cached_test_function` always executes, novel prefix is
always empty (pure random generation). Exhaustion semantics under the knob:
`is_exhausted` stays false forever, so small spaces run to the full
test-case budget and the exhausted-space FilterTooMuch check never fires
(the 50-invalid threshold variant still does). Choice-tree mismatch
detection is also lost. Default-off is a zero-behaviour change: with the env
vars unset, the full engine suite (1319 tests) and root crate suite (79
binaries, 1811 tests) pass unchanged.

`HEGEL_EXPERIMENT_STATS=<path>` writes per-run counters as one JSON line:
body executions split by phase (reuse/probe/generate/target/mutate/shrink),
tree-serve hits, duplicate executions of already-seen choice sequences
(order-sensitive fingerprint of the realised values), exhaustion flag, tree
node count, and the valid/invalid/overrun tallies.

## Harness

`experiments/tree-value/` drives the public frontend API
(`Hegel::new(body).settings(...).run()`), 20 fixed seeds per cell, each cell
tree-on vs tree-off, database disabled, quiet verbosity. The deterministic
section of the output is byte-identical across repeated invocations
(verified by diff). Wall-clock is reported separately. Raw output:
`results.txt`. The client-owned final replay (one body execution per
failure, outside the engine) is identical in both arms and not counted.

Workloads:

| name | shape |
|---|---|
| tiny_bools | two booleans, passes (4-leaf space, exhaustion value) |
| vec_i64_pass | `Vec<i64>` property, passes (generation-bound) |
| filtered_mod3 | int 0..=999, `assume(x % 3 == 0)` (rejection-heavy) |
| filter_all | boolean + `assume(false)` (FilterTooMuch parity) |
| shrink_sum | fail when `len >= 5 && sum >= 1000` (deep shrink) |
| stateful_pass / stateful_fail | counter state machine, `#[state_machine]` |
| regex_email | `from_regex` string, passes (span-heavy) |

## Results

Per-run means over 20 seeds. "execs" counts body executions and "wall" is
the median wall-clock in ms.

| cell | execs tree | execs no-tree | serves/run | dups/run (no-tree) | wall tree | wall no-tree |
|---|--:|--:|--:|--:|--:|--:|
| tiny_bools tc=100 | 4 | 100 | 0 | 96 | 0.05 | 0.16 |
| tiny_bools tc=1000 | 4 | 1000 | 0 | 996 | 0.05 | 1.43 |
| vec_i64_pass tc=100 | 100 | 100 | 16.1 | 16.7 | 0.55 | 0.35 |
| vec_i64_pass tc=1000 | 1000 | 1000 | 180.5 | 177.4 | 4.18 | 2.40 |
| filtered_mod3 tc=100 | 303.6 | 304.4 | 0 | 95.7 | 1.89 | 1.84 |
| filtered_mod3 tc=1000 | 1000 | 3076.8 | 0 | 2252.5 | 10.35 | 18.25 |
| filter_all tc=100 | 2 | 50 | 0 | 48 | 0.07 | 0.46 |
| shrink_sum tc=100 | 200.4 | 1308.0 | 1145.5 | 1134.0 | 2.71 | 6.04 |
| stateful_pass tc=100 | 100 | 100 | 41.2 | 24.9 | 5.97 | 3.28 |
| stateful_fail tc=100 | 7778.9 | 7137.6 | 831.1 | 2349.8 | 134.16 | 96.59 |
| regex_email tc=100 | 100 | 100 | 0 | 0 | 0.57 | 0.41 |
| regex_email tc=1000 | 1000 | 1000 | 0 | 0.4 | 5.34 | 3.71 |

Parity: every failing cell found its bug in both arms (20/20), and every
seed in both arms shrank to the same result (`[0, 0, 0, 0, 1000]` and
`total=301`). filter_all fails with FilterTooMuch in both arms: the
exhausted-space message after 2 executions tree-on, the threshold message
after 50 tree-off. filtered_mod3 tc=1000 tree-on stops exhausted with 334
valid cases (all that exist), while tree-off runs 3077 executions to collect 1000
valid, 2253 of them repeats. Tree memory at run end: ~1k nodes
(vec tc=100) to ~77k nodes (stateful_fail).

## Reading, by role

**Serving.** The one large win is non-stateful shrinking: shrink_sum runs
6.5x fewer bodies (200 vs 1308) because 85% of shrink probes (1145/1343)
are served from the tree. With an expensive test body this is the tree's
main value. But it inverts on the stateful shrink: the serve rate drops to
10% (831 serves against 7777 executions, 1918 of them duplicates the tree
declines to predict because they run through clone records), and tree-on
ends up running 9% *more* bodies and 39% *more* wall time for identical
shrink quality. On passing workloads serving saves nothing: the run is
valid-count-bound, so served mutation probes just shift the budget to other
executions (execs identical in both arms). What it buys there is diversity,
since tree-off wastes ~17% of the vec_i64 budget re-running already-seen
inputs.

**Novel prefix.** It eliminates duplicate generation completely (dups/run = 0
tree-on everywhere). That matters only when the value space is small
relative to the budget: 31% of tree-off executions are repeats on
filtered_mod3 tc=100, 96%+ on tiny_bools. On realistic-sized spaces
(vec, regex) tree-off's natural duplicate rate is ~0-17%, and bug-finding
was unaffected in every cell.

**Exhaustion.** It is decisive on tiny spaces (4 vs 100/1000 executions,
FilterTooMuch after 2 vs 50) and on the filtered tc=1000 cell, where it
stops at the 334 valid cases that exist instead of grinding to 3077
executions. Everywhere else it is inert. Losing it costs cheap-body wall
time and changes one health-check message, but it never changed a verdict.

**Recording.** It is pure cost wherever the other three roles are inert:
tree-on
is 40-80% slower wall-clock on the passing vec, regex, and stateful cells
with zero counter movement (regex builds 15k nodes for 0 serves and 0 dups
avoided). Recording also carries the choice-tree mismatch (nondeterminism)
check, which this experiment did not exercise.

## Caveats

- Bodies are near-free, so wall-clock deltas price engine overhead only.
  Execution deltas are the portable currency for expensive bodies.
- Bugs here are easy to find, so any discovery-rate value of novel-prefix
  steering on hard, rare bugs is not measured.
- There is one stateful machine, and its poor serve rate is driven by
  clone-record paths and may vary with machine shape.
- Tree serve counts are an upper bound on what a flat exact-match cache
  would hit: `simulate_full` can also serve proposals never executed
  verbatim (prefix prediction). The shrink_sum serve count (1146) vs the
  no-tree duplicate count (1134) suggests nearly all serves there are exact
  repeats, but a follow-up with an actual flat cache would settle it.

## Recommendation

The hypothesis mostly holds. On realistic workloads (large value spaces),
recording, novel-prefix generation, and exhaustion buy nothing measurable,
while recording alone costs 40-80% wall overhead at cheap-body prices plus
tens of thousands of retained nodes. Serving is a large win only for
non-stateful shrinking and is net-negative on the stateful shrink we
measured. Per role:

- **Serving: keep the capability, replace the mechanism.** A flat
  fingerprint(choices) -> (status, origin, nodes, spans) cache over
  executed runs would capture ~all of the shrink win here (serves ≈ exact
  repeats) at a fraction of the recording cost, and would also serve the
  stateful repeats the tree declines. Scope it to the shrink phase if
  memory matters.
- **Exhaustion: replace with a duplicate-counter stop.** Stopping
  generation after N consecutive already-seen cases, using the same
  fingerprint set, recovers the tiny-space early stop and the filtered
  tc=1000 win without any tree. The exhausted-space FilterTooMuch message
  degrades to the threshold variant, which fired correctly in both arms.
- **Novel prefix: drop.** Its measurable effect is duplicate elimination on
  small spaces, which the duplicate-counter stop already bounds. No cell
  showed a discovery or shrink-quality difference.
- **Recording: drop with the tree**, but the choice-tree mismatch check
  needs a home. The flat cache can do it (same fingerprint, different
  recorded outcome means nondeterminism), with weaker localisation than the
  per-node kind check.
