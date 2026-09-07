/*
 * fork.c — demo: fork-per-test-case, the pattern a C test harness uses
 * for crash isolation.
 *
 * The parent owns the run and makes every libhegel call. Each test case's
 * body runs in a forked child, so a crash kills only that child, and the
 * parent maps the child's exit status onto hegel_mark_complete. libhegel
 * then shrinks a crashing input like any other failure. This is fork-safe
 * because the engine runs entirely on the calling thread, holds no locks
 * or file descriptors between calls, and the child never touches it.
 *
 * Property: every integer in [0, 100] is < 5. A violating child abort()s,
 * standing in for a real crash. We expect the run to fail and to shrink
 * to the minimal crashing input, 5, verified by replaying the reproduce
 * blob.
 *
 * Build (same incantation as echo.c):
 *   cc -o fork fork.c -I../include -L../../target/release -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

#include "hegel.h"
#include "hegel_check.h"

#define ORIGIN "child crashed"

static void run_body(int64_t n) {
    if (n >= 5) abort();
}

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
            fprintf(stderr, "hegel_generate_integer: rc=%d %s\n", rc,
                    hegel_context_last_error(ctx));
            return 1;
        }

        /* All draws happen before the fork, so the parent's engine sees
         * them. The child only runs the body. */
        fflush(stdout);
        fflush(stderr);
        pid_t pid = fork();
        if (pid < 0) {
            perror("fork");
            return 1;
        }
        if (pid == 0) {
            run_body(n);
            _exit(0);
        }

        int wstatus = 0;
        while (waitpid(pid, &wstatus, 0) < 0) { /* EINTR */ }
        if (WIFEXITED(wstatus) && WEXITSTATUS(wstatus) == 0) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_VALID, NULL);
        } else {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING, ORIGIN);
        }
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
    if (nf != 1) {
        fprintf(stderr, "FAIL: expected one failure, got %zu\n", nf);
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

    /* Replay the blob to recover the minimal crashing input. */
    const char *blob;
    HEGEL_CHECK(hegel_failure_reproduction_blob, ctx, f, &blob);
    hegel_test_case_t *replay;
    HEGEL_CHECK(hegel_test_case_from_blob, ctx, s, blob, NULL, NULL, &replay);
    int64_t shrunk;
    HEGEL_CHECK(hegel_generate_integer, ctx, replay, 0, 100, &shrunk);
    HEGEL_CHECK(hegel_mark_complete, ctx, replay, HEGEL_STATUS_INTERESTING, ORIGIN);
    HEGEL_CHECK(hegel_test_case_free, ctx, replay);
    if (shrunk != 5) {
        fprintf(stderr, "FAIL: expected the minimal crash at 5, got %lld\n",
                (long long)shrunk);
        return 1;
    }

    printf("shrunk the crashing input to n=%lld\n", (long long)shrunk);

    HEGEL_CHECK(hegel_failure_free, ctx, f);
    HEGEL_CHECK(hegel_run_result_free, ctx, result);
    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return 0;
}
