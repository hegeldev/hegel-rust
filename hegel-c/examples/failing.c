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

/* CBOR-encoded {"type": "integer", "min_value": 0, "max_value": 100} */
static const uint8_t INTEGER_SCHEMA[] = {
    0xA3,                                            /* map(3) */
    0x64, 't', 'y', 'p', 'e',
    0x67, 'i', 'n', 't', 'e', 'g', 'e', 'r',
    0x69, 'm', 'i', 'n', '_', 'v', 'a', 'l', 'u', 'e',
    0x00,
    0x69, 'm', 'a', 'x', '_', 'v', 'a', 'l', 'u', 'e',
    0x18, 0x64
};

/* Decode a small CBOR unsigned integer (0..255). Returns -1 if the
 * encoding is something we don't handle (we know the engine only emits
 * the small-uint head for our [0, 100] range, so this is sufficient). */
static int decode_small_uint(const uint8_t *bytes, size_t len) {
    if (len < 1) return -1;
    uint8_t major = bytes[0] >> 5;
    uint8_t info  = bytes[0] & 0x1F;
    if (major != 0) return -1;
    if (info < 24) return info;
    if (info == 24 && len >= 2) return bytes[1];
    return -1;
}

#define ORIGIN "n >= 5"

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(ctx, hegel_settings_new(ctx, &s));
    HEGEL_CHECK(ctx, hegel_settings_test_cases(ctx, s, 200));
    HEGEL_CHECK(ctx, hegel_settings_database(ctx, s, ""));
    HEGEL_CHECK(ctx, hegel_settings_derandomize(ctx, s, true));
    HEGEL_CHECK(ctx, hegel_settings_seed(ctx, s, 0xc0ffee, true));

    hegel_run_t *run;
    HEGEL_CHECK(ctx, hegel_run_start(ctx, s, &run));

    for (;;) {
        hegel_test_case_t *tc;
        HEGEL_CHECK(ctx, hegel_next_test_case(ctx, run, &tc));
        if (tc == NULL) break;

        const uint8_t *value;
        size_t value_len;
        hegel_result_t rc = hegel_generate(ctx, tc, INTEGER_SCHEMA, sizeof(INTEGER_SCHEMA), &value, &value_len);
        if (rc == HEGEL_E_STOP_TEST) {
            HEGEL_CHECK(ctx, hegel_mark_complete(ctx, tc, HEGEL_STATUS_OVERRUN, NULL));
            continue;
        }
        if (rc != HEGEL_OK) {
            const char *err = hegel_context_last_error(ctx);
            fprintf(stderr, "hegel_generate: rc=%d %s\n", rc, err);
            HEGEL_CHECK(ctx, hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL));
            continue;
        }

        int n = decode_small_uint(value, value_len);
        if (n < 0) {
            fprintf(stderr, "decode failed\n");
            HEGEL_CHECK(ctx, hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL));
            continue;
        }

        if (n < 5) {
            HEGEL_CHECK(ctx, hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL));
        } else {
            HEGEL_CHECK(ctx, hegel_mark_complete(ctx, tc, HEGEL_STATUS_INTERESTING, ORIGIN));
        }
    }

    const hegel_run_result_t *result;
    HEGEL_CHECK(ctx, hegel_run_result(ctx, run, &result));
    hegel_run_status_t status;
    HEGEL_CHECK(ctx, hegel_run_result_status(ctx, result, &status));
    if (status != HEGEL_RUN_STATUS_FAILED) {
        fprintf(stderr, "FAIL: expected a failing run, got status %d\n", (int)status);
        return 1;
    }

    size_t nf;
    HEGEL_CHECK(ctx, hegel_run_result_failure_count(ctx, result, &nf));
    if (nf < 1) {
        fprintf(stderr, "FAIL: expected at least one failure, got %zu\n", nf);
        return 1;
    }

    const hegel_failure_t *f;
    HEGEL_CHECK(ctx, hegel_run_result_failure(ctx, result, 0, &f));
    const char *origin;
    HEGEL_CHECK(ctx, hegel_failure_origin(ctx, f, &origin));
    if (strstr(origin, ORIGIN) == NULL) {
        fprintf(stderr, "FAIL: expected origin to contain %s, got: %s\n", ORIGIN, origin);
        return 1;
    }

    printf("got expected failure: origin=%s\n", origin);

    HEGEL_CHECK(ctx, hegel_run_free(ctx, run));
    HEGEL_CHECK(ctx, hegel_settings_free(ctx, s));
    HEGEL_CHECK(ctx, hegel_context_free(ctx));
    return 0;
}
