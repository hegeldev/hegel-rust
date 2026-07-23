/*
 * libhegel — C bindings for Hegel's native property-based testing engine.
 *
 * This header is generated from hegel-c/src/lib.rs by cbindgen. Do not
 * edit it directly; re-run `just c-header` after changing the Rust source.
 *
 * Calling convention
 * ------------------
 * Every function takes a hegel_context_t* as its first argument and returns a
 * hegel_result_t code (HEGEL_OK is zero; negatives are errors), with two
 * exceptions: hegel_context_new, which creates a context and returns it, and
 * hegel_context_last_error, the error-reporting reader, which returns the
 * message pointer directly. Anything else a call produces — a handle, a
 * string, a count — is written through a trailing out-parameter named out_*. A
 * NULL context is allowed and simply opts out of error messages.
 *
 * Pointer ownership
 * -----------------
 * Pointers you pass *into* a libhegel function are always yours. The library
 * reads them during the call and copies whatever it needs to keep, so you may
 * free or reuse the memory as soon as the call returns. This covers strings
 * (char*), byte buffers, and arrays of strings alike.
 *
 * Of the pointers libhegel hands *back* (returned by hegel_context_new, or
 * written to an out-parameter otherwise), you own — and must release with the
 * matching free — every handle from these:
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
 *     hegel_generate_bytes       ->  hegel_generate_bytes_result_free
 *     hegel_generate_string      ->  hegel_generate_string_result_free
 *
 * Every test-case handle you receive — whether from hegel_test_case_from_blob,
 * hegel_next_test_case, or hegel_test_case_clone — is yours and must be
 * released with hegel_test_case_free exactly once.
 *
 * A test case and all clones descended from it are considered to be part of a
 * *family* of test cases. All test cases in a family are independent handles
 * onto one shared underlying test case; the resources associated with the test
 * case are released once its last handle is freed, so a clone keeps
 * working after the handle it was cloned from is freed. For a run-owned handle
 * the run keeps its own internal reference, so freeing your handle is always
 * memory-safe and never disturbs the run's state (this makes it easy to wrap a
 * handle in a garbage-collected language and free it from a finaliser). Note
 * that freeing is not completing, though: a run-owned test case still needs
 * hegel_mark_complete from some handle in its family before the run can
 * advance, so conclude every case before dropping your last handle to it —
 * see hegel_test_case_free.
 *
 * The result and failure snapshots those last two return own their data and
 * are independent of the run: they stay valid after hegel_run_free, so a
 * wrapper can free each object from its own finaliser in any order.
 *
 * The buffers hegel_generate_bytes and hegel_generate_string fill in are
 * yours too — release each with its matching result-free function above.
 *
 * Every *other* pointer libhegel hands back is a borrowed string: libhegel
 * still owns it, you must not free it, and it is valid only until a point
 * that the function documents. Strings read off a result or failure
 * snapshot (hegel_run_result_error, the hegel_failure_* getters) live until
 * that snapshot's free; hegel_context_last_error is invalidated by the next
 * call on that context. Copy them to keep them.
 */

#ifndef HEGEL_H
#define HEGEL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

/*
 Value written to `*out_rule_index` by `hegel_state_machine_next_rule`
 when the calling thread's round budget is exhausted (stop running rules
 and wait for the next group / join point), and to `*out_group_index` by
 `hegel_state_machine_next_group` when the whole state machine is done
 (run no further rounds).
 */
#define HEGEL_STATE_MACHINE_DONE -1

/*
 Result of a libhegel call.

 Every entry point returns one of these except `hegel_context_new` (which
 returns a context) and `hegel_context_last_error` (which returns the message
 pointer). `HEGEL_OK` is zero; every error is negative, so `result != HEGEL_OK`
 (or `result < 0`) tests for failure. Anything else a call produces — a
 handle, a string, a count — is written through a trailing `out_*` parameter.
 For the error variants that carry a diagnostic, the message is on the call's
 context — read it with `hegel_context_last_error()`.
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
     required, inverted bounds, a non-UTF-8 string, etc. See
     `hegel_context_last_error()` for specifics.
     */
    HEGEL_E_INVALID_ARG = -5,
    /*
     `hegel_mark_complete` (or a primitive on the same handle) was called
     for a test case that has already been completed.
     */
    HEGEL_E_ALREADY_COMPLETE = -6,
    /*
     Something was read before it was ready: `hegel_next_test_case` was
     called without first completing the previous test case with
     `hegel_mark_complete`, or `hegel_run_result` was called before the run
     finished (`hegel_next_test_case` has not yet reported completion).
     */
    HEGEL_E_NOT_COMPLETE = -7,
    /*
     An internal invariant failed inside libhegel. Should not happen in
     practice; please file a bug. See `hegel_context_last_error()` for the
     diagnostic.
     */
    HEGEL_E_INTERNAL = -8,
    /*
     A single test-case handle was used from two threads at once. Each
     handle may be driven by at most one thread at a time; to generate from
     several threads, `hegel_test_case_clone` the handle and give each
     thread its own clone. (Clones share the underlying test case's
     outcome and budgets but generate from independent streams, so they
     may be driven concurrently and deterministically.)
     Returned by the draw primitives; `hegel_mark_complete` instead waits
     for the in-flight operation, because completion always succeeds under
     first-caller-wins.
     */
    HEGEL_E_CONCURRENT_USE = -9,
} hegel_result_t;

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
 `hegel_settings_set_phases`.

 `hegel_settings_set_phases` takes a bitwise OR of these values (e.g.
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
     database path + `hegel_settings_set_database_key`).
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
 `hegel_settings_set_suppress_health_check`.

 `hegel_settings_set_suppress_health_check` takes a bitwise OR of these values
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
    /*
     Span around one regex string draw. Emitted internally by
     `hegel_generate_string`; callers normally never open this span
     themselves. Likewise for the other engine-side compound draws below.
     */
    HEGEL_LABEL_REGEX = 17,
    /*
     Span around one email-address draw (`hegel_generate_string`).
     */
    HEGEL_LABEL_EMAIL = 18,
    /*
     Span around one URL draw (`hegel_generate_string`).
     */
    HEGEL_LABEL_URL = 19,
    /*
     Span around one domain-name draw (`hegel_generate_string`).
     */
    HEGEL_LABEL_DOMAIN = 20,
    /*
     Span around one date draw (`hegel_generate_date`).
     */
    HEGEL_LABEL_DATE = 21,
    /*
     Span around one time draw (`hegel_generate_time`).
     */
    HEGEL_LABEL_TIME = 22,
    /*
     Span around one datetime draw (`hegel_generate_datetime`).
     */
    HEGEL_LABEL_DATETIME = 23,
    /*
     Span around one UUID draw (`hegel_generate_uuid`).
     */
    HEGEL_LABEL_UUID = 24,
    /*
     Span around one IP-address draw (`hegel_generate_ipv4` /
     `hegel_generate_ipv6`).
     */
    HEGEL_LABEL_IP_ADDRESS = 25,
    /*
     Span around one integer draw (`hegel_generate_integer` /
     `hegel_generate_integer_big`). Emitted internally, like every
     per-draw label: same-label spans are what the engine's mutation
     machinery duplicates to propose repeated values.
     */
    HEGEL_LABEL_INTEGER = 26,
    /*
     Span around one float draw (`hegel_generate_float`).
     */
    HEGEL_LABEL_FLOAT = 27,
    /*
     Span around one boolean draw (`hegel_generate_boolean`).
     */
    HEGEL_LABEL_BOOLEAN = 28,
    /*
     Span around one bytes draw (`hegel_generate_bytes`).
     */
    HEGEL_LABEL_BYTES = 29,
    /*
     Span around one text string draw (`hegel_generate_string` with a
     text generator).
     */
    HEGEL_LABEL_STRING = 30,
    /*
     Span around one concurrency-level draw
     (`hegel_generate_concurrency`).
     */
    HEGEL_LABEL_CONCURRENCY = 31,
} hegel_label_t;

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
 `hegel_settings_set_backend`.

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
 `hegel_settings_set_verbosity`.

 - `HEGEL_VERBOSITY_QUIET`: nothing besides the final result.
 - `HEGEL_VERBOSITY_NORMAL`: a short summary line per run (default).
 - `HEGEL_VERBOSITY_VERBOSE`: per-test-case progress and drawn values,
   panic diagnostics as they happen.
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
 Outcome of a single test case. Passed to `hegel_mark_complete`.

 - `HEGEL_STATUS_VALID`: the test body ran to completion without
   finding an interesting outcome (the property held).
 - `HEGEL_STATUS_INVALID`: an `assume` / precondition rejected this
   draw; the engine should discard it without counting it against
   the test-cases budget.
 - `HEGEL_STATUS_OVERRUN`: the engine ran out of choice budget mid
   test case (typically because a `hegel_generate_*` draw returned
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

 A context carries no output destination: that is chosen per run or test
 case at creation (see `hegel_run_start` / `hegel_test_case_from_blob`).
 */
typedef struct hegel_context_t hegel_context_t;

/*
 One distinct interesting test case surfaced by the run.
 `hegel_run_result_failure` writes a caller-owned snapshot that owns its
 strings: reading them via `hegel_failure_origin` /
 `_reproduction_blob` returns `const char*` pointers that stay valid until
 the failure is released with `hegel_failure_free`. The snapshot is
 independent of the result and run it came from.

 A failure carries the origin the engine grouped on and the reproduce blob.
 The caller replays the blob (via `hegel_test_case_from_blob`) to produce
 the diagnostic and re-raise the test's own failure.
 */
typedef struct hegel_failure_t hegel_failure_t;

/*
 In-flight property-test run.

 `hegel_run_start` returns one of these. The caller pulls test cases
 out via `hegel_next_test_case` until it writes NULL through its out
 parameter, then reads the aggregated outcome via `hegel_run_result`,
 and finally frees the handle with `hegel_run_free`. There is no
 background thread: the handle owns the suspended engine as a future,
 and each `hegel_next_test_case` call resumes it on the calling thread
 until it offers the next test case (or finishes).

 Unlike test-case handles (which detect and reject concurrent use),
 a run handle must only be used from one thread at a time: calling
 `hegel_next_test_case`, `hegel_run_result`, or `hegel_run_free`
 concurrently on the same run is undefined behavior. In particular,
 do not free a run from a garbage-collector finalizer thread while
 another thread may still be using it.
 */
typedef struct hegel_run_t hegel_run_t;

/*
 Aggregated outcome of a finished run. `hegel_run_result` writes a
 caller-owned snapshot of it: read the passed / failed / errored status via
 `hegel_run_result_status`, the number of distinct failures via
 `hegel_run_result_failure_count`, each failure via
 `hegel_run_result_failure(r, i)`, and — for an errored run — the
 run-level error message via `hegel_run_result_error`. The snapshot is
 independent of the run (it stays valid after `hegel_run_free`) and must be
 released with `hegel_run_result_free`; the strings read off it live until
 then.
 */
typedef struct hegel_run_result_t hegel_run_result_t;

/*
 Settings handle for a libhegel run.

 Construct with `hegel_settings_new`, configure via the
 `hegel_settings_*` family of setters, hand to `hegel_run_start`, then
 free with `hegel_settings_free`. Settings can be reused across
 multiple runs; the engine reads them at `hegel_run_start` time.

 A settings handle may be shared across threads once configured — e.g.
 built once and then handed to `hegel_run_start` from several threads
 concurrently. The `hegel_settings_set_*` setters mutate the handle, so
 each setter call requires exclusive access: do not call one concurrently
 with any other use of the same handle.
 */
typedef struct hegel_settings_t hegel_settings_t;

/*
 Opaque specification of a string draw — the alphabet-and-shape half of
 `hegel_generate_string`.

 Build one with a `hegel_string_generator_*` constructor (text, regex,
 email, url, domain); every parameter is validated at construction so a
 bad alphabet or pattern is reported immediately rather than mid-draw.
 A generator is immutable after construction and may be shared freely
 across test cases and threads. Free it with
 `hegel_string_generator_free` once no draws will use it again.
 */
typedef struct hegel_string_generator_t hegel_string_generator_t;

/*
 One in-flight test-case handle handed to the caller by
 `hegel_next_test_case`, `hegel_test_case_from_blob`, or
 `hegel_test_case_clone`. The caller drives it with the per-test-case
 primitives (the `hegel_generate_*` draws, `hegel_start_span` /
 `hegel_stop_span`, `hegel_target`, the collection primitives) and
 concludes it with `hegel_mark_complete`.

 A single handle must be driven by at most one thread at a time: If
 multiple threads attempt to use the handle at the same time, operations
 may raise `HEGEL_E_CONCURRENT_USE` on contention. To use a test case from
 several threads, clone the handle with `hegel_test_case_clone` and give
 each thread its own clone.

 Every handle — however it was produced — must be released with
 `hegel_test_case_free`
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
 `[0, 59]`, `microsecond` in `[0, 999999]`.
 */
typedef struct {
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
    uint32_t microsecond;
} hegel_time_t;

/*
 A drawn naive datetime (a date plus a time of day, no timezone).
 */
typedef struct {
    hegel_date_t date;
    hegel_time_t time;
} hegel_datetime_t;

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
 with NULL (a no-op that returns `HEGEL_OK`). The `ctx` argument is the
 context being freed; there is no separate error context to report into.
 */
hegel_result_t hegel_context_free(hegel_context_t *ctx);

/*
 Most recent error message recorded on `ctx`, or the empty string if the
 most recent call taking this context succeeded. Returns NULL only when
 `ctx` itself is NULL.

 This is the error-reporting reader, not a normal `hegel_*` call: it is the
 one function (besides `hegel_context_new`) that does not follow the
 `hegel_result_t` + `out_*` convention. It returns the message pointer
 directly so a caller can read it straight after the call it is diagnosing,
 and it does not reset the stored message.

 The returned pointer borrows `ctx`'s internal buffer and is invalidated by
 the next libhegel call that takes the same `ctx` — copy the bytes before
 making another such call.
 */
const char *hegel_context_last_error(const hegel_context_t *ctx);

/*
 Allocate a new settings handle initialised with libhegel's defaults
 (100 test cases, all phases enabled, normal verbosity, no seed,
 the default disk database under `.hegel/`), writing it into
 `*out_settings`. When a CI environment is detected (via `CI`,
 `GITHUB_ACTIONS`, and similar environment variables) the defaults
 change: the database is disabled and derandomization is enabled. Use
 the explicit setters to override either. Must be paired with a
 `hegel_settings_free` call. Returns `HEGEL_E_INVALID_ARG` if
 `out_settings` is NULL.

 See `hegel_settings_t` for the threading contract: a configured handle
 may be shared across threads, but each setter call requires exclusive
 access.
 */
hegel_result_t hegel_settings_new(hegel_context_t *ctx, hegel_settings_t **out_settings);

/*
 Free a settings handle previously returned by `hegel_settings_new`.
 Safe to call with NULL (a no-op that returns `HEGEL_OK`).
 */
hegel_result_t hegel_settings_free(hegel_context_t *ctx, hegel_settings_t *s);

/*
 Set whether the engine should drive a full run loop or stop after
 one test case. `mode` is a `hegel_mode_t` value; the parameter is typed
 as `uint32_t` so an out-of-range value from a miscast argument is a
 reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.
 */
hegel_result_t hegel_settings_set_mode(hegel_context_t *ctx, hegel_settings_t *s, uint32_t mode);

/*
 Select the engine's randomness backend. `backend` is a `hegel_backend_t`
 value; the parameter is typed as `uint32_t` so an out-of-range value is a
 reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.

 `HEGEL_BACKEND_AUTO` is the default and leaves the automatic choice in
 place; `HEGEL_BACKEND_DEFAULT` / `HEGEL_BACKEND_URANDOM` pin an explicit
 backend, overriding the automatic detection. Like the underlying setting,
 pinning is one-way: there is no way to un-pin back to AUTO on a handle
 once an explicit backend has been set.
 */
hegel_result_t hegel_settings_set_backend(hegel_context_t *ctx,
                                          hegel_settings_t *s,
                                          uint32_t backend);

/*
 Maximum number of valid test cases to run before declaring the
 property held. The default is 100. Note that this counts *valid*
 cases — assumed-rejected ones don't count against the budget, but
 see `HEGEL_HC_FILTER_TOO_MUCH` for the limit on consecutive
 rejections.
 */
hegel_result_t hegel_settings_set_test_cases(hegel_context_t *ctx, hegel_settings_t *s, uint64_t n);

/*
 Set the engine's output verbosity. `v` is a `hegel_verbosity_t` value;
 the parameter is typed as `uint32_t` so an out-of-range value is a
 reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.
 */
hegel_result_t hegel_settings_set_verbosity(hegel_context_t *ctx, hegel_settings_t *s, uint32_t v);

/*
 Set the RNG seed. When `has_seed = true`, `seed` is used to
 initialise generation; when `has_seed = false`, the engine picks a
 fresh random seed at run start (the default). Combined with
 `hegel_settings_set_derandomize(s, true)` this gives reproducible runs.
 */
hegel_result_t hegel_settings_set_seed(hegel_context_t *ctx,
                                       hegel_settings_t *s,
                                       uint64_t seed,
                                       bool has_seed);

/*
 Make the run reproducible: derive the seed from a stable hash of
 `database_key` instead of fresh randomness when no explicit seed is
 supplied. Useful in CI where you want runs of the same test to be
 deterministic but different tests to still see different inputs.
 */
hegel_result_t hegel_settings_set_derandomize(hegel_context_t *ctx,
                                              hegel_settings_t *s,
                                              bool derandomize);

/*
 When `yes = true` (the default), the engine keeps generating after
 the first failure to surface additional *distinct* bugs (different
 origins), and the final `hegel_run_result_t` lists all of them.
 When `false`, the run stops after the first failing example.
 */
hegel_result_t hegel_settings_set_report_multiple_failures(hegel_context_t *ctx,
                                                           hegel_settings_t *s,
                                                           bool yes);

/*
 Declare the run nondeterministic: the test may produce different
 outcomes (or draw different choice sequences) when run on identical
 data — e.g. because it exercises real concurrency. The frontend must
 set this whenever a run may be nondeterministic, typically because the
 test uses concurrent stateful testing.

 When set, the engine reports failures faithfully without attempting
 anything that assumes deterministic replay: it skips data-tree
 recording (and with it novel-prefix generation and the
 nondeterminism mismatch check), span mutation, the per-origin
 verify + shrink pass (and with it the flakiness check — generation
 stops at the first bug, so the run reports at most one failure),
 targeting, and database persistence and reuse. Failures from such a
 run carry no reproduce blob. The configured phases are left
 untouched; they simply don't take effect where this flag overrides
 them.
 */
hegel_result_t hegel_settings_set_nondeterministic(hegel_context_t *ctx,
                                                   hegel_settings_t *s,
                                                   bool nondeterministic);

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
hegel_result_t hegel_settings_set_database(hegel_context_t *ctx,
                                           hegel_settings_t *s,
                                           const char *database);

/*
 Set the database key used to scope stored / replayed examples for this run.
 `key = NULL` clears it (the default).
 */
hegel_result_t hegel_settings_set_database_key(hegel_context_t *ctx,
                                               hegel_settings_t *s,
                                               const char *key);

/*
 Enable a specific set of phases, given as a bitwise OR of `hegel_phase_t`
 values. Phases not included are disabled. The default is `HEGEL_PHASE_ALL`.
 Passing 0 produces a run that does nothing.
 */
hegel_result_t hegel_settings_set_phases(hegel_context_t *ctx,
                                         hegel_settings_t *s,
                                         uint32_t phases);

/*
 Suppress (disable) a set of health checks, given as a bitwise OR of
 `hegel_health_check_t` values. The default is "no suppression"; use this
 when you know a check is going to fire and accept the underlying behavior
 (e.g. you intentionally have a high rejection rate). Each call replaces
 the full set of suppressed checks, so passing 0 clears any previous
 suppression.
 */
hegel_result_t hegel_settings_set_suppress_health_check(hegel_context_t *ctx,
                                                        hegel_settings_t *s,
                                                        uint32_t checks);

/*
 Start a property-test run with the given settings, writing a handle the
 caller pulls test cases out of via `hegel_next_test_case` into `*out_run`.

 This only builds the run: no test case is generated until the first
 `hegel_next_test_case` call, and all engine work happens on the thread
 making those calls. The caller does not need to hold the settings handle
 alive — `hegel_run_start` snapshots the settings it needs.

 `callback` sets where the engine's output for this run goes: each line is
 delivered to it (with `user_data` passed through verbatim) instead of
 stderr, once per line, NUL-terminated UTF-8 of `len` bytes without a
 trailing newline, in a buffer owned by libhegel and valid only for the
 duration of the call. A NULL `callback` leaves the run's output on stderr
 (`user_data` is ignored). The engine emits while it runs inside
 `hegel_next_test_case`, so the callback is invoked on whichever thread
 makes that call, and it — along with whatever `user_data` points to —
 must stay valid until the run has been freed with `hegel_run_free`.
 Because it runs inside `hegel_next_test_case`, while the run handle is in
 use, the callback must not call back into libhegel on the same run (e.g.
 `hegel_next_test_case` or `hegel_run_free`). This sets only the
 *destination*; how much output the engine emits is controlled by
 `hegel_settings_set_verbosity`.

 Returns `HEGEL_E_INVALID_ARG` for a NULL `out_run` or
 `HEGEL_E_INVALID_HANDLE` for a NULL `settings`. The handle written to
 `*out_run` must be freed with `hegel_run_free`.
 */
hegel_result_t hegel_run_start(hegel_context_t *ctx,
                               const hegel_settings_t *settings,
                               hegel_output_callback_t callback,
                               void *user_data,
                               hegel_run_t **out_run);

/*
 Run the engine on the calling thread until it produces the next test case,
 writing a handle for it into `*out_test_case`.

 The handle is owned by the caller and must be released with
 `hegel_test_case_free` (the run keeps its own internal reference, so freeing
 the handle never disturbs the run). When the run is finished this writes
 NULL into `*out_test_case` and returns
 `HEGEL_OK`; call `hegel_run_result` to read the outcome. A non-`HEGEL_OK`
 code means something went wrong (caller misuse, engine crash) rather than
 normal completion: `HEGEL_E_NOT_COMPLETE` if the previous test case was not
 marked complete (call `hegel_mark_complete` first), `HEGEL_E_INVALID_HANDLE`
 for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a NULL `out_test_case`.

 All engine work between test cases — generation, mutation, shrinking —
 happens inside this call, so a call may take a while when the engine has
 exploring to do.
 */
hegel_result_t hegel_next_test_case(hegel_context_t *ctx,
                                    hegel_run_t *run,
                                    hegel_test_case_t **out_test_case);

/*
 Write a caller-owned snapshot of the aggregated result of a finished run
 into `*out_result`. Returns `HEGEL_E_NOT_COMPLETE` with
 `hegel_context_last_error` set if the run hasn't finished yet
 (`hegel_next_test_case` has not yet reported completion on this run),
 `HEGEL_E_INVALID_HANDLE` for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a
 NULL `out_result`.

 The snapshot is independent of the run: it stays valid after
 `hegel_run_free` and must be released with `hegel_run_result_free`. Each
 call writes a fresh snapshot, each freed separately.
 */
hegel_result_t hegel_run_result(hegel_context_t *ctx,
                                hegel_run_t *run,
                                hegel_run_result_t **out_result);

/*
 Release a run-result snapshot from `hegel_run_result`, along with the
 strings read off it. Safe to call with NULL (a no-op that returns
 `HEGEL_OK`). Must be called exactly once per snapshot; freeing the same
 snapshot twice is undefined behaviour.
 */
hegel_result_t hegel_run_result_free(hegel_context_t *ctx, hegel_run_result_t *r);

/*
 Free a run handle. Safe to call with NULL (a no-op that returns
 `HEGEL_OK`). Result and failure snapshots from `hegel_run_result` /
 `hegel_run_result_failure` are independent of the run and stay valid;
 they are released with their own frees.

 If the caller exited its test loop early (e.g. with a still-active
 test case), any in-flight test case is marked complete and the rest of
 the exploration is simply dropped — the engine was suspended waiting for
 the next `hegel_next_test_case` call, so there is nothing to wind down.
 */
hegel_result_t hegel_run_free(hegel_context_t *ctx, hegel_run_t *run);

/*
 Build a standalone test case that replays the example encoded in a
 base64 failure blob (obtained from `hegel_failure_reproduction_blob` on a
 prior run).

 There is no run handle and no engine run: the caller drives the
 returned test case with the usual per-test-case primitives
 (the `hegel_generate_*` draws, spans, …), concludes it with `hegel_mark_complete`,
 and decides for itself whether the blob reproduced the failure (the
 property failed again) or is stale (it passed). Replay several blobs by
 calling this once per blob. A blob whose choices no longer match the
 caller's generators surfaces as `HEGEL_E_STOP_TEST` from the draw that
 overruns. Replaying a blob is how a caller performs the *final replay* of
 a counterexample.

 `callback` sets where the engine's output for this replay goes — at debug
 verbosity the blob is decoded with a trace line, emitted synchronously
 during this call. Each line is delivered to `callback` (with `user_data`
 passed through verbatim) instead of stderr, NUL-terminated UTF-8 of `len`
 bytes without a trailing newline, in a buffer valid only for the duration
 of the call. A NULL `callback` leaves the replay's output on stderr
 (`user_data` is ignored). The callback is only ever invoked on this
 thread and need not outlive this call.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `s`, or `HEGEL_E_INVALID_ARG`
 for a NULL `out_test_case`, a NULL `blob`, or a `blob` that is not a valid
 failure blob (corrupt, non-UTF-8, or from an incompatible Hegel version),
 with a diagnostic in `hegel_context_last_error`. The handle written to
 `*out_test_case` is owned by the **caller** and must be released with
 `hegel_test_case_free`, like every test-case handle.
 */
hegel_result_t hegel_test_case_from_blob(hegel_context_t *ctx,
                                         const hegel_settings_t *s,
                                         const char *blob,
                                         hegel_output_callback_t callback,
                                         void *user_data,
                                         hegel_test_case_t **out_test_case);

/*
 Release a test-case handle, whatever its origin — a handle from
 `hegel_test_case_from_blob`, a clone from `hegel_test_case_clone`, or a
 run-owned handle from `hegel_next_test_case`. Safe to call with NULL (a
 no-op that returns `HEGEL_OK`), and safe whether or not the test case was
 marked complete.

 Each handle holds one reference to the shared test case. Freeing it drops
 that reference; the underlying data source is released once the last
 reference is gone (every handle freed, and — for a run-owned family — the
 run has released its own reference). Each handle must be freed exactly once;
 freeing the same handle twice is undefined behaviour.

 Freeing is not completing: a run-owned test case still needs
 `hegel_mark_complete` from some handle in its family before the run can
 advance. Freeing the last handle of an uncompleted run-owned family leaves
 `hegel_next_test_case` returning `HEGEL_E_NOT_COMPLETE` with no way to
 complete the case, and the run can then only be torn down with
 `hegel_run_free` — so conclude every case before dropping your last handle
 to it.
 */
hegel_result_t hegel_test_case_free(hegel_context_t *ctx, hegel_test_case_t *tc);

/*
 Clone a test-case handle, writing a new handle onto an *independent
 stream* of the same test case into `*out_test_case`.

 The clone shares the test case's outcome — `hegel_mark_complete` on any
 handle in the family marks them all complete, and budgets are shared —
 but generates from its own independent choice sequence. The clone and
 the handle it came from can therefore be driven concurrently from
 different threads without perturbing each other, and the values each
 produces are deterministic under replay and shrink correctly. (Whereas
 using a *single* handle from two threads returns
 `HEGEL_E_CONCURRENT_USE`.) Collections, variable pools, and state
 machines remain shared across the family — ids from one handle work on
 any other — but *concurrent* use of one such object from two streams
 makes the affected values scheduling-dependent.

 Cloning is a stream operation: it occupies one choice position on the
 source handle's stream, takes the source handle's lock like a draw
 (`HEGEL_E_CONCURRENT_USE` if another thread is mid-operation on it), and
 fails with `HEGEL_E_ALREADY_COMPLETE` once the family has completed.
 Cloning a clone creates a further independent stream.

 The new handle holds its own reference to the shared test case and must be
 released with `hegel_test_case_free`, like any other handle. The underlying
 test case stays alive until every handle (this clone, the handle it was
 cloned from, and any others) has been freed.

 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `tc`, or `HEGEL_E_INVALID_ARG`
 for a NULL `out_test_case`.
 */
hegel_result_t hegel_test_case_clone(hegel_context_t *ctx,
                                     const hegel_test_case_t *tc,
                                     hegel_test_case_t **out_test_case);

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
 rejection reason (NULL is allowed); it is validated but currently
 unused, reserved for future rejection diagnostics.
 */
hegel_result_t hegel_collection_reject(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       int64_t collection_id,
                                       const char *why);

/*
 Create a new engine-managed *variable pool* for stateful testing.

 A pool tracks a set of opaque variable ids that the engine can draw
 from and shrink over — the primitive behind hegel-rust's
 `stateful::Pool` and `#[hegel::state_machine]`. The caller keeps
 its own mapping from variable id to the actual value it generated
 (mirroring how `Pool<T>` holds a `HashMap<i64, T>`).

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
 returns `HEGEL_OK`. Returns `HEGEL_E_ASSUME` if the pool currently
 has no active variables — the caller should treat that like any other
 failed assumption: it may recover and continue the test case (as
 stateful testing does when a rule's assumption fails, by skipping the
 action), or give up on the case and mark it INVALID.
 */
hegel_result_t hegel_pool_generate(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   int64_t pool_id,
                                   bool consume,
                                   int64_t *out_variable_id);

/*
 Register a *state machine* for engine-owned stateful (rule-based)
 testing, sequential or concurrent: `num_groups` concurrency groups
 (identified by index only), `num_rules` rules — each assigned to a group
 by `rule_groups`, an array of group indices parallel to `rule_names` —
 and `num_invariants` invariants, with names as NUL-terminated UTF-8,
 plus the concurrency level (the number of worker threads that will pull
 rules; pass the value drawn by `hegel_generate_concurrency`, or 1 for a
 sequential machine).

 The engine owns rule selection — including swarm testing, where each
 thread enables a random subset of rules (at least one per group) and
 selection draws only from that subset. The caller drives execution in
 rounds: on the root test-case handle it asks
 `hegel_state_machine_next_group` whether another round should run, then
 each worker thread asks `hegel_state_machine_next_rule` which rule to
 run and applies it, until that call signals the join point. Rules in
 the same group may run concurrently; rules in different groups never
 overlap.

 Creating the machine draws from the calling handle's stream: the test
 case's round cap and each thread's swarm parameters are decided here,
 up front, so the machine is fully constructed before any rule is
 requested.

 On success writes the new machine's id into `*out_state_machine_id`
 and returns `HEGEL_OK`. The id is opaque; pass it to subsequent
 `hegel_state_machine_next_group` / `hegel_state_machine_next_rule`
 calls on the *same* test-case family. Returns `HEGEL_E_STOP_TEST` when
 the engine's choice budget is exhausted (the caller should abort the
 body and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 Returns `HEGEL_E_INVALID_ARG` if `num_rules` or `num_groups` is zero,
 an entry of `rule_groups` is outside `[0, num_groups)`, a group ends up
 with no rules, `concurrency < 1`, or on null / non-UTF-8 names.
 */
hegel_result_t hegel_new_state_machine(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       size_t num_groups,
                                       const char *const *rule_names,
                                       const int64_t *rule_groups,
                                       size_t num_rules,
                                       const char *const *invariant_names,
                                       size_t num_invariants,
                                       int64_t concurrency,
                                       int64_t *out_state_machine_id);

/*
 Start the machine's next round: draw whether another round should run
 at all and, if so, which concurrency group is current for it and each
 worker thread's step budget for the round. Writes the current group's
 index in `[0, num_groups)` into `*out_group_index` when a new round
 has begun and the worker threads should pull rules again — the index
 identifies the round's group, e.g. for trace output — or
 `HEGEL_STATE_MACHINE_DONE` (-1) to indicate termination of the whole
 state machine.

 Call this on the *root* test-case handle at every join point — after
 each worker thread's `hegel_state_machine_next_rule` stream is
 exhausted — including before the first rule is requested. This applies
 to sequential machines too: the frontend must advance the group when
 the rule stream is exhausted, even though there is only a single
 group. In single-test-case mode (steps unbounded, e.g. under
 Antithesis) `*out_group_index` is never set to
 `HEGEL_STATE_MACHINE_DONE`: rounds continue forever.

 `state_machine_id` must be an id returned by `hegel_new_state_machine`
 on this test-case family. Returns `HEGEL_E_STOP_TEST` when the
 engine's choice budget is exhausted (the caller should abort the body
 and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_next_group(hegel_context_t *ctx,
                                              hegel_test_case_t *tc,
                                              int64_t state_machine_id,
                                              int64_t *out_group_index);

/*
 Draw the index of the next rule for worker thread `thread_index` to run
 this round, letting the engine choose the rule sequence. The returned
 index is always a rule belonging to the current concurrency group (see
 `hegel_state_machine_next_group`). Swarm testing is applied per thread:
 a random subset of rules is enabled (at least one per group) on the
 thread's first selection and selection is restricted to that subset for
 the rest of the test case.

 `thread_index` identifies the calling worker and must satisfy
 `0 <= thread_index < concurrency` (passed at state-machine creation); a
 thread index rather than the handle identifies the thread because a
 single thread could hold multiple test-case clones. Draws consult only
 per-thread and per-clone state, so draws on one thread don't affect
 draws on another.

 Writes `HEGEL_STATE_MACHINE_DONE` (-1) into `*out_rule_index` when the
 thread's round budget is exhausted: stop running rules and wait for the
 next group / join point.

 `state_machine_id` must be an id returned by `hegel_new_state_machine`
 on this test-case family. Returns `HEGEL_E_STOP_TEST` when the engine's
 choice budget is exhausted (the caller should abort the body and call
 `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 */
hegel_result_t hegel_state_machine_next_rule(hegel_context_t *ctx,
                                             hegel_test_case_t *tc,
                                             int64_t state_machine_id,
                                             int64_t thread_index,
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
hegel_result_t hegel_generate_boolean(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      double p,
                                      bool forced,
                                      bool has_forced,
                                      bool *out_value);

/*
 Draw a concurrency level in `[1, max_value]`, for creating a state
 machine via `hegel_new_state_machine`. The engine owns the
 distribution, which is weighted toward `max_value` (concurrency bugs
 need concurrency) rather than shrink-biased toward 1 — which is why
 this is a dedicated primitive instead of a plain integer draw.

 On success writes the drawn level into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice
 budget is exhausted for this test case (the caller should abort the
 body and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
 Returns `HEGEL_E_INVALID_ARG` for a NULL `out_value` or
 `max_value < 1`; the diagnostic is in `hegel_context_last_error`.
 */
hegel_result_t hegel_generate_concurrency(hegel_context_t *ctx,
                                          hegel_test_case_t *tc,
                                          int64_t max_value,
                                          int64_t *out_value);

/*
 Draw an integer in `[min_value, max_value]` (both inclusive, both
 required). The engine biases toward boundary values and shrinks toward
 zero. For bounds outside the `int64_t` range use
 `hegel_generate_integer_big`.

 On success writes the drawn value into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case (the caller should abort the body and
 call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_value` or `min_value > max_value`;
 the diagnostic is in `hegel_context_last_error`.
 */
hegel_result_t hegel_generate_integer(hegel_context_t *ctx,
                                      hegel_test_case_t *tc,
                                      int64_t min_value,
                                      int64_t max_value,
                                      int64_t *out_value);

/*
 Draw an arbitrary-precision integer in `[min_value, max_value]`.

 Bounds and result are two's-complement **little-endian** signed byte
 buffers (the natural encoding of Go's `math/big` `FillBytes` reversed, or
 Rust's `i128::to_le_bytes` for fixed-width values). Both bounds are
 required and must be non-empty.

 On success writes the drawn value's two's-complement little-endian bytes
 into `out_value` (capacity `out_value_cap`), its minimal length into
 `*out_value_len`, sign-fills the rest of the buffer up to
 `out_value_cap` (so reading the whole buffer as a fixed-width
 two's-complement integer also yields the drawn value, with no
 sign-extension needed on the caller's side), and returns `HEGEL_OK`. A
 value in range never needs more bytes than the longer of the two bound
 encodings, so passing
 `out_value_cap >= max(min_value_len, max_value_len)` always succeeds.
 Returns `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted
 for this test case. Returns `HEGEL_E_INVALID_ARG` for NULL or empty
 bounds, NULL out parameters, `min_value > max_value`, or an `out_value`
 buffer too small for the drawn value; the diagnostic is in
 `hegel_context_last_error`.
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
 Draw a float of the given `width` (32 or 64) in
 `[min_value, max_value]`.

 Pass `-INFINITY` / `INFINITY` for unbounded ends. NaN is drawn only when
 `allow_nan` is set; infinities only when `allow_infinity` is set and the
 relevant endpoint is unbounded. `exclude_min` / `exclude_max` make the
 corresponding bound exclusive by stepping it to the next representable
 value at the requested width. Nonzero magnitudes below
 `smallest_nonzero_magnitude` are never drawn — it must be positive and
 finite; pass `5e-324` (width 64) or the smallest `float` subnormal
 (width 32) for no restriction. Width-32 bounds must be exactly
 representable as `float`, and finite width-32 results are exactly
 representable as `float`.

 On success writes the drawn value into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL
 `out_value`, an unsupported width, NaN bounds, width-32 bounds that are
 not exactly representable as `float`, an invalid
 `smallest_nonzero_magnitude`, or an empty range; the diagnostic is in
 `hegel_context_last_error`.
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
 Draw a byte string with length in `[min_size, max_size]` (both
 inclusive).

 On success fills `*out_result` with an engine-allocated buffer the caller
 owns (release with `hegel_generate_bytes_result_free`) and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL
 `out_result` or `min_size > max_size`; the diagnostic is in
 `hegel_context_last_error`.
 */
hegel_result_t hegel_generate_bytes(hegel_context_t *ctx,
                                    hegel_test_case_t *tc,
                                    uint64_t min_size,
                                    uint64_t max_size,
                                    hegel_generate_bytes_result_t *out_result);

/*
 Release a buffer returned by `hegel_generate_bytes` and reset the struct
 to `{NULL, 0}`. Safe to call with a NULL `result` or an already-freed
 (zeroed) struct — both are no-ops that return `HEGEL_OK`.
 */
hegel_result_t hegel_generate_bytes_result_free(hegel_context_t *ctx,
                                                hegel_generate_bytes_result_t *result);

/*
 Build a **text** string generator: strings with length in
 `[min_size, max_size]` whose characters are drawn from the described
 alphabet.

 The alphabet starts from `codec`'s range — `"ascii"`, `"latin-1"` /
 `"iso-8859-1"`, or `"utf-8"` / NULL for all of Unicode — intersected
 with `[min_codepoint, max_codepoint]` (pass `0` and `UINT32_MAX` for no
 constraint; surrogates are always removed). `categories` restricts to
 the union of the named Unicode general categories (NULL for no
 restriction; a non-NULL empty list means an empty alphabet), and
 `exclude_categories` removes categories. `include_characters` /
 `exclude_characters` are UTF-8 buffers (pointer + byte length; NULL for
 none) of individual characters unioned in / removed last. They are
 length-delimited rather than NUL-terminated because U+0000 is a valid
 character to include or exclude.

 On success writes a caller-owned handle into `*out_generator` (release
 with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
 — for a NULL `out_generator`, `min_size > max_size`, an unknown codec or
 category, non-UTF-8 string arguments, include/exclude conflicts, or
 constraints that leave no characters while `max_size > 0`.
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
 Build a **regex** string generator: strings matching `pattern`
 (Python-`re` syntax). When `fullmatch` is true the whole string matches
 the pattern; otherwise the match may be padded on either side.
 `alphabet` — optional (NULL for none) — must be a **text** generator; its
 character set constrains the padding and wildcard characters.

 On success writes a caller-owned handle into `*out_generator` (release
 with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
 — for a NULL `out_generator`, a NULL / non-UTF-8 / invalid `pattern`, or
 an `alphabet` that is not a text generator.
 */
hegel_result_t hegel_string_generator_regex(hegel_context_t *ctx,
                                            const char *pattern,
                                            bool fullmatch,
                                            const hegel_string_generator_t *alphabet,
                                            hegel_string_generator_t **out_generator);

/*
 Build an **email** string generator producing RFC 5321/5322 addresses
 like `alice@example.com`.

 On success writes a caller-owned handle into `*out_generator` (release
 with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_generator`.
 */
hegel_result_t hegel_string_generator_email(hegel_context_t *ctx,
                                            hegel_string_generator_t **out_generator);

/*
 Build a **URL** string generator producing RFC 3986 `http`/`https` URLs.

 On success writes a caller-owned handle into `*out_generator` (release
 with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_generator`.
 */
hegel_result_t hegel_string_generator_url(hegel_context_t *ctx,
                                          hegel_string_generator_t **out_generator);

/*
 Build a **domain-name** string generator producing RFC 1035
 fully-qualified domain names of total length at most `max_length`
 (4..=255; RFC 1035 §2.3.4 allows 255).

 On success writes a caller-owned handle into `*out_generator` (release
 with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
 `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
 — for a NULL `out_generator` or a `max_length` that leaves no eligible
 top-level domains.
 */
hegel_result_t hegel_string_generator_domain(hegel_context_t *ctx,
                                             uint64_t max_length,
                                             hegel_string_generator_t **out_generator);

/*
 Release a string generator built by a `hegel_string_generator_*`
 constructor. Safe to call with NULL (a no-op that returns `HEGEL_OK`).
 Each generator must be freed exactly once, and only after every draw
 using it has completed.
 */
hegel_result_t hegel_string_generator_free(hegel_context_t *ctx,
                                           hegel_string_generator_t *generator);

/*
 Draw a string described by `generator` (built with a
 `hegel_string_generator_*` constructor).

 On success fills `*out_result` with an engine-allocated UTF-8 buffer the
 caller owns (release with `hegel_generate_string_result_free`) and
 returns `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice
 budget is exhausted for this test case (the caller should abort the body
 and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`), and
 `HEGEL_E_ASSUME` when the draw rejected itself (e.g. an email exceeding
 the RFC length cap; discard the test case as invalid). Returns
 `HEGEL_E_INVALID_HANDLE` for a NULL `tc` or `generator`, and
 `HEGEL_E_INVALID_ARG` for a NULL `out_result`.
 */
hegel_result_t hegel_generate_string(hegel_context_t *ctx,
                                     hegel_test_case_t *tc,
                                     const hegel_string_generator_t *generator,
                                     hegel_generate_string_result_t *out_result);

/*
 Release a buffer returned by `hegel_generate_string` and reset the
 struct to `{NULL, 0}`. Safe to call with a NULL `result` or an
 already-freed (zeroed) struct — both are no-ops that return `HEGEL_OK`.
 */
hegel_result_t hegel_generate_string_result_free(hegel_context_t *ctx,
                                                 hegel_generate_string_result_t *result);

/*
 Draw a Gregorian calendar date in `[min_value, max_value]` (both
 inclusive), shrinking toward 2000-01-01, or the nearest bound when that
 is out of range. Bounds are proleptic Gregorian dates with `year` in
 `[-999999, 999999]`; pass `{1, 1, 1}` and `{9999, 12, 31}` for the
 conventional full range.

 On success writes the drawn date into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case (the caller should abort the body and
 call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_value`, an invalid calendar date
 in either bound, or `min_value > max_value`.
 */
hegel_result_t hegel_generate_date(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_date_t min_value,
                                   hegel_date_t max_value,
                                   hegel_date_t *out_value);

/*
 Draw a time of day in `[min_value, max_value]` (both inclusive),
 shrinking toward `min_value` (the representable time closest to
 midnight). Pass all-zeros and `{23, 59, 59, 999999}` for the full day.

 On success writes the drawn time into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a
 NULL `out_value`, an out-of-range field in either bound, or
 `min_value > max_value`.
 */
hegel_result_t hegel_generate_time(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   hegel_time_t min_value,
                                   hegel_time_t max_value,
                                   hegel_time_t *out_value);

/*
 Draw a naive datetime (no timezone) in `[min_value, max_value]` (both
 inclusive), shrinking toward 2000-01-01T00:00:00 clamped into range: a
 bounded date draw, then a time draw whose bounds tighten to the endpoint
 times when the drawn date lands on a boundary date.

 On success writes the drawn datetime into `*out_value` and returns
 `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
 is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a
 NULL `out_value`, an invalid date or time in either bound, or
 `min_value > max_value`.
 */
hegel_result_t hegel_generate_datetime(hegel_context_t *ctx,
                                       hegel_test_case_t *tc,
                                       hegel_datetime_t min_value,
                                       hegel_datetime_t max_value,
                                       hegel_datetime_t *out_value);

/*
 Draw a UUID as 16 big-endian bytes written to `out_bytes` (which must
 have room for 16 bytes).

 When `has_version` is set, the RFC 4122 version nibble is forced to
 `version` (a single hex nibble, 0..=15 — conventionally 1..=5) and the
 variant nibble to the RFC 4122 variant. Without a version the 128 bits
 are uniform, except that the nil UUID is never produced.

 On success writes 16 bytes and returns `HEGEL_OK`. Returns
 `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
 this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL `out_bytes` or
 a `version > 15`.
 */
hegel_result_t hegel_generate_uuid(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   uint8_t version,
                                   bool has_version,
                                   uint8_t *out_bytes);

/*
 Draw an IPv4 address. Half the draws are uniform over the whole address
 space and half are biased into the IANA special-purpose ranges
 (loopback, private, documentation, …).

 On success writes the address's 4 network-order bytes into `out_bytes`
 (which must have room for 4 bytes) and returns `HEGEL_OK`. Returns
 `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
 this test case, and `HEGEL_E_INVALID_ARG` for a NULL `out_bytes`.
 */
hegel_result_t hegel_generate_ipv4(hegel_context_t *ctx, hegel_test_case_t *tc, uint8_t *out_bytes);

/*
 Draw an IPv6 address, with the same special-range biasing as
 `hegel_generate_ipv4`.

 On success writes the address's 16 network-order bytes into `out_bytes`
 (which must have room for 16 bytes) and returns `HEGEL_OK`. Returns
 `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
 this test case, and `HEGEL_E_INVALID_ARG` for a NULL `out_bytes`.
 */
hegel_result_t hegel_generate_ipv6(hegel_context_t *ctx, hegel_test_case_t *tc, uint8_t *out_bytes);

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

 Completing a test case is **first-caller-wins and family-wide**: the first
 `hegel_mark_complete` anywhere in the family (any clone or the root) records
 the outcome and unblocks the run. A later call on a *different* handle in the
 family is then a safe no-op that returns `HEGEL_OK`, so two clones racing to
 complete the same test case do not error — whichever wins sets the result.
 Calling `hegel_mark_complete` on the *same* handle twice is a usage error and
 returns `HEGEL_E_ALREADY_COMPLETE`. Because completion always succeeds under
 first-caller-wins, `hegel_mark_complete` never returns
 `HEGEL_E_CONCURRENT_USE`: if another thread is mid-operation on this handle
 it waits for that operation to finish and then completes. A NULL `tc`
 returns `HEGEL_E_INVALID_HANDLE`; a non-UTF-8 `origin` returns
 `HEGEL_E_INVALID_ARG`.

 `status` is a `hegel_status_t` value; the parameter is typed as
 `uint32_t` so an out-of-range value is a reportable
 `HEGEL_E_INVALID_ARG` instead of undefined behavior.
 */
hegel_result_t hegel_mark_complete(hegel_context_t *ctx,
                                   hegel_test_case_t *tc,
                                   uint32_t status,
                                   const char *origin);

/*
 Write the run's aggregate status into `*out_status`: passed, failed (the
 property has counterexamples — see `hegel_run_result_failure`), or errored
 (the run itself failed and produced no verdict — see
 `hegel_run_result_error`). Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r`
 or `HEGEL_E_INVALID_ARG` for a NULL `out_status`.
 */
hegel_result_t hegel_run_result_status(hegel_context_t *ctx,
                                       const hegel_run_result_t *r,
                                       hegel_run_status_t *out_status);

/*
 Write the run-level error message into `*out_error` when the run ended in
 an error rather than a verdict on the property — a failed health check
 (e.g. FilterTooMuch, TooSlow), a nondeterministic test, or an engine panic
 — or NULL when it completed normally. An errored run has
 `hegel_run_result_status` of `HEGEL_RUN_STATUS_ERROR` and no failures: the
 error is a failure of the run itself, not a counterexample to the property.
 The written pointer is owned by the result snapshot and valid until
 `hegel_run_result_free`. Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r` or
 `HEGEL_E_INVALID_ARG` for a NULL `out_error`.
 */
hegel_result_t hegel_run_result_error(hegel_context_t *ctx,
                                      const hegel_run_result_t *r,
                                      const char **out_error);

/*
 Write the number of *distinct* failures (by origin) the run surfaced into
 `*out_count`. Each can be inspected via `hegel_run_result_failure(r, i)`.
 Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r` or `HEGEL_E_INVALID_ARG`
 for a NULL `out_count`.
 */
hegel_result_t hegel_run_result_failure_count(hegel_context_t *ctx,
                                              const hegel_run_result_t *r,
                                              size_t *out_count);

/*
 Write a caller-owned snapshot of the `index`-th failure (0-based) into
 `*out_failure`. `index` must be less than
 `hegel_run_result_failure_count(r)`. The snapshot is independent of the
 result and run it came from and must be released with
 `hegel_failure_free`; each call writes a fresh snapshot, each freed
 separately. Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r`, or
 `HEGEL_E_INVALID_ARG` for a NULL `out_failure` or an out-of-range `index`
 (with a diagnostic in `hegel_context_last_error`).
 */
hegel_result_t hegel_run_result_failure(hegel_context_t *ctx,
                                        const hegel_run_result_t *r,
                                        size_t index,
                                        hegel_failure_t **out_failure);

/*
 Release a failure snapshot from `hegel_run_result_failure`, along with the
 strings read off it. Safe to call with NULL (a no-op that returns
 `HEGEL_OK`). Must be called exactly once per snapshot; freeing the same
 snapshot twice is undefined behaviour.
 */
hegel_result_t hegel_failure_free(hegel_context_t *ctx, hegel_failure_t *f);

/*
 Write the failure's origin string — the stable identifier the shrinker used
 to group probes for this bug — into `*out_origin`. See `hegel_mark_complete`
 for what makes a good origin string. Returns `HEGEL_E_INVALID_HANDLE` for a
 NULL `f` or `HEGEL_E_INVALID_ARG` for a NULL `out_origin`.
 */
hegel_result_t hegel_failure_origin(hegel_context_t *ctx,
                                    const hegel_failure_t *f,
                                    const char **out_origin);

/*
 Write the failure's reproduce blob — a base64 string encoding the minimal
 counterexample's choice sequence, suitable for deterministic replay via
 `hegel_test_case_from_blob` — into `*out_blob`, or NULL if the engine
 produced no blob for this failure. The written pointer is owned by the
 failure snapshot and stays valid until `hegel_failure_free`. Returns
 `HEGEL_E_INVALID_HANDLE` for a NULL `f` or `HEGEL_E_INVALID_ARG` for a NULL
 `out_blob`.
 */
hegel_result_t hegel_failure_reproduction_blob(hegel_context_t *ctx,
                                               const hegel_failure_t *f,
                                               const char **out_blob);

/*
 Write libhegel's version — matching the parent `hegeltest` crate's
 `CARGO_PKG_VERSION` (e.g. `"0.14.12"`) — into `*out_version`. The written
 pointer is static and valid for the program's lifetime. Returns
 `HEGEL_E_INVALID_ARG` for a NULL `out_version`.
 */
hegel_result_t hegel_version(hegel_context_t *ctx, const char **out_version);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* HEGEL_H */
