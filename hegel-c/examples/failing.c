/*
 * failing.c — demo: a failing property, with libhegel shrinking to a
 * minimal counterexample and reporting it back through the result API.
 *
 * Property: every integer in [0, 100] is < 5. Obviously false. We expect
 * libhegel to find the smallest n that violates the property — which is
 * 5 itself. The program exits 0 if the failing run was correctly detected
 * and the reported failure carries the expected origin string.
 *
 * Build (same incantation as echo.c):
 *   cc -o failing failing.c -I../include -L../../target/release -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "hegel.h"
#include "hegel_check.h"

#define ORIGIN "n >= 5"

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 200);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true);
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 0xc0ffee, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, NULL, NULL, &run);

    while (true) {
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
            fprintf(stderr, "hegel_generate_integer: rc=%d %s\n", rc, err);
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }

        if (n < 5) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
        } else {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING, ORIGIN);
        }
        /* The caller owns every handle from hegel_next_test_case and must free it. */
        HEGEL_CHECK(hegel_test_case_free, ctx, tc);
    }

    hegel_run_result_t *result;
    HEGEL_CHECK(hegel_run_result, ctx, run, &result);
    hegel_run_status_t status;
    HEGEL_CHECK(hegel_run_result_status, ctx, result, &status);
    if (status != HEGEL_RUN_STATUS_FAILED) {
        fprintf(stderr, "FAIL: expected a failing run, got status %d\n", (int)status);
        return 1;
    }

    size_t nf;
    HEGEL_CHECK(hegel_run_result_failure_count, ctx, result, &nf);
    if (nf < 1) {
        fprintf(stderr, "FAIL: expected at least one failure, got %zu\n", nf);
        return 1;
    }

    hegel_failure_t *f;
    HEGEL_CHECK(hegel_run_result_failure, ctx, result, 0, &f);
    const char *origin;
    HEGEL_CHECK(hegel_failure_origin, ctx, f, &origin);
    if (strstr(origin, ORIGIN) == NULL) {
        fprintf(stderr, "FAIL: expected origin to contain %s, got: %s\n", ORIGIN, origin);
        return 1;
    }

    printf("got expected failure: origin=%s\n", origin);

    /* Result and failure are caller-owned snapshots, freed independently. */
    HEGEL_CHECK(hegel_failure_free, ctx, f);
    HEGEL_CHECK(hegel_run_result_free, ctx, result);

    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return 0;
}
