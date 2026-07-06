/*
 * echo.c — demo C program using libhegel.
 *
 * Draws an integer in [0, 100], runs 50 test cases via the libhegel event
 * loop, asserts every drawn value is in range, and prints a short summary.
 * Tests the "passing" path; for the "failing" path, change the predicate
 * below to e.g. `n < 5`.
 *
 * Build (from this directory, after `cargo build -p hegeltest-c --release`):
 *
 *   cc -o echo echo.c \
 *      -I../include \
 *      -L../../target/release \
 *      -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 *
 * Run:
 *   ./echo
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#include "hegel.h"
#include "hegel_check.h"

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 50);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");      /* disable database */
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true); /* deterministic */
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 42, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, &run);

    size_t cases = 0;
    while (true) {
        /* The run is finished when next returns HEGEL_OK with a NULL case. */
        hegel_test_case_t *tc;
        HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
        if (tc == NULL) break;

        int64_t n;
        hegel_result_t rc = hegel_generate_integer(ctx, tc, 0, 100, &n);
        if (rc == HEGEL_E_STOP_TEST) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }
        if (rc != HEGEL_OK) {
            const char *err = hegel_context_last_error(ctx);
            fprintf(stderr, "hegel_generate_integer failed: rc=%d %s\n", rc, err);
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }

        if (n < 0 || n > 100) {
            char origin[64];
            snprintf(origin, sizeof origin, "out-of-range value %lld", (long long)n);
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING, origin);
        } else {
            cases++;
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
        }
        /* Every handle hegel_next_test_case hands back is owned by the caller
           and must be released, even though the run keeps its own reference. */
        HEGEL_CHECK(hegel_test_case_free, ctx, tc);
    }

    hegel_run_result_t *result;
    HEGEL_CHECK(hegel_run_result, ctx, run, &result);
    hegel_run_status_t status;
    HEGEL_CHECK(hegel_run_result_status, ctx, result, &status);
    const char *status_str = status == HEGEL_RUN_STATUS_PASSED   ? "PASSED"
                             : status == HEGEL_RUN_STATUS_FAILED ? "FAILED"
                                                                 : "ERROR";
    size_t nf;
    HEGEL_CHECK(hegel_run_result_failure_count, ctx, result, &nf);
    printf("ran %zu valid test cases, %s, %zu failures\n", cases, status_str, nf);
    for (size_t i = 0; i < nf; i++) {
        hegel_failure_t *f;
        HEGEL_CHECK(hegel_run_result_failure, ctx, result, i, &f);
        const char *origin;
        HEGEL_CHECK(hegel_failure_origin, ctx, f, &origin);
        printf("  failure %zu: origin=%s\n", i, origin);
        HEGEL_CHECK(hegel_failure_free, ctx, f);
    }
    if (status == HEGEL_RUN_STATUS_ERROR) {
        const char *run_err;
        HEGEL_CHECK(hegel_run_result_error, ctx, result, &run_err);
        fprintf(stderr, "run error: %s\n", run_err);
    }
    /* The result is a caller-owned snapshot, freed independently of the run. */
    HEGEL_CHECK(hegel_run_result_free, ctx, result);

    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return status == HEGEL_RUN_STATUS_PASSED ? 0 : 1;
}
