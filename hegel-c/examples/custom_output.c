/*
 * custom_output.c — demo of redirecting libhegel's output.
 *
 * Passes an output callback to hegel_run_start, runs a small passing property
 * at debug verbosity, and checks the engine's progress lines arrived at the
 * callback (with the user_data pointer passed through) instead of being
 * written to stderr. This is the mechanism a language binding uses to deliver
 * engine output to its own test logger, e.g. a Go testing.T.
 *
 * Build (from this directory, after `cargo build -p hegeltest-c --release`):
 *
 *   cc -o custom_output custom_output.c \
 *      -I../include \
 *      -L../../target/release \
 *      -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 *
 * Run:
 *   ./custom_output
 */

#include <stdbool.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "hegel.h"
#include "hegel_check.h"

typedef struct {
    size_t lines;
    bool saw_summary;
} capture_t;

static void capture_line(void *user_data, const char *line, size_t len) {
    capture_t *cap = user_data;
    if (strlen(line) != len) {
        fprintf(stderr, "line/len mismatch: %zu vs %zu\n", strlen(line), len);
        abort();
    }
    cap->lines++;
    /* The engine ends every run with a "Test done. ..." summary at debug
       verbosity; remember whether it reached us. */
    if (strncmp(line, "Test done.", strlen("Test done.")) == 0) {
        cap->saw_summary = true;
    }
}

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    capture_t cap = {0, false};

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 10);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, ""); /* disable database */
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 42, true);
    HEGEL_CHECK(hegel_settings_set_verbosity, ctx, s, HEGEL_VERBOSITY_DEBUG);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, capture_line, &cap, &run);

    while (true) {
        hegel_test_case_t *tc;
        HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
        if (tc == NULL) break;

        int64_t n;
        hegel_result_t rc = hegel_generate_integer(ctx, tc, 0, 100, &n);
        hegel_status_t status =
            rc == HEGEL_OK ? HEGEL_STATUS_VALID : HEGEL_STATUS_OVERRUN;
        HEGEL_CHECK(hegel_mark_complete, ctx, tc, status, NULL);
        HEGEL_CHECK(hegel_test_case_free, ctx, tc);
    }

    hegel_run_result_t *result;
    HEGEL_CHECK(hegel_run_result, ctx, run, &result);
    hegel_run_status_t status;
    HEGEL_CHECK(hegel_run_result_status, ctx, result, &status);
    HEGEL_CHECK(hegel_run_result_free, ctx, result);
    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);

    printf("captured %zu engine output line(s), summary seen: %s\n",
           cap.lines, cap.saw_summary ? "yes" : "no");
    if (status != HEGEL_RUN_STATUS_PASSED || cap.lines == 0 ||
        !cap.saw_summary) {
        fprintf(stderr, "expected a passing run whose debug output reached "
                        "the callback\n");
        return 1;
    }
    return 0;
}
