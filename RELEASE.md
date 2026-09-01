RELEASE_TYPE: patch

This patch fixes a silent size collapse in `generators::recursive()` values whose branch generator stacks several combinators per recursion level (e.g. `one_of!` arms pairing subtrees with `tuples!`). Each combinator layer opens a span, and the engine's span-depth guard sat low enough that deep values were discarded as invalid, collapsing typical sizes by roughly 7x. The guard now sits far above any depth legitimate generation reaches.

`compose!` now also accepts the `move` keyword on its closure; captures were already by move, so `compose!(move |tc| { .. })` is identical to `compose!(|tc| { .. })`.
