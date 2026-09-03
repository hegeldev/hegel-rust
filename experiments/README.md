# Experiment harnesses (frozen)

One-off harnesses behind the nondeterminism work, kept for the record
alongside their write-ups in `notes/experiments/`. They are not workspace
members and are not maintained: each was built against the commit noted
below, and later API changes are expected to have broken some of them.

- `shrink-sim`, `nd-shrink`, `replay-semantics`, `nd-lifecycle`,
  `nd-boost`, `confirm-bar`, `fixate-cost`: built against `a1d1b6d2`.
  `fixate-cost` and `replay-semantics` no longer build — their `__bench`
  entry points (`fixate_cost_experiment`, `replay_once`) were deleted when
  the branch went to production grade.
- `concurrent-replay`: built against `7a4fd194` (experiment 007's full
  campaign); runs against the branch via its path dependency —
  `cargo build --release`, then `python3 drive.py [trials]`.
- `gauntlet-calibration`: built against `c68a89eb` (experiment 008's
  phase-12 in-engine spot check); drives the engine through the public
  C ABI via its path dependency — `cargo run --release -- spot`.
- `watermark`: experiment 009a, built at its own commit (the dump-hook
  extension and the `bind_deletion` fix land with it); runs against the
  branch via its path dependency — `cargo run --release` reproduces the
  notes' tables on stdout, byte-identical across reruns (~35 min
  single-threaded).
