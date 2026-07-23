RELEASE_TYPE: minor

This release replaces the stateful-testing interface with one that also supports *concurrent* stateful testing, and adds a run-level nondeterminism declaration.

State machines now carry concurrency groups, per-rule group assignments, and a concurrency level, and execution proceeds in rounds: the root handle asks `hegel_state_machine_next_group` (new) whether to run another round — it reports the round's current group index, e.g. for trace output, or a `-1` sentinel for termination — then each worker pulls rules for that round with `hegel_state_machine_next_rule` until it signals the join point. Rules in the same group may run concurrently; rules in different groups never overlap. Swarm selection is now per worker, with the "at least one rule enabled" guarantee applying within each group. The new `hegel_generate_concurrency` primitive draws a concurrency level in `[1, max_value]`, weighted toward `max_value`.

This is a breaking C ABI change:

- `hegel_new_state_machine` gains `num_groups`, `rule_groups` (group indices parallel to `rule_names`), and `concurrency` parameters. Groups are identified by index only and carry no names. A sequential machine passes a single group, all-zero `rule_groups`, and concurrency 1. Creating the machine now draws from the calling handle's stream, so it can return `HEGEL_E_STOP_TEST`, which the caller should report as an overrun.
- `hegel_state_machine_next_rule` gains a `worker_index` parameter and now hands out rules for one round at a time; the frontend must advance rounds with `hegel_state_machine_next_group`, even for sequential machines.

The choice-sequence shape of sequential stateful tests changes as a result (the round cap is drawn where the step cap used to be), so stored database entries and reproduce blobs for stateful tests are invalidated: stale database entries replay as invalid or overrun and are deleted quietly, while stale blobs fail loudly.

The new `hegel_settings_set_nondeterministic` declares the whole run nondeterministic — the frontend must set it whenever a run may be nondeterministic, typically because the test uses concurrent stateful testing. Such a run reports failures faithfully from the discovering execution and skips everything that assumes deterministic replay: data-tree recording (and with it novel-prefix generation and the nondeterminism mismatch check), span mutation, the per-origin verify and shrink pass (and with it the flakiness check — generation stops at the first bug, so at most one failure is reported), targeting, and database persistence and reuse. Failures from such a run carry no reproduce blob.
