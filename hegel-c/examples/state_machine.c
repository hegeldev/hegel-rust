/*
 * state_machine.c — demo: engine-owned rule selection for stateful testing.
 * Verifies that hegel_new_state_machine / hegel_state_machine_next_rule
 * compose into the building block behind hegel-rust's
 * `hegel::stateful::run` — the engine decides which rule runs at each
 * step (applying swarm testing: each test case enables a random subset
 * of the rules), and the caller applies it.
 *
 * Each test case models a tiny counter machine with three rules:
 *   - increment: counter += 1
 *   - decrement: counter -= 1 (skipped when the counter is at zero)
 *   - reset:     counter  = 0
 * The invariant checked after every step is that the counter stays
 * non-negative.
 *
 * Build (same incantation as echo.c):
 *   cc -o state_machine state_machine.c -I../include \
 *      -L../../target/release -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#include "hegel.h"

static const char *RULES[] = { "increment", "decrement", "reset" };
#define NUM_RULES (sizeof(RULES) / sizeof(RULES[0]))
static const char *INVARIANTS[] = { "non_negative" };
#define NUM_INVARIANTS (sizeof(INVARIANTS) / sizeof(INVARIANTS[0]))

int main(void) {
    hegel_settings_t *s = hegel_settings_new();
    hegel_settings_test_cases(s, 100);
    hegel_settings_database(s, "");
    hegel_settings_derandomize(s, true);
    hegel_settings_seed(s, 0x5ca1ab1e, true);

    hegel_run_t *run = hegel_run_start(s);

    const int STEPS = 20;
    size_t total = 0;
    size_t rule_counts[NUM_RULES] = { 0 };
    bool ok = true;

    hegel_test_case_t *tc;
    while ((tc = hegel_next_test_case(run)) != NULL) {
        int64_t machine;
        if (hegel_new_state_machine(tc, RULES, NUM_RULES,
                                    INVARIANTS, NUM_INVARIANTS,
                                    &machine) != HEGEL_OK) {
            hegel_mark_complete(tc, HEGEL_STATUS_OVERRUN, NULL);
            continue;
        }

        int64_t counter = 0;
        bool overran = false;
        bool bad = false;
        for (int step = 0; step < STEPS && !overran; step++) {
            uint64_t rule;
            int rc = hegel_state_machine_next_rule(tc, machine, &rule);
            if (rc != HEGEL_OK) { overran = true; break; }
            if (rule >= NUM_RULES) { bad = true; break; }
            rule_counts[rule]++;

            switch (rule) {
                case 0: counter += 1; break;
                case 1: if (counter > 0) counter -= 1; break;
                case 2: counter = 0; break;
            }

            /* Invariant: the counter never goes negative. */
            if (counter < 0) { bad = true; break; }
        }

        if (bad) {
            hegel_mark_complete(tc, HEGEL_STATUS_INTERESTING,
                                "invariant violated");
            ok = false;
            continue;
        }
        if (overran) {
            hegel_mark_complete(tc, HEGEL_STATUS_OVERRUN, NULL);
            continue;
        }
        total++;
        hegel_mark_complete(tc, HEGEL_STATUS_VALID, NULL);
    }

    const hegel_run_result_t *result = hegel_run_result(run);
    bool passed = hegel_run_result_passed(result);

    /* Every rule should have been selected at least once across the run —
     * swarm testing restricts the mix per test case, not globally. */
    for (size_t i = 0; i < NUM_RULES; i++) {
        if (rule_counts[i] == 0) {
            fprintf(stderr, "rule %s was never selected\n", RULES[i]);
            ok = false;
        }
    }

    printf("ran %zu valid cases (rule mix: %zu/%zu/%zu), %s\n",
           total, rule_counts[0], rule_counts[1], rule_counts[2],
           passed ? "PASSED" : "FAILED");

    hegel_run_free(run);
    hegel_settings_free(s);
    return (passed && ok) ? 0 : 1;
}
