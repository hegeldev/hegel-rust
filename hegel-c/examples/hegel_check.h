/*
 * hegel_check.h — HEGEL_CHECK, shared by the example programs.
 *
 * HEGEL_CHECK(fn, ctx, ...) calls fn(ctx, ...) and aborts with a diagnostic
 * (the function name plus the context's last error message) if it does not
 * return HEGEL_OK. Use it for the setup / bookkeeping / teardown calls whose
 * only non-zero outcome would be a programming error:
 *
 *     HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 50);
 *
 * Do NOT wrap calls with a meaningful non-OK code, such as the
 * hegel_generate_* draw primitives returning HEGEL_E_STOP_TEST on
 * choice-budget exhaustion; handle those explicitly.
 */

#ifndef HEGEL_CHECK_H
#define HEGEL_CHECK_H

#include <stdio.h>
#include <stdlib.h>

#include "hegel.h"

#define HEGEL_STRING(data, len) ((hegel_string_t){(len) == 0 ? NULL : (data), (len)})
#define STRING(value) HEGEL_STRING((value), sizeof(value) - 1)

#define HEGEL_CHECK(fn, ctx, ...)                                                                      \
    do {                                                                                               \
        hegel_result_t hegel_check_rc_ = fn((ctx), ##__VA_ARGS__);                                     \
        if (hegel_check_rc_ != HEGEL_OK) {                                                             \
            hegel_string_t hegel_check_msg_ = hegel_context_last_error((ctx));                         \
            fprintf(stderr, "%s:%d: %s failed: rc=%d", __FILE__, __LINE__, #fn, (int)hegel_check_rc_); \
            if (hegel_check_msg_.len) {                                                                \
                fputs(": ", stderr);                                                                   \
                fwrite(hegel_check_msg_.data, 1, hegel_check_msg_.len, stderr);                        \
            }                                                                                          \
            fputc('\n', stderr);                                                                       \
            abort();                                                                                   \
        }                                                                                              \
    } while (0)

#endif
