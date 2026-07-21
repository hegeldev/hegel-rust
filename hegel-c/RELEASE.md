RELEASE_TYPE: patch

This patch adds `hegel_settings_set_stateful_step_count`, which sets the target number of steps a stateful test case runs (default 50).

The stateful stop generation decision has changed. Instead of drawing a single per-case step cap up front, `hegel_state_machine_next_rule` makes a per-step stop decision, forced to keep going before the first step and forced to halt once `stateful_step_count` steps have been handed out. Every stateful case therefore runs at least one step and at most `stateful_step_count`.
