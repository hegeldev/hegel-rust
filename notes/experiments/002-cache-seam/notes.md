# 002: cache seam + fixate cost

Questions (from `000-plan.md`): where does the resampling seam go in
`cached_test_function`, and what does one fixate iteration cost with tree dedup off, on a
real ~50-node target?

## The seam

`Engine::cached_test_function` (`hegel-c/src/native/test_runner.rs:1202`) is the only place
the choice tree serves a recorded conclusion instead of executing. Every other replay-shaped
path already executes unconditionally:

- Targeting trials (`hegel-c/src/native/targeting.rs:133`) go straight to `test_function`
  via `for_probe` — no tree consult.
- Database replay (`test_runner.rs:508`) and the final verify replay (`test_runner.rs:536`)
  use `for_choices` + `test_function` directly.
- Generation runs novel prefixes through `test_function`; the tree's role there is proposing
  prefixes, not serving outcomes.

So "the tree never serves conclusions in ND mode" is one boolean: `Engine::serve_replays`
(`test_runner.rs:1003`), now gating `simulate_full` at `test_runner.rs:1208`. Flipping it
converts every replay consumer — shrinker full runs and probes, span mutation — to resample
semantics with no other changes. Prototyped on this branch; default stays `true`.

What the flag does *not* touch: executions still record into the tree (`record_run`), which
is what ND mode wants — recording feeds the kind-mismatch divergence detector and
novel-prefix dedup (004/005 decide how mismatches are tolerated rather than fatal).

## Fixate cost

`hegel_c::__bench::fixate_cost_experiment` (harness: `/experiments/fixate-cost`) records one
interesting case whose body draws N booleans, then replays it 10k times through
`cached_test_function` with serving on/off. Engine overhead only — the body is N boolean
draws, the cheapest possible node, so these are floors for the engine side and exclude any
real test-body work.

| draws | serve | executions | total | ns/replay |
| --- | --- | --- | --- | --- |
| 10 | tree | 0 | 10.5ms | 1055 |
| 10 | execute | 10000 | 29.5ms | 2948 |
| 50 | tree | 0 | 19.2ms | 1918 |
| 50 | execute | 10000 | 59.0ms | 5899 |
| 200 | tree | 0 | 52.8ms | 5276 |
| 200 | execute | 10000 | 203.8ms | 20384 |
| 1000 | tree | 0 | 246.2ms | 24624 |
| 1000 | execute | 10000 | 1.0s | 104439 |

Both sides are linear in node count: ~25-100ns/node served, ~100-300ns/node executed
(smaller cases pay proportionally more fixed cost). At the target size (~50 nodes) a real
re-execution costs ~6us of engine overhead vs ~2us served — a 4us delta, so a full B = 30
confirmation gauntlet adds ~120us of engine time per candidate. Any test body worth ND
treatment (threads, channels, real work) costs orders of magnitude more than that per run.

## Conclusions

1. The resampling seam is `serve_replays`, a single gate in front of `simulate_full`. No
   other code path serves conclusions, so ND mode needs no cache surgery elsewhere.
2. Turning dedup off is cheap on the engine side. The dominant fixate cost is the body
   itself, which is irreducible — re-running the body is the entire point of resampling. No
   result-cache substitute is needed in ND mode.
3. Budget arithmetic from experiment 001 (gauntlets of up to 30 runs, confirmation runs of
   ~20) translates to microseconds of engine overhead; the budgets should be set by body
   cost and statistics, not engine throughput.

Caveats: boolean draws only (typed constraints record/compare more per node, but both sides
scale together); no spans in the body; single-threaded; `update_interesting` and disabled-DB
persistence included in the executed path.
