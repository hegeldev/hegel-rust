RELEASE_TYPE: minor

GitHub releases now include static `libhegel` libraries and `hegel.h` alongside
the existing shared libraries, making the C ABI easier to consume without
building Hegel from source.

This release also changes `hegel_new_state_machine`: it takes a new
`invariant_always_check` argument, an array of per-invariant flags parallel to
`invariant_names` (NULL for all false).
`hegel_state_machine_should_check_invariant` answers true unconditionally for
a flagged invariant, consuming no entropy, and samples the rest as before.
