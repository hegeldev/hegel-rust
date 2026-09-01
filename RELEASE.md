RELEASE_TYPE: minor

This release changes when stateful invariants run. `#[invariant]` methods previously ran after every rule; they now run in full on the machine's initial and final state, and are sampled in between — after any given rule, each invariant runs with probability 1/`stateful_step_count`. This keeps an invariant's expected cost per test case constant as the step count grows, and a violation that persists to the end of a test case is still always caught; what is given up is observing most intermediate states, so a violation a later rule *undoes* is only caught when a sampled check lands inside the window. This applies to both sequential and concurrent state machines.

This release also fixes a silent size collapse in `generators::recursive()` values whose branch generator stacks several combinators per recursion level (e.g. `one_of!` arms pairing subtrees with `tuples!`). Each combinator layer opens a span, and the engine's span-depth guard sat low enough that deep values were discarded as invalid, collapsing typical sizes by roughly 7x. The guard now sits far above any depth legitimate generation reaches.

`compose!` now also accepts the `move` keyword on its closure; captures were already by move, so `compose!(move |tc| { .. })` is identical to `compose!(|tc| { .. })`.
