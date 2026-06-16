/*
 * invalid_schema.c — regression demo for the hegel-java SIGABRT report.
 *
 * A plausible-but-wrong schema type (`{"type":"ipv4"}`) used to
 * `panic!("Unknown schema type")` inside the engine. Because hegel_generate
 * is `extern "C"`, that panic crossed the FFI boundary and aborted the whole
 * host process (SIGABRT) — it killed an embedding JVM.
 *
 * This program feeds such a schema to hegel_generate and asserts it returns
 * HEGEL_E_INVALID_ARG with a diagnostic, then exits 0. The interesting case
 * is the *statically linked* build under `panic = "abort"` (see
 * `just c-test-abort`): if any panic were reachable here, the process would
 * abort and this example would exit non-zero, failing the run.
 *
 * Build (from this directory, after `cargo build -p hegeltest-c --release`):
 *
 *   cc -o invalid_schema invalid_schema.c \
 *      -I../include \
 *      ../../target/release/libhegel.a -lpthread -ldl -lm -lrt
 *
 * Run:
 *   ./invalid_schema
 */

#include <stdio.h>
#include <stdint.h>
#include <string.h>

#include "hegel.h"

/*
 * Hand-rolled CBOR encoding of { "type": "ipv4" }:
 *   map(1) = 0xA1, text("type") = 0x64 + 4 bytes, text("ipv4") = 0x64 + 4 bytes.
 */
static const uint8_t IPV4_TYPE_SCHEMA[] = {
    0xA1,                            /* map(1) */
    0x64, 't', 'y', 'p', 'e',        /* "type" */
    0x64, 'i', 'p', 'v', '4',        /* "ipv4" */
};

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s = hegel_settings_new();
    hegel_settings_test_cases(s, 1);
    hegel_settings_database(ctx, s, "");
    hegel_settings_derandomize(s, true);
    hegel_settings_seed(s, 1, true);

    hegel_run_t *run = hegel_run_start(ctx, s);
    if (!run) {
        fprintf(stderr, "hegel_run_start failed: %s\n", hegel_context_last_error(ctx));
        hegel_settings_free(s);
        hegel_context_free(ctx);
        return 1;
    }

    hegel_test_case_t *tc = hegel_next_test_case(ctx, run);
    if (!tc) {
        fprintf(stderr, "expected a test case\n");
        hegel_run_free(run);
        hegel_settings_free(s);
        hegel_context_free(ctx);
        return 1;
    }

    const uint8_t *value;
    size_t value_len;
    int rc = hegel_generate(ctx, tc, IPV4_TYPE_SCHEMA, sizeof(IPV4_TYPE_SCHEMA), &value, &value_len);

    int ok = 1;
    if (rc != HEGEL_E_INVALID_ARG) {
        fprintf(stderr, "expected HEGEL_E_INVALID_ARG (%d), got rc=%d\n",
                HEGEL_E_INVALID_ARG, rc);
        ok = 0;
    }
    const char *err = hegel_context_last_error(ctx);
    if (err[0] == '\0') {
        fprintf(stderr, "expected a diagnostic message for the invalid schema\n");
        ok = 0;
    } else {
        printf("invalid schema correctly rejected: rc=%d, message=\"%s\"\n", rc, err);
    }

    hegel_mark_complete(ctx, tc, HEGEL_STATUS_INVALID, NULL);
    hegel_run_free(run);
    hegel_settings_free(s);
    hegel_context_free(ctx);
    return ok ? 0 : 1;
}
