RELEASE_TYPE: patch

This patch adds `hegel_context_set_output` and `hegel_context_unset_output`,
which redirect engine-emitted output (verbose / debug progress traces and
warnings) to a caller-supplied callback instead of stderr
([#355](https://github.com/hegeldev/hegel-rust/issues/355)):

```c
void deliver(void *user_data, const char *line, size_t len) { ... }

hegel_context_set_output(ctx, deliver, my_logger);
```

The callback is invoked once per line of output, together with the registered
`user_data` pointer passed through verbatim, so a binding can deliver engine
output to its own test logger (say, a Go `testing.T`) with a single
library-wide callback.

A run or standalone test case inherits the destination registered on the
context it was created with, and every later call that passes a context
together with the run or one of its test cases re-resolves the destination
from that context: a callback registered on it is used instead of the
inherited one, and a context with no callback falls back to the inherited
destination — so a run keeps printing to its creation-time callback when
driven through fresh (say, thread-local) contexts. `hegel_context_unset_output`
(or a NULL callback) unsets a context's callback. The callback must be safe
to invoke from libhegel's worker thread; see the header documentation for the
full contract.
