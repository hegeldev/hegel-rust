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

#define HEGEL_CHECK(fn, ctx, ...)                                             \
    do {                                                                      \
        hegel_result_t hegel_check_rc_ = fn((ctx), ##__VA_ARGS__);            \
        if (hegel_check_rc_ != HEGEL_OK) {                                    \
            const char *hegel_check_msg_ = hegel_context_last_error((ctx));   \
            fprintf(stderr, "%s:%d: %s failed: rc=%d%s%s\n",                  \
                    __FILE__, __LINE__, #fn, (int)hegel_check_rc_,            \
                    (hegel_check_msg_ && hegel_check_msg_[0]) ? ": " : "",    \
                    hegel_check_msg_ ? hegel_check_msg_ : "");                \
            abort();                                                          \
        }                                                                     \
    } while (0)

#endif /* HEGEL_CHECK_H */
