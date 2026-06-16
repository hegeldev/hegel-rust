/*
 * libhegel — C bindings for Hegel's native property-based testing engine.
 *
 * This header is generated from hegel-c/src/lib.rs by cbindgen. Do not
 * edit it directly; re-run `just c-header` after changing the Rust source.
 *
 * Pointer ownership
 * -----------------
 * Every pointer you pass *into* a libhegel function stays owned by you: the
 * library reads it during the call and copies out whatever it needs to keep,
 * so you may free or reuse the memory as soon as the call returns. This
 * applies to strings (char*), CBOR byte buffers, and arrays of strings alike.
 *
 * Pointers libhegel returns *to* you are borrows into a handle and are only
 * valid until a stated point — each function documents its own lifetime (e.g.
 * the bytes from hegel_generate are invalidated by the next call on that test
 * case; result strings live until hegel_run_free). Copy them if you need them
 * longer. Opaque handles (hegel_context_t*, hegel_settings_t*, hegel_run_t*,
 * hegel_test_case_t* from hegel_test_case_from_blob) are owned by you and must
 * be released with the matching hegel_*_free function.
 */

#ifndef HEGEL_H
#define HEGEL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

/*
 How the engine should treat the run: a full property-test loop or a
 single test case.

 - `HEGEL_MODE_TEST_RUN`: the engine drives a full
   generate / shrink / replay loop until `max_examples` or the
   choice tree is exhausted.
 - `HEGEL_MODE_SINGLE_TEST_CASE`: the engine produces exactly one
   test case and stops, with no shrinking. Useful for replaying a
   stored counterexample or running an exploratory probe.
 */
typedef enum {
    HEGEL_MODE_TEST_RUN = 0,
    HEGEL_MODE_SINGLE_TEST_CASE = 1,
} hegel_mode_t;

/*
 Which source of randomness the engine draws from. Set via
 `hegel_settings_backend`.

 - `HEGEL_BACKEND_AUTO`: choose automatically (the default) —
   `HEGEL_BACKEND_URANDOM` when running inside Antithesis, otherwise
   `HEGEL_BACKEND_DEFAULT`.
 - `HEGEL_BACKEND_DEFAULT`: expand a single seeded PRNG. Runs are
   reproducible from the seed and shrinking / replay work as usual.
 - `HEGEL_BACKEND_URANDOM`: read fresh entropy from `/dev/urandom` on
   every draw (falling back to an OS-seeded PRNG on platforms without
   it). Intended for running under Antithesis, whose fuzzer controls
   `/dev/urandom`; you almost certainly don't want it otherwise.
 */
typedef enum {
    HEGEL_BACKEND_AUTO = 0,
    HEGEL_BACKEND_DEFAULT = 1,
    HEGEL_BACKEND_URANDOM = 2,
} hegel_backend_t;

/*
 Verbosity of engine-emitted output (logs, per-case traces). Set via
 `hegel_settings_verbosity`.

 - `HEGEL_VERBOSITY_QUIET`: nothing besides the final result.
 - `HEGEL_VERBOSITY_NORMAL`: a short summary line per run (default).
 - `HEGEL_VERBOSITY_VERBOSE`: per-test-case progress, drawn values
   for the final replay, panic diagnostics as they happen.
 - `HEGEL_VERBOSITY_DEBUG`: as verbose, plus Hypothesis-style
   shrinker trace output.
 */
typedef enum {
    HEGEL_VERBOSITY_QUIET = 0,
    HEGEL_VERBOSITY_NORMAL = 1,
    HEGEL_VERBOSITY_VERBOSE = 2,
    HEGEL_VERBOSITY_DEBUG = 3,
} hegel_verbosity_t;

/*
 Result of a fallible libhegel call.

 Every `int`-returning entry point (the per-test-case primitives, etc.)
 returns one of these. `HEGEL_OK` is zero; every error is negative, so
 `result != HEGEL_OK` (or `result < 0`) tests for failure. Handle-returning
 entry points signal failure with NULL instead. For the error variants that
 carry a diagnostic, the message is on the call's context — read it with
 `hegel_context_last_error()`.
 */
typedef enum {
    /*
     Success.
     */
    HEGEL_OK = 0,
    /*
     The engine has exhausted its choice budget for this test case and
     wants the caller to abort the body and return. Treat the same as a
     validly-completed test case.
     */
    HEGEL_E_STOP_TEST = -1,
    /*
     An `assume` / `reject` precondition failed. The current test case is
     invalid and should be discarded.
     */
    HEGEL_E_ASSUME = -2,
    /*
     The underlying engine reported an error. See
     `hegel_context_last_error()` for the diagnostic.
     */
    HEGEL_E_BACKEND = -3,
    /*
     A handle pointer (`hegel_settings_t*`, `hegel_run_t*`,
     `hegel_test_case_t*`, …) was NULL where it must be non-NULL.
     */
    HEGEL_E_INVALID_HANDLE = -4,
    /*
     An argument other than a handle was invalid — NULL where a value was
     required, malformed CBOR, non-UTF-8 string, etc. See
     `hegel_context_last_error()` for specifics.
     */
    HEGEL_E_INVALID_ARG = -5,
    /*
     `hegel_mark_complete` (or a primitive on the same handle) was called
     for a test case that has already been completed.
     */
    HEGEL_E_ALREADY_COMPLETE = -6,
    /*
     `hegel_next_test_case` was called without first completing the
     previous test case with `hegel_mark_complete`.
     */
    HEGEL_E_NOT_COMPLETE = -7,
    /*
     An internal invariant failed inside libhegel (e.g. CBOR
     re-serialisation). Should not happen in practice; please file a
     bug. See `hegel_context_last_error()` for the diagnostic.
     */
    HEGEL_E_INTERNAL = -8,
} hegel_result_t;

/*
 Outcome of a single test case. Passed to `hegel_mark_complete`.

 - `HEGEL_STATUS_VALID`: the test body ran to completion without
   finding an interesting outcome (the property held).
 - `HEGEL_STATUS_INVALID`: an `assume` / precondition rejected this
   draw; the engine should discard it without counting it against
   the test-cases budget.
 - `HEGEL_STATUS_OVERRUN`: the engine ran out of choice budget mid
   test case (typically because `hegel_generate` returned
   `HEGEL_E_STOP_TEST`); treat the case as inconclusive.
 - `HEGEL_STATUS_INTERESTING`: the property failed and this draw is
   a candidate counterexample. Pass a stable origin string to
   `hegel_mark_complete` so the shrinker can identify the bug.
 */
typedef enum {
    HEGEL_STATUS_VALID = 0,
    HEGEL_STATUS_INVALID = 1,
    HEGEL_STATUS_OVERRUN = 2,
    HEGEL_STATUS_INTERESTING = 3,
} hegel_status_t;

/*
 Aggregate outcome of a finished run, read via `hegel_run_result_status`.

 - `HEGEL_RUN_STATUS_PASSED`: the property held across every generated
   test case.
 - `HEGEL_RUN_STATUS_FAILED`: the property failed; inspect each distinct
   counterexample via `hegel_run_result_failure_count` /
   `hegel_run_result_failure`.
 - `HEGEL_RUN_STATUS_ERROR`: the run itself failed — a failed health
   check, a nondeterministic test, an engine panic — and produced no
   verdict on the property. There are no failures to inspect; the
   message is read via `hegel_run_result_error`.
 */
typedef enum {
    HEGEL_RUN_STATUS_PASSED = 0,
    HEGEL_RUN_STATUS_FAILED = 1,
    HEGEL_RUN_STATUS_ERROR = 2,
} hegel_run_status_t;

/*
 A phase of the property-test loop, used as a bit flag for
 `hegel_settings_phases`.

 `hegel_settings_phases` takes a bitwise OR of these values (e.g.
 `HEGEL_PHASE_GENERATE | HEGEL_PHASE_SHRINK`); the phases not included are
 disabled. The default is `HEGEL_PHASE_ALL`, which is almost always what you
 want — turning a phase off is mainly useful for debugging or replay tooling.
 */
typedef enum {
    /*
     Run hard-coded explicit examples (none today, reserved for future use).
     */
    HEGEL_PHASE_EXPLICIT = (1 << 0),
    /*
     Replay counterexamples persisted from previous runs (requires a
     database path + `hegel_settings_database_key`).
     */
    HEGEL_PHASE_REUSE = (1 << 1),
    /*
     Randomly generate fresh test cases up to the `test_cases` budget.
     */
    HEGEL_PHASE_GENERATE = (1 << 2),
    /*
     Apply hill-climbing toward observed `hegel_target` scores between
     generation rounds.
     */
    HEGEL_PHASE_TARGET = (1 << 3),
    /*
     Shrink discovered failing examples toward minimal counterexamples.
     */
    HEGEL_PHASE_SHRINK = (1 << 4),
    /*
     Convenience: all five phases enabled. This is the default.
     */
    HEGEL_PHASE_ALL = 31,
} hegel_phase_t;

/*
 A health check, used as a bit flag for
 `hegel_settings_suppress_health_check`.

 `hegel_settings_suppress_health_check` takes a bitwise OR of these values
 naming the checks to *disable*. The default is "all enabled"; suppress a
 check only when you understand why it is firing and accept the behavior.
 */
typedef enum {
    /*
     Aborts the run if too many draws are rejected via `assume` / `Invalid`
     (default threshold: 200 in a row with no valid case).
     */
    HEGEL_HC_FILTER_TOO_MUCH = (1 << 0),
    /*
     Aborts the run if individual test cases take so long that the overall
     run is impractical.
     */
    HEGEL_HC_TOO_SLOW = (1 << 1),
    /*
     Aborts the run if generated values are so large that retaining them for
     shrinking is impractical.
     */
    HEGEL_HC_TEST_CASES_TOO_LARGE = (1 << 2),
    /*
     Warns if the first generated test case is already disproportionately
     large.
     */
    HEGEL_HC_LARGE_INITIAL_TEST_CASE = (1 << 3),
} hegel_health_check_t;

/*
 Identifies what kind of compound structure a span groups, passed to
 `hegel_start_span` so the shrinker can choose appropriate shrink moves
 (e.g. shortening lists vs. simplifying individual list elements). Pick
 whichever label best describes the surrounding context. Mirrors
 `hegeltest::test_case::labels`.
 */
typedef enum {
    /*
     Outer span around a list / sequence.
     */
    HEGEL_LABEL_LIST = 1,
    /*
     One element of a list.
     */
    HEGEL_LABEL_LIST_ELEMENT = 2,
    /*
     Outer span around a set (unordered, no duplicates).
     */
    HEGEL_LABEL_SET = 3,
    /*
     One element of a set.
     */
    HEGEL_LABEL_SET_ELEMENT = 4,
    /*
     Outer span around a map / dictionary.
     */
    HEGEL_LABEL_MAP = 5,
    /*
     One (key, value) entry of a map.
     */
    HEGEL_LABEL_MAP_ENTRY = 6,
    /*
     Outer span around a tuple / fixed-arity record.
     */
    HEGEL_LABEL_TUPLE = 7,
    /*
     Outer span around a `one_of` / disjunction; useful so the shrinker
     can swap which branch is taken.
     */
    HEGEL_LABEL_ONE_OF = 8,
    /*
     Outer span around an `optional` (None vs Some(value)).
     */
    HEGEL_LABEL_OPTIONAL = 9,
    /*
     Outer span around a fixed-shape record (named fields known
     statically).
     */
    HEGEL_LABEL_FIXED_DICT = 10,
    /*
     Outer span around a `flat_map` / monadic dependent draw.
     */
    HEGEL_LABEL_FLAT_MAP = 11,
    /*
     Outer span around a `filter` / rejection-sampling wrapper.
     */
    HEGEL_LABEL_FILTER = 12,
    /*
     Outer span around a `map` / pure transformation.
     */
    HEGEL_LABEL_MAPPED = 13,
    /*
     Outer span around a `sampled_from` / pick-from-collection draw.
     */
    HEGEL_LABEL_SAMPLED_FROM = 14,
    /*
     Outer span around the variant discriminator of a sum-type draw.
     */
    HEGEL_LABEL_ENUM_VARIANT = 15,
    /*
     Span around one swarm-testing feature-flag draw. Emitted internally
     by the engine's state-machine rule selection
     (`hegel_state_machine_next_rule`); callers normally never open this
     span themselves.
     */
    HEGEL_LABEL_FEATURE_FLAG = 16,
} hegel_label_t;

/*
 Opaque error-reporting context.

 libhegel records the diagnostic for a failed call on a context the caller
 supplies, rather than in thread-local state. Thread-local error buffers
 are ill-defined under runtimes (e.g. Go) that migrate a goroutine between
 OS threads mid-call, so the message could be written on one thread and
 read on another; an explicit context sidesteps that entirely.

 Create one with `hegel_context_new`, pass it as the first argument to
 every fallible `hegel_*` call, read the most recent message with
 `hegel_context_last_error`, and free it with `hegel_context_free`. A
 context is cheap; the expected usage is one per test (or per thread).

 A single context must not be used concurrently from multiple threads —
 each fallible call overwrites the stored message, so sharing one across
 threads is a data race and unsupported. Passing `NULL` wherever a context
 is accepted is allowed and simply opts out of error messages: the call
 still returns its usual error code, there is just nothing to read back.
 */
typedef struct hegel_context_t hegel_context_t;

/*
 One distinct failure surfaced by the run. The strings are owned by
 the parent `hegel_run_result_t`; reading them via
 `hegel_failure_panic_message` / `_origin` returns `const char*`
 pointers that stay valid until `hegel_run_free`.
 */
typedef struct hegel_failure_t hegel_failure_t;

/*
 In-flight property-test run.

 `hegel_run_start` returns one of these. The caller pulls test cases
 out via `hegel_next_test_case` until it returns NULL, then reads the
 aggregated outcome via `hegel_run_result`, and finally frees the
 handle with `hegel_run_free`. The engine runs on a separate worker
 thread inside libhegel; the handle owns the channel that ferries
 test cases between caller and worker.
 */
typedef struct hegel_run_t hegel_run_t;

/*
 Aggregated outcome of a finished run, returned by
 `hegel_run_result`. Read the passed / failed / errored status via
 `hegel_run_result_status`, the number of distinct failures via
 `hegel_run_result_failure_count`, each failure via
 `hegel_run_result_failure(r, i)`, and — for an errored run — the
 run-level error message via `hegel_run_result_error`. The pointer is
 borrowed from the `hegel_run_t` and stays valid until `hegel_run_free`
 is called.
 */
typedef struct hegel_run_result_t hegel_run_result_t;

/*
 Settings handle for a libhegel run.

 Construct with `hegel_settings_new`, configure via the
 `hegel_settings_*` family of setters, hand to `hegel_run_start`, then
 free with `hegel_settings_free`. Settings can be reused across
 multiple runs; the engine reads them at `hegel_run_start` time.
 */
typedef struct hegel_settings_t hegel_settings_t;

/*
 One in-flight test case handed to the caller by
 `hegel_next_test_case` (borrowed from the run) or constructed
 standalone by `hegel_test_case_from_blob` (owned by the caller). The
 caller drives it with the per-test-case primitives (`hegel_generate`,
 `hegel_start_span` / `hegel_stop_span`, `hegel_target`, the collection
 primitives) and concludes it with `hegel_mark_complete`. A run-owned
 handle becomes invalid once marked complete; calling
 `hegel_next_test_case` again returns the next test case (or NULL when
 the run is finished). A standalone handle must be released with
 `hegel_test_case_free`.
 */
typedef struct hegel_test_case_t hegel_test_case_t;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/*
 Allocate a new error-reporting context initialised with an empty message.
 Never returns NULL. Must be paired with a `hegel_context_free` call.
 */
hegel_context_t *hegel_context_new(void);

/*
 Free a context previously returned by `hegel_context_new`. Safe to call
 with NULL (no-op).
 */
void hegel_context_free(hegel_context_t *ctx);

/*
 Most recent error message recorded on `ctx`, or the empty string if the
 most recent call taking this context succeeded. Returns NULL only when
 `ctx` itself is NULL.

 The returned pointer borrows `ctx`'s internal buffer and is invalidated by
 the next libhegel call that takes the same `ctx` — copy the bytes before
 making another such call.
 */
const char *hegel_context_last_error(const hegel_context_t *ctx);

/*
 Allocate a new settings handle initialised with libhegel's defaults
 (100 test cases, all phases enabled, normal verbosity, no seed,
 the default disk database under `.hegel/`). Must be paired with a
 `hegel_settings_free` call. Never returns NULL.
 */
hegel_settings_t *hegel_settings_new(void);

/*
 Free a settings handle previously returned by `hegel_settings_new`.
 Safe to call with NULL (no-op).
 */
void hegel_settings_free(hegel_settings_t *s);

/*
 Set whether the engine should drive a full run loop or stop after
 one test case. See `hegel_mode_t`.
 */
void hegel_settings_mode(hegel_settings_t *s, hegel_mode_t mode);

/*
 Select the engine's randomness backend. See `hegel_backend_t`.

 `HEGEL_BACKEND_AUTO` is the default and leaves the automatic choice in
 place; `HEGEL_BACKEND_DEFAULT` / `HEGEL_BACKEND_URANDOM` pin an explicit
 backend, overriding the automatic detection. Like the underlying setting,
 pinning is one-way: there is no way to un-pin back to AUTO on a handle
 once an explicit backend has been set.
 */
void hegel_settings_backend(hegel_settings_t *s, hegel_backend_t backend);

/*
 Maximum number of valid test cases to run before declaring the
 property held. The default is 100. Note that this counts *valid*
 cases — assumed-rejected ones don't count against the budget, but
 see `HEGEL_HC_FILTER_TOO_MUCH` for the limit on consecutive
 rejections.
 */
void hegel_settings_test_cases(hegel_settings_t *s, uint64_t n);

/*
 Set the engine's output verbosity. See `hegel_verbosity_t`.
 */
void hegel_settings_verbosity(hegel_settings_t *s, hegel_verbosity_t v);

/*
 Set the RNG seed. When `has_seed = true`, `seed` is used to
 initialise generation; when `has_seed = false`, the engine picks a
 fresh random seed at run start (the default). Combined with
 `hegel_settings_derandomize(s, true)` this gives reproducible runs.
 */
void hegel_settings_seed(hegel_settings_t *s, uint64_t seed, bool has_seed);

/*
 Make the run reproducible: derive the seed from a stable hash of
 `database_key` instead of fresh randomness when no explicit seed is
 supplied. Useful in CI where you want runs of the same test to be
 deterministic but different tests to still see different inputs.
 */
void hegel_settings_derandomize(hegel_settings_t *s, bool derandomize);

/*
 When `yes = true` (the default), the engine keeps generating after
 the first failure to surface additional *distinct* bugs (different
 origins), and the final `hegel_run_result_t` lists all of them.
 When `false`, the run stops after the first failing example.
 */
void hegel_settings_report_multiple_failures(hegel_settings_t *s, bool yes);

/*
 Configure the on-disk example database used by `HEGEL_PHASE_REUSE`
 and the auto-persistence path.

 - `database = NULL` → leave at the current value (default
   `.hegel/examples/` next to the cwd).
 - `database = ""` → disable the database entirely. Replay phase
   becomes a no-op and discovered failures are not persisted.
 - Otherwise → use the directory at `database` as the database root.
   The directory is created lazily.
 */
void hegel_settings_database(hegel_context_t *ctx, hegel_settings_t *s, const char *database);

/*
 Set the database key used to scope stored / replayed examples for this run.
 `key = NULL` clears it (the default).
 */
void hegel_settings_database_key(hegel_context_t *ctx, hegel_settings_t *s, const char *key);

/*
 Enable a specific set of phases, given as a bitwise OR of `hegel_phase_t`
 values. Phases not included are disabled. The default is `HEGEL_PHASE_ALL`.
 Passing 0 produces a run that does nothing.
 */
void hegel_settings_phases(hegel_settings_t *s, uint32_t phases);

/*
 Suppress (disable) a set of health checks, given as a bitwise OR of
 `hegel_health_check_t` values. The default is "no suppression"; use this
 when you know a check is going to fire and accept the underlying behavior
 (e.g. you intentionally have a high rejection rate).
 */
void hegel_settings_suppress_health_check(hegel_settings_t *s, uint32_t checks);

/*
 Start a property-test run with the given settings. Returns a handle
 the caller pulls test cases out of via `hegel_next_test_case`.

 The engine runs on a worker thread inside libhegel; this function
 returns immediately after spawning it. The caller does not need to
 hold the settings handle alive — `hegel_run_start` snapshots the
 settings it needs.

 Returns NULL on failure with a diagnostic in
 `hegel_context_last_error`. The returned handle must be freed with
 `hegel_run_free`.
 */
hegel_run_t *hegel_run_start(hegel_context_t *ctx, const hegel_settings_t *settings);

/*
 Block until the engine produces the next test case, returning a
 borrowed handle pointing into the parent `hegel_run_t`.

 The caller must complete the previous test case (via
 `hegel_mark_complete`) before requesting the next one — otherwise
 this returns NULL and sets `hegel_context_last_error`.

 Returns NULL when the run is finished; call `hegel_run_result` to
 read the outcome. A NULL with `hegel_context_last_error` set means
 something went wrong (engine crash, caller misuse) rather than
 normal completion.
 */
hegel_test_case_t *hegel_next_test_case(hegel_context_t *ctx, hegel_run_t *run);

/*
 Return the aggregated result of a finished run, borrowed from the
 parent `hegel_run_t`. Returns NULL with
 `hegel_context_last_error` set if the run hasn't finished yet
 (`hegel_next_test_case` has not yet returned NULL on this run).

 The pointer is valid until `hegel_run_free`.
 */
const hegel_run_result_t *hegel_run_result(hegel_context_t *ctx, hegel_run_t *run);

/*
 Free a run handle and its result. Safe to call with NULL.

 If the caller exited its test loop early (e.g. with a still-active
 test case), this drains the worker thread cleanly: any in-flight
 test case is marked complete, the abort flag is set so the worker
 short-circuits, and the worker is joined before the handle is
 destroyed.
 */
void hegel_run_free(hegel_run_t *run);

/*
 Build a standalone test case that replays the example encoded in a
 base64 failure blob (obtained from `hegel_failure_reproduction_blob` on a
 prior run).

 There is no run handle and no engine worker: the caller drives the
 returned test case with the usual per-test-case primitives
 (`hegel_generate`, spans, …), concludes it with `hegel_mark_complete`,
 and decides for itself whether the blob reproduced the failure (the
 property failed again) or is stale (it passed). Replay several blobs by
 calling this once per blob. A blob whose choices no longer match the
 caller's generators surfaces as `HEGEL_E_STOP_TEST` from the draw that
 overruns. `hegel_test_case_is_final_replay` reports true: the replayed
 example *is* the counterexample.

 Returns NULL with a diagnostic in `hegel_context_last_error` if `s` or
 `blob` is NULL, or if `blob` is not a valid failure blob (corrupt, or
 from an incompatible Hegel version). The returned handle is owned by
 the **caller** — unlike test cases from `hegel_next_test_case`, it must
 be released with `hegel_test_case_free`.
 */
hegel_test_case_t *hegel_test_case_from_blob(hegel_context_t *ctx,
                                             const hegel_settings_t *s,
                                             const char *blob);

/*
 Free a standalone test case previously returned by
 `hegel_test_case_from_blob`. Safe to call with NULL (no-op), and safe
 whether or not the test case was marked complete.

 Must NOT be called on a test case obtained from
 `hegel_next_test_case` — those are borrowed from the parent
 `hegel_run_t` and are released by `hegel_run_free`. Passing one here is
 detected (while the run is still alive) and refused, with a diagnostic
 in `hegel_context_last_error`.
 */
void hegel_test_case_free(hegel_context_t *ctx, hegel_test_case_t *tc);

/*
 Draw a value from the test case's data source, using the
 CBOR-encoded `schema_cbor` to describe its shape (type + bounds +
 optional category filters, depending on the type).

 On success returns `HEGEL_OK` and writes a borrowed pointer to the
 CBOR-encoded value into `*out_value_cbor` (length in
 `*out_value_len`). The pointer is invalidated by the next call into
 libhegel on this test case — copy the bytes if you need to keep
 them.

 Returns `HEGEL_E_STOP_TEST` when the engine's choice budget is
 exhausted for this test case (the caller should abort the body and
 call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 Returns `HEGEL_E_INVALID_ARG` on malformed schema, NULL outputs, or
 other argument errors; the diagnostic is in
 `hegel_context_last_error`.
 */
hegel_result_t hegel_generate(hegel_context_t *ctx,
                              hegel_test_case_t *tc,
                              const uint8_t *schema_cbor,
                              size_t schema_len,
                              const uint8_t **out_value_cbor,
                              size_t *out_value_len);

/*
 Open a labeled span around a group of draws so the shrinker can
 reason about them as a unit. Pair with exactly one
 `hegel_stop_span(tc, false)` call when the structure is complete.

 `label` is a `hegel_label_t` value for one of the well-known structure
 kinds, but the type is `uint64_t` rather than the enum because the label
 space is open: callers may pass any stable `u64` to tag their own span
 kinds (the engine treats unrecognised labels as opaque grouping keys).
 */
hegel_result_t hegel_start_span(hegel_context_t *ctx, hegel_test_case_t *tc, uint64_t label);

/*
 Close the most-recently opened span. Pass `discard = true` to mark
 the span as rejected (e.g. a `filter` predicate didn't hold and the
 engine should retry from before the span opened).
 */
hegel_result_t hegel_stop_span(hegel_context_t *ctx, hegel_test_case_t *tc, bool discard);

/*
 Start an engine-managed variable-length collection. The engine
 chooses how many elements to produce; the caller pulls them one at
 a time by calling `hegel_collection_more` in a loop. Pass
 `max_size = UINT64_MAX` for no upper bound.

 On success writes the new collection's id into `*out_collection_id`
 and returns `HEGEL_OK`. The id is opaque; pass it to subsequent
 `hegel_collection_more` / `hegel_collection_reject` calls.
 */
hegel_result_t hegel_new_collection(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    uint64_t min_size,
                                    uint64_t max_size,
                                    int64_t *out_collection_id);

/*
 Ask whether the engine wants another element in this collection.
 On success writes `true` or `false` into `*out_more` and returns
 `HEGEL_OK`. Call in a loop until `*out_more` is `false`, drawing
 the next element each time.
 */
hegel_result_t hegel_collection_more(hegel_context_t *ctx,
                                     hegel_test_case_t *tc,
                                     int64_t collection_id,
                                     bool *out_more);

/*
 Tell the engine the last element it produced for this collection
 is not acceptable (e.g. would create a duplicate in a set), so it
 should try a different one. `why` is an optional human-readable
 rejection reason (NULL is allowed).
 */
hegel_result_t hegel_collection_reject(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       int64_t collection_id,
                                       const char *why);

/*
 Create a new engine-managed *variable pool* for stateful testing.

 A pool tracks a set of opaque variable ids that the engine can draw
 from and shrink over — the primitive behind hegel-rust's
 `stateful::Variables` and `#[hegel::state_machine]`. The caller keeps
 its own mapping from variable id to the actual value it generated
 (mirroring how `Variables<T>` holds a `HashMap<i64, T>`).

 On success writes the new pool's id into `*out_pool_id` and returns
 `HEGEL_OK`. The id is opaque; pass it to subsequent `hegel_pool_add`
 / `hegel_pool_generate` calls on the *same* test case.
 */
hegel_result_t hegel_new_pool(hegel_context_t *ctx, hegel_test_case_t *tc, int64_t *out_pool_id);

/*
 Register a new variable in the pool. The engine assigns it a fresh
 id, which the caller associates with the value it just generated.

 On success writes the new variable's id into `*out_variable_id` and
 returns `HEGEL_OK`. `pool_id` must be an id returned by
 `hegel_new_pool` on this test case.
 */
hegel_result_t hegel_pool_add(hegel_context_t *ctx,
                              hegel_test_case_t *tc,
                              int64_t pool_id,
                              int64_t *out_variable_id);

/*
 Draw a variable id from the pool, letting the engine choose (and
 shrink) which previously-added variable to reuse. When
 `consume = true` the drawn variable is removed from the pool (model a
 destructive action); when `false` it stays available for future
 draws.

 On success writes the chosen variable id into `*out_variable_id` and
 returns `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` if the pool currently
 has no active variables — the caller should guard against that (e.g.
 only draw when it knows it has added at least one variable) or treat
 it like any other budget-exhaustion outcome.
 */
hegel_result_t hegel_pool_generate(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   int64_t pool_id,
                                   bool consume,
                                   int64_t *out_variable_id);

/*
 Register a *state machine* for engine-owned stateful (rule-based)
 testing: `num_rules` rules and `num_invariants` invariants, each
 identified by a NUL-terminated UTF-8 name. The engine owns rule
 selection — including swarm testing, where each test case enables a
 random subset of rules (at least one) and selection draws only from
 that subset. The caller drives execution: it asks
 `hegel_state_machine_next_rule` which rule to run at each step and
 applies it.

 On success writes the new machine's id into `*out_state_machine_id`
 and returns `HEGEL_OK`. The id is opaque; pass it to subsequent
 `hegel_state_machine_next_rule` calls on the *same* test case.
 Returns `HEGEL_E_INVALID_ARG` if `num_rules` is zero, or on null /
 non-UTF-8 names.
 */
hegel_result_t hegel_new_state_machine(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       const char *const *rule_names,
                                       size_t num_rules,
                                       const char *const *invariant_names,
                                       size_t num_invariants,
                                       int64_t *out_state_machine_id);

/*
 Draw the index of the next rule to run, in `[0, num_rules)`, letting
 the engine choose (and shrink) the rule sequence. Swarm testing is
 applied per test case: a random subset of rules is enabled on the
 first call and selection is restricted to that subset for the rest
 of the test case, with restrictions that shrink away in minimal
 counterexamples.

 On success writes the chosen rule index into `*out_rule_index` and
 returns `HEGEL_OK`. `state_machine_id` must be an id returned by
 `hegel_new_state_machine` on this test case. Returns
 `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted
 (the caller should abort the body and call `hegel_mark_complete`
 with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_next_rule(hegel_context_t *ctx,
                                             hegel_test_case_t *tc,
                                             int64_t state_machine_id,
                                             int64_t *out_rule_index);

/*
 Draw a single boolean that is `true` with probability `p`. `p`
 must be in `[0.0, 1.0]`; `p = 0.0` always yields `false` and
 `p = 1.0` always yields `true` without consuming entropy.

 When `has_forced` is `true` the result is forced to `forced`: the
 engine still records the choice (so replay and shrinking stay
 aligned) but consumes no entropy, and the shrinker will not flip it.
 Forcing `true` with `p = 0.0` or `false` with `p = 1.0` is
 contradictory and rejected.

 On success writes the drawn value into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice
 budget is exhausted for this test case (the caller should abort the
 body and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 Returns `HEGEL_E_INVALID_ARG` for a NULL `out_value`, a `p` outside
 `[0.0, 1.0]` (including NaN), or a contradictory forced value; the
 diagnostic is in `hegel_context_last_error`.
 */
hegel_result_t hegel_primitive_boolean(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       double p,
                                       bool forced,
                                       bool has_forced,
                                       bool *out_value);

/*
 Record a numeric observation under `label` for the engine's
 targeting phase to hill-climb toward. Higher values are "more
 interesting"; the engine biases later test cases toward inputs that
 produced higher observations under the same label. Has no effect
 unless `HEGEL_PHASE_TARGET` is enabled. `label` must be non-NULL
 and valid UTF-8.

 Returns `HEGEL_E_INVALID_ARG` (with a diagnostic in
 `hegel_context_last_error`) if `value` is not finite, or if `label`
 has already been observed on this test case — each label may be
 recorded at most once per case.
 */
hegel_result_t hegel_target(hegel_context_t *ctx,
                            hegel_test_case_t *tc,
                            double value,
                            const char *label);

/*
 Mark this test case complete with the given status.

 `origin` is used only when `status == HEGEL_STATUS_INTERESTING`; for
 other statuses it can be NULL. It identifies *which bug* this failure
 is — two failures with identical origin strings are treated as the
 same bug and shrunk together; failures with different origins are
 treated as distinct bugs and the shrink budget is *partitioned*
 across them.

 This makes the choice of origin string load-bearing for shrinker
 quality. In particular, bindings that recover from a host-language
 panic to call this function MUST NOT pass the recovered panic value
 (or its stringification) as origin if that value depends on the
 failing draw — every distinct draw would then look like a fresh bug
 to the engine and the shrinker would never converge.

 The conventional shape is `"Panic at <file>:<line>"` — i.e. derive
 origin from the *location* of the failing assertion, not the
 assertion's message. hegel-rust's own panic-to-failure path does
 exactly this (see `src/run_lifecycle.rs`).
 */
hegel_result_t hegel_mark_complete(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_status_t status,
                                   const char *origin);

/*
 True iff this test case is the engine's *final replay* of a
 minimal failing example. Bindings that want to emit verbose draw
 traces only for the final counterexample (rather than every probe
 the shrinker tries) gate their tracing on this flag.
 */
bool hegel_test_case_is_final_replay(const hegel_test_case_t *tc);

/*
 The run's aggregate status: passed, failed (the property has
 counterexamples — see `hegel_run_result_failure`), or errored (the run
 itself failed and produced no verdict — see `hegel_run_result_error`).
 A NULL `r` reports `HEGEL_RUN_STATUS_ERROR`.
 */
hegel_run_status_t hegel_run_result_status(const hegel_run_result_t *r);

/*
 The run-level error message when the run ended in an error rather than
 a verdict on the property — a failed health check (e.g. FilterTooMuch,
 TooSlow), a nondeterministic test, or an engine panic — or NULL when it
 completed normally. An errored run has `hegel_run_result_status(r) ==
 HEGEL_RUN_STATUS_ERROR` and no failures: the error is a failure of the
 run itself, not a counterexample to the property. The pointer is valid
 until `hegel_run_free`.
 */
const char *hegel_run_result_error(const hegel_run_result_t *r);

/*
 Number of *distinct* failures (by origin) the run surfaced. Each
 can be inspected via `hegel_run_result_failure(r, i)`.
 */
size_t hegel_run_result_failure_count(const hegel_run_result_t *r);

/*
 Borrowed pointer to the `index`-th failure (0-based). Returns NULL
 if `r` is NULL or `index >= hegel_run_result_failure_count(r)`. The
 pointer is valid until `hegel_run_free` is called on the parent
 run.
 */
const hegel_failure_t *hegel_run_result_failure(const hegel_run_result_t *r, size_t index);

/*
 The failure's panic message — e.g. the assertion text or
 engine-emitted message like `"FailedHealthCheck: FilterTooMuch — …"`.
 Returns NULL if `f` is NULL.
 */
const char *hegel_failure_panic_message(const hegel_failure_t *f);

/*
 The failure's origin string — the stable identifier that the
 shrinker used to group probes for this bug. Returns NULL if `f` is
 NULL. See `hegel_mark_complete` for what makes a good origin
 string.
 */
const char *hegel_failure_origin(const hegel_failure_t *f);

/*
 The failure's reproduce blob — a base64 string encoding the minimal
 counterexample's choice sequence, suitable for deterministic replay via
 `hegel_test_case_from_blob`. Returns NULL if `f` is NULL or the
 engine produced no blob for this failure. The pointer is borrowed from the
 parent `hegel_run_result_t` and stays valid until `hegel_run_free`.
 */
const char *hegel_failure_reproduction_blob(const hegel_failure_t *f);

/*
 Libhegel's version, matching the parent `hegeltest` crate's
 `CARGO_PKG_VERSION` (e.g. `"0.14.12"`). The returned pointer is
 static and valid for the program's lifetime.
 */
const char *hegel_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* HEGEL_H */
