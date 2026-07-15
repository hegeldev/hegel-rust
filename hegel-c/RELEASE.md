RELEASE_TYPE: minor

This release moves control over the lifecycle of stateful tests into the
engine. Frontends no longer draw a step cap up front; instead, they poll for
rules from the engine until they receive a termination signal. This is
necessary groundwork for future work on concurrent stateful testing and better
shrinking.

The signature for requesting the next rule is unchanged, but termination is now
indicated by setting `out_rule_index` to `HEGEL_STATE_MACHINE_DONE`:

```c
hegel_result_t hegel_state_machine_next_rule(hegel_context_t *ctx,
                                             hegel_test_case_t *tc,
                                             int64_t state_machine_id,
                                             int64_t *out_rule_index);
```
