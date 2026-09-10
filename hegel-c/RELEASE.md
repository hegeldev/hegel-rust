RELEASE_TYPE: minor

This release makes the stateful step count a per-machine parameter. `hegel_new_state_machine` takes a new `step_count` argument. `hegel_settings_set_stateful_step_count` is removed, and the engine no longer has a default step count. Frontends pass one explicitly (50 is the conventional choice). A `step_count` below 1 is rejected with `HEGEL_E_INVALID_ARG`.
