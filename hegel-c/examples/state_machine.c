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
#include "hegel_check.h"

static const char *RULES[] = { "increment", "decrement", "reset" };
#define NUM_RULES (int64_t) (sizeof(RULES) / sizeof(RULES[0])) 
static const char *INVARIANTS[] = { "non_negative" };
#define NUM_INVARIANTS (sizeof(INVARIANTS) / sizeof(INVARIANTS[0]))

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 100);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true);
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 0x5ca1ab1e, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, NULL, NULL, &run);

    const int STEPS = 20;
    size_t total = 0;
    size_t rule_counts[NUM_RULES] = { 0 };
    bool ok = true;

    while (true) {
        hegel_test_case_t *tc;
        HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
        if (tc == NULL) break;

        int64_t machine;
        if (hegel_new_state_machine(ctx, tc, RULES, NUM_RULES,
                                    INVARIANTS, NUM_INVARIANTS,
                                    &machine) != HEGEL_OK) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }

        int64_t counter = 0;
        bool overran = false;
        bool bad = false;
        for (int step = 0; step < STEPS && !overran; step++) {
            int64_t rule;
            hegel_result_t rc = hegel_state_machine_next_rule(ctx, tc, machine, &rule);
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
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING,
                        "invariant violated");
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            ok = false;
            continue;
        }
        if (overran) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }
        total++;
        HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
        /* The caller owns every handle from hegel_next_test_case and must free it. */
        HEGEL_CHECK(hegel_test_case_free, ctx, tc);
    }

    hegel_run_result_t *result;
    HEGEL_CHECK(hegel_run_result, ctx, run, &result);
    hegel_run_status_t status;
    HEGEL_CHECK(hegel_run_result_status, ctx, result, &status);
    bool passed = status == HEGEL_RUN_STATUS_PASSED;
    HEGEL_CHECK(hegel_run_result_free, ctx, result);

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

    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return (passed && ok) ? 0 : 1;
}
