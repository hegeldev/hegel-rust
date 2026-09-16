# Experiment harnesses (frozen)

One-off harnesses behind the nondeterminism work, kept for the record
alongside their write-ups in `notes/experiments/`. They are not workspace
members and are not maintained: each was built against the commit noted
below, and later API changes are expected to have broken some of them.

- `target-sim`: experiment 013 (ND targeting constants); pure simulation
  with no engine dependency, so it stays runnable — `cargo run --release`
  reproduces `results.txt` from the seed printed in its header.

- `fcr-sim`: experiment 014 (multiplicity control, decision 72); pure
  simulation with no engine dependency — exact DP over the bar and
  gauntlet rules plus a seeded Monte Carlo, `cargo run --release`
  reproduces the tables in the notes in about a second.

- `shrink-sim`, `nd-shrink`, `replay-semantics`, `nd-lifecycle`,
  `nd-boost`, `confirm-bar`, `fixate-cost`: built against `a1d1b6d2`.
  `fixate-cost` and `replay-semantics` no longer build — their `__bench`
  entry points (`fixate_cost_experiment`, `replay_once`) were deleted when
  the branch went to production grade.
- `live-set`: experiment 016 (decisions 74–77); runs against the branch via its
  path dependency — `python3 drive.py --episodes 20 --out results.jsonl` (the
  campaign files are checked in). Amended for experiment 017 part A with the
  `kblock<k>`/`kshift<k>` bodies, a shape signature per stored timeline and the
  first-failure execution count (`results-graph-a.jsonl`).
- `graph-replay`: experiment 017 part B (the counterexample as a graph); runs
  against the branch through `__bench::replay_case` and the `ExternalReplay`
  hook — `TRIALS=20 R=50 cargo run --release -- results.jsonl`, then
  `python3 summarize.py results.jsonl` prints the tables in the notes.
- `graph-shrink`: experiment 018 (shrinking the counterexample as a graph); a
  prototype graph shrinker outside the engine, judged through the same hook —
  `TRIALS=10 KS=20 R=50 WARMUP=0 cargo run --release -- results.jsonl` for
  campaign 1 and the same without `WARMUP=0` for campaign 2
  (`results-warmup.jsonl`), then `python3 summarize.py <file>` prints the
  tables in the notes.
- `concurrent-replay`: built against `7a4fd194` (experiment 007's full
  campaign); runs against the branch via its path dependency —
  `cargo build --release`, then `python3 drive.py [trials]`.
- `gauntlet-calibration`: built against `c68a89eb` (experiment 008's
  phase-12 in-engine spot check); drives the engine through the public
  C ABI via its path dependency — `cargo run --release -- spot`. Amended
  2026-09-04 for experiment 011 (the seam plan's acceptance run): a new
  `seam` subcommand adds the flip-site/incumbent/evict decomposition
  columns via the engine's `__bench` seam dump, plus the D0 control cell;
  `spot` and `one` are unchanged and reproduce the 008 output at the
  engine commit they were measured on.
- `watermark`: experiments 009a and 009b; runs against the branch via its
  path dependency. `cargo run --release` is 009a (the dump-hook extension
  and the `bind_deletion` fix land with it); its episodes replay the
  engine, so it reproduces the 009a tables only on the phase-11 tree it
  was measured on (~35 min single-threaded). `cargo run --release --
  composed` is 009b, added at its own commit: the same episodes against
  the composed-rules engine plus the composed bar/gauntlet replay,
  byte-identical across reruns at that commit (~2 h single-threaded).
  Decision 71 removed the `watermark_dump` hook with the weighting it
  measured, so the crate no longer compiles against the current branch;
  its measurements describe the retired estimator.
