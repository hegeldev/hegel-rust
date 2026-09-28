RELEASE_TYPE: patch

This patch fixes `hegel_run_start_blob` to reject a blob it cannot decode. 
It now returns `HEGEL_E_INVALID_ARG`, with the message in `hegel_context_last_error`. 
No run is started. Before, it returned `HEGEL_OK`, started a run, and the run ended 
with `HEGEL_RUN_STATUS_ERROR`.

`hegel_run_start` and `hegel_run_start_blob` now also return `HEGEL_E_INVALID_ARG` when
`ANTITHESIS_OUTPUT_DIR` is set to a nonexistent directory. Before, the run started and then
ended with `HEGEL_RUN_STATUS_ERROR`, and a replay of a deterministic blob did not check the
directory at all.
