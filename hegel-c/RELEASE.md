RELEASE_TYPE: minor

This release changes the numeric values of `hegel_verbosity_t` so that the default, `HEGEL_VERBOSITY_NORMAL`, is 0. A zero-initialized value now selects the default level
([#357](https://github.com/hegeldev/hegel-rust/issues/357)):

```c
/* before */
HEGEL_VERBOSITY_QUIET = 0, HEGEL_VERBOSITY_NORMAL = 1

/* after */
HEGEL_VERBOSITY_NORMAL = 0, HEGEL_VERBOSITY_QUIET = 1
```
RELEASE_TYPE: patch

This patch adds `hegel_settings_set_stateful_step_count`, which sets the target number of steps a stateful test case runs (default 50).

The stateful stop generation decision has changed. Instead of drawing a single per-case step cap up front, `hegel_state_machine_next_rule` makes a per-step stop decision, forced to keep going before the first step and forced to halt once `stateful_step_count` steps have been handed out. Every stateful case therefore runs at least one step and at most `stateful_step_count`.
