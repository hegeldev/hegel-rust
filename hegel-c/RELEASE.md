RELEASE_TYPE: minor

This release makes the stateful step count a per-machine parameter. `hegel_new_state_machine` takes a new `step_count` argument. `hegel_settings_set_stateful_step_count` is removed, and the engine no longer has a default step count. Frontends pass one explicitly (50 is the conventional choice). A `step_count` below 1 is rejected with `HEGEL_E_INVALID_ARG`.

This release also fixes an encode/decode gap in the Hegel test case format where an encoder would allow a sequence that the decoder rejected ([#477](https://github.com/hegeldev/hegel-rust/issues/477)). This code path was unreachable in normal test execution so this is mostly an internal change.
