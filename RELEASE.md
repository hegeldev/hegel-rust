RELEASE_TYPE: patch

This patch changes libhegel's C ABI so that *finality* — performing the final
replay of a discovered counterexample — is owned entirely by the caller rather
than the engine. The Rust API (`hegel()`, `Hegel`, `draw()`, …) is unchanged;
this only affects consumers driving the engine through libhegel's C ABI (e.g.
hegel-go).

A libhegel run now only *explores* — database replay, generation, and
shrinking. Every test case it hands out via `hegel_next_test_case` is a
non-final exploration case; the engine no longer pumps a "final replay" of each
counterexample back through the run loop. Instead, each interesting test case in
the run result carries a reproduce blob, and the caller performs the final
replay itself by constructing a test case with `hegel_test_case_from_blob`.

As a result:

- `hegel_test_case_is_final_replay` is removed. A run case is always non-final;
  a caller that wants to treat a replay as final does so by replaying a blob it
  obtained from `hegel_failure_reproduction_blob`.
- `hegel_failure_panic_message` is removed. Because the engine never runs the
  test body for the counterexample, it has no panic message to report — only
  the `hegel_failure_origin` it grouped on and the reproduce blob. The caller
  obtains the message by replaying the blob.
