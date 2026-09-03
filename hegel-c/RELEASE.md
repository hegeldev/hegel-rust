RELEASE_TYPE: minor

This release adds engine-side nondeterministic handling — failures confirmed by repeated replay before being shrunk or persisted, shrinking guarded by a statistical bound on how much reproduction reliability a step can trade away, unconfirmed failures still reported with a caveat — controlled by the new `hegel_settings_set_nondeterminism_strictness` (quiet by default; error restores the old abort). Two ABI changes for frontends:

- `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` is retired: a failing nondeterministic run reports plain `HEGEL_RUN_STATUS_FAILED`, and its failures carry the new per-failure caveat (`hegel_failure_caveat`) quoting the run's replay evidence; NULL for a deterministic failure.
- The engine now runs every reported failure one final time before the run concludes, with the case stamped via `hegel_test_case_is_nondeterministic` (which now also stamps confirmation and database-reuse replays). Clients should capture stamped cases' output per origin and report each failure from the freshest capture instead of replaying its blob after the run.

Concurrent state machines (a concurrency bound above 1) now run under nondeterministic handling instead of a separate regime: creation always succeeds rather than rejecting the run's first test case, and concurrent failures are confirmed, shrunk, and carry reproduce blobs and caveats like any other nondeterministic failure.

Reproduce blobs for nondeterministic failures use a new self-identifying format carrying the failing timeline and its replay pool; older engines safely reject the new one. The new `hegel_run_start_blob` replays a blob as a run — a nondeterministic blob replays its stored timelines until one fails — and is what reproduce-failure features should use; `hegel_test_case_from_blob` still replays both formats as a single attempt.
