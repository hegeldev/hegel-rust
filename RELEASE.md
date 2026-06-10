RELEASE_TYPE: patch

This patch adds swarm testing to stateful tests. Rule selection is owned by the engine and exposed to libhegel consumers as `hegel_new_state_machine` / `hegel_state_machine_next_rule`.

