/*
 * echo.c — demo C program using libhegel.
 *
 * Builds a CBOR schema for an integer in [0, 100], runs 50 test cases via
 * the libhegel event loop, asserts every drawn value is in range, and prints
 * a short summary. Tests the "passing" path; for the "failing" path, change
 * the predicate below to e.g. `n < 5`.
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
 *
 * Note: this demo hand-encodes a tiny CBOR schema rather than depending on
 * a CBOR library, so it stays as a single self-contained .c file. Real
 * users would build schemas with libcbor / tinycbor / cbor.h.
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "hegel.h"

/*
 * Hand-rolled CBOR encoding of:
 *
 *   { "type": "integer", "min_value": 0, "max_value": 100 }
 *
 * CBOR map of 3 entries (header 0xA3), each entry is a text key followed
 * by a value. min/max are encoded as small unsigned ints (0 and 24-followed-
 * by-one-byte for 100, since 100 > 23).
 */
static const uint8_t INTEGER_SCHEMA[] = {
    0xA3,                                            /* map(3) */
    0x64, 't', 'y', 'p', 'e',                        /* "type" */
    0x67, 'i', 'n', 't', 'e', 'g', 'e', 'r',         /* "integer" */
    0x69, 'm', 'i', 'n', '_', 'v', 'a', 'l', 'u', 'e',
    0x00,                                            /* 0 */
    0x69, 'm', 'a', 'x', '_', 'v', 'a', 'l', 'u', 'e',
    0x18, 0x64                                       /* 100 */
};

/* Decode an integer-valued CBOR value as produced by the engine for an
 * "integer" schema. Handles the small-uint (0..23), one-byte-uint (24..255),
 * and one-byte negative encodings; that's enough for [0, 100]. */
static int64_t decode_small_integer(const uint8_t *bytes, size_t len) {
    if (len < 1) { fprintf(stderr, "decode: empty\n"); exit(2); }
    uint8_t major = bytes[0] >> 5;
    uint8_t info  = bytes[0] & 0x1F;
    if (major == 0) {            /* unsigned */
        if (info < 24)                          return info;
        if (info == 24 && len >= 2)             return bytes[1];
    } else if (major == 1) {     /* negative */
        if (info < 24)                          return -1 - (int64_t)info;
        if (info == 24 && len >= 2)             return -1 - (int64_t)bytes[1];
    }
    fprintf(stderr, "decode: unsupported CBOR head 0x%02x (len=%zu)\n", bytes[0], len);
    exit(2);
}

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    hegel_settings_new(ctx, &s);
    hegel_settings_test_cases(ctx, s, 50);
    hegel_settings_database(ctx, s, "");      /* disable database */
    hegel_settings_derandomize(ctx, s, true); /* deterministic */
    hegel_settings_seed(ctx, s, 42, true);

    hegel_run_t *run;
    if (hegel_run_start(ctx, s, &run) != HEGEL_OK) {
        const char *err;
        hegel_context_last_error(ctx, &err);
        fprintf(stderr, "hegel_run_start failed: %s\n", err);
        hegel_settings_free(ctx, s);
        hegel_context_free(ctx);
        return 1;
    }

    size_t cases = 0;
    hegel_test_case_t *tc;
    /* The run is finished when next returns HEGEL_OK with a NULL case. */
    while (hegel_next_test_case(ctx, run, &tc) == HEGEL_OK && tc != NULL) {
        const uint8_t *value;
        size_t value_len;
        hegel_result_t rc = hegel_generate(ctx, tc, INTEGER_SCHEMA, sizeof(INTEGER_SCHEMA), &value, &value_len);
        if (rc == HEGEL_E_STOP_TEST) {
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            continue;
        }
        if (rc != HEGEL_OK) {
            const char *err;
            hegel_context_last_error(ctx, &err);
            fprintf(stderr, "hegel_generate failed: rc=%d %s\n", rc, err);
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL);
            continue;
        }

        int64_t n = decode_small_integer(value, value_len);
        if (n < 0 || n > 100) {
            char origin[64];
            snprintf(origin, sizeof origin, "out-of-range value %lld", (long long)n);
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_INTERESTING, origin);
        } else {
            cases++;
            hegel_mark_complete(ctx, tc, HEGEL_STATUS_VALID, NULL);
        }
    }

    const char *err;
    hegel_context_last_error(ctx, &err);
    if (err[0] != '\0') {
        fprintf(stderr, "loop exited with error: %s\n", err);
    }

    const hegel_run_result_t *result;
    hegel_run_result(ctx, run, &result);
    hegel_run_status_t status;
    hegel_run_result_status(ctx, result, &status);
    const char *status_str = status == HEGEL_RUN_STATUS_PASSED   ? "PASSED"
                             : status == HEGEL_RUN_STATUS_FAILED ? "FAILED"
                                                                 : "ERROR";
    size_t nf;
    hegel_run_result_failure_count(ctx, result, &nf);
    printf("ran %zu valid test cases, %s, %zu failures\n", cases, status_str, nf);
    for (size_t i = 0; i < nf; i++) {
        const hegel_failure_t *f;
        hegel_run_result_failure(ctx, result, i, &f);
        const char *origin;
        hegel_failure_origin(ctx, f, &origin);
        printf("  failure %zu: origin=%s\n", i, origin);
    }
    if (status == HEGEL_RUN_STATUS_ERROR) {
        const char *run_err;
        hegel_run_result_error(ctx, result, &run_err);
        fprintf(stderr, "run error: %s\n", run_err);
    }

    hegel_run_free(ctx, run);
    hegel_settings_free(ctx, s);
    hegel_context_free(ctx);
    return status == HEGEL_RUN_STATUS_PASSED ? 0 : 1;
}
