RELEASE_TYPE: minor

This release adds rule weights to state machines. `hegel_new_state_machine` takes a new `rule_weights` parallel to `rule_names`:

```c
// before
hegel_new_state_machine(ctx, tc, rule_names, rule_groups, num_rules, ...);

// after
hegel_new_state_machine(ctx, tc, rule_names, rule_groups, rule_weights, num_rules, ...);
```

Pass `NULL` to keep every rule at the same weight, which is the previous behavior. Otherwise each weight must be finite and positive.

A weight is a hint about how often the engine should hand out a rule relative to the other rules of its concurrency group. It applies only on enabled rules, so relative weight of a rule depends on which other rules are enabled. The weights are not a distributional guarantee.
