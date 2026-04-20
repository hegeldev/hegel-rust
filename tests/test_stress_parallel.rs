// Stress test: many small hegel tests run in parallel against the single
// shared hegel subprocess. Goal is to reproduce the intermittent crash we
// see in CI where a test panics with "Flaky test detected" or
// "connection aborted" mid-run. See hegel-core-diagnosis.md.
//
// We rely on `cargo test` running these in parallel (RUST_TEST_THREADS).
// The more concurrent #[hegel::test] functions share the HegelSession
// at the same time, the more load the Python server's ThreadPoolExecutor
// has to sustain and the better chance we have of hitting the race.
//
// Every test here should be cheap so it finishes fast and loops back to
// hammering the connection again.

mod common;

use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

macro_rules! cheap_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[hegel::test(test_cases = 50)]
            fn $name(tc: TestCase) {
                let _: bool = tc.draw(gs::booleans());
                let _: i32 = tc.draw(gs::integers::<i32>());
            }
        )*
    };
}

cheap_tests!(
    stress_cheap_000,
    stress_cheap_001,
    stress_cheap_002,
    stress_cheap_003,
    stress_cheap_004,
    stress_cheap_005,
    stress_cheap_006,
    stress_cheap_007,
    stress_cheap_008,
    stress_cheap_009,
    stress_cheap_010,
    stress_cheap_011,
    stress_cheap_012,
    stress_cheap_013,
    stress_cheap_014,
    stress_cheap_015,
    stress_cheap_016,
    stress_cheap_017,
    stress_cheap_018,
    stress_cheap_019,
    stress_cheap_020,
    stress_cheap_021,
    stress_cheap_022,
    stress_cheap_023,
    stress_cheap_024,
    stress_cheap_025,
    stress_cheap_026,
    stress_cheap_027,
    stress_cheap_028,
    stress_cheap_029,
    stress_cheap_030,
    stress_cheap_031,
    stress_cheap_032,
    stress_cheap_033,
    stress_cheap_034,
    stress_cheap_035,
    stress_cheap_036,
    stress_cheap_037,
    stress_cheap_038,
    stress_cheap_039,
);

macro_rules! string_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[hegel::test(test_cases = 30)]
            fn $name(tc: TestCase) {
                let s: String = tc.draw(gs::text());
                let _ = s.len();
            }
        )*
    };
}

string_tests!(
    stress_strings_000,
    stress_strings_001,
    stress_strings_002,
    stress_strings_003,
    stress_strings_004,
    stress_strings_005,
    stress_strings_006,
    stress_strings_007,
    stress_strings_008,
    stress_strings_009,
    stress_strings_010,
    stress_strings_011,
    stress_strings_012,
    stress_strings_013,
    stress_strings_014,
    stress_strings_015,
    stress_strings_016,
    stress_strings_017,
    stress_strings_018,
    stress_strings_019,
);

macro_rules! list_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[hegel::test(test_cases = 30)]
            fn $name(tc: TestCase) {
                let v: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()));
                let _ = v.len();
            }
        )*
    };
}

list_tests!(
    stress_lists_000,
    stress_lists_001,
    stress_lists_002,
    stress_lists_003,
    stress_lists_004,
    stress_lists_005,
    stress_lists_006,
    stress_lists_007,
    stress_lists_008,
    stress_lists_009,
    stress_lists_010,
    stress_lists_011,
    stress_lists_012,
    stress_lists_013,
    stress_lists_014,
    stress_lists_015,
    stress_lists_016,
    stress_lists_017,
    stress_lists_018,
    stress_lists_019,
);

// Tests that use map on a basic generator - exercise the single-request
// fast path that's the most heavily used pattern.
macro_rules! mapped_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[hegel::test(test_cases = 30)]
            fn $name(tc: TestCase) {
                let n: i64 = tc.draw(gs::integers::<i32>().map(|x| x as i64 * 2));
                let _ = n;
            }
        )*
    };
}

mapped_tests!(
    stress_mapped_000,
    stress_mapped_001,
    stress_mapped_002,
    stress_mapped_003,
    stress_mapped_004,
    stress_mapped_005,
    stress_mapped_006,
    stress_mapped_007,
    stress_mapped_008,
    stress_mapped_009,
    stress_mapped_010,
    stress_mapped_011,
    stress_mapped_012,
    stress_mapped_013,
    stress_mapped_014,
    stress_mapped_015,
    stress_mapped_016,
    stress_mapped_017,
    stress_mapped_018,
    stress_mapped_019,
);

// Multiple draws per case -> more concurrent span/request traffic.
macro_rules! multidraw_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[hegel::test(test_cases = 25)]
            fn $name(tc: TestCase) {
                for _ in 0..5 {
                    let _: i32 = tc.draw(gs::integers::<i32>());
                    let _: bool = tc.draw(gs::booleans());
                }
            }
        )*
    };
}

multidraw_tests!(
    stress_multidraw_000,
    stress_multidraw_001,
    stress_multidraw_002,
    stress_multidraw_003,
    stress_multidraw_004,
    stress_multidraw_005,
    stress_multidraw_006,
    stress_multidraw_007,
    stress_multidraw_008,
    stress_multidraw_009,
    stress_multidraw_010,
    stress_multidraw_011,
    stress_multidraw_012,
    stress_multidraw_013,
    stress_multidraw_014,
    stress_multidraw_015,
    stress_multidraw_016,
    stress_multidraw_017,
    stress_multidraw_018,
    stress_multidraw_019,
);
