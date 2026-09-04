RELEASE_TYPE: patch

This patch inverts the per-round decision drawn by `hegel_state_machine_next_group`. We were using a stop signal where we should have been using a continue signal.
