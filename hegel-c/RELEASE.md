RELEASE_TYPE: patch

This patch removes libhegel's background worker thread. `hegel_run_start` no longer spawns a thread: the engine is suspended inside the run handle, and each `hegel_next_test_case` call runs it on the calling thread until it hands over the next test case. The API is unchanged, but the threading behaviour is simpler:

- Output callbacks are now invoked on whichever thread calls `hegel_next_test_case`, rather than from a separate engine thread.
- Engine work between test cases (generation, mutation, shrinking) now happens inside `hegel_next_test_case`, where the caller previously blocked waiting for the worker to do the same work; total run time is unchanged, minus two thread context switches per test case.
- `hegel_run_start` can no longer fail to spawn a thread, and `hegel_run_free` no longer has a worker to wind down — freeing a run mid-run simply drops the rest of the exploration.

This makes libhegel usable in environments where spawning threads is unavailable or awkward.
