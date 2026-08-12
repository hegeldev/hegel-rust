/*
 * open_window.c — demo: several test cases open at once via
 * hegel_settings_set_max_open_test_cases.
 *
 * A single thread pumps hegel_next_test_case with a window of 3: it pulls
 * cases until HEGEL_E_PENDING (or the window is full), runs each body as it
 * is pulled, and marks the pooled cases complete newest-first — the
 * out-of-order completion a thread pool would produce, minus the threads.
 * Property: every integer in [0, 1000] is < 900. The run must fail with
 * that one origin, and the pending state must have been observed.
 *
 * Build (same incantation as echo.c):
 *   cc -o open_window open_window.c -I../include -L../../target/release \
 *      -lhegel -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "hegel.h"
#include "hegel_check.h"

#define ORIGIN "n >= 900"
#define WINDOW 3

struct open_case {
    hegel_test_case_t *tc;
    hegel_status_t status;
    const char *origin;
};

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 200);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 0xc0ffee, true);
    HEGEL_CHECK(hegel_settings_set_max_open_test_cases, ctx, s, WINDOW);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, NULL, NULL, &run);

    struct open_case open[WINDOW];
    size_t n_open = 0;
    bool saw_pending = false;
    bool finished = false;

    while (!finished) {
        /* Fill the pool: pull cases until the engine reports PENDING, the
         * window is full, or the run finishes. */
        while (n_open < WINDOW) {
            hegel_test_case_t *tc;
            hegel_result_t rc = hegel_next_test_case(ctx, run, &tc);
            if (rc == HEGEL_E_PENDING) {
                saw_pending = true;
                break;
            }
            if (rc != HEGEL_OK) {
                fprintf(stderr, "hegel_next_test_case: rc=%d %s\n", (int)rc,
                        hegel_context_last_error(ctx));
                return 1;
            }
            if (tc == NULL) {
                finished = true;
                break;
            }

            /* Run the body now; report the outcome later, out of order. */
            struct open_case c = {tc, HEGEL_STATUS_VALID, NULL};
            int64_t n;
            hegel_result_t draw = hegel_generate_integer(ctx, tc, 0, 1000, &n);
            if (draw == HEGEL_E_STOP_TEST) {
                c.status = HEGEL_STATUS_OVERRUN;
            } else if (draw != HEGEL_OK) {
                fprintf(stderr, "hegel_generate_integer: rc=%d %s\n",
                        (int)draw, hegel_context_last_error(ctx));
                return 1;
            } else if (n >= 900) {
                c.status = HEGEL_STATUS_INTERESTING;
                c.origin = ORIGIN;
            }
            open[n_open++] = c;
        }

        /* Complete one pooled case, newest first. */
        if (n_open > 0) {
            struct open_case c = open[--n_open];
            HEGEL_CHECK(hegel_mark_complete, ctx, c.tc, c.status, c.origin);
            HEGEL_CHECK(hegel_test_case_free, ctx, c.tc);
        }
    }

    /* The run can finish while cases that concluded during a draw are still
     * held; report and free the leftovers. */
    while (n_open > 0) {
        struct open_case c = open[--n_open];
        HEGEL_CHECK(hegel_mark_complete, ctx, c.tc, c.status, c.origin);
        HEGEL_CHECK(hegel_test_case_free, ctx, c.tc);
    }

    if (!saw_pending) {
        fprintf(stderr, "FAIL: never observed HEGEL_E_PENDING\n");
        return 1;
    }

    hegel_run_result_t *result;
    HEGEL_CHECK(hegel_run_result, ctx, run, &result);
    hegel_run_status_t status;
    HEGEL_CHECK(hegel_run_result_status, ctx, result, &status);
    if (status != HEGEL_RUN_STATUS_FAILED) {
        fprintf(stderr, "FAIL: expected a failing run, got status %d\n",
                (int)status);
        return 1;
    }

    size_t nf;
    HEGEL_CHECK(hegel_run_result_failure_count, ctx, result, &nf);
    if (nf != 1) {
        fprintf(stderr, "FAIL: expected exactly one failure, got %zu\n", nf);
        return 1;
    }
    hegel_failure_t *failure;
    HEGEL_CHECK(hegel_run_result_failure, ctx, result, 0, &failure);
    const char *origin;
    HEGEL_CHECK(hegel_failure_origin, ctx, failure, &origin);
    if (strcmp(origin, ORIGIN) != 0) {
        fprintf(stderr, "FAIL: unexpected origin %s\n", origin);
        return 1;
    }

    printf("open_window: window of %d, out-of-order completion, "
           "failure reported: %s\n",
           WINDOW, origin);

    HEGEL_CHECK(hegel_failure_free, ctx, failure);
    HEGEL_CHECK(hegel_run_result_free, ctx, result);
    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return 0;
}
