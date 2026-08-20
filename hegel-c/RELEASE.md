RELEASE_TYPE: minor

This release replaces the stateful-testing interface with one that also supports concurrent stateful testing.

Every state machine — sequential or concurrent — is now driven by the same round-based protocol. Creating the machine draws the number of workers the caller must run; the owning thread advances rounds on the root test-case handle and checks the machine's invariants at the join points between rounds, while no rule is running:

```c
hegel_new_state_machine(ctx, tc, rule_names, rule_groups, num_rules,
                        invariant_names, num_invariants,
                        min_concurrency, max_concurrency,
                        &machine, &concurrency);
/* spawn `concurrency` worker threads, each holding its own clone handle */

while (true) {
    /* root handle: ask whether to run another round, and for which group */
    hegel_state_machine_next_group(ctx, tc, machine, &group);
    if (group == HEGEL_STATE_MACHINE_DONE) break;

    /* wake every worker, then wait for all of them to finish the round */

    /* the join point: every worker is parked — check the invariants */
}
/* tell the workers to exit */
```

Each worker thread `w`, woken once per round, pulls rules on its own clone handle `tc_w` until the engine signals its join point, then parks until the next round:

```c
while (true) {
    hegel_state_machine_next_rule(ctx, tc_w, machine, w, &rule);
    if (rule == HEGEL_STATE_MACHINE_DONE) break;  /* w's round is over */
    /* apply rules[rule]; if it fails an assumption, report it with
       hegel_state_machine_rule_rejected(ctx, tc_w, machine, w) */
}
```

Each rule is assigned to a concurrency group at machine creation, and for each round the engine draws a random group: only that group's rules are handed out for the round, so rules in the same group may run concurrently with each other and rules in different groups never overlap. A sequential machine is a degenerate case — all-zero `rule_groups`, concurrency bounds `1, 1`, no worker threads at all, and the root handle drives everything: the engine hands out exactly one rule per round, so the two loops collapse into the old rule loop on one thread with a `hegel_state_machine_next_group` call between rules.

This is a breaking C ABI change:

- `hegel_new_state_machine` gains `rule_groups` (group ids parallel to `rule_names`) and `min_concurrency`/`max_concurrency` parameters, plus an `out_concurrency` out-parameter: the engine draws the machine's concurrency level in `[min_concurrency, max_concurrency]` and writes it to `*out_concurrency`. The caller must run exactly that many workers. Passing `min_concurrency == max_concurrency` fixes the level. Groups are identified by arbitrary `int64_t` ids and carry no names: the machine has one concurrency group per distinct value of `rule_groups` (`INT64_MIN` is rejected — it is reserved as the `HEGEL_STATE_MACHINE_DONE` sentinel). Creating the machine now draws from the calling handle's stream, so it can return `HEGEL_E_STOP_TEST`, which the caller should report as an overrun.
- `hegel_state_machine_next_rule` gains a `worker_index` parameter and now hands out rules for one round at a time; the frontend must advance rounds with `hegel_state_machine_next_group` (new), even for sequential machines.
- `hegel_state_machine_rule_rejected` gains the same `worker_index` parameter, and its accounting is per worker: it refers to the rule most recently handed to that worker.
- `HEGEL_STATE_MACHINE_DONE` changes value from `-1` to `INT64_MIN`, so the sentinel sits outside the group-id space callers plausibly use. A frontend that compares `hegel_state_machine_next_rule`'s `out_rule_index` against a hard-coded `-1` instead of the named constant must update.

Creating a state machine with `max_concurrency > 1` declares the run nondeterministic. The engine detects this itself, so frontends have nothing to set up front. A nondeterministic run can't be replayed, so discovery is the only chance to capture. Frontends should check for nondeterminism at test case start, with the new `hegel_test_case_is_nondeterministic`, and capture a nondeterministic case's whole trace — draws, notes, and any failure's diagnostics — while it runs. Only the most recent capture needs to be stored, because the run will end as soon as a failure is found. The engine skips everything that assumes deterministic replay: data-tree recording (and with it novel-prefix generation and the nondeterminism mismatch check), span mutation, the verify and shrink pass (and with it the flakiness check), targeting, and database persistence and reuse.

A failing nondeterministic run reports itself with a new run status, `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` (other `hegel_run_status_t` values are unchanged). Frontends should skip any final replay they normally perform, print no reproducer, and report the bug from what they captured at discovery. A failing single-test-case run still reports plain `HEGEL_RUN_STATUS_FAILED`, since its caller reports from the case's own execution anyway.

The `stateful_step_count` setting now bounds *rounds*. At concurrency 1 each round is exactly one rule and a round whose rule was rejected does not count, so sequential budgets behave as before.

The choice-sequence shape of sequential stateful tests changes as a result (the stop decision moves to the round boundary, and the swarm disabling probability is drawn at machine creation rather than at the first selection), so stored database entries and reproduce blobs for stateful tests are invalidated: stale database entries replay as invalid or overrun and are deleted quietly, while stale blobs fail loudly.
