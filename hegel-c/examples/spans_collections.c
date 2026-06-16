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

/* CBOR-encoded {"type": "boolean"} */
static const uint8_t BOOLEAN_SCHEMA[] = {
    0xA1,                                    /* map(1) */
    0x64, 't', 'y', 'p', 'e',
    0x67, 'b', 'o', 'o', 'l', 'e', 'a', 'n'
};

static bool decode_bool(const uint8_t *bytes, size_t len) {
    if (len < 1) { fprintf(stderr, "decode_bool: empty\n"); exit(2); }
    /* CBOR true = 0xF5, false = 0xF4. */
    if (bytes[0] == 0xF5) return true;
    if (bytes[0] == 0xF4) return false;
    fprintf(stderr, "decode_bool: unexpected head 0x%02x\n", bytes[0]);
    exit(2);
}

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
    for (;;) {
        bool more;
        int rc = hegel_collection_more(ctx, tc, cid, &more);
        if (rc != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        if (!more) break;

        if (hegel_start_span(ctx, tc, HEGEL_LABEL_LIST_ELEMENT) != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        const uint8_t *value;
        size_t value_len;
        rc = hegel_generate(ctx, tc, BOOLEAN_SCHEMA, sizeof(BOOLEAN_SCHEMA), &value, &value_len);
        if (rc != HEGEL_OK) {
            hegel_stop_span(ctx, tc, false);
            hegel_stop_span(ctx, tc, false);
            return -1;
        }
        (void)decode_bool(value, value_len);   /* exercise the decode path */
        hegel_stop_span(ctx, tc, false);
        n++;
    }

    hegel_stop_span(ctx, tc, false);
    return n;
}

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s = hegel_settings_new();
    hegel_settings_test_cases(s, 100);
    hegel_settings_database(ctx, s, "");
    hegel_settings_derandomize(s, true);
    hegel_settings_seed(s, 0xfeedface, true);

    hegel_run_t *run = hegel_run_start(ctx, s);

    const uint64_t MIN_SIZE = 0;
    const uint64_t MAX_SIZE = 8;
    size_t total = 0;
    size_t max_seen = 0;

    hegel_test_case_t *tc;
    while ((tc = hegel_next_test_case(ctx, run)) != NULL) {
        int n = draw_bool_list(ctx, tc, MIN_SIZE, MAX_SIZE);
        if (n < 0) {
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            continue;
        }
        if ((uint64_t)n < MIN_SIZE || (uint64_t)n > MAX_SIZE) {
            char origin[64];
            snprintf(origin, sizeof origin, "size %d out of range", n);
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_INTERESTING, origin);
            continue;
        }
        total++;
        if ((size_t)n > max_seen) max_seen = (size_t)n;
        hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL);
    }

    const hegel_run_result_t *result = hegel_run_result(ctx, run);
    bool passed = hegel_run_result_status(result) == HEGEL_RUN_STATUS_PASSED;

    printf("ran %zu valid cases (max list size seen: %zu), %s\n",
           total, max_seen, passed ? "PASSED" : "FAILED");

    hegel_run_free(run);
    hegel_settings_free(s);
    hegel_context_free(ctx);
    return passed ? 0 : 1;
}
