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

This patch also unifies libhegel's C ABI on a single calling convention. Every
function except `hegel_context_new` now takes a `hegel_context_t*` as its first
argument and returns a `hegel_result_t` code (`HEGEL_OK` is zero; negatives are
errors). Anything a call previously returned — a handle, a string, a count, a
status enum — is now written through a trailing out-parameter named `out_*`.
This affects every consumer driving the engine through the C ABI (e.g.
hegel-go):

- The handle constructors no longer return the handle directly:
  `hegel_settings_new`, `hegel_run_start`, and `hegel_test_case_from_blob` take
  an `out_*` parameter and return a `hegel_result_t`. `hegel_context_new` is the
  sole exception and still returns its handle.
- The setters, the frees (`hegel_settings_free`, `hegel_run_free`,
  `hegel_test_case_free`, `hegel_context_free`), the result-inspection getters
  (`hegel_run_result_status` / `_error` / `_failure_count` / `_failure`,
  `hegel_failure_origin` / `_reproduction_blob`), `hegel_context_last_error`,
  and `hegel_version` all gain a leading context and return a `hegel_result_t`,
  delivering any value through an `out_*` parameter.
- `hegel_next_test_case` no longer overloads a NULL return for both "finished"
  and "error". The run is finished when it returns `HEGEL_OK` with the out
  handle set to NULL; a non-`HEGEL_OK` code is a real error. The idiomatic loop
  is now `while (hegel_next_test_case(ctx, run, &tc) == HEGEL_OK && tc != NULL)`.
