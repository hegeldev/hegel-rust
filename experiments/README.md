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
