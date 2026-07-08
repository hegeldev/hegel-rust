RELEASE_TYPE: minor

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
