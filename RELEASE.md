RELEASE_TYPE: patch

The native backend (`--features native`) no longer hangs when a `text()`/`binary()` generator has a very large `max_size`. The index-based shrink passes now skip string and bytes nodes entirely, deferring to the dedicated length-reduction and per-element passes that already handle them.
