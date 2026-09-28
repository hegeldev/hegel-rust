RELEASE_TYPE: patch

This patch fixes `hegel_run_start_blob` to reject a blob it cannot decode. 
It now returns `HEGEL_E_INVALID_ARG`, with the message in `hegel_context_last_error`. 
No run is started. Before, it returned `HEGEL_OK`, started a run, and the run ended 
with `HEGEL_RUN_STATUS_ERROR`.
