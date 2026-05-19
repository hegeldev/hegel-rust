RELEASE_TYPE: patch

This release improves an internal invariant: the native backend (`--features native`) now records an enclosing span around every schema it interprets, so the shrinker can see compound draws as logical units.
