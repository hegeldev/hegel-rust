/*
 * pool.c — demo: stateful-style testing with the variable-pool primitives.
 * Verifies that hegel_new_pool / hegel_pool_add / hegel_pool_generate
 * compose into the building block behind hegel-rust's `stateful::Variables`
 * and `#[hegel::state_machine]`.
 *
 * Each test case models a tiny stack of integers. On every step the engine
 * chooses between two actions:
 *   - push: generate an integer, register it in the pool, and remember its
 *           value keyed by the variable id the engine hands back;
 *   - pop:  draw a variable id out of the pool with consume=true, removing
 *           it both from the pool and from our shadow map.
 * The invariant checked is that every id the engine draws is one we put in
 * and have not yet consumed — i.e. the pool only ever hands back live
 * variables. The C caller keeps its own id -> value map, exactly as
 * `Variables<T>` holds a `HashMap`.
 *
 * Build (same incantation as echo.c):
 *   cc -o pool pool.c -I../include \
 *      -L../../target/release -lhegel \
 *      -Wl,-rpath,$PWD/../../target/release
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#include "hegel.h"
#include "hegel_check.h"

/* CBOR-encoded {"type": "boolean"} — used to pick push vs pop. */
static const uint8_t BOOLEAN_SCHEMA[] = {
    0xA1,                                    /* map(1) */
    0x64, 't', 'y', 'p', 'e',
    0x67, 'b', 'o', 'o', 'l', 'e', 'a', 'n'
};

/* CBOR-encoded {"type": "integer", "min_value": 0, "max_value": 1000}. */
static const uint8_t INTEGER_SCHEMA[] = {
    0xA3,                                              /* map(3) */
    0x64, 't', 'y', 'p', 'e',
    0x67, 'i', 'n', 't', 'e', 'g', 'e', 'r',
    0x69, 'm', 'i', 'n', '_', 'v', 'a', 'l', 'u', 'e',
    0x00,                                              /* 0 */
    0x69, 'm', 'a', 'x', '_', 'v', 'a', 'l', 'u', 'e',
    0x19, 0x03, 0xE8                                   /* 1000 */
};

static bool decode_bool(const uint8_t *bytes, size_t len) {
    if (len < 1) { fprintf(stderr, "decode_bool: empty\n"); exit(2); }
    if (bytes[0] == 0xF5) return true;
    if (bytes[0] == 0xF4) return false;
    fprintf(stderr, "decode_bool: unexpected head 0x%02x\n", bytes[0]);
    exit(2);
}

/* Shadow map of live variables: parallel arrays of id -> value. A real
 * caller would use a hash map; linear scan keeps the example dependency
 * free. */
#define MAX_LIVE 64
struct live_set {
    int64_t ids[MAX_LIVE];
    int64_t values[MAX_LIVE];
    size_t count;
};

static void live_add(struct live_set *s, int64_t id, int64_t value) {
    if (s->count >= MAX_LIVE) { fprintf(stderr, "live_set overflow\n"); exit(2); }
    s->ids[s->count] = id;
    s->values[s->count] = value;
    s->count++;
}

/* Find id and remove it, returning its stored value, or -1 if absent. */
static int64_t live_remove(struct live_set *s, int64_t id) {
    for (size_t i = 0; i < s->count; i++) {
        if (s->ids[i] == id) {
            int64_t value = s->values[i];
            s->ids[i] = s->ids[s->count - 1];
            s->values[i] = s->values[s->count - 1];
            s->count--;
            return value;
        }
    }
    return -1;
}

int main(void) {
    hegel_context_t *ctx = hegel_context_new();

    hegel_settings_t *s;
    HEGEL_CHECK(hegel_settings_new, ctx, &s);
    HEGEL_CHECK(hegel_settings_set_test_cases, ctx, s, 100);
    HEGEL_CHECK(hegel_settings_set_database, ctx, s, "");
    HEGEL_CHECK(hegel_settings_set_derandomize, ctx, s, true);
    HEGEL_CHECK(hegel_settings_set_seed, ctx, s, 0x5ca1ab1e, true);

    hegel_run_t *run;
    HEGEL_CHECK(hegel_run_start, ctx, s, &run);

    const int STEPS = 12;
    size_t total = 0;
    size_t max_pool = 0;
    bool ok = true;

    while (true) {
        hegel_test_case_t *tc;
        HEGEL_CHECK(hegel_next_test_case, ctx, run, &tc);
        if (tc == NULL) break;

        struct live_set live = { .count = 0 };
        int64_t pool;
        if (hegel_new_pool(ctx, tc, &pool) != HEGEL_OK) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }

        bool overran = false;
        bool bad = false;
        for (int step = 0; step < STEPS && !overran; step++) {
            const uint8_t *bytes;
            size_t len;

            /* Decide push vs pop. */
            hegel_result_t rc = hegel_generate(ctx, tc, BOOLEAN_SCHEMA, sizeof(BOOLEAN_SCHEMA), &bytes, &len);
            if (rc != HEGEL_OK) { overran = true; break; }
            bool push = decode_bool(bytes, len);

            if (push || live.count == 0) {
                /* Push: generate a value and register it in the pool. */
                rc = hegel_generate(ctx, tc, INTEGER_SCHEMA, sizeof(INTEGER_SCHEMA), &bytes, &len);
                if (rc != HEGEL_OK) { overran = true; break; }
                /* The integer fits in one or two CBOR bytes for [0,1000];
                 * we only need a representative value, so use the length. */
                int64_t value = (int64_t)len;

                int64_t var_id;
                if (hegel_pool_add(ctx, tc, pool, &var_id) != HEGEL_OK) { overran = true; break; }
                live_add(&live, var_id, value);
            } else {
                /* Pop: draw a live variable and consume it. */
                int64_t var_id;
                rc = hegel_pool_generate(ctx, tc, pool, true, &var_id);
                if (rc == HEGEL_E_STOP_TEST) { overran = true; break; }
                if (rc != HEGEL_OK) { overran = true; break; }

                /* Invariant: the engine only hands back live ids. */
                if (live_remove(&live, var_id) < 0) {
                    bad = true;
                    break;
                }
            }
            if (live.count > max_pool) max_pool = live.count;
        }

        if (bad) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_INTERESTING,
                        "drew a non-live variable");
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            ok = false;
            continue;
        }
        if (overran) {
            HEGEL_CHECK(hegel_mark_complete, ctx, tc, HEGEL_STATUS_OVERRUN, NULL);
            HEGEL_CHECK(hegel_test_case_free, ctx, tc);
            continue;
        }
        total++;
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

    printf("ran %zu valid cases (max live pool size seen: %zu), %s\n",
           total, max_pool, passed ? "PASSED" : "FAILED");

    HEGEL_CHECK(hegel_run_free, ctx, run);
    HEGEL_CHECK(hegel_settings_free, ctx, s);
    HEGEL_CHECK(hegel_context_free, ctx);
    return (passed && ok) ? 0 : 1;
}
