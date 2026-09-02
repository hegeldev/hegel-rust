RELEASE_TYPE: minor

This release adds engine-side nondeterministic handling — failures confirmed by repeated replay, shrunk without lowering reproduction probability, persisted and reported like any other failure — controlled by the new `hegel_settings_set_nondeterminism_strictness` (quiet by default; error restores the old abort). Two ABI changes for frontends:

- `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` is retired: a failing nondeterministic run reports plain `HEGEL_RUN_STATUS_FAILED`, and its failures carry the new per-failure caveat (`hegel_failure_caveat`) quoting the run's replay evidence; NULL for a deterministic failure.
- The engine now runs every reported failure one final time before the run concludes, with the case stamped via `hegel_test_case_is_nondeterministic` (which now also stamps confirmation and database-reuse replays, besides every case of a concurrent-machine run). Clients should capture stamped cases' output per origin and report each failure from the freshest capture instead of replaying its blob after the run.

Reproduce blobs for nondeterministic failures use a new self-identifying format carrying the failing timeline and its replay pool; `hegel_test_case_from_blob` replays both formats, and older engines safely reject the new one.
