# Changelog

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
