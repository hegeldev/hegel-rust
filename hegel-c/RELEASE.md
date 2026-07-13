RELEASE_TYPE: minor

This release lets a caller redirect engine-emitted output (verbose / debug
progress traces and warnings) to a callback instead of stderr, by choosing the
destination per run or test case at creation
([#355](https://github.com/hegeldev/hegel-rust/issues/355)).

`hegel_run_start` and `hegel_test_case_from_blob` each take a new
`hegel_output_callback_t callback` and `void *user_data` before the
out-parameter. The callback is invoked once per line of output, with
`user_data` passed through verbatim, so a binding can deliver engine output to
its own test logger (say, a Go `testing.T`). A NULL `callback` keeps the
output on stderr.

```c
void deliver(void *user_data, const char *line, size_t len) { ... }

/* before */
hegel_run_start(ctx, settings, &run);
hegel_test_case_from_blob(ctx, settings, blob, &tc);

/* after */
hegel_run_start(ctx, settings, deliver, my_logger, &run);
hegel_test_case_from_blob(ctx, settings, blob, deliver, my_logger, &tc);
```

The destination is fixed when the run or test case is created — the engine
emits from its worker thread, and a run's output starts flowing the instant it
starts, so a per-call setter could not capture it without a race. For a run,
the callback (and whatever `user_data` points to) must stay valid until the run
is freed; for a blob replay, whose only line is emitted during the creating
call, it need not outlive that call. See the header documentation for the full
contract.
