/*
 * spans_collections.c — demo: building a list using the span + collection
 * primitives. Verifies that hegel_start_span / hegel_stop_span /
 * hegel_new_collection / hegel_collection_more compose correctly into a
 * variable-length structure that shrinks predictably.
 *
 * The C caller manually drives a list-of-booleans for each test case,
 * asserting that every drawn list stays inside its size bounds and that
 * the run as a whole passes.
 *
 * Build (same incantation as echo.c):
 *   cc -o spans_collections spans_collections.c -I../include \
 *      -L../../target/release -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#include "hegel.h"
#include "hegel_check.h"

/* Draw a list of booleans, sized between min_size and max_size, using
 * a span (LIST) wrapping a collection (more/draw loop). Returns the
 * number of elements drawn, or -1 on engine error. */
static int draw_bool_list(hegel_context_t *ctx, hegel_test_case_t *tc, uint64_t min_size, uint64_t max_size) {
    if (hegel_start_span(ctx, tc, HEGEL_LABEL_LIST) != HEGEL_OK) return -1;

    int64_t cid;
    if (hegel_new_collection(ctx, tc, min_size, max_size, &cid) != HEGEL_OK) {
        hegel_stop_span(ctx, tc, false);
        return -1;
    }

    int n = 0;
    while (true) {
        bool more;
        hegel_result_t rc = hegel_collection_more(ctx, tc, cid, &more);
        if (rc != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        if (!more) break;

        if (hegel_start_span(ctx, tc, HEGEL_LABEL_LIST_ELEMENT) != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        bool value;
        rc = hegel_generate_boolean(ctx, tc, 0.5, false, false, &value);
        if (rc != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        hegel_stop_span(ctx, tc, false);
        n++;
    }

    hegel_stop_span(ctx, tc, false);
    return n;
}

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 100);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true);
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 0xfeedface, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, NULL, NULL, &run);

    const uint64_t MIN_SIZE = 0;
    const uint64_t MAX_SIZE = 8;
    size_t total = 0;
    size_t max_seen = 0;

    while (true) {
        hegel_test_case_t *tc;
        HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
        if (tc == NULL) break;

        int n = draw_bool_list(ctx, tc, MIN_SIZE, MAX_SIZE);
        if (n < 0) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }
        if ((uint64_t)n < MIN_SIZE || (uint64_t)n > MAX_SIZE) {
            char origin[64];
            snprintf(origin, sizeof origin, "size %d out of range", n);
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING, origin);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }
        total++;
        if ((size_t)n > max_seen) max_seen = (size_t)n;
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

    printf("ran %zu valid cases (max list size seen: %zu), %s\n",
           total, max_seen, passed ? "PASSED" : "FAILED");

    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return passed ? 0 : 1;
}
