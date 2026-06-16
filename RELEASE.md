RELEASE_TYPE: patch

This patch tightens libhegel's C ABI: it removes the last thread-local state,
replaces the `#define`d integer constants with named enums, and documents
pointer ownership.

Error reporting
no longer goes through a thread-local "last error" buffer; instead every
fallible call records its diagnostic on an explicit `hegel_context_t` the
caller passes in. A thread-local buffer is ill-defined under runtimes that
migrate work between OS threads mid-call (for example Go, whose goroutines can
move between threads), where a message could be written on one thread and read
on another. Threading an explicit context through the API removes that hazard.

For Rust users this is an internal change — the public API is unchanged. The
`hegeltest` crate keeps its error context in thread-local storage, which is
sound because it only ever drives the engine from ordinary OS threads.

For libhegel C-ABI consumers (such as hegel-go) this is a **breaking change**:

- New `hegel_context_t` opaque handle with `hegel_context_new()`,
  `hegel_context_free(ctx)`, and `hegel_context_last_error(ctx)`. Create one
  per test (or per thread), pass it as the first argument to every fallible
  call, and free it when done. A context is cheap; a single context must not be
  shared across threads concurrently.

- `hegel_last_error_message()` has been removed. Read diagnostics with
  `hegel_context_last_error(ctx)` instead.

- Every fallible entry point now takes a leading `hegel_context_t *ctx`
  argument: `hegel_run_start`, `hegel_next_test_case`, `hegel_run_result`,
  `hegel_generate`, `hegel_start_span`, `hegel_stop_span`,
  `hegel_new_collection`, `hegel_collection_more`, `hegel_collection_reject`,
  `hegel_new_pool`, `hegel_pool_add`, `hegel_pool_generate`,
  `hegel_new_state_machine`, `hegel_state_machine_next_rule`,
  `hegel_primitive_boolean`, `hegel_target`, `hegel_mark_complete`,
  `hegel_test_case_from_blob`, `hegel_test_case_free`,
  `hegel_settings_database`, and `hegel_settings_database_key`. Passing a NULL
  context is allowed and simply opts out of error messages — the call still
  returns its usual error code. Pure accessors that cannot fail
  (`hegel_run_result_status`, the `hegel_failure_*` getters,
  `hegel_test_case_is_final_replay`, `hegel_version`, and the infallible
  `hegel_settings_*` setters) are unchanged.

The `#define`d constants are now named enums, so they appear in debug info and
type-check at the call site (also a **breaking change** for C-ABI consumers):

- The `HEGEL_OK` / `HEGEL_E_*` codes are now the `hegel_result_t` enum, and
  every fallible entry point returns `hegel_result_t` instead of `int`. The
  values are unchanged (`HEGEL_OK` is 0, errors are negative), so the binary
  ABI is the same — only the declared types change.
- `HEGEL_PHASE_*`, `HEGEL_HC_*`, and `HEGEL_LABEL_*` are now the
  `hegel_phase_t`, `hegel_health_check_t`, and `hegel_label_t` enums. The phase
  and health-check parameters stay `uint32_t` (a bitwise OR of the flag
  values), and the span label stays `uint64_t` (the label space is open — you
  may pass your own stable keys beyond the named ones), so only the constants'
  declaring type changes.

Finally, the header now documents pointer ownership explicitly: every pointer
you pass into a libhegel function stays owned by you (the library copies
whatever it needs during the call), and pointers libhegel returns are borrows
valid only until a documented point.
