RELEASE_TYPE: minor

This release changes `hegel_new_state_machine`: it takes a new
`invariant_always_check` argument, an array of per-invariant flags parallel to
`invariant_names` (NULL for all false).
`hegel_state_machine_should_check_invariant` answers true unconditionally for
a flagged invariant, consuming no entropy, and samples the rest as before.
