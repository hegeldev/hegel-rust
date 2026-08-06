RELEASE_TYPE: minor

This release adds `hegel_state_machine_rule_rejected(ctx, tc, state_machine_id)`. Frontends are now responsible for calling this when the rule most recently returned by `hegel_state_machine_next_rule` was rejected before it completed.
