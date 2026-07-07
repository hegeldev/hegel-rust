RELEASE_TYPE: patch

This patch adds `hegel_context_set_output`, which redirects engine-emitted
output (verbose / debug progress traces and warnings) for the runs and test
cases created with a context to a caller-supplied callback instead of stderr
([#355](https://github.com/hegeldev/hegel-rust/issues/355)):

```c
void deliver(void *user_data, const char *line, size_t len) { ... }

hegel_context_set_output(ctx, deliver, my_logger);
```

The callback is invoked once per line of output, together with the registered
`user_data` pointer passed through verbatim, so a binding can deliver engine
output to its own test logger (say, a Go `testing.T`) with a single
library-wide callback. Passing a NULL callback resets the context to stderr.
The destination is snapshotted when a run or blob replay is created, and the
callback must be safe to invoke from libhegel's worker thread; see the header
documentation for the full contract.
