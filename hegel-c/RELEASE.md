RELEASE_TYPE: patch

This patch adds support for keeping several test cases open at once. The new `hegel_settings_set_max_open_test_cases` setting (default 1, which preserves the strict-alternation contract exactly) lets a caller take up to that many cases from `hegel_next_test_case` before marking them complete, so their bodies can run concurrently; the new `HEGEL_E_PENDING` result code reports that no case is available until one of the open ones completes. `hegel_mark_complete` may be called from any thread on any open handle, `hegel_run_free` now completes every open case, and concurrent misuse of a run handle reports `HEGEL_E_CONCURRENT_USE` instead of being undefined behavior. See the threading section of the header preamble and the new `open_window.c` example.

The TooSlow health check now measures generation-phase wall-clock time rather than summed per-case time, which would overcount when test cases overlap.
