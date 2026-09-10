//! Mirrors of the `repr(C)` types the `hegel_*` functions traffic in.
//!
//! Every definition must match `hegel-c/src/lib.rs` exactly — same
//! discriminants, fields, and layout; the meanings are documented there.
//! `sys_tests.rs` compile-asserts the match whenever the `static-engine`
//! feature makes both sets of definitions visible. The handle structs stand
//! in for types the engine never exposes by value, so they are deliberately
//! opaque. `dead_code` is allowed because this is an ABI surface: variants
//! exist because the engine can produce them, not because the frontend
//! constructs them.

#![allow(dead_code)]
#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void};

macro_rules! opaque_handles {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            pub(crate) struct $name {
                _opaque: [u8; 0],
            }
        )*
    };
}

opaque_handles!(
    HegelCollection,
    HegelContext,
    HegelFailure,
    HegelPool,
    HegelPrinter,
    HegelPrinterOptions,
    HegelRecursion,
    HegelRun,
    HegelRunResult,
    HegelSettings,
    HegelStateMachine,
    HegelStringGenerator,
    HegelTestCase,
);

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[must_use]
pub(crate) enum hegel_result_t {
    HEGEL_OK = 0,
    HEGEL_E_STOP_TEST = -1,
    HEGEL_E_ASSUME = -2,
    HEGEL_E_BACKEND = -3,
    HEGEL_E_INVALID_HANDLE = -4,
    HEGEL_E_INVALID_ARG = -5,
    HEGEL_E_ALREADY_COMPLETE = -6,
    HEGEL_E_NOT_COMPLETE = -7,
    HEGEL_E_INTERNAL = -8,
    HEGEL_E_CONCURRENT_USE = -9,
    HEGEL_E_RETRY = -10,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_status_t {
    HEGEL_STATUS_VALID = 0,
    HEGEL_STATUS_INVALID = 1,
    HEGEL_STATUS_OVERRUN = 2,
    HEGEL_STATUS_INTERESTING = 3,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_backend_t {
    HEGEL_BACKEND_AUTO = 0,
    HEGEL_BACKEND_DEFAULT = 1,
    HEGEL_BACKEND_URANDOM = 2,
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum hegel_run_status_t {
    HEGEL_RUN_STATUS_PASSED = 0,
    HEGEL_RUN_STATUS_FAILED = 1,
    HEGEL_RUN_STATUS_ERROR = 2,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_verbosity_t {
    HEGEL_VERBOSITY_QUIET = 0,
    HEGEL_VERBOSITY_NORMAL = 1,
    HEGEL_VERBOSITY_VERBOSE = 2,
    HEGEL_VERBOSITY_DEBUG = 3,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_nondeterminism_strictness_t {
    HEGEL_NONDETERMINISM_QUIET = 0,
    HEGEL_NONDETERMINISM_WARN = 1,
    HEGEL_NONDETERMINISM_ERROR = 2,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_phase_t {
    HEGEL_PHASE_EXPLICIT = 1 << 0,
    HEGEL_PHASE_REUSE = 1 << 1,
    HEGEL_PHASE_GENERATE = 1 << 2,
    HEGEL_PHASE_TARGET = 1 << 3,
    HEGEL_PHASE_SHRINK = 1 << 4,
    HEGEL_PHASE_ALL = 0x1F,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_health_check_t {
    HEGEL_HC_FILTER_TOO_MUCH = 1 << 0,
    HEGEL_HC_TOO_SLOW = 1 << 1,
    HEGEL_HC_TEST_CASES_TOO_LARGE = 1 << 2,
    HEGEL_HC_LARGE_INITIAL_TEST_CASE = 1 << 3,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) enum hegel_label_t {
    HEGEL_LABEL_LIST = 1,
    HEGEL_LABEL_LIST_ELEMENT = 2,
    HEGEL_LABEL_SET = 3,
    HEGEL_LABEL_SET_ELEMENT = 4,
    HEGEL_LABEL_MAP = 5,
    HEGEL_LABEL_MAP_ENTRY = 6,
    HEGEL_LABEL_TUPLE = 7,
    HEGEL_LABEL_ONE_OF = 8,
    HEGEL_LABEL_OPTIONAL = 9,
    HEGEL_LABEL_FIXED_DICT = 10,
    HEGEL_LABEL_FLAT_MAP = 11,
    HEGEL_LABEL_FILTER = 12,
    HEGEL_LABEL_MAPPED = 13,
    HEGEL_LABEL_SAMPLED_FROM = 14,
    HEGEL_LABEL_ENUM_VARIANT = 15,
    HEGEL_LABEL_FEATURE_FLAG = 16,
    HEGEL_LABEL_REGEX = 17,
    HEGEL_LABEL_EMAIL = 18,
    HEGEL_LABEL_URL = 19,
    HEGEL_LABEL_DOMAIN = 20,
    HEGEL_LABEL_DATE = 21,
    HEGEL_LABEL_TIME = 22,
    HEGEL_LABEL_DATETIME = 23,
    HEGEL_LABEL_UUID = 24,
    HEGEL_LABEL_IP_ADDRESS = 25,
    HEGEL_LABEL_INTEGER = 26,
    HEGEL_LABEL_FLOAT = 27,
    HEGEL_LABEL_BOOLEAN = 28,
    HEGEL_LABEL_BYTES = 29,
    HEGEL_LABEL_STRING = 30,
    HEGEL_LABEL_STATEFUL_RULE = 31,
    HEGEL_LABEL_FRESH_ID = 32,
    HEGEL_LABEL_SET_CHOICE = 33,
    HEGEL_LABEL_CONCURRENCY = 34,
    HEGEL_LABEL_RECURSIVE = 35,
}

pub(crate) type hegel_output_callback_t =
    Option<unsafe extern "C" fn(user_data: *mut c_void, line: *const c_char, len: usize)>;

pub(crate) const HEGEL_STATE_MACHINE_DONE: i64 = i64::MIN;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct hegel_date_t {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct hegel_time_t {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct hegel_datetime_t {
    pub date: hegel_date_t,
    pub time: hegel_time_t,
}

#[repr(C)]
pub(crate) struct hegel_generate_bytes_result_t {
    pub data: *mut u8,
    pub len: usize,
}

#[repr(C)]
pub(crate) struct hegel_generate_string_result_t {
    pub data: *mut c_char,
    pub len: usize,
}

#[repr(C)]
pub(crate) struct hegel_printer_value_result_t {
    pub data: *mut c_char,
    pub len: usize,
}
