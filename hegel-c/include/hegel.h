/*
 * libhegel — C bindings for Hegel's native property-based testing engine.
 *
 * This header is generated from hegel-c/src/lib.rs by cbindgen. Do not
 * edit it directly; re-run `just c-header` after changing the Rust source.
 *
 * libhegel implements the core of property-based testing: generation,
 * shrinking, the example database, and the decision of what to run next. It
 * ships as a shared library (libhegel.so, libhegel.dylib, hegel.dll) with a
 * C ABI.
 *
 * Terminology
 * -----------
 * - Library: the language-specific frontend that calls into libhegel.
 *   Sometimes called the caller below, since from libhegel's point of view
 *   it is whatever is making the calls.
 * - Context: holds the diagnostic message of a failed call. Passed as the
 *   first argument to nearly every function.
 * - Run: the full lifecycle of one property test, including executing many
 *   test cases and shrinking any failures.
 * - Test case: a single execution of the test function and the concrete
 *   values generated for it. Cloning a handle yields more handles onto the
 *   same test case, each with its own choice sequence.
 * - Span: a labeled grouping of draws that tells the shrinker which draws
 *   belong to one unit.
 * - Reproduce blob: a base64 string encoding a test case's choice sequence,
 *   which can be replayed later to reproduce it exactly. It is only
 *   guaranteed to reproduce the failure in the version of Hegel in which it
 *   was generated.
 *
 * Calling convention
 * ------------------
 * Every function takes a hegel_context_t* as its first argument and returns
 * a hegel_result_t code, except for hegel_context_new, which returns a
 * context, and hegel_context_last_error, which returns the message pointer
 * directly.
 *
 * HEGEL_OK is zero and every error code is negative. Anything else a call
 * produces is written through a trailing out-parameter named out_*.
 *
 * Every function returns HEGEL_E_INVALID_HANDLE when passed a NULL handle
 * (except the *_free functions, where NULL is a no-op) and
 * HEGEL_E_INVALID_ARG when passed any other invalid argument (a NULL
 * out-parameter, inverted bounds, a non-UTF-8 string, and so on). The
 * functions below leave these implicit.
 *
 * A NULL context is always allowed and opts out of error messages. The call
 * still returns its usual error code. A context must not be used
 * concurrently from multiple threads, since each fallible call overwrites
 * the stored message.
 *
 * Ownership
 * ---------
 * Pointers you pass into a libhegel function are always owned by the
 * caller. libhegel reads them during the call and copies whatever it needs
 * to keep, so you may free or reuse the memory as soon as the call returns.
 * Run results own their data and are independent of the run they came from.
 *
 * Release every pointer returned by these functions with its matching free:
 *
 *     hegel_context_new          ->  hegel_context_free
 *     hegel_settings_new         ->  hegel_settings_free
 *     hegel_run_start            ->  hegel_run_free
 *     hegel_test_case_from_blob  ->  hegel_test_case_free
 *     hegel_next_test_case       ->  hegel_test_case_free
 *     hegel_test_case_clone      ->  hegel_test_case_free
 *     hegel_run_result           ->  hegel_run_result_free
 *     hegel_run_result_failure   ->  hegel_failure_free
 *     hegel_string_generator_*   ->  hegel_string_generator_free
 *     hegel_new_collection       ->  hegel_collection_free
 *     hegel_new_pool             ->  hegel_pool_free
 *     hegel_new_state_machine    ->  hegel_state_machine_free
 *     hegel_generate_bytes       ->  hegel_generate_bytes_result_free
 *     hegel_generate_string      ->  hegel_generate_string_result_free
 *     hegel_printer_options_new  ->  hegel_printer_options_free
 *     hegel_printer_new          ->  hegel_printer_free
 *     hegel_printer_deferred     ->  hegel_printer_free
 *     hegel_test_case_printer    ->  hegel_printer_free
 *     hegel_printer_value        ->  hegel_printer_value_result_free
 *
 * Every other pointer libhegel hands back is a borrowed string. The caller
 * must not free it, and it is valid only until a documented point.
 * hegel_context_last_error is invalidated by the next call on that context.
 *
 * Threading
 * ---------
 * Each kind of handle has its own threading contract:
 *
 * - A context must not be used concurrently from multiple threads. Each
 *   fallible call overwrites its stored message, so sharing one across
 *   threads is a data race.
 * - A settings handle may be shared across threads once configured, but
 *   each setter call requires exclusive access.
 * - A run handle must only be used from one thread at a time. Calling
 *   hegel_next_test_case, hegel_run_result, or hegel_run_free concurrently
 *   on the same run is undefined behavior.
 * - A test-case handle may be driven by at most one thread at a time.
 *   Concurrent operations on it return HEGEL_E_CONCURRENT_USE. To generate
 *   from several threads, hegel_test_case_clone the handle and give each
 *   thread its own clone.
 */

#ifndef HEGEL_H
#define HEGEL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

/*
 Value written to `*out_rule_index` by `hegel_state_machine_next_rule`
 when the calling worker's round budget is exhausted (stop running rules
 and wait for the next group / join point), and to `*out_group_id` by
 `hegel_state_machine_next_group` when the whole state machine is done
 (run no further rounds).
 */
#define HEGEL_STATE_MACHINE_DONE INT64_MIN

/*
 Result of a libhegel call. See "Calling convention" in the header
 preamble.
 */
typedef enum {
    /*
     Success.
     */
    HEGEL_OK = 0,
    /*
     libhegel has exhausted its choice budget for this test case and wants
     the caller to abort the body and return.
     */
    HEGEL_E_STOP_TEST = -1,
    /*
     An `assume` / `reject` precondition failed. The current test case is
     invalid and should be discarded.
     */
    HEGEL_E_ASSUME = -2,
    /*
     The underlying backend reported an error. See
     `hegel_context_last_error`.
     */
    HEGEL_E_BACKEND = -3,
    /*
     A handle pointer was NULL where it must be non-NULL.
     */
    HEGEL_E_INVALID_HANDLE = -4,
    /*
     An argument other than a handle was invalid.
     */
    HEGEL_E_INVALID_ARG = -5,
    /*
     `hegel_mark_complete` (or a primitive on the same handle) was called
     for a test case that has already been completed.
     */
    HEGEL_E_ALREADY_COMPLETE = -6,
    /*
     Something was read before it was ready: `hegel_next_test_case`
     without first completing the previous test case, or
     `hegel_run_result` before the run finished.
     */
    HEGEL_E_NOT_COMPLETE = -7,
    /*
     An internal invariant failed inside libhegel. Should not happen in
     practice. Please file a bug at
     <https://github.com/hegeldev/hegel-rust/issues>.
     */
    HEGEL_E_INTERNAL = -8,
    /*
     A single test-case handle was used from two threads at once. Clone
     the handle instead.
     */
    HEGEL_E_CONCURRENT_USE = -9,
    /*
     A recursive generation attempt must be regenerated from the root.
     From `hegel_recursion_leaf`, the attempt exceeded its leaf budget:
     unwind it — drawing nothing further for it — back to where
     `hegel_new_recursion` was called, then call `hegel_recursion_retry`
     to discard it. From `hegel_recursion_finish`, the completed value
     was mispriced and the engine has already discarded it: drop it and
     start again from the root directly.
     */
    HEGEL_E_RETRY = -10,
} hegel_result_t;

/*
 Verbosity of engine-emitted output (logs, per-case traces). Set via
 `hegel_settings_set_verbosity`.
 */
typedef enum {
    /*
     Nothing besides the final result.
     */
    HEGEL_VERBOSITY_QUIET = 0,
    /*
     A short summary line per run. The default.
     */
    HEGEL_VERBOSITY_NORMAL = 1,
    /*
     Per-test-case progress and drawn values, plus panic diagnostics as
     they happen.
     */
    HEGEL_VERBOSITY_VERBOSE = 2,
    /*
     As verbose, plus shrinker trace output.
     */
    HEGEL_VERBOSITY_DEBUG = 3,
} hegel_verbosity_t;

/*
 Which source of randomness the engine draws from. Set via
 `hegel_settings_set_backend`.
 */
typedef enum {
    /*
     Expand a single seeded PRNG (the base setting). Runs are
     reproducible from the seed and shrinking / replay work as usual.
     */
    HEGEL_BACKEND_DEFAULT = 1,
    /*
     Read fresh entropy from `/dev/urandom` on every draw, falling back to
     an OS-seeded PRNG on platforms without it. Intended for running under
     Antithesis, whose fuzzer controls `/dev/urandom`, and selected by the
     shipped `antithesis` profile; you almost certainly don't want it
     otherwise.
     */
    HEGEL_BACKEND_URANDOM = 2,
} hegel_backend_t;

/*
 Aggregate outcome of a finished run, read via `hegel_run_result_status`.
 */
typedef enum {
    /*
     The property held across every generated test case.
     */
    HEGEL_RUN_STATUS_PASSED = 0,
    /*
     The property failed. Inspect each distinct counterexample.
     */
    HEGEL_RUN_STATUS_FAILED = 1,
    /*
     The run itself failed — a failed health check, a nondeterminism
     mismatch, a violated engine invariant — and produced no verdict on
     the property. There are no failures to inspect; read the message with
     `hegel_run_result_error`.
     */
    HEGEL_RUN_STATUS_ERROR = 2,
    /*
     The property failed on a run that was declared nondeterministic (a
     test case created a state machine with `max_concurrency > 1`). The
     failures carry no reproduce blob — there was no shrinking and there
     is no final replay — so the caller should report the bug from
     whatever it captured while running the discovering test case (the
     engine stamps every case of such a run nondeterministic up front,
     see `hegel_test_case_is_nondeterministic`, precisely so the caller
     captures each case's output as it runs).
     */
    HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC = 3,
} hegel_run_status_t;

/*
 A phase of the property-test loop, used as a bit flag.

 A bitwise OR of these is passed to `hegel_settings_set_phases`. The
 default is `HEGEL_PHASE_ALL`. Turn a phase off for debugging or replay
 tooling.
 */
typedef enum {
    /*
     Run hard-coded explicit examples (none today, reserved for future use).
     */
    HEGEL_PHASE_EXPLICIT = (1 << 0),
    /*
     Replay counterexamples persisted from previous runs. If a database
     path and database key aren't passed, this phase is a no-op.
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
     Shrink discovered failing examples.
     */
    HEGEL_PHASE_SHRINK = (1 << 4),
    /*
     All five phases enabled. The default.
     */
    HEGEL_PHASE_ALL = 31,
} hegel_phase_t;

/*
 A health check, used as a bit flag.

 A bitwise OR of these is passed to
 `hegel_settings_set_suppress_health_check`. The default is all enabled.
 */
typedef enum {
    /*
     Aborts the run if too many draws are rejected by assumptions.
     */
    HEGEL_HC_FILTER_TOO_MUCH = (1 << 0),
    /*
     Aborts the run if individual test cases take too long.
     */
    HEGEL_HC_TOO_SLOW = (1 << 1),
    /*
     Aborts the run if generated values are too large.
     */
    HEGEL_HC_TEST_CASES_TOO_LARGE = (1 << 2),
    /*
     Warns if the first generated test case is already disproportionately
     large.
     */
    HEGEL_HC_LARGE_INITIAL_TEST_CASE = (1 << 3),
} hegel_health_check_t;

/*
 Outcome of a single test case. Passed to `hegel_mark_complete`.
 */
typedef enum {
    /*
     The test body ran to completion without issues.
     */
    HEGEL_STATUS_VALID = 0,
    /*
     An assumption was violated in this test case.
     */
    HEGEL_STATUS_INVALID = 1,
    /*
     libhegel ran out of choice budget mid test case, typically because a
     draw returned `HEGEL_E_STOP_TEST`. Treat the case as inconclusive.
     */
    HEGEL_STATUS_OVERRUN = 2,
    /*
     The property failed and this test case is a counterexample.
     */
    HEGEL_STATUS_INTERESTING = 3,
} hegel_status_t;

/*
 Opaque handle to an engine-managed variable-length collection.

 Created by `hegel_new_collection` on a test case; driven by
 `hegel_collection_more` / `hegel_collection_reject` through any handle of
 the *same* test-case family (the root or any clone) — the continue/stop
 decisions are drawn from whichever handle makes the call. A collection
 must not be used from two threads at once: the operations take an
 internal non-blocking lock and return `HEGEL_E_CONCURRENT_USE` on
 contention.

 The handle is independent of the test case and run it was created under:
 free it with `hegel_collection_free` exactly once, at any point — before
 or after the test case or run is freed, in any order relative to other
 frees.
 */
typedef struct hegel_collection_t hegel_collection_t;

/*
 Opaque error-reporting context: holds the diagnostic message of a failed
 call. Passed as the first argument to nearly every function.
 */
typedef struct hegel_context_t hegel_context_t;

/*
 One distinct interesting test case surfaced by the run.
 `hegel_run_result_failure` writes a caller-owned run result.
 Reading strings within the run result via `hegel_failure_origin` /
 `_reproduction_blob` returns `const char*` pointers that stay valid until
 the memory is released with `hegel_failure_free`. The snapshot is
 independent of the result and run it came from.

 A failure carries the origin `libhegel` grouped on and the reproduce blob.
 The caller replays the blob (via `hegel_test_case_from_blob`) to produce
 the diagnostic and re-raise the test's own failure.
 */
typedef struct hegel_failure_t hegel_failure_t;

/*
 Opaque handle to an engine-managed *variable pool* for stateful testing.

 Created by `hegel_new_pool` on a test case; driven by `hegel_pool_add` /
 `hegel_pool_generate` through any handle of the *same* test-case family
 (the root or any clone) — the draw comes from whichever handle makes the
 call. Unlike a collection, a pool may legitimately be shared between
 clone handles driven from parallel threads: it holds an internal lock,
 so concurrent operations serialize instead of erroring. (Which variable
 a concurrent draw picks then depends on scheduling order, with the usual
 replay caveat for racy tests.)

 The handle is independent of the test case and run it was created under:
 free it with `hegel_pool_free` exactly once, at any point — before or
 after the test case or run is freed, in any order relative to other
 frees.
 */
typedef struct hegel_pool_t hegel_pool_t;

/*
 A pretty-printer document.

 Built from three primitives: `hegel_printer_text` emits unbreakable text,
 `hegel_printer_breakable` marks a point that renders as a separator if the
 enclosing group fits on one line and as a newline plus indentation if it
 does not, and `hegel_printer_begin_group` / `hegel_printer_end_group`
 delimit the groups those decisions are made over. Breaking is
 all-or-nothing per group, decided outermost groups first. The engine only
 provides the layout machinery; what gets printed — and in which language's
 syntax — is entirely the client's choice.

 Two facilities support printing values *while generating them*:
 `hegel_printer_deferred` opens a hole whose content is written later
 (while the test body runs) and spliced in by `hegel_printer_resolve`, and
 `hegel_printer_begin_speculative` buffers output that a rejected draw
 (a filter retry, a failed assumption) can retract.

 Create a standalone document with `hegel_printer_new`, or fetch a handle
 onto a test-case handle's region of the family document with
 `hegel_test_case_printer`.

 # Ownership and concurrency

 A printer handle addresses one *region* of a document — the document
 body for a root handle, or a hole for a handle from
 `hegel_printer_deferred`. Handles follow the test-case handles' model: a
 handle may move between threads, but belongs to one thread at a time —
 concurrent operations on the *same* handle return
 `HEGEL_E_CONCURRENT_USE`. To print from several threads, give each
 thread its own region: `hegel_printer_deferred` opens a hole at the
 handle's current position, and content written into it from any thread,
 on any schedule, renders at that anchor point — so concurrent output is
 deterministic, and two handles never interleave within one region.

 Every handle — including those returned by `hegel_printer_deferred` —
 must be released with `hegel_printer_free`.
 */
typedef struct hegel_printer_t hegel_printer_t;

/*
 Options for constructing a pretty-printer document.

 Construct with `hegel_printer_options_new`, configure via the
 `hegel_printer_options_set_*` functions, pass to `hegel_printer_new` /
 `hegel_test_case_printer`, and free with `hegel_printer_options_free`.
 Every option has a default, and a NULL options pointer means "all
 defaults", so callers that are happy with the defaults never construct
 one. New options are added as new setters, never by changing existing
 signatures.

 An options handle only parameterizes construction: it is read during the
 construction call and may be freed (or reconfigured and reused)
 immediately afterwards.
 */
typedef struct hegel_printer_options_t hegel_printer_options_t;

/*
 Opaque handle to an engine-managed *recursive generation scope*: the
 leaf budget and retry bookkeeping for one draw of a recursively defined
 value (a tree, a document, ...).

 Created by `hegel_new_recursion` on a test case, once per recursive
 value drawn; driven by `hegel_recursion_branch` / `hegel_recursion_leaf`
 / `hegel_recursion_retry` / `hegel_recursion_finish` through any handle
 of the *same* test-case family (the root or any clone) — decisions are
 drawn from whichever handle makes the call. Like a pool, the scope holds
 an internal lock, so clone handles driven from parallel threads share
 the leaf budget safely.

 The protocol, for one sub-value (starting with the root at depth 0):
 call `hegel_recursion_branch`; on `true` invoke the user's branch
 function, drawing each of its sub-values at `depth + 1` with this same
 protocol; on `false` call `hegel_recursion_leaf` and then draw one leaf.
 When `hegel_recursion_leaf` returns `HEGEL_E_RETRY` the attempt has
 outgrown the leaf budget: unwind out of the user's generators without
 drawing anything further, call `hegel_recursion_retry`, and on `HEGEL_OK`
 start the whole value again from the root. Once the root sub-value has
 finished, call `hegel_recursion_finish`: `HEGEL_OK` accepts the value,
 while `HEGEL_E_RETRY` means the engine discarded the attempt as
 mispriced — drop the value and start again from the root (without
 calling `hegel_recursion_retry`). All policy — the branch probabilities
 and their adaptation to the branch arities actually produced, the depth
 and leaf limits, and when to give up — lives in the engine, so recursive
 values are identically distributed in every language frontend.

 The handle is independent of the test case and run it was created under:
 free it with `hegel_recursion_free` exactly once, at any point — before
 or after the test case or run is freed, in any order relative to other
 frees.
 */
typedef struct HegelRecursion HegelRecursion;

/*
 In-flight property-test run.

 The caller starts a run, repeatedly asks for the next test case, reports
 its outcome, and reads the run result after all test cases have been
 run.

 The run handle owns the suspended run loop as a future, and each
 `hegel_next_test_case` call resumes it on the calling thread until it
 returns the next test case or finishes.
 */
typedef struct hegel_run_t hegel_run_t;

/*
 A run result is the outcome of a finished run, returned as a
 caller-owned copy. It stays valid after `hegel_run_free`, and is
 released separately.

 A failed run produced counterexamples to the property. An errored run
 produced no verdict on the property at all, so it has no failures to
 inspect. A run errors on a failed health check, a nondeterminism
 mismatch, or a violated internal invariant of libhegel.
 */
typedef struct hegel_run_result_t hegel_run_result_t;

/*
 A settings handle is built up with setters, handed to `hegel_run_start`,
 and then freed. Settings can be reused across runs.

 A configured handle may be shared across threads, but do not call setters
 concurrently on the same handle, or concurrently with
 `hegel_settings_get_database` reads of it.
 */
typedef struct hegel_settings_t hegel_settings_t;

/*
 Opaque handle to an engine-owned *state machine* for stateful
 (rule-based) testing, sequential or concurrent.

 Created by `hegel_new_state_machine` on a test case; driven by
 `hegel_state_machine_next_group` / `hegel_state_machine_next_rule` /
 `hegel_state_machine_rule_rejected` through any handle of the *same*
 test-case family (the root or any clone) — each choice is drawn from
 whichever handle makes the call. The machine holds an internal lock, so
 concurrent use from two clone handles serializes instead of erroring.

 The handle is independent of the test case and run it was created under:
 free it with `hegel_state_machine_free` exactly once, at any point —
 before or after the test case or run is freed, in any order relative to
 other frees.
 */
typedef struct hegel_state_machine_t hegel_state_machine_t;

/*
 Specification of a string draw

 Build one with a `hegel_string_generator_*` constructor (text, regex,
 email, url, domain). Every parameter is validated at construction.
 A generator is immutable after construction and may be shared freely
 across test cases and threads. Free it with
 `hegel_string_generator_free` once no draws will use it again.
 */
typedef struct hegel_string_generator_t hegel_string_generator_t;

/*
 A test-case handle is what a test body draws from. The caller drives it
 with the per-test-case primitives, concludes it with
 `hegel_mark_complete`, and releases it with `hegel_test_case_free`.

 A test case is a single execution of the test function and the concrete
 values generated for it. Cloning a handle yields more handles onto the
 same test case, each with its own choice sequence.
 */
typedef struct hegel_test_case_t hegel_test_case_t;

/*
 Per-line output callback, passed to `hegel_run_start` /
 `hegel_test_case_from_blob` (see there for the full contract). `user_data`
 is the pointer supplied alongside the callback; `line` is one line of
 engine output, NUL-terminated UTF-8 of `len` bytes (not counting the
 terminator) without a trailing newline, valid only for the duration of
 the call.
 */
typedef void (*hegel_output_callback_t)(void *user_data, const char *line, size_t len);

/*
 An engine-allocated byte buffer returned by `hegel_generate_bytes`.

 The caller owns the buffer and must release it with
 `hegel_generate_bytes_result_free` (freeing through any other allocator
 is undefined behaviour). `data` is never NULL after a successful draw,
 even for `len == 0`.
 */
typedef struct {
    uint8_t *data;
    size_t len;
} hegel_generate_bytes_result_t;

/*
 An engine-allocated string buffer returned by `hegel_generate_string`.

 `data` points to `len` bytes of UTF-8. The buffer is **not**
 NUL-terminated and may contain interior NUL bytes (the drawn alphabet
 can include U+0000), so it is not a C string — always use `len`. The
 caller owns the buffer and must release it with
 `hegel_generate_string_result_free` (freeing through any other allocator
 is undefined behaviour). `data` is never NULL after a successful draw,
 even for `len == 0`.
 */
typedef struct {
    char *data;
    size_t len;
} hegel_generate_string_result_t;

/*
 A drawn proleptic Gregorian calendar date: `year` in
 `[-999999, 999999]` (bounded by the range passed to
 `hegel_generate_date`), `month` in `[1, 12]`, `day` in
 `[1, days-in-month]`.
 */
typedef struct {
    int32_t year;
    uint8_t month;
    uint8_t day;
} hegel_date_t;

/*
 A drawn time of day: `hour` in `[0, 23]`, `minute` and `second` in
 `[0, 59]`, `nanosecond` in `[0, 999999999]`.
 */
typedef struct {
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
    uint32_t nanosecond;
} hegel_time_t;

/*
 A drawn naive datetime (a date plus a time of day, no timezone).
 */
typedef struct {
    hegel_date_t date;
    hegel_time_t time;
} hegel_datetime_t;

/*
 An engine-allocated string buffer returned by `hegel_printer_value`.

 `data` points to `len` bytes of UTF-8. The buffer is **not**
 NUL-terminated (printed values can contain any character), so always use
 `len`. The caller owns the buffer and must release it with
 `hegel_printer_value_result_free` (freeing through any other allocator is
 undefined behaviour). `data` is never NULL after a successful call, even
 for `len == 0`.
 */
typedef struct {
    char *data;
    size_t len;
} hegel_printer_value_result_t;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/*
 Returns a new error reporting context initialized with an empty message.
 Never returns NULL. Must be freed with `hegel_context_free`.
 */
hegel_context_t *hegel_context_new(void);

/*
 Parameters:
 `ctx`: The context being freed. No-op when called with NULL.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_context_free(hegel_context_t *ctx);

/*
 Parameters:
 `ctx`: The context to read.

 Returns the most recent error message recorded on `ctx`, or the empty
 string if the most recent call taking `ctx` succeeded. NULL only if `ctx`
 is NULL. The pointer borrows the context's internal buffer and is
 invalidated by the next call taking the same context.
 */
const char *hegel_context_last_error(const hegel_context_t *ctx);

/*
 Parameters:
 `out_settings`: Receives a handle initialized from the default settings
   profile.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` when profile resolution
 fails: a default-profile setting names an unknown profile, or a
 `hegel.toml` is malformed. Read the message with
 `hegel_context_last_error`.

 A profile is a named settings delta. Two names are reserved: `base` is
 the immutable base settings (100 test cases, all phases enabled, normal
 verbosity, no seed, the disk database under `.hegel/`), and `default`
 is the default profile, the one in effect when nothing names a profile:
 the strongest set of `hegel_set_default_profile`, the
 `HEGEL_DEFAULT_PROFILE` environment variable, and the top-level
 `default = "<profile>"` entry in `hegel.toml`, else the detected
 environment (`antithesis` inside Antithesis, detected via
 `ANTITHESIS_OUTPUT_DIR`, or `ci` on a CI server, detected via `CI`,
 `GITHUB_ACTIONS`, and similar variables), else `development`. This
 function resolves `default`.

 Three ordinary profiles ship with libhegel: `development` (the base
 settings, unchanged — what local runs get), `ci` (derandomization on,
 database disabled, the `too_slow` health check suppressed, reproduction
 lines printed), and `antithesis` (database disabled, every health check
 suppressed). A custom profile without an explicit `extends` extends
 `default`, skipping any candidate already in its chain, so it sits on
 `ci` when resolved on a CI server and on `development` locally. The
 shipped profiles themselves extend `base` and never layer over one
 another. Extending or selecting `base` pins the plain base settings.

 Profiles are modified and defined in a `hegel.toml` found in the current
 directory or the nearest ancestor, and registered programmatically with
 `hegel_settings_register_profile`; use `hegel_settings_new_for_profile`
 to resolve one by name.
 */
hegel_result_t hegel_settings_new(hegel_context_t *ctx, hegel_settings_t **out_settings);

/*
 Parameters:
 `name`: The profile to resolve: reserved (`base`, `default`), shipped
   (`development`, `ci`, `antithesis`), defined in `hegel.toml`, or
   registered with `hegel_settings_register_profile`.
 `out_settings`: Receives a handle initialized from that profile.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` when the profile is unknown
 or a `hegel.toml` is malformed. Read the message with
 `hegel_context_last_error`.

 Selecting a profile by name does not change what the default profile is:
 the named profile still implicitly extends `default` (see
 `hegel_settings_new`), so it layers over the environment's profile —
 except `base`, which is always the plain base settings.
 */
hegel_result_t hegel_settings_new_for_profile(hegel_context_t *ctx,
                                              const char *name,
                                              hegel_settings_t **out_settings);

/*
 Parameters:
 `s`: The handle to free. Safe to call with NULL.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_free(hegel_context_t *ctx, hegel_settings_t *s);

/*
 Parameters:
 `backend`: A `hegel_backend_t` value selecting the source of
   randomness.

 Returns `HEGEL_OK`.

 The enum-valued setters take `uint32_t` rather than the enum type so
 that an out-of-range value is an error instead of undefined behavior.
 */
hegel_result_t hegel_settings_set_backend(hegel_context_t *ctx,
                                          hegel_settings_t *s,
                                          uint32_t backend);

/*
 Parameters:
 `n`: Maximum number of valid test cases to run before declaring the
   property held. 100 by default. Cases rejected by an assumption do not
   count against this budget.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_test_cases(hegel_context_t *ctx, hegel_settings_t *s, uint64_t n);

/*
 Parameters:
 `v`: Controls the output verbosity. See `hegel_verbosity_t`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_verbosity(hegel_context_t *ctx, hegel_settings_t *s, uint32_t v);

/*
 Parameters:
 `seed`: The RNG seed to initialize generation with.
 `has_seed`: When `false` (the default), libhegel picks a fresh random
   seed at run start.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_seed(hegel_context_t *ctx,
                                       hegel_settings_t *s,
                                       uint64_t seed,
                                       bool has_seed);

/*
 Parameters:
 `derandomize`: Derive the seed from a stable hash of the database key
   instead of fresh randomness when no explicit seed is set.

 Returns `HEGEL_OK`.

 Useful in CI, where you want repeated runs of one test to be
 deterministic but different tests to still see different inputs.
 */
hegel_result_t hegel_settings_set_derandomize(hegel_context_t *ctx,
                                              hegel_settings_t *s,
                                              bool derandomize);

/*
 Parameters:
 `yes`: When `true`, libhegel keeps generating after the first failure
   to surface additional distinct bugs. Failures from different locations
   in the program are considered distinct bugs. The final result lists
   all of them. When `false`, the run stops after the first failing
   example.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_report_multiple_failures(hegel_context_t *ctx,
                                                           hegel_settings_t *s,
                                                           bool yes);

/*
 Parameters:
 `yes`: When `true`, libhegel prints a statistics block on the run's
   output at the end of the run: for each label recorded with
   `hegel_event`, the fraction of generation-phase test cases it
   occurred in, and for each label recorded with `hegel_event_value`, a
   distribution summary of the observed values. Defaults to off.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_show_statistics(hegel_context_t *ctx,
                                                  hegel_settings_t *s,
                                                  bool yes);

/*
 Parameters:
 `database`: NULL sets it to the default: `./.hegel/examples/`, including
   resetting a value the handle already carries. `""` disables the
   database entirely. Discovered failures will not be stored. Anything
   else is used as the database root directory. The directory will be
   created if it does not already exist.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_database(hegel_context_t *ctx,
                                           hegel_settings_t *s,
                                           const char *database);

/*
 Parameters:
 `key`: Scopes stored and replayed examples. NULL clears it (the
   default).

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_database_key(hegel_context_t *ctx,
                                               hegel_settings_t *s,
                                               const char *key);

/*
 Parameters:
 `phases`: A bitwise OR of `hegel_phase_t` values to toggle phases. The
   default is `HEGEL_PHASE_ALL`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_phases(hegel_context_t *ctx,
                                         hegel_settings_t *s,
                                         uint32_t phases);

/*
 Parameters:
 `checks`: A bitwise OR of `hegel_health_check_t` values naming the
   checks to toggle. Each call overwrites the previous suppressions.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_suppress_health_check(hegel_context_t *ctx,
                                                        hegel_settings_t *s,
                                                        uint32_t checks);

/*
 Parameters:
 `yes`: When `true`, a failure should be reported with a copy-pasteable
   reproduction line for its counterexample. Defaults to `false`; the
   shipped `ci` profile turns it on. libhegel itself never acts on this
   value — the reproduce blob is always attached to the failure and
   printing it is the caller's decision — but carrying it in the settings
   lets profiles configure it for every Hegel library.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_set_print_blob(hegel_context_t *ctx, hegel_settings_t *s, bool yes);

/*
 Parameters:
 `name`: The profile name to register: ASCII letters, digits, `-` and
   `_` only. The reserved names `base` and `default` are rejected.
 `settings`: The settings to snapshot. The caller keeps ownership; the
   handle's database key is not part of the snapshot.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for an invalid or reserved
 name.

 Registers a complete snapshot of `settings` as the profile `name`,
 process-wide, replacing any earlier registration of the same name.
 Registering a shipped name replaces that profile wholesale. A
 `hegel.toml` section for `name` still merges over the snapshot, and may
 not set `extends` on it. Registration is not retroactive: settings
 handles already created keep their values.
 */
hegel_result_t hegel_settings_register_profile(hegel_context_t *ctx,
                                               const char *name,
                                               const hegel_settings_t *settings);

/*
 Parameters:
 `name`: The profile the `default` alias should resolve to, or NULL to
   clear an earlier call. The name is not required to exist yet; naming
   the `default` alias itself is rejected.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for an invalid name.

 Sets the default settings profile for the whole process, taking
 precedence over `HEGEL_DEFAULT_PROFILE`, the `default` entry in
 `hegel.toml`, and environment detection (see `hegel_settings_new`). Like
 registration it is not retroactive: settings handles already created
 keep their values.
 */
hegel_result_t hegel_set_default_profile(hegel_context_t *ctx, const char *name);

/*
 Parameters:
 `out`: Receives the configured number of test cases.

 Returns `HEGEL_OK`.

 The `hegel_settings_get_*` functions read a settings handle back — for
 example one resolved from a profile by `hegel_settings_new` — so a
 Hegel library can present the effective configuration. Each takes a
 `const` handle and one out parameter, and fails with
 `HEGEL_E_INVALID_HANDLE` / `HEGEL_E_INVALID_ARG` on a null handle or out
 pointer.
 */
hegel_result_t hegel_settings_get_test_cases(hegel_context_t *ctx,
                                             const hegel_settings_t *s,
                                             uint64_t *out);

/*
 Parameters:
 `out`: Receives the configured verbosity.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_verbosity(hegel_context_t *ctx,
                                            const hegel_settings_t *s,
                                            hegel_verbosity_t *out);

/*
 Parameters:
 `out_seed`: Receives the configured seed when one is set, else 0.
 `out_has_seed`: Receives whether a seed is set.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_seed(hegel_context_t *ctx,
                                       const hegel_settings_t *s,
                                       uint64_t *out_seed,
                                       bool *out_has_seed);

/*
 Parameters:
 `out`: Receives whether derandomization is enabled.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_derandomize(hegel_context_t *ctx,
                                              const hegel_settings_t *s,
                                              bool *out);

/*
 Parameters:
 `out_database`: Receives the database value, mirroring the setter's
   convention: NULL for the default, `""` for disabled, else the root
   directory path. The pointer borrows the handle and stays valid until
   the next `hegel_settings_set_database` call on it or
   `hegel_settings_free`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_database(hegel_context_t *ctx,
                                           const hegel_settings_t *s,
                                           const char **out_database);

/*
 Parameters:
 `out`: Receives the enabled phases as a bitwise OR of `hegel_phase_t`
   values.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_phases(hegel_context_t *ctx,
                                         const hegel_settings_t *s,
                                         uint32_t *out);

/*
 Parameters:
 `out`: Receives the suppressed health checks as a bitwise OR of
   `hegel_health_check_t` values, as resolved from the profile and any
   `hegel_settings_set_suppress_health_check` call.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_suppress_health_check(hegel_context_t *ctx,
                                                        const hegel_settings_t *s,
                                                        uint32_t *out);

/*
 Parameters:
 `out`: Receives whether multi-bug reporting is enabled.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_report_multiple_failures(hegel_context_t *ctx,
                                                           const hegel_settings_t *s,
                                                           bool *out);

/*
 Parameters:
 `out`: Receives whether the statistics block is enabled.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_show_statistics(hegel_context_t *ctx,
                                                  const hegel_settings_t *s,
                                                  bool *out);

/*
 Parameters:
 `out`: Receives whether reproduction lines should be printed. See
   `hegel_settings_set_print_blob`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_print_blob(hegel_context_t *ctx,
                                             const hegel_settings_t *s,
                                             bool *out);

/*
 Parameters:
 `out`: Receives the configured backend.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_settings_get_backend(hegel_context_t *ctx,
                                          const hegel_settings_t *s,
                                          hegel_backend_t *out);

/*
 Parameters:
 `settings`: The settings for this run. The caller can free the
   settings after passing them in since libhegel copies the memory.
 `callback`: Where libhegel's output for this run goes. NULL leaves
   output on stderr.
 `user_data`: Passed through to `callback` verbatim. Ignored when
   `callback` is NULL.
 `out_run`: Receives the run handle.

 Returns `HEGEL_OK`.

 This only sets up the run. No test case is generated until the first
 `hegel_next_test_case` call. libhegel emits while it runs inside that
 call, so the callback is invoked on whichever thread makes it. Because
 it runs inside `hegel_next_test_case`, the callback must not call back
 into libhegel on the same run.
 */
hegel_result_t hegel_run_start(hegel_context_t *ctx,
                               const hegel_settings_t *settings,
                               hegel_output_callback_t callback,
                               void *user_data,
                               hegel_run_t **out_run);

/*
 Parameters:
 `out_test_case`: Receives a handle for the next test case, or NULL
   once the run is finished.

 Returns `HEGEL_OK`, including at normal completion, where
 `*out_test_case` is NULL and you should call `hegel_run_result`.
 `HEGEL_E_NOT_COMPLETE` if the previous test case was not marked
 complete.

 The handle is owned by the caller and must be released with
 `hegel_test_case_free`.
 */
hegel_result_t hegel_next_test_case(hegel_context_t *ctx,
                                    hegel_run_t *run,
                                    hegel_test_case_t **out_test_case);

/*
 Parameters:
 `out_result`: Receives a caller-owned copy of the finished run's
   result.

 Returns `HEGEL_OK`, or `HEGEL_E_NOT_COMPLETE` if the run hasn't finished
 yet.

 Each call produces a copy, freed separately. It stays valid after
 `hegel_run_free`.
 */
hegel_result_t hegel_run_result(hegel_context_t *ctx,
                                hegel_run_t *run,
                                hegel_run_result_t **out_result);

/*
 Parameters:
 `r`: The run result to free and the strings read off it. Safe to call
   with NULL.

 Returns `HEGEL_OK`.

 Must be called exactly once per run result.
 */
hegel_result_t hegel_run_result_free(hegel_context_t *ctx, hegel_run_result_t *r);

/*
 Parameters:
 `run`: The run to free. Safe to call with NULL.

 Returns `HEGEL_OK`.

 If the caller exited its loop early, any in-flight test case is marked
 complete and the rest of the exploration is dropped.
 */
hegel_result_t hegel_run_free(hegel_context_t *ctx, hegel_run_t *run);

/*
 A library uses a reproduce blob to replay a counterexample: it reruns
 the minimal failing test case so it can display the drawn values and
 re-raise the test's own failure.

 There is no run handle and no run loop involved. The caller drives the
 returned test case with the usual per-test-case primitives, concludes it
 with `hegel_mark_complete`, and decides for itself whether the blob
 reproduced the failure (the property failed again) or is stale/flaky (it
 passed).

 Parameters:
 `blob`: A base64 blob from `hegel_failure_reproduction_blob`.
 `callback` / `user_data`: Where this replay's output goes, with the
   same contract as `hegel_run_start`. The callback is only ever invoked
   on this thread and need not outlive the call.
 `out_test_case`: Receives a caller-owned test-case handle. Released
   like any other with `hegel_test_case_free`.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for a blob that is not
 valid (corrupt, non-UTF-8, or from an incompatible Hegel version).

 A blob whose choices no longer match the caller's generators returns
 `HEGEL_E_STOP_TEST` from the draw that overruns.
 */
hegel_result_t hegel_test_case_from_blob(hegel_context_t *ctx,
                                         const hegel_settings_t *s,
                                         const char *blob,
                                         hegel_output_callback_t callback,
                                         void *user_data,
                                         hegel_test_case_t **out_test_case);

/*
 Parameters:
 `tc`: Any test-case handle. Safe to call with NULL.

 Returns `HEGEL_OK`.

 Each handle holds one reference to the shared test case. The underlying
 data source is released once the last reference is gone. Each handle
 must be freed exactly once. A run-owned test case still needs
 `hegel_mark_complete` from one of its handles before the run can
 advance, so make every test case complete before freeing your last
 handle to it.
 */
hegel_result_t hegel_test_case_free(hegel_context_t *ctx, hegel_test_case_t *tc);

/*
 Returns whether this test case belongs to a run already known to be
 nondeterministic.
 */
hegel_result_t hegel_test_case_is_nondeterministic(hegel_context_t *ctx,
                                                   const hegel_test_case_t *tc,
                                                   bool *out_is_nondeterministic);

/*
 Parameters:
 `out_test_case`: Receives a new handle onto an independent stream of
   the same test case.

 Returns `HEGEL_OK`, `HEGEL_E_CONCURRENT_USE` if another thread is
 mid-operation on the source handle, `HEGEL_E_ALREADY_COMPLETE` once the
 test case has completed.

 The clone shares the test case's outcome and budgets but generates from
 its own choice sequence, so a clone and its source can be driven
 concurrently from different threads while staying deterministic under
 replay. Collections, pools, and state machines remain shared across all
 handles to the test case, but do not use shared objects from two streams
 since it makes tests flaky (and a *collection* used from two threads at
 once reports `HEGEL_E_CONCURRENT_USE`; see `hegel_collection_t`).
 */
hegel_result_t hegel_test_case_clone(hegel_context_t *ctx,
                                     const hegel_test_case_t *tc,
                                     hegel_test_case_t **out_test_case);

/*
 Parameters:
 `indent`: How many columns further than `tc`'s own lines every line of
   the block is indented.
 `out_test_case`: Receives a new handle onto the *same* choice stream as
   `tc`.

 Returns `HEGEL_OK`, `HEGEL_E_INVALID_HANDLE` for a NULL `tc`, and
 `HEGEL_E_INVALID_ARG` for a NULL `out_test_case`.

 A block handle is how a client prints an indented section under a
 heading — the body of a stateful rule under its `Step 3: add {` line, the
 body of a repeated section — without touching every line itself. Its
 print region (see `hegel_test_case_printer`) is a block nested in `tc`'s
 region at the current position: everything printed or noted through the
 handle, and through the clones and blocks derived from it, lands there,
 each line indented `indent` columns further than `tc`'s lines (blocks
 nest, and their indentation adds up). The indentation is applied to a
 line when it gets its first content, so it covers the continuation lines
 of a value broken across lines too, and it ends exactly with the block:
 a line `tc` writes after the block's last one is back at `tc`'s
 indentation. It is independent of the break-point indentation
 `hegel_printer_begin_group` / `hegel_printer_shift_indent` manage.

 Unlike a clone, a block handle draws from `tc`'s own choice sequence:
 drawing through it and through `tc` are the same thing, so the two must
 not be driven concurrently (give a thread a clone instead). It shares
 everything else with `tc` — outcome, budgets, worker attribution as of
 its creation — and is released with `hegel_test_case_free` like any
 other handle. If `tc`'s region is dead (the document was read), the
 block shares the dead region and its prints are no-ops.
 */
hegel_result_t hegel_test_case_block(hegel_context_t *ctx,
                                     const hegel_test_case_t *tc,
                                     uint64_t indent,
                                     hegel_test_case_t **out_test_case);

/*
 Attribute this handle's output to concurrent worker `worker_index`:
 every line recorded from now on through the handle — `hegel_note` lines,
 and lines started through a printer fetched from it with
 `hegel_test_case_printer`, before or after this call — is prefixed with
 `[worker N +X.XXXms] `, where `X.XXX` is the time since the test case
 started at which the line was recorded. Blocks and clones derived from
 the handle after this call inherit the attribution (a worker's rule
 bodies and their clones print as that worker's), and may be attributed
 afresh on their own. Lines a group breaks across are stamped on their
 first line only.

 This is the attribution a concurrent stateful runner gives the clone it
 hands each worker thread (see `hegel_state_machine_next_rule`), so the
 report can be read across workers: regions order a worker's lines
 together, and the offsets say how they interleaved in time.

 Returns `HEGEL_OK`, `HEGEL_E_INVALID_HANDLE` for a NULL `tc`, and
 `HEGEL_E_INVALID_ARG` for a negative `worker_index`.
 */
hegel_result_t hegel_test_case_set_worker(hegel_context_t *ctx,
                                          const hegel_test_case_t *tc,
                                          int64_t worker_index);

/*
 A span groups a set of draws so the shrinker can treat them as a unit.
 Libraries should wrap each compound generator in a span.

 Parameters:
 `label`: Identifies the generator that opened the span. Labels have no
   meaning beyond identity: libhegel treats two spans with the same label
   as coming from the same generator, and so as candidates for swapping,
   duplicating and reordering with each other, and does nothing else with
   them. Any value is valid as long as the same generator always uses the
   same label; derive labels with `hegel_label_from_name` and
   `hegel_label_combine` (or the same hashes computed ahead of time)
   rather than numbering them by hand, so that generators with the same
   shape but different components — a list of integers and a list of
   strings, say — get different labels. libhegel opens spans around its
   own draws with labels derived from names of the form `hegel.<kind>`.

 Returns `HEGEL_OK`.

 Pair with exactly one `hegel_stop_span` call.
 */
hegel_result_t hegel_start_span(hegel_context_t *ctx, hegel_test_case_t *tc, uint64_t label);

/*
 Parameters:
 `discard`: Pass `true` to mark the span rejected (e.g. a `filter`
   predicate didn't hold) so libhegel retries from before the span
   opened.

 Returns `HEGEL_OK`.

 Closes the most recently opened span.
 */
hegel_result_t hegel_stop_span(hegel_context_t *ctx, hegel_test_case_t *tc, bool discard);

/*
 The span label for a generator identified by a name: the 64-bit FNV-1a
 hash of the name's bytes. Use it for the label of a generator with no
 component generators (`hegel_label_from_name("mylib.integers")`), and
 for the first argument of `hegel_label_combine` for one that has
 components. A name only has to be stable and unique to its generator; a
 prefix naming the library keeps it clear of libhegel's own `hegel.<kind>`
 names and of other libraries'.

 Parameters:
 `name`: Non-NULL, NUL-terminated. Hashed as bytes, so any encoding
   works as long as the generator always uses the same one.
 `out_label`: Receives the label.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` on a NULL `name` or
 `out_label`.
 */
hegel_result_t hegel_label_from_name(hegel_context_t *ctx, const char *name, uint64_t *out_label);

/*
 The span label for a generator built from other generators: a hash of
 the given labels, in order. Pass the generator's own label (from
 `hegel_label_from_name`) first and its components' labels after it, so
 that `lists(integers())` and `lists(text())` get different labels while
 every `lists(integers())` gets the same one. Combining is
 order-sensitive, and combining a single label does not return it
 unchanged.

 Parameters:
 `labels`: `len` labels. May be NULL when `len` is 0.
 `out_label`: Receives the combined label.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` on a NULL `labels` with a
 non-zero `len` or a NULL `out_label`.
 */
hegel_result_t hegel_label_combine(hegel_context_t *ctx,
                                   const uint64_t *labels,
                                   size_t len,
                                   uint64_t *out_label);

/*
 For variable-length values, libhegel decides how many elements to
 produce. The caller loops on `hegel_collection_more`, drawing one
 element per returned `true`.

 Parameters:
 `min_size` / `max_size`: Inclusive size bounds. Pass `UINT64_MAX` as
   `max_size` for no upper bound.
 `out_collection`: Receives a caller-owned handle to pass to the calls
   below (through any handle of the same test-case family). Release it
   with `hegel_collection_free` exactly once.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_new_collection(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    uint64_t min_size,
                                    uint64_t max_size,
                                    hegel_collection_t **out_collection);

/*
 Parameters:
 `out_more`: Receives whether libhegel wants another element, drawn from
   `tc`'s stream. Call in a loop until it is `false` and draw the next
   element in each loop iteration.

 Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_CONCURRENT_USE`
 when another thread is mid-operation on the collection.
 */
hegel_result_t hegel_collection_more(hegel_context_t *ctx,
                                     hegel_test_case_t *tc,
                                     hegel_collection_t *collection,
                                     bool *out_more);

/*
 Parameters:
 `why`: Optional human-readable rejection reason (NULL is allowed).
   Validated but currently unused, reserved for future rejection
   diagnostics.

 Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_CONCURRENT_USE`
 when another thread is mid-operation on the collection.

 Tells libhegel the last element it produced is invalid.
 */
hegel_result_t hegel_collection_reject(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       hegel_collection_t *collection,
                                       const char *why);

/*
 Release a collection handle from `hegel_new_collection`. Safe to call
 with NULL (a no-op that returns `HEGEL_OK`), and safe at any point in any
 order relative to freeing the test case or the run. Each handle must be
 freed exactly once; freeing the same handle twice is undefined behaviour.
 */
hegel_result_t hegel_collection_free(hegel_context_t *ctx, hegel_collection_t *collection);

/*
 Open a recursive generation scope: libhegel decides where the value
 branches, where it bottoms out in leaves, and when an attempt has grown
 too large and must be retried. See `hegel_recursion_t` for the protocol.
 Draws the scope's target size from `tc`'s stream (when `max_leaves` is
 at least 2), so it must be called at the point in the draw sequence
 where the recursive value begins.

 Parameters:
 `max_depth`: Branches nest at most this deep; sub-values at this depth
   are always leaves, so 0 generates only leaves.
 `max_leaves`: The most leaves one generated value may contain. Each
   value steers toward a target size drawn from this range. Attempts
   that outgrow the budget are discarded and retried steering toward a
   smaller target, and the test case is rejected as invalid when several
   attempts in a row fail to fit.
 `out_recursion`: Receives a caller-owned handle to pass to the calls
   below (through any handle of the same test-case family). Release it
   with `hegel_recursion_free` exactly once.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_new_recursion(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   uint64_t max_depth,
                                   uint64_t max_leaves,
                                   HegelRecursion **out_recursion);

/*
 Parameters:
 `depth`: The nesting depth of the sub-value about to be drawn: 0 for the
   root, and one more than the enclosing branch for its sub-values.
 `out_branch`: Receives the leaf-or-branch decision, drawn from `tc`'s
   stream: `true` means invoke the branch function, `false` means the
   sub-value is a leaf (call `hegel_recursion_leaf`, then draw it).

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_recursion_branch(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      HegelRecursion *recursion,
                                      uint64_t depth,
                                      bool *out_branch);

/*
 Count one leaf against the current attempt's budget. Call immediately
 before drawing each leaf value.

 Returns `HEGEL_OK` (draw the leaf), `HEGEL_E_RETRY` (the attempt has
 outgrown `max_leaves`: unwind it without drawing anything further and
 call `hegel_recursion_retry`), or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_recursion_leaf(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    HegelRecursion *recursion);

/*
 Discard a generation attempt that returned `HEGEL_E_RETRY`: the spans it
 left open are closed and marked discarded, its leaf budget is reset, and
 the next attempt uses a lower branching probability. Call only after
 unwinding out of the user's generators, from the stack depth at which
 `hegel_new_recursion` was called.

 Returns `HEGEL_OK` (start the value again from the root),
 `HEGEL_E_ASSUME` (attempts exhausted: the test case has been concluded
 invalid, abort the body as for any failed assumption), or
 `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_recursion_retry(hegel_context_t *ctx,
                                     hegel_test_case_t *tc,
                                     HegelRecursion *recursion);

/*
 Report that the recursive value has finished generating: its root
 sub-value (and therefore the whole tree) is complete. The engine checks
 the branch pricing the attempt started from against the branch arities
 it actually produced.

 Returns `HEGEL_OK` (the value is accepted — use it),
 `HEGEL_E_RETRY` (the attempt was mispriced and has been discarded, its
 spans closed as discarded: drop the value and start again from the
 root, *without* calling `hegel_recursion_retry`), or
 `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_recursion_finish(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      HegelRecursion *recursion);

/*
 Release a recursion handle from `hegel_new_recursion`. Safe to call with
 NULL (a no-op that returns `HEGEL_OK`), and safe at any point in any
 order relative to freeing the test case or the run. Each handle must be
 freed exactly once; freeing the same handle twice is undefined behaviour.
 */
hegel_result_t hegel_recursion_free(hegel_context_t *ctx, HegelRecursion *recursion);

/*
 A pool tracks a set of variable ids libhegel can draw from and shrink
 over. It is mostly used for stateful testing, where a rule needs to act
 on some previously generated value. The caller keeps its own mapping
 from variable id to the value it generated.

 Parameters:
 `out_pool`: Receives a caller-owned handle to pass to the calls below
   (through any handle of the same test-case family). Release it with
   `hegel_pool_free` exactly once.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_new_pool(hegel_context_t *ctx, hegel_test_case_t *tc, hegel_pool_t **out_pool);

/*
 Parameters:
 `out_variable_id`: Receives a fresh variable id, which the caller
   associates with the value it just generated.

 The id is drawn from `tc`'s stream and recorded in the choice sequence
 by value, so it stays stable while the test case shrinks: deleting an
 earlier addition never renumbers the survivors.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_pool_add(hegel_context_t *ctx,
                              hegel_test_case_t *tc,
                              hegel_pool_t *pool,
                              int64_t *out_variable_id);

/*
 Draws a variable from the pool, letting libhegel choose (and shrink)
 which previously added variable to reuse. The choice is drawn from
 `tc`'s stream and recorded as the chosen variable id itself, not as an
 index into the pool's current contents, so shrinking away other
 additions never changes which variable a recorded choice refers to.

 Parameters:
 `consume`: When `true` the drawn variable is removed from the pool.
   When `false` it is not removed.
 `out_variable_id`: Receives the variable id libhegel chose.

 Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_ASSUME` if the pool
 has no variables — treat it like any other failed assumption.
 */
hegel_result_t hegel_pool_generate(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_pool_t *pool,
                                   bool consume,
                                   int64_t *out_variable_id);

/*
 Release a pool handle from `hegel_new_pool`. Safe to call with NULL (a
 no-op that returns `HEGEL_OK`), and safe at any point in any order
 relative to freeing the test case or the run, provided no pool operation
 is still in flight on another thread. Each handle must be freed exactly
 once; freeing the same handle twice is undefined behaviour.
 */
hegel_result_t hegel_pool_free(hegel_context_t *ctx, hegel_pool_t *pool);

/*
 Register a *state machine* for engine-owned stateful (rule-based)
 testing, sequential or concurrent: `num_rules` rules — each assigned to
 a concurrency group by `rule_groups`, an array of group ids parallel to
 `rule_names` — and `num_invariants` invariants, with names as
 NUL-terminated UTF-8, plus concurrency bounds. `invariant_always_check`
 is an array of `num_invariants` flags parallel to `invariant_names`
 (NULL for all-false): `hegel_state_machine_should_check_invariant`
 answers true unconditionally for a flagged invariant and samples the
 rest. Group ids are arbitrary
 (any value except `HEGEL_STATE_MACHINE_DONE`, which
 `hegel_state_machine_next_group` reserves as its termination sentinel):
 the machine has one concurrency group per distinct value of
 `rule_groups`. The engine draws the machine's concurrency
 level — the number of workers (typically worker threads) that will pull
 rules — in `[min_concurrency, max_concurrency]` and writes it into
 `*out_concurrency`; the caller must run exactly that many workers. The
 engine owns the distribution, which is weighted toward
 `max_concurrency` (concurrency bugs need concurrency) rather than
 shrink-biased toward the minimum. Pass `min_concurrency ==
 max_concurrency` to fix the level without consuming entropy — `1, 1`
 for a sequential machine. `step_count` is the target number of counted
 rounds the machine runs per test case: every case runs at least one
 round and at most `step_count` (at concurrency 1, where a round is one
 rule, that is at most `step_count` completed rules), and each sampled
 invariant is checked with probability `1 / step_count` per join point.
 The engine has no default; frontends typically use 50.

 The engine owns rule selection — including swarm testing, where each
 worker enables a random subset of rules (at least one per group) and
 selection draws only from that subset. The caller drives execution in
 rounds: on the root test-case handle it asks
 `hegel_state_machine_next_group` whether another round should run, then
 each worker asks `hegel_state_machine_next_rule` which rule to run and
 applies it, until that call signals the join point. Rules in
 the same group may run concurrently; rules in different groups never
 overlap.

 Creating the machine draws from the calling handle's stream: the
 concurrency level and each worker's swarm parameters are decided here,
 up front, so the machine is fully constructed before any rule is
 requested.

 Creating a machine with `max_concurrency > 1` declares the run
 nondeterministic: thread scheduling is outside the engine's control, so
 nothing that assumes deterministic replay can be trusted. On a run not
 already known to be nondeterministic, the first such creation is
 rejected with `HEGEL_E_ASSUME` — the caller should abandon the body and
 report the case `HEGEL_STATUS_INVALID`, exactly as for a failed
 assumption — and the engine flips the run at that case's end. Every
 later test case is marked nondeterministic before it starts (so a
 frontend can capture its whole trace for the failure report, including
 draws made before the machine is created) and its creations succeed.
 From the flip on, the run reports failures faithfully from the
 discovering execution and skips data-tree recording (and with it
 novel-prefix generation and the nondeterminism mismatch check), span
 mutation, the verify and shrink pass (and with it the flakiness check —
 generation stops at the first bug, so at most one failure is reported),
 targeting, and database persistence and reuse. Failures from such a run
 carry no reproduce blob. A notice explaining this is printed once, on
 the run's output, unless verbosity is quiet. This applies even to test
 cases whose drawn concurrency level is 1: the declared bound is what
 counts. Standalone test cases — `hegel_test_case_from_blob` replays —
 are never rejected.

 On success writes a caller-owned handle into `*out_state_machine` —
 pass it to subsequent `hegel_state_machine_next_group` /
 `hegel_state_machine_next_rule` / `hegel_state_machine_rule_rejected`
 calls (through any handle of the same test-case family) and release it
 with `hegel_state_machine_free` exactly once — writes the drawn
 concurrency level into `*out_concurrency`, and returns `HEGEL_OK`.
 Returns `HEGEL_E_ASSUME` for the run's first `max_concurrency > 1`
 creation (the caller should abort the body and call
 `hegel_mark_complete` with `HEGEL_STATUS_INVALID`; see above). Returns
 `HEGEL_E_STOP_TEST` when the engine's choice budget is
 exhausted (the caller should abort the body and call
 `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
 `HEGEL_E_INVALID_ARG` if `num_rules` is zero, an entry of `rule_groups`
 is `HEGEL_STATE_MACHINE_DONE`, `min_concurrency < 1`,
 `max_concurrency < min_concurrency`, `step_count < 1`, or on null /
 non-UTF-8 names.
 */
hegel_result_t hegel_new_state_machine(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       const char *const *rule_names,
                                       const int64_t *rule_groups,
                                       size_t num_rules,
                                       const char *const *invariant_names,
                                       const bool *invariant_always_check,
                                       size_t num_invariants,
                                       int64_t min_concurrency,
                                       int64_t max_concurrency,
                                       int64_t step_count,
                                       hegel_state_machine_t **out_state_machine,
                                       int64_t *out_concurrency);

/*
 Start the machine's next round: make the per-round stop decision (a
 recorded boolean draw with a small stop probability, bounded by the
 machine's `step_count`) and, if the test case continues, draw
 which concurrency group is current for the round. Writes the current
 group's id (its value in the creating `rule_groups`) into
 `*out_group_id` when a new round has begun and the workers should pull
 rules again — the id identifies the round's group, e.g. for trace
 output — or `HEGEL_STATE_MACHINE_DONE` (`INT64_MIN`) to indicate
 termination of the whole state machine. (`hegel_new_state_machine`
 rejects `HEGEL_STATE_MACHINE_DONE` as a group id so it stays
 unambiguous here.)

 Call this on the root test-case handle (the handle used for
 hegel_new_state_machine) at every join point — after each worker's
 `hegel_state_machine_next_rule` stream is exhausted — including before the
 first rule is requested. This applies to sequential machines too: the
 frontend must advance the group when the rule stream is exhausted, even
 though there is only a single group.

 `state_machine` must be a handle returned by `hegel_new_state_machine`
 on this test-case family. Returns `HEGEL_E_STOP_TEST` when the
 engine's choice budget is exhausted (the caller should abort the body
 and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_next_group(hegel_context_t *ctx,
                                              hegel_test_case_t *tc,
                                              hegel_state_machine_t *state_machine,
                                              int64_t *out_group_id);

/*
 Draw the index of the next rule for worker `worker_index` to run this
 round, letting the engine choose the rule sequence. The returned index
 is always a rule belonging to the current concurrency group (see
 `hegel_state_machine_next_group`). Swarm testing is applied per worker:
 a random subset of rules is enabled (at least one per group) on the
 worker's first selection and selection is restricted to that subset for
 the rest of the test case.

 `tc` may be any handle of the machine's test-case family: the machine's
 state is family-wide, and the handle only determines which choice
 stream the selection draws land in. At concurrency 1, it's safe to use
 the root handle for everything. At concurrency > 1, each worker should
 draw from its own `hegel_test_case_clone` handle (a single handle may
 be driven by at most one thread at a time), cloned once before the
 first round and kept for the whole test case, while the root handle
 stays with whoever drives `hegel_state_machine_next_group`.

 `worker_index` identifies the calling worker and must satisfy
 `0 <= worker_index < concurrency` (the level drawn at state-machine
 creation and written to `*out_concurrency`);
 an index rather than the handle identifies the worker because a single
 OS thread could hold multiple test-case clones. Draws consult only
 per-worker and per-clone state, so draws on one worker don't affect
 draws on another.

 Writes `HEGEL_STATE_MACHINE_DONE` (`INT64_MIN`) into `*out_rule_index`
 when the worker's round budget is exhausted: stop running rules and wait
 for the next group / join point.

 `state_machine` must be a handle returned by `hegel_new_state_machine`
 on this test-case family. Returns `HEGEL_E_STOP_TEST` when the engine's
 choice budget is exhausted (the caller should abort the body and call
 `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_next_rule(hegel_context_t *ctx,
                                             hegel_test_case_t *tc,
                                             hegel_state_machine_t *state_machine,
                                             int64_t worker_index,
                                             int64_t *out_rule_index);

/*
 Report that the rule most recently returned by
 `hegel_state_machine_next_rule` to worker `worker_index` was rejected:
 an assumption failed before the rule completed, so it should not count
 toward libhegel's budget for the test case. At concurrency 1 the
 current round then does not count toward the step budget; at
 concurrency > 1 the rule does not advance the worker's per-round
 continue/stop decision, so the worker's next
 `hegel_state_machine_next_rule` call retries the slot.

 `worker_index` must satisfy `0 <= worker_index < concurrency`, exactly
 as for `hegel_state_machine_next_rule`.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` when the worker has no
 outstanding rule — no rule has been returned to it this round, its
 current rule was already reported as rejected, or it has already pulled
 another rule.
 */
hegel_result_t hegel_state_machine_rule_rejected(hegel_context_t *ctx,
                                                 hegel_test_case_t *tc,
                                                 hegel_state_machine_t *state_machine,
                                                 int64_t worker_index);

/*
 Decide whether the caller should run invariant `invariant_index` at the
 current join point, writing the decision into `*out_should_check`: true
 unconditionally (consuming no entropy) for an invariant whose
 `invariant_always_check` flag was set at creation, otherwise a
 recorded boolean draw that is true with probability `1 / step_count`
 (the machine's creation-time step count), so each sampled invariant's
 expected number of sampled runs over a full-length test case is one,
 regardless of the step count. The caller owns the machine's guaranteed invariant checks —
 its initial state, and its final state once
 `hegel_state_machine_next_group` signals termination — and should run
 those unconditionally, without calling this.

 `invariant_index` identifies the invariant by its position in the
 creating `invariant_names`. Call once per invariant per join point,
 from the same handle that makes the `hegel_state_machine_next_group`
 calls.

 Returns `HEGEL_OK`, `HEGEL_E_INVALID_ARG` when `invariant_index` is
 outside the machine's registered invariants or `out_should_check` is
 null, or `HEGEL_E_STOP_TEST` when the engine's choice budget is
 exhausted (the caller should abort the body and call
 `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_should_check_invariant(hegel_context_t *ctx,
                                                          hegel_test_case_t *tc,
                                                          hegel_state_machine_t *state_machine,
                                                          int64_t invariant_index,
                                                          bool *out_should_check);

/*
 Release a state-machine handle from `hegel_new_state_machine`. Safe to
 call with NULL (a no-op that returns `HEGEL_OK`), and safe at any point
 in any order relative to freeing the test case or the run. Each handle
 must be freed exactly once; freeing the same handle twice is undefined
 behaviour.
 */
hegel_result_t hegel_state_machine_free(hegel_context_t *ctx, hegel_state_machine_t *state_machine);

/*
 Parameters:
 `p`: Probability of drawing `true`. Must be in `[0.0, 1.0]`; `p = 0.0`
   always yields `false` and `p = 1.0` always yields `true` without
   consuming entropy.
 `forced` / `has_forced`: When `has_forced` is set, the result is
   forced to `forced`.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_boolean(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      double p,
                                      bool forced,
                                      bool has_forced,
                                      bool *out_value);

/*
 Parameters:
 `min_value` / `max_value`: Inclusive bounds. Both required.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_integer(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      int64_t min_value,
                                      int64_t max_value,
                                      int64_t *out_value);

/*
 Parameters:
 `min_value` / `max_value`: Inclusive bounds as two's-complement
   little-endian signed byte buffers. Both required and must be
   non-empty.
 `out_value`: Receives the drawn value's two's-complement little-endian
   bytes. libhegel sign-fills the rest of the buffer up to
   `out_value_cap`, so reading the whole buffer as a fixed-width integer
   also yields the drawn value with no sign extension needed.
 `out_value_len`: Receives the value's minimal length. Passing
   `out_value_cap >= max(min_value_len, max_value_len)` always succeeds.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.

 Use this for bounds outside the `int64_t` range; otherwise prefer
 `hegel_generate_integer`.
 */
hegel_result_t hegel_generate_integer_big(hegel_context_t *ctx,
                                          hegel_test_case_t *tc,
                                          const uint8_t *min_value,
                                          size_t min_value_len,
                                          const uint8_t *max_value,
                                          size_t max_value_len,
                                          uint8_t *out_value,
                                          size_t out_value_cap,
                                          size_t *out_value_len);

/*
 Parameters:
 `width`: 32 or 64. 32 bit bounds must be exactly representable as
   `float`, and finite 32 bit results are exactly representable as
   `float`.
 `min_value` / `max_value`: Inclusive bounds. Pass `-INFINITY` /
   `INFINITY` for unbounded ends.
 `allow_nan`: NaN is drawn only when this is set.
 `allow_infinity`: Infinities are drawn only when this is set and the
   corresponding endpoint is unbounded.
 `exclude_min` / `exclude_max`: Make the corresponding bound exclusive
   by stepping it to the next representable value at the requested width.
 `smallest_nonzero_magnitude`: Nonzero magnitudes below this are never
   drawn. Must be positive and finite; pass `5e-324` (width 64) or the
   smallest `float` subnormal (width 32) for no restriction.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_float(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    uint32_t width,
                                    double min_value,
                                    double max_value,
                                    bool allow_nan,
                                    bool allow_infinity,
                                    bool exclude_min,
                                    bool exclude_max,
                                    double smallest_nonzero_magnitude,
                                    double *out_value);

/*
 Parameters:
 `min_size` / `max_size`: Inclusive length bounds.
 `out_result`: Receives a libhegel-allocated
   `{uint8_t *data; size_t len;}` the caller owns. `data` is never NULL
   after a successful draw. Release with
   `hegel_generate_bytes_result_free`.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_bytes(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    uint64_t min_size,
                                    uint64_t max_size,
                                    hegel_generate_bytes_result_t *out_result);

/*
 Parameters:
 `result`: Released and reset to `{NULL, 0}`. Safe to call with NULL or
   an already-freed (zeroed) struct.

 Returns `HEGEL_OK`.

 Freeing the buffer any other way is undefined behavior.
 */
hegel_result_t hegel_generate_bytes_result_free(hegel_context_t *ctx,
                                                hegel_generate_bytes_result_t *result);

/*
 Parameters:
 `min_size` / `max_size`: Inclusive length bounds, in characters.
 `codec`: The alphabet's starting range: `"ascii"`, `"latin-1"` /
   `"iso-8859-1"`, or `"utf-8"` / NULL for Unicode.
 `min_codepoint` / `max_codepoint`: Intersected with the codec's range.
   Pass `0` and `UINT32_MAX` for no constraint. Surrogates are always
   removed.
 `categories`: Restricts to the union of the named Unicode general
   categories. NULL means no restriction. A non-NULL empty list means an
   empty alphabet.
 `exclude_categories`: Removes the named categories.
 `include_characters` / `exclude_characters`: UTF-8 buffers (pointer
   plus byte length) of individual characters. Characters in
   `include_characters` are included first, then characters in
   `exclude_characters` are removed.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for constraints that leave
 no characters while `max_size > 0`.
 */
hegel_result_t hegel_string_generator_text(hegel_context_t *ctx,
                                           uint64_t min_size,
                                           uint64_t max_size,
                                           const char *codec,
                                           uint32_t min_codepoint,
                                           uint32_t max_codepoint,
                                           const char *const *categories,
                                           size_t categories_len,
                                           const char *const *exclude_categories,
                                           size_t exclude_categories_len,
                                           const uint8_t *include_characters,
                                           size_t include_characters_len,
                                           const uint8_t *exclude_characters,
                                           size_t exclude_characters_len,
                                           hegel_string_generator_t **out_generator);

/*
 Parameters:
 `pattern`: The pattern to match, in Python `re` syntax.
 `fullmatch`: When true, the whole string must match the pattern.
   Otherwise, the match may be padded on either side.
 `alphabet`: Optional (NULL for none). Must be a text generator. Its
   character set constrains the padding and wildcard characters.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_string_generator_regex(hegel_context_t *ctx,
                                            const char *pattern,
                                            bool fullmatch,
                                            const hegel_string_generator_t *alphabet,
                                            hegel_string_generator_t **out_generator);

/*
 Returns `HEGEL_OK`. Produces RFC 5321/5322 addresses like
 `alice@example.com`.
 */
hegel_result_t hegel_string_generator_email(hegel_context_t *ctx,
                                            hegel_string_generator_t **out_generator);

/*
 Returns `HEGEL_OK`. Produces RFC 3986 `http`/`https` URLs.
 */
hegel_result_t hegel_string_generator_url(hegel_context_t *ctx,
                                          hegel_string_generator_t **out_generator);

/*
 Parameters:
 `max_length`: Total length of the fully-qualified domain name, in
   `4..=255`.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for a `max_length` that
 leaves no eligible top-level domains.
 */
hegel_result_t hegel_string_generator_domain(hegel_context_t *ctx,
                                             uint64_t max_length,
                                             hegel_string_generator_t **out_generator);

/*
 Parameters:
 `generator`: The generator to release. Safe to call with NULL.

 Returns `HEGEL_OK`.

 Each generator must be freed exactly once, and only after every draw
 using it has completed.
 */
hegel_result_t hegel_string_generator_free(hegel_context_t *ctx,
                                           hegel_string_generator_t *generator);

/*
 Parameters:
 `generator`: A generator built by one of the constructors above.
 `out_result`: Receives a libhegel-allocated
   `{char *data; size_t len;}` the caller owns. Not NUL-terminated, and
   it may contain interior NUL bytes since the drawn alphabet can include
   U+0000, so always use `len`. Release with
   `hegel_generate_string_result_free`.

 Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_ASSUME` when the
 draw rejected itself (for example an email exceeding the RFC length
 cap).
 */
hegel_result_t hegel_generate_string(hegel_context_t *ctx,
                                     hegel_test_case_t *tc,
                                     const hegel_string_generator_t *generator,
                                     hegel_generate_string_result_t *out_result);

/*
 Parameters:
 `result`: Released and reset to `{NULL, 0}`. Safe to call with NULL or
   an already-freed (zeroed) struct.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_generate_string_result_free(hegel_context_t *ctx,
                                                 hegel_generate_string_result_t *result);

/*
 Parameters:
 `min_value` / `max_value`: Inclusive bounds, as proleptic Gregorian
   dates with `year` in `[-999999, 999999]`. Pass `{1, 1, 1}` and
   `{9999, 12, 31}` for the full range.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.

 Shrinks toward 2000-01-01 or the nearest bound when that is out of
 range.
 */
hegel_result_t hegel_generate_date(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_date_t min_value,
                                   hegel_date_t max_value,
                                   hegel_date_t *out_value);

/*
 Parameters:
 `min_value` / `max_value`: Inclusive bounds. Pass all-zeros and
   `{23, 59, 59, 999999999}` for the full day.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.

 Shrinks toward `min_value`, the representable time closest to midnight.
 */
hegel_result_t hegel_generate_time(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_time_t min_value,
                                   hegel_time_t max_value,
                                   hegel_time_t *out_value);

/*
 Parameters:
 `min_value` / `max_value`: Inclusive bounds on a naive datetime (no
   timezone).

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.

 Shrinks toward 2000-01-01T00:00:00 or the nearest bound when that is out
 of range.
 */
hegel_result_t hegel_generate_datetime(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       hegel_datetime_t min_value,
                                       hegel_datetime_t max_value,
                                       hegel_datetime_t *out_value);

/*
 Parameters:
 `version` / `has_version`: When `has_version` is set, the RFC 4122
   version nibble is forced to `version` (0..=15, conventionally 1..=5)
   and the variant nibble to the RFC 4122 variant. Without a version the
   128 bits are uniform, except that the nil UUID is never produced.
 `out_bytes`: Receives 16 big-endian bytes.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_uuid(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   uint8_t version,
                                   bool has_version,
                                   uint8_t *out_bytes);

/*
 Parameters:
 `out_bytes`: Receives the address's 4 network-order bytes.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_ipv4(hegel_context_t *ctx, hegel_test_case_t *tc, uint8_t *out_bytes);

/*
 Parameters:
 `out_bytes`: Receives the address's 16 network-order bytes.

 Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
 */
hegel_result_t hegel_generate_ipv6(hegel_context_t *ctx, hegel_test_case_t *tc, uint8_t *out_bytes);

/*
 Parameters:
 `value`: A numeric observation. Must be finite. Higher is "more
   interesting." libhegel biases later test cases toward inputs that
   produced higher observations under the same label.
 `label`: Non-NULL, valid UTF-8. Each label may be recorded at most
   once per test case.

 Returns `HEGEL_OK`.

 Has no effect unless `HEGEL_PHASE_TARGET` is enabled.
 */
hegel_result_t hegel_target(hegel_context_t *ctx,
                            hegel_test_case_t *tc,
                            double value,
                            const char *label);

/*
 Record an event for the current test case, for the end-of-run
 statistics report: the report shows, per label, the fraction of
 generation-phase test cases in which the label was recorded at least
 once. The report prints only when the `show_statistics` setting is on
 (`hegel_settings_set_show_statistics`); without it events cost almost
 nothing and report nothing.

 Parameters:
 `label`: Non-NULL, valid UTF-8.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` on a null / non-UTF-8
 label.
 */
hegel_result_t hegel_event(hegel_context_t *ctx, hegel_test_case_t *tc, const char *label);

/*
 Record a numeric observation under `label` for the current test case,
 for the end-of-run statistics report: the report shows, per label, a
 summary of the observed distribution (count, min, median, mean, p90,
 max) over generation-phase test cases. The report prints only when the
 `show_statistics` setting is on
 (`hegel_settings_set_show_statistics`); without it observations cost
 almost nothing and report nothing.

 Parameters:
 `value`: The observation. Must be finite.
 `label`: Non-NULL, valid UTF-8. Unlike `hegel_target`, a label may be
   observed any number of times per test case.

 Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` on a null / non-UTF-8
 label or a non-finite value.
 */
hegel_result_t hegel_event_value(hegel_context_t *ctx,
                                 hegel_test_case_t *tc,
                                 double value,
                                 const char *label);

/*
 Create a printer-options handle with every option at its default
 (`max_width` 79).

 On success writes a caller-owned handle into `*out_options` (release with
 `hegel_printer_options_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_options`.
 */
hegel_result_t hegel_printer_options_new(hegel_context_t *ctx,
                                         hegel_printer_options_t **out_options);

/*
 Free an options handle previously returned by `hegel_printer_options_new`.
 Safe to call with NULL (a no-op that returns `HEGEL_OK`).
 */
hegel_result_t hegel_printer_options_free(hegel_context_t *ctx, hegel_printer_options_t *options);

/*
 Set the line width documents constructed with these options are laid out
 to: lines stay within `max_width` characters where the group structure
 allows it. The default is 79.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `options` and
 `HEGEL_E_INVALID_ARG` for a `max_width` of 0 (a document cannot lay
 anything out inside zero columns).
 */
hegel_result_t hegel_printer_options_set_max_width(hegel_context_t *ctx,
                                                   hegel_printer_options_t *options,
                                                   uint64_t max_width);

/*
 Create a standalone pretty-printer document laid out per `options`
 (NULL for all defaults; see `hegel_printer_options_t`).

 On success writes a caller-owned handle into `*out_printer` (release with
 `hegel_printer_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_printer`.
 */
hegel_result_t hegel_printer_new(hegel_context_t *ctx,
                                 const hegel_printer_options_t *options,
                                 hegel_printer_t **out_printer);

/*
 Release a printer handle (from `hegel_printer_new`,
 `hegel_printer_deferred`, or `hegel_test_case_printer`). Safe to call
 with NULL (a no-op that returns `HEGEL_OK`). Freeing a handle never
 discards document content — a deferred slot's content stays spliced in —
 it only releases this reference to the shared document.
 */
hegel_result_t hegel_printer_free(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Emit `len` bytes of UTF-8 at `text` only if the innermost group open at
 this point renders broken; a group that fits on one line renders nothing
 here. The text never counts toward width (measurement uses the flat
 form, which is empty).

 This is how a layout expresses text that only the multi-line form needs
 — e.g. Go's mandatory trailing comma before a composite literal's
 closing brace: emit each element, then
 `hegel_printer_if_break(",")` and an empty `hegel_printer_breakable`
 before the `hegel_printer_end_group` that closes the literal.

 The text must not contain newlines. Errors as `hegel_printer_text`.
 */
hegel_result_t hegel_printer_if_break(hegel_context_t *ctx,
                                      hegel_printer_t *printer,
                                      const uint8_t *text,
                                      size_t len);

/*
 Emit `len` bytes of UTF-8 at `text` as literal, unbreakable text.

 The text must not contain newlines: express line structure with
 `hegel_printer_hard_break` (or breakable points) so column accounting
 stays correct. Returns `HEGEL_E_INVALID_HANDLE` — with a diagnostic in
 `hegel_context_last_error` — for a NULL `printer` or a handle whose
 deferred slot is already dead, and `HEGEL_E_INVALID_ARG` for non-UTF-8
 or newline-containing text or a NULL `text` with `len > 0`.
 */
hegel_result_t hegel_printer_text(hegel_context_t *ctx,
                                  hegel_printer_t *printer,
                                  const uint8_t *text,
                                  size_t len);

/*
 Emit a potential break point: renders as the given separator if the
 enclosing group fits on one line, and as a newline plus the current
 indentation if the group breaks.

 `sep` follows the same rules as `hegel_printer_text` (UTF-8, no
 newlines, NULL only with `len == 0`), and errors are reported the same
 way.
 */
hegel_result_t hegel_printer_breakable(hegel_context_t *ctx,
                                       hegel_printer_t *printer,
                                       const uint8_t *sep,
                                       size_t len);

/*
 Attach a comment to the line currently being written: the text is
 emitted verbatim at the end of that line, every group open at this
 position is forced to break — a comment poisons the rest of its line, so
 nothing else may share it — and the text contributes nothing to width
 accounting. A comment-forced group also breaks before its closing text,
 so a trailing delimiter is not annotated by a comment on the group's last
 element.

 The engine stores the text verbatim: pass the full rendered form of the
 comment, in the comment syntax of the language being printed (e.g.
 `"  // like this"` or `"  (* like this *)"`), including any separating
 whitespace.

 `text` follows the same rules as `hegel_printer_text` (UTF-8, no
 newlines, NULL only with `len == 0`), and errors are reported the same
 way.
 */
hegel_result_t hegel_printer_comment(hegel_context_t *ctx,
                                     hegel_printer_t *printer,
                                     const uint8_t *text,
                                     size_t len);

/*
 Emit an unconditional newline followed by the current indentation.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a handle whose
 deferred slot is already dead.
 */
hegel_result_t hegel_printer_hard_break(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Open a group: emit `open` (same rules as `hegel_printer_text`), then
 increase the indentation applied by subsequent break points by `indent`.
 Whether to break is decided per group — a group either fits on the
 current line or every one of its break points becomes a newline.

 Errors as `hegel_printer_text`.
 */
hegel_result_t hegel_printer_begin_group(hegel_context_t *ctx,
                                         hegel_printer_t *printer,
                                         uint64_t indent,
                                         const uint8_t *open,
                                         size_t open_len);

/*
 Close the innermost group: undo the indentation its
 `hegel_printer_begin_group` added, then emit `close` (same rules as
 `hegel_printer_text`).

 Errors as `hegel_printer_text`; closing with no group open is
 `HEGEL_E_INVALID_ARG` (reported by `hegel_printer_resolve` instead when
 the unbalanced close was recorded into a deferred session).
 */
hegel_result_t hegel_printer_end_group(hegel_context_t *ctx,
                                       hegel_printer_t *printer,
                                       const uint8_t *close,
                                       size_t close_len);

/*
 Adjust the indentation applied by subsequent break points by `delta`
 (may be negative to undo an earlier shift).

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a handle whose
 deferred slot is already dead.
 */
hegel_result_t hegel_printer_shift_indent(hegel_context_t *ctx,
                                          hegel_printer_t *printer,
                                          int64_t delta);

/*
 Open a deferred hole at the handle's current position and write a
 caller-owned handle for it into `*out_printer` (release with
 `hegel_printer_free`).

 Content written through the returned handle — at any later point, e.g.
 while the test body runs — is spliced in at the hole's position when
 `hegel_printer_resolve` runs on the document's root handle, with
 line-breaking behaving exactly as if it had been printed inline. After
 resolve the slot is dead and writes to it return
 `HEGEL_E_INVALID_HANDLE`; use `hegel_printer_is_live` to probe. Holes
 nest: calling this on a deferred handle opens a hole inside that slot.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a dead slot
 handle and `HEGEL_E_INVALID_ARG` for a NULL `out_printer`.
 */
hegel_result_t hegel_printer_deferred(hegel_context_t *ctx,
                                      hegel_printer_t *printer,
                                      hegel_printer_t **out_printer);

/*
 Open a speculative region on this handle: subsequent writes through it
 buffer until `hegel_printer_commit_speculative` emits them or
 `hegel_printer_abort_speculative` discards them. Regions nest. This is
 how draw-time printing survives rejection: print each attempt inside a
 region, commit on acceptance, abort on rejection.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a dead slot
 handle.
 */
hegel_result_t hegel_printer_begin_speculative(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Close the innermost speculative region on this handle, keeping its
 content.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a dead slot
 handle and `HEGEL_E_INVALID_ARG` — with a diagnostic — when no region is
 open.
 */
hegel_result_t hegel_printer_commit_speculative(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Close the innermost speculative region on this handle, discarding its
 content. Deferred slots opened inside the region die with it.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` or a dead slot
 handle and `HEGEL_E_INVALID_ARG` — with a diagnostic — when no region is
 open.
 */
hegel_result_t hegel_printer_abort_speculative(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Splice every deferred hole's content in at its position and seal the
 document. Must be called on the document's root handle (from
 `hegel_printer_new` / `hegel_test_case_printer`). Sealing ends all
 writing: every slot dies, a speculative region still open on any target —
 a straggler thread caught mid-draw — is aborted (uncommitted content was
 never part of the document), and every later write on any handle reports
 `HEGEL_E_INVALID_HANDLE` like any other dead region, so a straggler's
 late writes are harmless to tolerate.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer`, and
 `HEGEL_E_INVALID_ARG` — with a diagnostic — when called on a deferred
 handle, with no deferred session outstanding, or when a recorded
 `hegel_printer_end_group` turns out to be unbalanced at replay.
 */
hegel_result_t hegel_printer_resolve(hegel_context_t *ctx, hegel_printer_t *printer);

/*
 Write whether this handle can still be written to into `*out_live`:
 `true` for a root handle, and for a deferred handle whose session has not
 yet been resolved or aborted.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer` and
 `HEGEL_E_INVALID_ARG` for a NULL `out_live`.
 */
hegel_result_t hegel_printer_is_live(hegel_context_t *ctx,
                                     hegel_printer_t *printer,
                                     bool *out_live);

/*
 Read everything printed to the document, flushing pending break points.
 Must be called on the document's root handle. Reading seals the document
 exactly like `hegel_printer_resolve` does: open speculative regions on
 any target are aborted and every later write on any handle reports
 `HEGEL_E_INVALID_HANDLE`. Reading again is fine and returns the same
 document.

 On success fills `*out_result` with an engine-allocated UTF-8 buffer the
 caller owns (release with `hegel_printer_value_result_free`) and returns
 `HEGEL_OK`. Returns `HEGEL_E_INVALID_HANDLE` for a NULL `printer`, and
 `HEGEL_E_INVALID_ARG` — with a diagnostic — for a NULL `out_result`, a
 deferred handle, or an unresolved deferred session (call
 `hegel_printer_resolve` first).
 */
hegel_result_t hegel_printer_value(hegel_context_t *ctx,
                                   hegel_printer_t *printer,
                                   hegel_printer_value_result_t *out_result);

/*
 Release a buffer returned by `hegel_printer_value` and reset the struct
 to `{NULL, 0}`. Safe to call with a NULL `result` or an already-freed
 (zeroed) struct — both are no-ops that return `HEGEL_OK`.
 */
hegel_result_t hegel_printer_value_result_free(hegel_context_t *ctx,
                                               hegel_printer_value_result_t *result);

/*
 Fetch a handle onto this test-case handle's *print region* of the family
 document, writing a caller-owned handle into `*out_printer` (release
 with `hegel_printer_free`; the document itself lives as long as any
 handle or the family).

 The family document exists from the family's creation. Each test-case
 handle owns one region of it: the root handle's region is the document
 body, and a `hegel_test_case_clone` handle's region is a hole opened in
 its parent's region at the moment the clone was made. Regions make
 concurrent printing deterministic: a clone's output appears at its
 anchor point — where the clone was created — however the threads that
 produced it were scheduled, and two handles never interleave within one
 region. The document remains readable after the case completes, so the
 client can assemble output while drawing and read it back after
 `hegel_mark_complete` (through a root-handle printer).

 `options` may be NULL for defaults (see `hegel_printer_options_t`). The
 first call that explicitly configures `max_width` fixes the document's
 width; later calls may restate it, but a *different* explicit width is
 an error — the width of the shared document cannot be two things.
 Content printed before the width is configured (`hegel_note` never
 configures it) still renders at the configured width: layout happens
 when the document is read.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `tc` and
 `HEGEL_E_INVALID_ARG` — with a diagnostic — for a NULL `out_printer` or a
 width conflict.
 */
hegel_result_t hegel_test_case_printer(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       const hegel_printer_options_t *options,
                                       hegel_printer_t **out_printer);

/*
 Append a note — `len` bytes of UTF-8 at `text` — to this test-case
 handle's print region (see `hegel_test_case_printer` for the region
 model). Each `\n`-separated line of the note becomes its own output
 line, so notes may contain newlines. Notes and drawn values from *one
 handle* appear in the order they were appended; a clone's notes appear
 in the clone's region.

 A note appended while a speculative region is open on the handle's
 region — the client is mid-way through printing a drawn value, and the
 note comes from inside that value's generation — is held back rather
 than spliced into the value's line, and appended once the outermost
 region closes, whether it is committed or aborted. Held notes are lost
 if the document is read first (the writer was a straggler).

 Notes never configure the document's width; they render at whatever
 width ends up configured (default 79).

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `tc` or a handle whose
 region is dead (the document was already read), and
 `HEGEL_E_INVALID_ARG` — with a diagnostic — for non-UTF-8 text or a NULL
 `text` with `len > 0`.
 */
hegel_result_t hegel_note(hegel_context_t *ctx,
                          hegel_test_case_t *tc,
                          const uint8_t *text,
                          size_t len);

/*
 Parameters:
 `status`: A `hegel_status_t` value describing how the test case ended.
 `origin`: Identifies the origin of a failure. Used only when `status`
   is `HEGEL_STATUS_INTERESTING`; NULL otherwise.

 Returns `HEGEL_OK`, or `HEGEL_E_ALREADY_COMPLETE` if called twice on the
 same handle.

 Completion is first-caller-wins and applies to the whole test case: the
 first call from any handle records the outcome, and a later call on a
 different handle is a safe no-op. This function never returns
 `HEGEL_E_CONCURRENT_USE`: if another thread is mid-operation on the
 handle it waits, then completes.

 Choosing an origin string: libhegel groups failures by their `origin`. Two failures with identical
 origins are the same bug and get shrunk together. Each new origin is a
 new bug.

 A library must pass a stable value for the origin, such as the location
 of the failing assertion.
 */
hegel_result_t hegel_mark_complete(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   uint32_t status,
                                   const char *origin);

/*
 Parameters:
 `out_status`: Receives `HEGEL_RUN_STATUS_PASSED`,
   `HEGEL_RUN_STATUS_FAILED`, `HEGEL_RUN_STATUS_ERROR`, or
   `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_run_result_status(hegel_context_t *ctx,
                                       const hegel_run_result_t *r,
                                       hegel_run_status_t *out_status);

/*
 Parameters:
 `out_error`: Receives the run-level error message when the run
   errored — a failed health check, a nondeterministic test, a violated
   engine invariant — or NULL when it completed normally. Owned by the
   run result and valid until `hegel_run_result_free`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_run_result_error(hegel_context_t *ctx,
                                      const hegel_run_result_t *r,
                                      const char **out_error);

/*
 Parameters:
 `out_count`: Receives the number of distinct failures, by origin, that
   the run surfaced.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_run_result_failure_count(hegel_context_t *ctx,
                                              const hegel_run_result_t *r,
                                              size_t *out_count);

/*
 Parameters:
 `index`: 0-based; must be less than the failure count.
 `out_failure`: Receives a caller-owned copy of the failure.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_run_result_failure(hegel_context_t *ctx,
                                        const hegel_run_result_t *r,
                                        size_t index,
                                        hegel_failure_t **out_failure);

/*
 Parameters:
 `f`: The failure to free and the strings read off it. Safe to call
   with NULL.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_failure_free(hegel_context_t *ctx, hegel_failure_t *f);

/*
 Parameters:
 `out_origin`: Receives the origin string the shrinker grouped this
   bug's probes under. Valid until `hegel_failure_free`.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_failure_origin(hegel_context_t *ctx,
                                    const hegel_failure_t *f,
                                    const char **out_origin);

/*
 Parameters:
 `out_blob`: Receives a base64 reproduce blob encoding the minimal
   counterexample's choice sequence, or NULL if libhegel produced none
   for this failure. Valid until `hegel_failure_free`.

 Returns `HEGEL_OK`.

 A blob can be replayed later via `hegel_test_case_from_blob` to
 reproduce the test case exactly. It is only guaranteed to reproduce the
 failure in the version of Hegel in which it was generated.
 */
hegel_result_t hegel_failure_reproduction_blob(hegel_context_t *ctx,
                                               const hegel_failure_t *f,
                                               const char **out_blob);

/*
 Parameters:
 `out_version`: Receives libhegel's version string, e.g. `"0.14.12"`.
   The pointer is static and valid for the program's lifetime.

 Returns `HEGEL_OK`.
 */
hegel_result_t hegel_version(hegel_context_t *ctx, const char **out_version);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* HEGEL_H */
