RELEASE_TYPE: patch

This patch consolidates the engine's per-run bookkeeping into a single recording path, mirroring the structure of Hypothesis's `ConjectureRunner.test_function`. Previously each phase (generation, database reuse, targeting, span mutation) kept its own copy of the counter updates, which had already let the same accounting bug appear in two places.

Three small behavioural unifications come with it, all matching Hypothesis: database-reuse replays and targeting trials now count toward the same budgets as generated examples (and feed the choice tree, so generation starts informed by what replays explored); span mutation only runs once the health-check warm-up is over; and generation is skipped entirely when a database replay already reproduced a failure, so known failures are reported as fast as possible.
