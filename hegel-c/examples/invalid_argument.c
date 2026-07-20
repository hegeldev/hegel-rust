/*
 * invalid_argument.c — regression demo for the hegel-java SIGABRT report.
 *
 * Plausible-but-wrong arguments used to `panic!` inside the engine. Because
 * the draw functions are `extern "C"`, such a panic crossed the FFI boundary
 * and aborted the whole host process (SIGABRT) — it killed an embedding JVM.
 *
 * This program feeds typed invalid arguments to the API and asserts each
 * returns HEGEL_E_INVALID_ARG with a diagnostic, then exits 0:
 *   - hegel_generate_integer with min_value > max_value;
 *   - hegel_string_generator_text with an unknown codec ("ebcdic").
 * The interesting case is the *statically linked* build under
 * `panic = "abort"` (see `just c-test-abort`): if any panic were reachable
 * here, the process would abort and this example would exit non-zero,
 * failing the run.
 *
 * Build (from this directory, after `cargo build -p hegeltest-c --release`):
 *
 *   cc -o invalid_argument invalid_argument.c \
 *      -I../include \
 *      ../../target/release/libhegel.a -lpthread -ldl -lm -lrt
 *
 * Run:
 *   ./invalid_argument
 */

#include <stdio.h>
#include <stdint.h>
#include <string.h>

#include "hegel.h"
#include "hegel_check.h"

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 1);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true);
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 1, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, NULL, NULL, &run);

    hegel_test_case_t *tc;
    HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
    if (!tc) {
        fprintf(stderr, "expected a test case\n");
        return 1;
    }

    int ok = 1;

    int64_t n;
    hegel_result_t rc = hegel_generate_integer(ctx, tc, 10, 5, &n);
    if (rc != HEGEL_E_INVALID_ARG) {
        fprintf(stderr, "expected HEGEL_E_INVALID_ARG (%d) for min > max, got rc=%d\n",
                HEGEL_E_INVALID_ARG, rc);
        ok = 0;
    }
    const char *err = hegel_context_last_error(ctx);
    if (err[0] == '\0') {
        fprintf(stderr, "expected a diagnostic message for min > max\n");
        ok = 0;
    } else {
        printf("min > max correctly rejected: rc=%d, message=\"%s\"\n", rc, err);
    }

    hegel_string_generator_t *gen;
    rc = hegel_string_generator_text(ctx, 0, 10, "ebcdic", 0, UINT32_MAX,
                                     NULL, 0, NULL, 0, NULL, 0, NULL, 0, &gen);
    if (rc != HEGEL_E_INVALID_ARG) {
        fprintf(stderr, "expected HEGEL_E_INVALID_ARG (%d) for unknown codec, got rc=%d\n",
                HEGEL_E_INVALID_ARG, rc);
        ok = 0;
    }
    err = hegel_context_last_error(ctx);
    if (err[0] == '\0') {
        fprintf(stderr, "expected a diagnostic message for the unknown codec\n");
        ok = 0;
    } else {
        printf("unknown codec correctly rejected: rc=%d, message=\"%s\"\n", rc, err);
    }

    HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INVALID, NULL);
    HEGEL_CHECK(hegel_test_case_free, ctx, tc);
    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return ok ? 0 : 1;
}
