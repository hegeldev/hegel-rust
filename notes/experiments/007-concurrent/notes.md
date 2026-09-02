# Experiment 007: clone streams and concurrency under ND handling

Status: complete (full experiment run after phase 6, alongside the phase 7
unification).

## Smoke: nd_force on a clone-bearing body (2026-09-02)

Body: draw a boolean, `clone_stream`, draw a boolean from the clone,
fail iff both are true. Run with `nd_force = true`, phases
Generate+Shrink.

Result: the full ND pipeline completes. The discovery sweep confirms the
origin (for_probe replays reproduce the failure through the Clone-tag
choice), the gauntleted shrink reduces it, and the run reports one
failure with a reproduce blob carrying the Clone tag. Pinned as
`nd_handling_confirms_and_shrinks_a_clone_bearing_body` in
`test_runner_tests.rs`.

## Full experiment (2026-09-02)

Setup: a standalone frontend binary built against the branch
(`experiments/concurrent-replay/`, driven by its `drive.py`), one process
per run, a fresh temp database per trial, 20 trials per workload. Two
workloads:

- **racy**: a `#[hegel::concurrent_state_machine]` counter whose rule does
  a load/yield/store increment (lost updates) with a `value == increments`
  invariant, `run_concurrent(m, tc, 2, 4)`. Genuine scheduling
  nondeterminism; discovery confirmation anchors observed around 5/6.
- **clone**: a plain body drawing an integer in 0..=1000 through
  `tc.clone()`, failing on `x >= 500` only every third call (a
  process-global counter). Deterministic choices, flaky outcome — isolates
  round-trip fidelity from scheduling noise.

Each trial: discovery run (`print_blob`) → database-reuse run on the same
directory → 3 blob replays via `Hegel::reproduce_failure`.

| workload | discovery | DB reuse | blob replay |
|----------|-----------|----------|-------------|
| racy     | 20/20 (median 0.61s) | 20/20 | 60/60 |
| clone    | 20/20 (median 0.02s) | 20/20 | 60/60 |

All failures reported the confirmed caveat and a v2 blob; no flakiness or
generation-mismatch complaints.

### Blob replay had to be routed through the replay primitive

Before the fix, `reproduce_failure` on the racy blob reproduced 4/30: the
frontend replayed one case via `hegel_test_case_from_blob` (incumbent
only, single attempt), and the shrunk minimal schedule fires its race only
~13% of the time in a fresh process. Phase 4's plan ("v2 replays route
through the replay primitive like DB reuse") had only been implemented for
the database path. Fixed by `hegel_run_start_blob`: the engine replays the
blob as a run — deterministic blobs once, ND blobs through
`nd_reproduce` (pool first-fit, splices, no fresh tier — a fresh case
could fail for an unrelated reason) with stamped cases. After: 30/30 and
the 60/60 above.

### Clone-kind round-trip: values-only accepted

`serialize_choices` tag 5 stores clone children as bare values;
deserialization yields `CloneRecord::from_values` (no realized nodes). The
only consumer of realized prefix nodes is `resolve_choice`'s is-simplest
check, which fires solely on constraint drift — a verbatim replay never
consults it. Measured: clone-workload replays re-raise the stored shrunk
value exactly (all failures report x = 500/502, never a regenerated
draw), at ceiling rates cross-run. Disposition: values-only serialization
kept; on constraint drift a stale stored value puns to `unit()` instead
of `simplest()`, which costs nothing observable at these rates.

### Splice safety across clone boundaries

Structural: `nd_reproduce` splices cut timelines at top-level positions
and a whole clone stream is a single `ChoiceValue::Clone` element, so a
splice can never tear a clone record. Pinned by
`a_positional_splice_carries_whole_clone_records_across_intact`
(test_runner_tests.rs): a splice recombining a clone-bearing pair replays
the crossed-over record verbatim.

### Decision 14 (per-stream span-anchored splicing): closed

Whole-timeline pool replay reproduces both workloads at ceiling, with the
positional splice tier as rescue (exercised in engine tests, including
across clone boundaries). No data supports building span-anchored
per-stream grafting; reopen only if a real workload shows pool + splices
missing at meaningful rates.

### Boost and clone streams

Observed anchors sat well above the 0.5 reliability floor (G2), so boost
never engaged; nothing suggests it needs to descend into clone streams.
Left whole-timeline.

### G3(a): no-tree generation cost

Discovery medians above (0.61s racy, 0.02s clone) include confirmation
batches and the gauntleted shrink; disabling the data tree under ND
handling showed no pathology on these workloads.
