# Changelog

## Unreleased

The engine now suppresses every health check when running inside Antithesis (detected via `ANTITHESIS_OUTPUT_DIR`, the same signal that selects the urandom backend). Health checks are heuristics tuned for ordinary environments; under Antithesis, thread pausing and CPU throttling stretch wall-clock time so `TooSlow` fires on healthy tests, and the platform's steering of `/dev/urandom` similarly skews the statistics the other checks rely on. Explicit `suppress_health_check` settings continue to apply outside Antithesis.

## 0.33.1 - 2026-08-26

This patch improves the value distribution of bounded integer generation for large and full-width ranges, so that property tests surface off-by-one, overflow, and sign bugs far more readily.

Drawing an integer from a wide range now chooses between four categories of value — the range endpoints and their inner neighbours (`min`, `max`, `min + 1`, `max - 1`); the curated "interesting" values (zero, ±1, small magnitudes, powers of two, type limits); the diffuse pool of large constants; and the ordinary middle of the range — using a mixture whose weights are drawn afresh for each test case from a Dirichlet distribution (a form of *swarm testing*, after Groce et al., ISSTA 2012). Most test cases stay middle-dominated, so ordinary values remain the common case, while a lumpy minority concentrate on one special category.

Splitting the range endpoints into their own category, and drawing the weights per test case rather than fixing them, is what makes interactions that need *several* operands to be extreme at once reachable. Both operands of an expression drawn in an endpoint-heavy case land on `{min, max, …}` together, so — measured over 20,000 full-width `i64` cases — `x + y` now overflows about 1.4% of the time, where a fixed per-value boundary probability would reach it only at roughly its square. A boundary value appears about 2% of draws (up from about 0.3% before) and a small value about 3% (up from about 0.6%), now clustered in the cases that exercise them rather than spread thinly across every draw.

The special values still shrink toward zero and the simplest endpoints, and narrow ranges are unaffected. Mirrors the boundary-value injection Hypothesis performs (see [#350](https://github.com/hegeldev/hegel-rust/issues/350) and hypothesis#4722).

This patch also fixes the distribution of unbounded width-32 float draws, which previously came out infinite about a third of the time. Finite draws whose magnitude exceeds `f32::MAX` are now remapped into the finite `f32` range instead of rounding to infinity, and out-of-range draws are no longer clamped to `-inf` when a bound is infinite. Large finite `f32` magnitudes (above `f32::MAX / 2`) now appear about 20% of the time instead of essentially never, and infinities about 2%, matching the width-64 shape. The `-inf` clamp fix is width-agnostic: unbounded width-64 draws with NaN or subnormals disallowed previously collapsed a small fraction of draws to `-inf`, and no longer do.

This patch also improves shrinking through a size binding (the `flat_map`-into-collection pattern) for large test cases. When lowering the size value realises a shorter test case, the shrinker now only tries deleting regions of exactly the realised deficit — the candidates that can realign the sequence — instead of every smaller region too, so a single large binding can no longer exhaust the shrinker's stall budget before it reaches a winning candidate.

## 0.33.0 - 2026-08-20

This release replaces the stateful-testing interface with one that also supports concurrent stateful testing.

Every state machine — sequential or concurrent — is now driven by the same round-based protocol. Creating the machine draws the number of workers the caller must run; the owning thread advances rounds on the root test-case handle and checks the machine's invariants at the join points between rounds, while no rule is running:

```c
hegel_new_state_machine(ctx, tc, rule_names, rule_groups, num_rules,
                        invariant_names, num_invariants,
                        min_concurrency, max_concurrency,
                        &machine, &concurrency);
/* spawn `concurrency` worker threads, each holding its own clone handle */

while (true) {
    /* root handle: ask whether to run another round, and for which group */
    hegel_state_machine_next_group(ctx, tc, machine, &group);
    if (group == HEGEL_STATE_MACHINE_DONE) break;

    /* wake every worker, then wait for all of them to finish the round */

    /* the join point: every worker is parked — check the invariants */
}
/* tell the workers to exit */
```

Each worker thread `w`, woken once per round, pulls rules on its own clone handle `tc_w` until the engine signals its join point, then parks until the next round:

```c
while (true) {
    hegel_state_machine_next_rule(ctx, tc_w, machine, w, &rule);
    if (rule == HEGEL_STATE_MACHINE_DONE) break;  /* w's round is over */
    /* apply rules[rule]; if it fails an assumption, report it with
       hegel_state_machine_rule_rejected(ctx, tc_w, machine, w) */
}
```

Each rule is assigned to a concurrency group at machine creation, and for each round the engine draws a random group: only that group's rules are handed out for the round, so rules in the same group may run concurrently with each other and rules in different groups never overlap. A sequential machine is a degenerate case — all-zero `rule_groups`, concurrency bounds `1, 1`, no worker threads at all, and the root handle drives everything: the engine hands out exactly one rule per round, so the two loops collapse into the old rule loop on one thread with a `hegel_state_machine_next_group` call between rules.

This is a breaking C ABI change:

- `hegel_new_state_machine` gains `rule_groups` (group ids parallel to `rule_names`) and `min_concurrency`/`max_concurrency` parameters, plus an `out_concurrency` out-parameter: the engine draws the machine's concurrency level in `[min_concurrency, max_concurrency]` and writes it to `*out_concurrency`. The caller must run exactly that many workers. Passing `min_concurrency == max_concurrency` fixes the level. Groups are identified by arbitrary `int64_t` ids and carry no names: the machine has one concurrency group per distinct value of `rule_groups` (`INT64_MIN` is rejected — it is reserved as the `HEGEL_STATE_MACHINE_DONE` sentinel). Creating the machine now draws from the calling handle's stream, so it can return `HEGEL_E_STOP_TEST`, which the caller should report as an overrun.
- `hegel_state_machine_next_rule` gains a `worker_index` parameter and now hands out rules for one round at a time; the frontend must advance rounds with `hegel_state_machine_next_group` (new), even for sequential machines.
- `hegel_state_machine_rule_rejected` gains the same `worker_index` parameter, and its accounting is per worker: it refers to the rule most recently handed to that worker.
- `HEGEL_STATE_MACHINE_DONE` changes value from `-1` to `INT64_MIN`, so the sentinel sits outside the group-id space callers plausibly use. A frontend that compares `hegel_state_machine_next_rule`'s `out_rule_index` against a hard-coded `-1` instead of the named constant must update.

Creating a state machine with `max_concurrency > 1` declares the run nondeterministic. The engine detects this itself, so frontends have nothing to set up front. A nondeterministic run can't be replayed, so discovery is the only chance to capture. Frontends should check for nondeterminism at test case start, with the new `hegel_test_case_is_nondeterministic`, and capture a nondeterministic case's whole trace — draws, notes, and any failure's diagnostics — while it runs. Only the most recent capture needs to be stored, because the run will end as soon as a failure is found. The engine skips everything that assumes deterministic replay: data-tree recording (and with it novel-prefix generation and the nondeterminism mismatch check), span mutation, the verify and shrink pass (and with it the flakiness check), targeting, and database persistence and reuse.

A failing nondeterministic run reports itself with a new run status, `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` (other `hegel_run_status_t` values are unchanged). Frontends should skip any final replay they normally perform, print no reproducer, and report the bug from what they captured at discovery. A failing single-test-case run still reports plain `HEGEL_RUN_STATUS_FAILED`, since its caller reports from the case's own execution anyway.

The `stateful_step_count` setting now bounds *rounds*. At concurrency 1 each round is exactly one rule and a round whose rule was rejected does not count, so sequential budgets behave as before.

The choice-sequence shape of sequential stateful tests changes as a result (the stop decision moves to the round boundary, and the swarm disabling probability is drawn at machine creation rather than at the first selection), so stored database entries and reproduce blobs for stateful tests are invalidated: stale database entries replay as invalid or overrun and are deleted quietly, while stale blobs fail loudly.

## 0.32.5 - 2026-08-13

This patch improves shrinking for stateful tests that use variable pools. Previously adding a variable to a pool could be hard to remove if other variables in the pool were used after that point. This should no longer be the case.

This patch also fixes shrinking sometimes stopping just short of the simplest failing example. When the randomized escape passes burned through the shrinker's stall budget without progress, the exhausted budget silently discarded every later candidate, so the deterministic passes' final round could no longer normalize the result it was handed. Each scheduling round now starts with a fresh stall budget, so the deterministic passes always get a real final pass over the shrunk example.

## 0.32.4 - 2026-08-13

This patch makes shrinking much cheaper and better at escaping locally-minimal branches: finding the minimal counterexample for a failing test typically takes around 10x fewer test executions, and inputs whose true minimum sits in another `one_of`-style branch reach it more often than before. Individual runs are still randomized, so a particular seed may shrink differently than it used to, but every deterministic benchmark result is unchanged and the branch-escape rate is higher across the board.

## 0.32.3 - 2026-08-11

This patch fixes `hegel_test_case_from_blob` ignoring the `stateful_step_count` setting ([#396](https://github.com/hegeldev/hegel-rust/issues/396)). A stateful counterexample that needed more than 50 steps did not reproduce.

## 0.32.2 - 2026-08-10

This patch improves the generation phase's span mutation. A mutated choice sequence that diverges from its donor's path previously ran out of data and was discarded as an overrun; mutation probes now draw randomly past the end of the spliced choices, so a diverged proposal becomes a complete test case seeded with the mutation. On recursive-generator workloads this turns a substantial fraction of previously wasted probes into productive test cases.

## 0.32.1 - 2026-08-07

This patch fixes a bias in data generation where some choices were made with the wrong probability.
The most visible effect should be the stateful tests should run a full set of steps more often.
Collection sizes may also be affected.

## 0.32.0 - 2026-08-07

This release adds `hegel_state_machine_rule_rejected(ctx, tc, state_machine)`. Frontends are now responsible for calling this when the rule most recently returned by `hegel_state_machine_next_rule` was rejected before it completed.

## 0.31.0 - 2026-08-07

This release replaces the `int64_t` ids for collections, variable pools, and state machines with opaque caller-owned handles, matching how every other libhegel object works. This is a breaking change to the C ABI.

`hegel_new_collection`, `hegel_new_pool`, and `hegel_new_state_machine` now write a handle (`hegel_collection_t*`, `hegel_pool_t*`, `hegel_state_machine_t*`) through their out-parameter instead of an id, and `hegel_collection_more`, `hegel_collection_reject`, `hegel_pool_add`, `hegel_pool_generate`, and `hegel_state_machine_next_rule` take that handle instead of an id (still alongside the test-case handle, whose stream the draws come from — any clone of the creating test case works, as before). Each handle must be released with its new matching free function — `hegel_collection_free`, `hegel_pool_free`, or `hegel_state_machine_free` — exactly once. The handles are independent of the test case and run they were created under, so the frees are safe in any order, including after `hegel_run_free`. A NULL handle is reported as `HEGEL_E_INVALID_HANDLE`, and the ids' `HEGEL_E_INVALID_ARG` "unknown id" errors are gone.

```c
/* before */
int64_t collection;
hegel_new_collection(ctx, tc, 0, 10, &collection);
hegel_collection_more(ctx, tc, collection, &more);

/* after */
hegel_collection_t *collection;
hegel_new_collection(ctx, tc, 0, 10, &collection);
hegel_collection_more(ctx, tc, collection, &more);
hegel_collection_free(ctx, collection);
```

The threading contract is now per object rather than per family. A collection may be driven by at most one thread at a time: concurrent use reports `HEGEL_E_CONCURRENT_USE`, like a test-case handle. Pools and state machines may be shared between clone handles driven from parallel threads; their operations serialize internally. This also removes a hidden serialization point — collection and pool operations from parallel clones previously contended on one family-wide lock even when touching different objects.

Internal refactoring of the engine's choice-sequence representation. Choice constraints and values are now carried as a single paired type, removing a large class of internal panics on impossible constraint/value combinations; the on-disk choice serialization and reproduce-blob formats are unchanged.

libhegel no longer installs a process-global panic hook. Violated internal invariants of the engine (bugs in hegel itself) no longer panic: during a draw they now report `HEGEL_E_INTERNAL` with the bug-report diagnostic in `hegel_context_last_error`, and during the engine's own exploration (generation, mutation, shrinking) they finish the run with a run-level error read back through `hegel_run_result_error` — exactly where a caught engine panic's message went before. Applications that install their own panic hook no longer have libhegel's hook chained in front of it, and unloading libhegel with `dlclose` no longer leaves a dangling hook behind.

Run-scoped client mistakes no longer abort the process either: an embedding that resumes the engine without concluding the offered test case, and a process launched with `ANTITHESIS_OUTPUT_DIR` pointing at a missing directory, now finish the run with a run-level error naming the mistake, read back through `hegel_run_result_error`.

All of the engine's operating-system access — the failure database's file I/O, the monotonic clock behind the shrink deadline and the TooSlow health check, PRNG seeding entropy, `/dev/urandom` reads, environment lookups, and stderr output — now goes through one narrow internal platform layer that talks to the OS directly (raw syscalls on Linux, kernel32/bcryptprimitives on Windows) instead of through `tempfile` and `rand`'s thread-local generator. This removes more of the thread-local state that made unloading libhegel with `dlclose` unsafe. Two observable details change: the failure database writes each value through a uniquely named `<value>.tmp.<pid>.<counter>` sibling file before its atomic same-directory rename (previously a randomly named temporary), and engine stderr output is written straight to the stderr file descriptor, so in-process capture that only intercepts a language runtime's own printing (such as the Rust test harness's output capture) no longer sees it.

The engine's locking now goes through that platform layer too, replacing `parking_lot` and the standard library's locks with a futex-backed mutex and a lock-free lazy initialiser. With those gone, none of libhegel's own code or dependencies registers thread-local storage or thread-exit destructors — the main sources of the crashes seen when a thread that used libhegel outlives a `dlclose` of it.

Unloading libhegel with `dlclose` is now safe: no code path in the library registers a thread-local destructor, an atexit hook, or any other process-global pointer into the library, so nothing is left behind to dangle after unload. The crate is now `#![no_std]`, and a new off-by-default `runtime` cargo feature builds a fully self-contained library that does not link the Rust standard library at all: `cargo build -p hegeltest-c --no-default-features --features runtime` with `RUSTFLAGS="-C panic=abort"` produces a `libhegel` with no thread-local-storage machinery in its dynamic symbol table, no TLS segment, and no dynamic exports outside the `hegel_*` API (CI now verifies all three on a `--release`-profile build). The default build still links the standard library for its allocator and panic support; its engine paths register no thread-local state either, but the standard library's own runtime remains present in the binary, so embedders who want the strongest unload guarantee should use the `runtime` build. The prebuilt `libhegel-<goos>-<goarch>` release assets are still the default (standard-library-linked) build, so for now the `runtime` configuration means building from source.

This release also changes what happens when libhegel itself has a bug. Violated internal engine invariants are reported as `HEGEL_E_INTERNAL` or as run-level errors, as described above; a residual panic that slips past that reporting — which would indicate a further bug in hegel — now aborts the process at the library boundary instead of being caught and converted into the run-level error previously prefixed `"Engine panic:"`. No unwind ever crosses the C ABI, and a corrupted engine can no longer keep handing out results.

## 0.30.5 - 2026-08-04

This patch adds `hegel_settings_set_stateful_step_count`, which sets the target number of steps a stateful test case runs (default 50).

The stateful stop generation decision has changed. Instead of drawing a single per-case step cap up front, `hegel_state_machine_next_rule` makes a per-step stop decision, forced to keep going before the first step and forced to halt once `stateful_step_count` steps have been handed out. Every stateful case therefore runs at least one step and at most `stateful_step_count`.

## 0.30.4 - 2026-08-04

Internal preparation for making the engine core `no_std`-compatible: the engine's hash tables now use `hashbrown` and its floating-point math now goes through the `libm` crate instead of the platform math library, and the unused `crc32fast` dependency is gone. `libm` can round differently from a platform math library in the last bit, so on some platforms fixed-seed runs may generate different values than previous releases. Stored failures are unaffected: database replay is value-based, and seed reproducibility has always been build-specific.

## 0.30.3 - 2026-07-27

This patch simplifies hegeltest-c's build step.

## 0.30.2 - 2026-07-27

This patch fixes an engine panic during shrinking. When the span-reordering pass ran against a test case with several groups of same-label sibling spans, and reordering one group produced an improvement that shortened the recorded span list, the pass went on to index the remaining groups with positions from the old, longer list. The run then aborted with `Engine panic: index out of bounds` instead of reporting the shrunk counterexample (re-running the test recovered via the failure database, but the first run's result was lost). Span groups are now re-validated against the current spans after each improvement, and groups that no longer exist are skipped.

## 0.30.1 - 2026-07-21

This patch removes libhegel's background worker thread. `hegel_run_start` no longer spawns a thread: the engine is suspended inside the run handle, and each `hegel_next_test_case` call runs it on the calling thread until it hands over the next test case. The API is unchanged, but the threading behaviour is simpler:

- Output callbacks are now invoked on whichever thread calls `hegel_next_test_case`, rather than from a separate engine thread.
- Engine work between test cases (generation, mutation, shrinking) now happens inside `hegel_next_test_case`, where the caller previously blocked waiting for the worker to do the same work; total run time is unchanged, minus two thread context switches per test case.
- `hegel_run_start` can no longer fail to spawn a thread, and `hegel_run_free` no longer has a worker to wind down — freeing a run mid-run simply drops the rest of the exploration.

This makes libhegel usable in environments where spawning threads is unavailable or awkward.

## 0.30.0 - 2026-07-20

This release moves control over the lifecycle of stateful tests into the
engine. Frontends no longer draw a step cap up front; instead, they poll for
rules from the engine until they receive a termination signal. This is
necessary groundwork for future work on concurrent stateful testing and better
shrinking.

The signature for requesting the next rule is unchanged, but termination is now
indicated by setting `out_rule_index` to `HEGEL_STATE_MACHINE_DONE`:

```c
hegel_result_t hegel_state_machine_next_rule(hegel_context_t *ctx,
                                             hegel_test_case_t *tc,
                                             int64_t state_machine_id,
                                             int64_t *out_rule_index);
```

## 0.29.0 - 2026-07-13

This release lets a caller redirect engine-emitted output (verbose / debug
progress traces and warnings) to a callback instead of stderr, by choosing the
destination per run or test case at creation
([#355](https://github.com/hegeldev/hegel-rust/issues/355)).

`hegel_run_start` and `hegel_test_case_from_blob` each take a new
`hegel_output_callback_t callback` and `void *user_data` before the
out-parameter. The callback is invoked once per line of output, with
`user_data` passed through verbatim, so a binding can deliver engine output to
its own test logger (say, a Go `testing.T`). A NULL `callback` keeps the
output on stderr.

```c
void deliver(void *user_data, const char *line, size_t len) { ... }

/* before */
hegel_run_start(ctx, settings, &run);
hegel_test_case_from_blob(ctx, settings, blob, &tc);

/* after */
hegel_run_start(ctx, settings, deliver, my_logger, &run);
hegel_test_case_from_blob(ctx, settings, blob, deliver, my_logger, &tc);
```

The destination is fixed when the run or test case is created — the engine
emits from its worker thread, and a run's output starts flowing the instant it
starts, so a per-call setter could not capture it without a race. For a run,
the callback (and whatever `user_data` points to) must stay valid until the run
is freed; for a blob replay, whose only line is emitted during the creating
call, it need not outlive that call. See the header documentation for the full
contract.

## 0.28.0 - 2026-07-08

This release fixes a number of correctness bugs found in a full review of the engine, hardens the C ABI against misuse, and improves generation and shrinking performance.

Breaking C ABI changes:

- `hegel_settings_set_mode`, `hegel_settings_set_backend`, `hegel_settings_set_verbosity`, and `hegel_mark_complete` now take their enum-valued parameter as a validated `uint32_t` instead of the enum type itself. Passing an out-of-range value is now a reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior in the library. C callers passing the enum constants are source-compatible and just need a recompile against the new header.
- `hegel_settings_set_suppress_health_check` now *replaces* the set of suppressed checks on each call, like `hegel_settings_set_phases`, instead of accumulating across calls (which made it impossible to clear a suppression). Callers that relied on accumulation should OR their bits together into a single call.
- `hegel_next_test_case`, `hegel_run_result`, `hegel_test_case_from_blob`, and `hegel_test_case_clone` now check the handle before the out parameter, so passing both as NULL returns `HEGEL_E_INVALID_HANDLE` rather than `HEGEL_E_INVALID_ARG`, consistent with every other function.

Generation fixes:

- Strings generated from regex patterns now actually match patterns using `\b`, `\B`, or `$`/`\Z` in non-final positions (previously the anchors were ignored, so e.g. most strings generated for `\bfoo\b` contained no match), and fullmatch generation no longer emits lookaround assertion bodies into the output. Atomic groups and possessive repeats re-validate their output against the pattern, and `(?i)` negated character classes exclude the full case-folding closure of their members.
- A string generator whose alphabet is empty with `max_size = 0` — a legal configuration whose only value is the empty string — no longer crashes the engine on its first test case.
- Times and datetimes drawn near a bound expressed with chrono's leap-second representation could exceed the bound; such bounds are now rejected up front (except the end-of-day leap second, which remains fully supported).

Shrinking and replay fixes:

- Fixed an engine panic when a shrink pass revisited an integer node whose kind had changed under it mid-pass.
- The pre-shrink verification run now requires the failure to reproduce with the *same* origin. Previously a test that panicked at a different location on replay could be reported under the wrong origin with a reproduction blob that did not reproduce it; it is now correctly reported as a flaky test.
- Several shrink passes are substantially more effective per invocation: the length-redistribution passes can move more than one element at a time, the adaptive deletion pass's leftward walk accumulates across accepted steps, and string truncation binary-searches instead of trying every length.
- The targeting phase no longer corrupts its hill-climbing steps for byte values wider than 128 bits.
- Database replay no longer runs an example twice when it is stored under both the primary and secondary keys, and a stored counterexample that replays with different values no longer skips the shrink phase just because it realised the same length.

Performance: regex `.` and negated-literal draws, string-constant injection, and codepoint lookups no longer rescan their alphabets on every drawn character, and the per-draw choice-configuration clone in the draw hot path is gone.

Diagnostics: test-case handle errors (`HEGEL_E_INVALID_HANDLE`, `HEGEL_E_ALREADY_COMPLETE`, `HEGEL_E_CONCURRENT_USE`) now record a message on the context like every other handle family, and the header documentation has been corrected in several places (the `hegel_pool_generate` empty-pool result is `HEGEL_E_ASSUME` and callers may recover from it like any failed assumption, `hegel_settings_new` defaults are CI-dependent, run handles are single-threaded while settings handles document their share-after-configuring contract, and `hegel_date_t` spans the proleptic year range its draws actually use).

## 0.27.1 - 2026-07-08

This patch tightens argument validation on two C ABI draws so they reject
inconsistent configurations that were previously accepted, matching the checks
the native generator builders already enforce:

- `hegel_generate_float` now returns `HEGEL_E_INVALID_ARG` for `allow_nan=true`
  with a finite `min_value` or `max_value` (which otherwise drew NaN outside the
  stated range), and for `allow_infinity=true` with both bounds finite (a silent
  no-op).
- `hegel_new_collection` now returns `HEGEL_E_INVALID_ARG` when
  `min_size > max_size`, instead of silently accepting the request with undefined
  sizing. Oversized-but-satisfiable requests are still left to the existing
  choice-budget overrun path.

## 0.27.0 - 2026-07-06

This release adds inclusive `min_value` / `max_value` bounds to
`hegel_generate_date`, `hegel_generate_time`, and
`hegel_generate_datetime` (a breaking signature change). Pass
`{1, 1, 1}` / `{9999, 12, 31}` and all-zeros / `{23, 59, 59, 999999}`
for the conventional full ranges.

Dates are proleptic Gregorian with `year` in `[-999999, 999999]` and
draw as a single day offset centred on 2000-01-01 (clamped into range),
mirroring Hypothesis's `DateStrategy`, so bounded dates keep the
2000-01-01 shrink target. Times draw as a single microsecond offset
shrinking toward `min_value`, mirroring `TimeStrategy`; previously they
drew four separate components. Datetimes draw a bounded date, then a
time whose bounds tighten to the endpoint times when the drawn date
lands on a boundary date. Invalid calendar dates, out-of-range time
fields, and inverted bounds are rejected with `HEGEL_E_INVALID_ARG`.

Because the underlying choice sequences changed shape, failure
databases and reproduce blobs from earlier versions will not replay
against these draws.

## 0.26.0 - 2026-07-06

This release replaces the CBOR schema protocol with typed draw functions.
`hegel_generate` — which took a CBOR-encoded schema and returned a
CBOR-encoded value — is gone, along with the entire schema vocabulary.
In its place the ABI now exposes one function per foundational generator:

- `hegel_generate_integer` draws an integer in `[min, max]`, and
  `hegel_generate_integer_big` does the same for bounds beyond `int64_t`
  (two's-complement little-endian byte encodings in and out). The big
  variant sign-fills the output buffer beyond the value's minimal
  encoding, so a caller can read the whole buffer as a fixed-width
  two's-complement integer without doing its own sign extension.
- `hegel_generate_float` takes the full float specification directly:
  width (32 or 64), bounds, NaN/infinity policy, exclusive-bound flags,
  and the smallest nonzero magnitude.
- `hegel_generate_bytes` returns an engine-allocated buffer
  (`hegel_generate_bytes_result_t`) that the caller frees with
  `hegel_generate_bytes_result_free`.
- `hegel_generate_boolean` replaces `hegel_primitive_boolean` (same
  signature). It now draws from the handle's own stream, matching every
  other draw; previously it drew from the family's root stream even on a
  cloned handle.
- String generation goes through opaque `hegel_string_generator_t`
  handles built by typed constructors — `hegel_string_generator_text`
  (codec / codepoint bounds / Unicode categories / include & exclude
  characters), `hegel_string_generator_regex` (with an optional text
  generator as its alphabet), `hegel_string_generator_email`,
  `hegel_string_generator_url`, and `hegel_string_generator_domain`.
  Constructors validate all their parameters immediately, so a bad
  pattern or alphabet is reported at construction rather than mid-draw.
  A generator is immutable after construction, may be shared freely
  across test cases and threads, and is released with
  `hegel_string_generator_free`. `hegel_generate_string` draws through a
  handle and returns an engine-allocated, length-prefixed UTF-8 buffer
  (`hegel_generate_string_result_t`; not NUL-terminated, may contain
  interior NULs) that the caller frees with
  `hegel_generate_string_result_free`.
- `hegel_generate_date`, `hegel_generate_time`, and
  `hegel_generate_datetime` return structured values (`hegel_date_t`,
  `hegel_time_t`, `hegel_datetime_t`) instead of ISO-formatted strings;
  `hegel_generate_uuid` writes the UUID's 16 big-endian bytes (with an
  optional forced RFC 4122 version nibble) and `hegel_generate_ipv4` /
  `hegel_generate_ipv6` write the address's network-order bytes (4 and
  16 respectively).

To migrate a binding, replace each schema construction + `hegel_generate`
call with the corresponding typed call. For example, a bounded integer
draw goes from building `{"type": "integer", "min_value": 0,
"max_value": 100}` as CBOR and decoding the CBOR response to:

```c
int64_t n;
hegel_result_t rc = hegel_generate_integer(ctx, tc, 0, 100, &n);
```

Compound client-side generators (tuples, lists, dictionaries, unions)
should compose the typed draws using the existing span
(`hegel_start_span`/`hegel_stop_span`) and collection
(`hegel_new_collection`/`hegel_collection_more`) primitives, which are
unchanged. New `hegel_label_t` values document the spans the engine now
emits internally around its own draws (`HEGEL_LABEL_REGEX` through
`HEGEL_LABEL_STRING`).

Failure databases and reproduce blobs written by earlier versions will
not replay against generators using the new draw functions (the database
has never been stable across upgrades).

## 0.25.0 - 2026-07-06

This release changes `hegel_test_case_clone` to hand out an *independent
stream* of the test case rather than a view onto the same choice sequence.
A clone still shares the test case's outcome — `hegel_mark_complete` on any
handle completes the whole family, and the choice budget is shared — but it
generates from its own choice sequence, so clones can be driven
concurrently from different threads without perturbing each other, and the
values every stream produces are deterministic under replay and shrink
correctly. Previously concurrent clone draws interleaved into one shared
sequence, which was explicitly non-deterministic.

Each cloned stream is recorded as a single choice in the stream it was
cloned from, so cloning now consumes one choice position on the source
handle, takes the source handle's lock like a draw (it can return
`HEGEL_E_CONCURRENT_USE` on contention), and fails with
`HEGEL_E_ALREADY_COMPLETE` once the test case has completed, where it
previously succeeded and returned a dead handle. Reproduce blobs now encode
the cloned streams' choices alongside their parent's, so blobs from tests
that clone are not readable by older libhegel versions.

Collections, variable pools, and state machines remain shared across the
family — ids from one handle work on any other — but concurrent use of one
such object from two streams makes the affected values scheduling-dependent.

## 0.24.0 - 2026-07-03

This release adds primitives for cloning test-case handles, and clears up the semantics of concurrent use of test cases so that a single test-case handle may not be used concurrently, but clones may. In addition, it changes all of the handle types to be caller-owned and freed by the caller.

This is a breaking change for callers of `hegel_next_test_case`. Previously a run-owned handle was freed by the run, and calling `hegel_test_case_free` on it returned `HEGEL_E_INVALID_HANDLE`; now the caller owns it and must free it.
Run results and failures follow the same caller-owned rule, which is also breaking.
