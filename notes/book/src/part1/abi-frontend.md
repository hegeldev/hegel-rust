# The C ABI and the frontend

The frontend and every other language binding drive the engine through the
`hegel_*` functions in `hegel-c/src/lib.rs`. The checked-in header
`hegel-c/include/hegel.h` is generated from that file by cbindgen, so header
comments mirror the rustdoc. This chapter describes the branch's ABI break as a
client sees it, the capture contract that replaces client-side blob replay, how
clone streams cross the ABI and reassemble into one replayable timeline, and how
the Rust frontend turns the run result into a failure report. Mechanism
internals — the confirmation machinery, blob byte formats, the final replay —
live in [the lifecycle](lifecycle.md), [persistence](persistence.md), and [the
final replay](final-replay.md).

## The break: a migration view

hegel-c sits at 0.34.1 with `RELEASE_TYPE: minor` pending, making the release
the "0.35 ABI break" the retired-status rustdoc names. The whole break, across
`hegel.h` and `lib.rs`, is 369 insertions and 138 deletions. The symbol-level
changes:

| Change | Symbol | Disposition |
|---|---|---|
| Retired | `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` (value 3) | Removed from `hegel_run_status_t`; value reserved forever |
| Renamed | `hegel_test_case_is_nondeterministic` | Now `hegel_test_case_should_capture`; no compatibility shim |
| Added | `hegel_settings_set_nondeterminism_strictness` | With new enum `hegel_nondeterminism_strictness_t` |
| Added | `hegel_run_start_blob` | Replays a reproduce blob as a run; freed with `hegel_run_free` |
| Added | `hegel_failure_caveat` | Per-failure `const char*`, NULL for a deterministic failure |
| Added | Blob prefixes 2/3 | The self-identifying v2 ND format ([persistence](persistence.md)) |

**Retired.** On main, run status 3 meant a concurrent-machine failure: no blob,
no shrinking, no final replay, report from client capture. A failing
nondeterministic run now reports plain `HEGEL_RUN_STATUS_FAILED`, and
`hegel_run_result_status` documents the three-value set. The value is retired
rather than recycled: "Value 3 … is retired and must never be reused for a new
meaning: bindings built against the old header may still compare against it
(decision 27)". Decision 27 chose a per-failure caveat accessor over a run-level
status because caveat standing is per-origin information a run status cannot
carry, rejecting both reuse of value 3 with changed semantics and a v2 status. Decision 43 closed the bindings survey: the ts and ocaml bindings
never adopted status 3, go's handling of it is dead code but harmless, and cpp
vendors the header so it gets a compile-time migration signal.

One retirement is behavioural rather than symbolic: `hegel_new_state_machine` no
longer rejects the run's first `max_concurrency > 1` case with `HEGEL_E_ASSUME`.
Creation always succeeds, and the engine switches the run into nd handling at
the end of the first executed case that makes such a creation, whatever the
configured strictness — the concurrency was asked for. A binding that
special-cased the sacrificed first case can delete that path.

**Renamed.** `hegel_test_case_is_nondeterministic` became
`hegel_test_case_should_capture` with no shim, so bindings compiled against the
old header get a compile-time error on their next header sync (decision 50). The
rename is semantic: the stamp marks an execution the client should capture, not
a nondeterministic one — deterministic runs stamp their final replay and
first-check replays too.

**Added.** `hegel_settings_set_nondeterminism_strictness` takes
`HEGEL_NONDETERMINISM_QUIET = 0` (the default), `WARN = 1`, or `ERROR = 2`. What
each does is [detection](detection.md)'s subject. `hegel_run_start_blob` replays
a reproduce blob as a run driven exactly like `hegel_run_start`: a reproducing
replay is the run's failure, a run with no failures means the blob is stale, and
an undecodable blob surfaces as the run's error from `hegel_run_result`.
`hegel_failure_caveat` returns the failure's standing under the run's
nondeterministic handling, quoting the run's own replay evidence, valid until
`hegel_failure_free`. The `HegelFailure` snapshot carries origin, blob, and
caveat as lossy CStrings (interior NULs become U+FFFD via `cstring_lossy`).

**What a binding must now do.** The migration, in order of consequence:

1. Stop branching on status 3. `FAILED` is the only failing status, and value 3
   must never get a new meaning.
2. Rename `is_nondeterministic` call sites to `should_capture` and change what
   the answer drives: read the stamp once at case start, buffer the case's
   output and, on failure, its rendered diagnostic, keyed by the failure's
   origin. Report each failure from the freshest capture instead of replaying
   its blob after the run — the engine now runs the final replay itself.
3. Read `hegel_failure_caveat` per failure and print it alongside the report. A
   failure may carry a caveat and no blob (a caveat-only report); the report
   must not assume a blob exists.
4. Route reproduce-failure features through `hegel_run_start_blob`, which
   handles both blob formats and replays until a replay fails.
   `hegel_test_case_from_blob` remains for embedders as a documented single
   attempt — an ND blob replays there only its incumbent timeline, so the header
   steers callers to the run-based entry point.
5. Expose the strictness setting to users.

## The capture contract

The stamp is the ABI's replacement for main's capture-at-discovery stash.
`hegel_test_case_should_capture` answers whether the engine stamped this case:
"a stamped failing execution is the material for that origin's failure report".
The engine stamps the executions whose failures can become the report:

- the report-time final replay, and each generation-discovered failure's
  first-check replays — both on deterministic runs too;
- every `hegel_run_start_blob` replay;
- under nd handling: confirmation batches, database-reuse replays, and
  generation-phase cases, whose failing origins may be reported unconfirmed
  (decision 49).

Shrink-gauntlet and boost replays stay unstamped — capturing and symbolising
output for every discarded probe is the dominant cost of failing-heavy runs,
which is why capture happens at confirmation rather than discovery (decision
10). Two documented gaps remain bare: gauntlet-discovered origins, and the case
that itself flips the run (decision 49). Engine-side, the stamp is fed by the
`capture_replays` and `capture_discoveries` flags in
`hegel-c/src/native/test_runner.rs` through `set_should_capture` in
`test_function_tagged`. The header's instruction to "read the stamp once at case
start" holds because the engine stamps before the case starts, so the answer is
stable for the case's lifetime. Blob-replay cases are always stamped.

The contract closes over the report: `build_report` (test_runner.rs) gives blobs
and caveats only to origins past confirmation, applying that filter before the
sort and the single-failure truncation so a leaked unconfirmed origin can never
displace a confirmed one (decision 35). Deterministic failures get
choice-sequence blobs and a NULL caveat. When nothing confirmed survived,
unconfirmed origins are reported caveat-only with no blob (decisions 3 and 24).
The run-status rustdoc ties the client's end together: "A blobless failure is
reported from what the caller captured while running the stamped test cases".

## Clone streams and concurrency across the ABI

A test-case handle may be driven by at most one thread at a time, and concurrent
operations on one return `HEGEL_E_CONCURRENT_USE`. To generate from several threads,
the client calls `hegel_test_case_clone` and gives each thread its own clone.
The clone shares the test case's outcome and budgets but generates from its own
choice sequence. Collections, pools, and state machines are family-wide across
all handles: a collection used from two threads at once errors with
`HEGEL_E_CONCURRENT_USE`, while a pool holds an internal lock and serialises,
accepting that a shared pool couples workers' streams. `hegel_mark_complete` is
first-caller-wins across the family and never returns `HEGEL_E_CONCURRENT_USE`.

For a concurrent state machine, `hegel_new_state_machine` draws the concurrency
level in `[min_concurrency, max_concurrency]` (weighted toward the maximum —
concurrency bugs need concurrency) and the caller must run exactly that many
workers. The root handle drives `hegel_state_machine_next_group` at every join
point. Each worker draws its rules from its own clone via
`hegel_state_machine_next_rule`, identified by `worker_index` rather than by
handle because one OS thread could hold several clones. Draws consult only
per-worker and per-clone state, so draws on one worker never affect draws on
another.

**How clone streams reassemble into one replayable sequence.** Cloning does not
fork the recorded history. `clone_stream` (hegel-c/src/native/core/state.rs)
records a single Clone node at the parent stream's current position and gives
the child its own node vector and a spawned RNG. When the family concludes,
`reassemble` recursively freezes each child into a `RealizedStream`, yielding
one self-contained tree-shaped choice sequence — the timeline the nd machinery
pools, splices, and replays. There is no cross-thread ordering or timestamp
merging: a handle is only cloned from its owning thread, so where a Clone node
anchors is schedule-independent, and only the values within each stream are
replayed. The cross-thread interleaving of side effects is sampled fresh on
every execution: schedules are sampled, never replayed. That asymmetry is why a
shrunk racy failure reproduces only sometimes on exact replay (experiment 007)
and why the pool and splice machinery, not exact replay, carries concurrent
reproduction.

Serialisation preserves the shape. Choice tag 5 is Clone, recursing with the
same count-then-entries layout, values only (spans and kinds are recreated on
replay), with nesting bounded by `MAX_CLONE_DEPTH = 100`
(hegel-c/src/native/core/mod.rs). Splices cut whole timelines at top-level
positions, so a clone stream crosses over intact (decision 31).

Output stays deterministic the same way. Each test-case handle owns one print
region of the family document: the root handle's region is the document body,
and a clone's region is a hole opened in its parent's region at the moment the
clone was made, so a clone's output appears at its anchor point however the
threads were scheduled. The frontend's worker threads clone per round, anchoring
each round's output where the round began, and tag lines with a
`[worker N +X.XXXms]` prefix carrying the worker's thread-local index and time
offset (src/stateful.rs, src/test_case.rs) — pinned by
`a_rounds_lines_group_by_worker` in `tests/test_concurrent_stateful.rs`.

## How the frontend builds a report

`src/ffi.rs` is the only frontend module touching the raw `hegel_*` functions.
`SettingsHandle::build` forwards every setting, including
`set_nondeterminism_strictness`. `print_blob` is deliberately not forwarded —
the engine always returns the blob and printing is a frontend decision.
`RunHandle::start_blob` is infallible from the wrapper: an undecodable blob is
the run's error, read off the result. `CTestCase::should_capture` wraps the
stamp query, and `RunResult::failure(index)` copies origin, blob, and caveat out
via `hegel_failure_origin` / `_reproduction_blob` / `_caveat`, freeing the
failure snapshot immediately. The frontend `Failure` carries `origin`,
`reproduce_blob: Option<String>`, and `caveat: Option<String>`.

`run_test_case` (src/run_lifecycle.rs) reads the stamp once — `stamped =
!is_final && c_tc.should_capture()` — and gates backtrace capture on `((is_final
|| stamped) && !quiet) || verbose`, since capturing and symbolising backtraces
for every discarded shrink probe is the dominant cost of failing-heavy property
runs. On a caught panic in a stamped or final case it renders a diagnostic block
mirroring the default Rust panic handler and derives the origin as `"Panic at
{location}"`. The outcome goes back through `hegel_mark_complete`, with message
and blob travelling via the run result, not the completion call.

`drive_run` pumps `next_test_case`, buffers each case's output through a
per-case sink, and stores a `CapturedReport` for every interesting case:
buffered draw and note lines, the optional diagnostic, and the caught panic
payload. Replacement is rank-gated by `capture_rank` — diagnostic 2, lines-only
1, bare 0 — with a new capture replacing the stored one only at rank at or above
the stored rank, so newer wins at equal rank: final replay over confirmation
over discovery (decision 37). The payload travels with its capture so the
re-raised panic always matches the printed diagnostic. When the final replay is
dry, the printed lines are the freshest stamped failing execution while the blob
carries the shrunk incumbent.

On `FAILED`, each failure prints as one block in order: captured lines,
diagnostic, `note: {caveat}` when a caveat is present (suppressed at Quiet
verbosity, as is the multi-failure header "Property-based test failed with
{count} distinct failures."), then the reproducer line. A failure with no
capture is an internal error. The run ends by re-raising the last failure's own
panic payload, or a count-carrying panic when there are multiple distinct
failures. `reproducer_line` prints only when `Settings::print_blob` is enabled
and the failure carries a blob, emitting the
`#[hegel::reproduce_failure("{blob}")]` attribute text. Caveat-only ND failures,
SingleTestCase runs, and blob replays print nothing.

`drive_blob_replay` backs `#[hegel::reproduce_failure]`: it starts the run
through `RunHandle::start_blob` and reuses `drive_run`, so a reproducing replay
fails the test with its own panic, while a passing run panics with the stale
message naming both hypotheses — the failure may have been fixed, or a
nondeterministic blob may not have recurred within the replay budget ("a bug
failing 10% of the time escapes it about 5% of the time").

## The tests pinning the ND user experience

The user-visible contract is pinned by frontend integration tests in `tests/`:

- `tests/test_flaky_replay.rs` —
  `a_vanishing_failure_is_still_reported_under_quiet_strictness` shows a
  fail-once body still failing the run and re-raising the test's own panic
  message. `error_strictness_aborts_a_vanishing_failure_as_flaky` runs the same
  body under Error and panics with "Flaky test detected".
  `a_confirmed_but_dry_failure_prints_its_confirmation_capture` calibrates a
  hidden counter to the engine's execution schedule and asserts the report
  prints the stamped confirmation capture's draw lines and diagnostic plus the
  "not reproduced at report time" caveat, not the empty capture of the one
  unstamped shrink probe. The file's doc comment is the terse statement of
  decision 3's user contract.
- `tests/test_concurrent_stateful.rs` — the concurrent surface end to end:
  `quiet_nondeterministic_runs_stay_quiet_but_still_fail`,
  `a_worker_panic_is_reported_with_its_real_origin_and_buffered_output`,
  `a_stale_blob_on_a_concurrent_test_reports_that_it_did_not_reproduce`,
  `an_unconfirmed_one_shot_failure_reports_its_discovering_case` (decision 49's
  stamped discoveries), `a_run_with_max_concurrency_one_stays_deterministic`
  (the declared bound, not the drawn level, is what flips),
  `a_verbose_nondeterministic_run_streams_every_cases_output_live`, and the
  panic-precedence pin
  `a_panic_that_loses_to_an_engine_side_conclusion_is_discarded`.
- `tests/test_flaky_global_state.rs` — the minimal hidden-global-state body
  under all three strictness values, including the
  `#[hegel::test(nondeterminism_strictness = ...)]` attribute spelling.

Beyond the Rust frontend, the branch's own C-ABI coverage includes a test
replaying an ND blob through `hegel_run_start_blob` (added in the review-fix
commit 48894dc4, which also aligned the should-capture, reproduce-blob, and
blob-replay docs across both crates and regenerated the header).
