RELEASE_TYPE: minor

This release adds `hegel_state_machine_should_check_invariant`: the engine-side sampling decision for stateful invariant checks, a recorded boolean draw that is true with probability 1/`stateful_step_count`. Frontends call it per invariant at each join point and run their guaranteed initial and final checks unconditionally.
