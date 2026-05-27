/*
 * libhegel — C bindings for Hegel's native property-based testing engine.
 *
 * This header is generated from hegel-c/src/lib.rs by cbindgen. Do not
 * edit it directly; re-run `just c-header` after changing the Rust source.
 */

#ifndef HEGEL_H
#define HEGEL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#define HEGEL_OK 0

#define HEGEL_E_STOP_TEST -1

#define HEGEL_E_ASSUME -2

#define HEGEL_E_BACKEND -3

#define HEGEL_E_INVALID_HANDLE -4

#define HEGEL_E_INVALID_ARG -5

#define HEGEL_E_ALREADY_COMPLETE -6

#define HEGEL_E_NOT_COMPLETE -7

#define HEGEL_E_INTERNAL -8

#define HEGEL_PHASE_EXPLICIT (1 << 0)

#define HEGEL_PHASE_REUSE (1 << 1)

#define HEGEL_PHASE_GENERATE (1 << 2)

#define HEGEL_PHASE_TARGET (1 << 3)

#define HEGEL_PHASE_SHRINK (1 << 4)

#define HEGEL_PHASE_ALL 31

#define HEGEL_HC_FILTER_TOO_MUCH (1 << 0)

#define HEGEL_HC_TOO_SLOW (1 << 1)

#define HEGEL_HC_TEST_CASES_TOO_LARGE (1 << 2)

#define HEGEL_HC_LARGE_INITIAL_TEST_CASE (1 << 3)

#define HEGEL_LABEL_LIST 1

#define HEGEL_LABEL_LIST_ELEMENT 2

#define HEGEL_LABEL_SET 3

#define HEGEL_LABEL_SET_ELEMENT 4

#define HEGEL_LABEL_MAP 5

#define HEGEL_LABEL_MAP_ENTRY 6

#define HEGEL_LABEL_TUPLE 7

#define HEGEL_LABEL_ONE_OF 8

#define HEGEL_LABEL_OPTIONAL 9

#define HEGEL_LABEL_FIXED_DICT 10

#define HEGEL_LABEL_FLAT_MAP 11

#define HEGEL_LABEL_FILTER 12

#define HEGEL_LABEL_MAPPED 13

#define HEGEL_LABEL_SAMPLED_FROM 14

#define HEGEL_LABEL_ENUM_VARIANT 15

typedef enum {
    HEGEL_MODE_TEST_RUN = 0,
    HEGEL_MODE_SINGLE_TEST_CASE = 1,
} hegel_mode_t;

typedef enum {
    HEGEL_VERBOSITY_QUIET = 0,
    HEGEL_VERBOSITY_NORMAL = 1,
    HEGEL_VERBOSITY_VERBOSE = 2,
    HEGEL_VERBOSITY_DEBUG = 3,
} hegel_verbosity_t;

typedef enum {
    HEGEL_STATUS_VALID = 0,
    HEGEL_STATUS_INVALID = 1,
    HEGEL_STATUS_OVERRUN = 2,
    HEGEL_STATUS_INTERESTING = 3,
} hegel_status_t;

typedef struct hegel_failure_t hegel_failure_t;

typedef struct hegel_run_t hegel_run_t;

typedef struct hegel_run_result_t hegel_run_result_t;

typedef struct hegel_settings_t hegel_settings_t;

typedef struct hegel_test_case_t hegel_test_case_t;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

hegel_settings_t *hegel_settings_new(void);

void hegel_settings_free(hegel_settings_t *s);

void hegel_settings_mode(hegel_settings_t *s, hegel_mode_t mode);

void hegel_settings_test_cases(hegel_settings_t *s, uint64_t n);

void hegel_settings_verbosity(hegel_settings_t *s, hegel_verbosity_t v);

void hegel_settings_seed(hegel_settings_t *s, uint64_t seed, bool has_seed);

void hegel_settings_derandomize(hegel_settings_t *s, bool derandomize);

void hegel_settings_report_multiple_failures(hegel_settings_t *s, bool yes);

/*
 `database = NULL` → default; `database = ""` → disabled; else → path.
 */
void hegel_settings_database(hegel_settings_t *s, const char *database);

/*
 Set the database key used to scope stored / replayed examples for this run.
 `key = NULL` clears it (the default).
 */
void hegel_settings_database_key(hegel_settings_t *s, const char *key);

void hegel_settings_phases(hegel_settings_t *s, uint32_t phases);

void hegel_settings_suppress_health_check(hegel_settings_t *s, uint32_t checks);

hegel_run_t *hegel_run_start(const hegel_settings_t *settings);

hegel_test_case_t *hegel_next_test_case(hegel_run_t *run);

const hegel_run_result_t *hegel_run_result(hegel_run_t *run);

void hegel_run_free(hegel_run_t *run);

int hegel_generate(hegel_test_case_t *tc,
                   const uint8_t *schema_cbor,
                   size_t schema_len,
                   const uint8_t **out_value_cbor,
                   size_t *out_value_len);

int hegel_start_span(hegel_test_case_t *tc, uint64_t label);

int hegel_stop_span(hegel_test_case_t *tc, bool discard);

/*
 `max_size = UINT64_MAX` (i.e. `u64::MAX`) means unbounded.
 */
int hegel_new_collection(hegel_test_case_t *tc,
                         uint64_t min_size,
                         uint64_t max_size,
                         int64_t *out_collection_id);

int hegel_collection_more(hegel_test_case_t *tc, int64_t collection_id, bool *out_more);

int hegel_collection_reject(hegel_test_case_t *tc, int64_t collection_id, const char *why);

int hegel_target(hegel_test_case_t *tc, double value, const char *label);

/*
 Mark this test case complete with the given status.

 `origin` is used only when `status == HEGEL_STATUS_INTERESTING`; for
 other statuses it can be NULL. It identifies *which bug* this failure
 is — two failures with identical origin strings are treated as the
 same bug and shrunk together; failures with different origins are
 treated as distinct bugs and the shrink budget is *partitioned*
 across them.

 This makes the choice of origin string load-bearing for shrinker
 quality. In particular, bindings that recover from a host-language
 panic to call this function MUST NOT pass the recovered panic value
 (or its stringification) as origin if that value depends on the
 failing draw — every distinct draw would then look like a fresh bug
 to the engine and the shrinker would never converge.

 The conventional shape is `"Panic at <file>:<line>"` — i.e. derive
 origin from the *location* of the failing assertion, not the
 assertion's message. hegel-rust's own panic-to-failure path does
 exactly this (see `src/run_lifecycle.rs`).
 */
int hegel_mark_complete(hegel_test_case_t *tc, hegel_status_t status, const char *origin);

bool hegel_test_case_is_final_replay(const hegel_test_case_t *tc);

bool hegel_run_result_passed(const hegel_run_result_t *r);

size_t hegel_run_result_failure_count(const hegel_run_result_t *r);

const hegel_failure_t *hegel_run_result_failure(const hegel_run_result_t *r, size_t index);

const char *hegel_failure_panic_message(const hegel_failure_t *f);

const char *hegel_failure_diagnostic(const hegel_failure_t *f);

const char *hegel_failure_origin(const hegel_failure_t *f);

const char *hegel_last_error_message(void);

const char *hegel_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* HEGEL_H */
