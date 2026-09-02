# Experiment 007: clone streams and concurrency under ND handling

Status: early smoke only (run immediately after phase 3, per the plan).
The full experiment — removing the concurrent shrink gate
(`test_runner.rs` 509-era gate, now `!concurrent`) and the first-case
handling, plus clone splice safety — runs after phase 6.

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

Open for the full experiment: `serialize_choices` Clone encoding (tag 5,
values only) loses realized clone kinds on DB round-trip (phase 4 notes
this); splice-tier safety across clone boundaries; concurrent-machine
(`max_concurrency > 1`) bodies, which stay in the legacy blobless regime
until this experiment concludes.
