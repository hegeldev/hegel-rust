RELEASE_TYPE: minor

This release makes Hegel handle nondeterministic tests instead of refusing them. A test whose structure or outcome changes when the same generated data is replayed — hidden global state, time, an outside service — previously aborted the run with a flaky-test error. Now the run switches to nondeterministic handling: failures are confirmed by repeated replay, shrunk without lowering how reliably they reproduce, persisted to the database, and reported with a caveat quoting the run's replay evidence, e.g. "nondeterministic failure, confirmed: failed 7 of 9 replays this run". Their reproduce blobs encode the failing timeline with its replay pool, and `#[hegel::reproduce_failure]` replays them until a replay fails rather than judging them stale after a single attempt.

The new `nondeterminism_strictness` setting controls the reaction: `Quiet` (the default) switches silently, `Warn` prints a one-line notice, and `Error` aborts the run as before, for suites that use determinism as a lint.

Concurrent state-machine tests get the same handling: their failures are confirmed by replay, shrunk, and reported with a reproduce blob and caveat like any other nondeterministic failure.
