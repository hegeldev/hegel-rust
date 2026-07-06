//! In-process exercise of the typed `hegel_generate_*` draw functions.
//!
//! Covers the happy paths plus the null-handle / argument-validation /
//! overrun paths for the typed primitive draws, mirroring the approach of
//! `c_abi_inprocess.rs`: calling the exported functions directly as Rust
//! items so the branches are measured by coverage.

use hegel_c::hegel_result_t::*;
use hegel_c::{
    HegelContext, HegelRun, HegelSettings, HegelStringGenerator, HegelTestCase, hegel_context_free,
    hegel_context_last_error, hegel_context_new, hegel_generate_boolean, hegel_generate_bytes,
    hegel_generate_bytes_result_free, hegel_generate_bytes_result_t, hegel_generate_date,
    hegel_generate_datetime, hegel_generate_float, hegel_generate_integer,
    hegel_generate_integer_big, hegel_generate_ipv4, hegel_generate_ipv6, hegel_generate_string,
    hegel_generate_string_result_free, hegel_generate_string_result_t, hegel_generate_time,
    hegel_generate_uuid, hegel_mark_complete, hegel_next_test_case, hegel_run_free,
    hegel_run_start, hegel_settings_free, hegel_settings_new, hegel_settings_set_database,
    hegel_settings_set_test_cases, hegel_status_t, hegel_string_generator_domain,
    hegel_string_generator_email, hegel_string_generator_free, hegel_string_generator_regex,
    hegel_string_generator_text, hegel_string_generator_url,
};
use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;

fn ok(rc: hegel_c::hegel_result_t) {
    assert_eq!(rc, HEGEL_OK);
}

fn last_error(ctx: *const HegelContext) -> String {
    let p = unsafe { hegel_context_last_error(ctx) };
    if p.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    }
}

unsafe fn make_settings(ctx: *mut HegelContext) -> *mut HegelSettings {
    let mut s: *mut HegelSettings = ptr::null_mut();
    assert_eq!(unsafe { hegel_settings_new(ctx, &mut s) }, HEGEL_OK);
    let empty = CString::new("").unwrap();
    assert_eq!(
        unsafe { hegel_settings_set_database(ctx, s, empty.as_ptr()) },
        HEGEL_OK
    );
    s
}

unsafe fn start(ctx: *mut HegelContext, s: *const HegelSettings) -> *mut HegelRun {
    let mut run: *mut HegelRun = ptr::null_mut();
    assert_eq!(unsafe { hegel_run_start(ctx, s, &mut run) }, HEGEL_OK);
    run
}

unsafe fn next_case(ctx: *mut HegelContext, run: *mut HegelRun) -> *mut HegelTestCase {
    let mut tc: *mut HegelTestCase = ptr::null_mut();
    assert_eq!(unsafe { hegel_next_test_case(ctx, run, &mut tc) }, HEGEL_OK);
    tc
}

unsafe fn complete_valid(ctx: *mut HegelContext, tc: *mut HegelTestCase) {
    unsafe {
        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID,
            ptr::null(),
        ));
        ok(hegel_c::hegel_test_case_free(ctx, tc));
    }
}

unsafe fn drain(ctx: *mut HegelContext, run: *mut HegelRun) {
    loop {
        let tc = unsafe { next_case(ctx, run) };
        if tc.is_null() {
            break;
        }
        unsafe { complete_valid(ctx, tc) };
    }
}

/// Decode a little-endian two's-complement byte buffer that is known to hold
/// a non-negative value into a u128.
fn decode_unsigned_le(bytes: &[u8]) -> u128 {
    assert!(bytes.len() <= 17);
    assert!(*bytes.last().unwrap() & 0x80 == 0 || bytes.len() < 17);
    let mut v: u128 = 0;
    for (i, b) in bytes.iter().enumerate().take(16) {
        v |= u128::from(*b) << (8 * i);
    }
    if bytes.len() == 17 {
        assert_eq!(bytes[16], 0, "value must be non-negative");
    }
    v
}

#[test]
fn integer_draws_respect_bounds_and_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let mut out = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, ptr::null_mut(), -5, 10, &mut out),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 20));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            assert_eq!(
                hegel_generate_integer(ctx, tc, -5, 10, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("out parameter is null"));
            assert_eq!(
                hegel_generate_integer(ctx, tc, 10, -5, &mut out),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("min_value"));
            assert!(
                last_error(ctx).contains("[10, -5]"),
                "bounds should print as plain integers: {:?}",
                last_error(ctx)
            );

            ok(hegel_generate_integer(ctx, tc, -5, 10, &mut out));
            assert!((-5..=10).contains(&out));
            ok(hegel_generate_integer(ctx, tc, 7, 7, &mut out));
            assert_eq!(out, 7);
            ok(hegel_generate_integer(
                ctx,
                tc,
                i64::MIN,
                i64::MAX,
                &mut out,
            ));
            complete_valid(ctx, tc);
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn big_integer_draws_round_trip_and_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let min = [0u8];
        let max = [0xFFu8; 17];
        let max = {
            let mut m = max;
            m[16] = 0;
            m
        };
        let mut out_buf = [0u8; 17];
        let mut out_len = 0usize;

        assert_eq!(
            hegel_generate_integer_big(
                ctx,
                ptr::null_mut(),
                min.as_ptr(),
                1,
                max.as_ptr(),
                17,
                out_buf.as_mut_ptr(),
                17,
                &mut out_len,
            ),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 20));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            for (min_ptr, min_len, max_ptr, max_len, out_ptr, out_cap, out_len_ptr, expect) in [
                (
                    ptr::null(),
                    1usize,
                    max.as_ptr(),
                    17usize,
                    out_buf.as_mut_ptr(),
                    17usize,
                    &raw mut out_len,
                    "min_value pointer is null",
                ),
                (
                    min.as_ptr(),
                    1,
                    ptr::null(),
                    17,
                    out_buf.as_mut_ptr(),
                    17,
                    &raw mut out_len,
                    "max_value pointer is null",
                ),
                (
                    min.as_ptr(),
                    0,
                    max.as_ptr(),
                    17,
                    out_buf.as_mut_ptr(),
                    17,
                    &raw mut out_len,
                    "must not be empty",
                ),
                (
                    min.as_ptr(),
                    1,
                    max.as_ptr(),
                    0,
                    out_buf.as_mut_ptr(),
                    17,
                    &raw mut out_len,
                    "must not be empty",
                ),
                (
                    min.as_ptr(),
                    1,
                    max.as_ptr(),
                    17,
                    ptr::null_mut(),
                    17,
                    &raw mut out_len,
                    "out parameter is null",
                ),
                (
                    min.as_ptr(),
                    1,
                    max.as_ptr(),
                    17,
                    out_buf.as_mut_ptr(),
                    17,
                    ptr::null_mut(),
                    "out parameter is null",
                ),
            ] {
                assert_eq!(
                    hegel_generate_integer_big(
                        ctx,
                        tc,
                        min_ptr,
                        min_len,
                        max_ptr,
                        max_len,
                        out_ptr,
                        out_cap,
                        out_len_ptr,
                    ),
                    HEGEL_E_INVALID_ARG
                );
                assert!(
                    last_error(ctx).contains(expect),
                    "expected {expect:?} in {:?}",
                    last_error(ctx)
                );
            }

            assert_eq!(
                hegel_generate_integer_big(
                    ctx,
                    tc,
                    max.as_ptr(),
                    17,
                    min.as_ptr(),
                    1,
                    out_buf.as_mut_ptr(),
                    17,
                    &mut out_len,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("min_value"));

            assert_eq!(
                hegel_generate_integer_big(
                    ctx,
                    tc,
                    min.as_ptr(),
                    1,
                    max.as_ptr(),
                    17,
                    out_buf.as_mut_ptr(),
                    0,
                    &mut out_len,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("buffer"));

            ok(hegel_generate_integer_big(
                ctx,
                tc,
                min.as_ptr(),
                1,
                max.as_ptr(),
                17,
                out_buf.as_mut_ptr(),
                17,
                &mut out_len,
            ));
            assert!((1..=17).contains(&out_len));
            decode_unsigned_le(&out_buf[..out_len]);

            let seven = [7u8];
            ok(hegel_generate_integer_big(
                ctx,
                tc,
                seven.as_ptr(),
                1,
                seven.as_ptr(),
                1,
                out_buf.as_mut_ptr(),
                17,
                &mut out_len,
            ));
            assert_eq!(&out_buf[..out_len], &[7]);
            complete_valid(ctx, tc);
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn big_integer_boundary_values_fit_documented_buffer_size() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 5));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            for bound in [
                i64::MIN.to_le_bytes().to_vec(),
                (-128i8).to_le_bytes().to_vec(),
                (-32768i16).to_le_bytes().to_vec(),
            ] {
                let mut out_buf = [0u8; 8];
                let mut out_len = 0usize;
                ok(hegel_generate_integer_big(
                    ctx,
                    tc,
                    bound.as_ptr(),
                    bound.len(),
                    bound.as_ptr(),
                    bound.len(),
                    out_buf.as_mut_ptr(),
                    bound.len(),
                    &mut out_len,
                ));
                assert_eq!(out_len, bound.len());
                assert_eq!(&out_buf[..out_len], &bound[..]);
            }

            for (value, fill) in [((-128i64), 0xFFu8), (7, 0x00)] {
                let bound = value.to_le_bytes();
                let mut out_buf = [0xABu8; 8];
                let mut out_len = 0usize;
                ok(hegel_generate_integer_big(
                    ctx,
                    tc,
                    bound.as_ptr(),
                    1,
                    bound.as_ptr(),
                    1,
                    out_buf.as_mut_ptr(),
                    out_buf.len(),
                    &mut out_len,
                ));
                assert_eq!(out_len, 1);
                assert_eq!(out_buf[0], bound[0]);
                assert_eq!(
                    &out_buf[1..],
                    &[fill; 7],
                    "buffer beyond out_len must be sign-filled so fixed-width \
                     reads of the full buffer are correct"
                );
                assert_eq!(i64::from_le_bytes(out_buf), value);
            }
            complete_valid(ctx, tc);
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn float_draws_respect_spec_and_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let mut out = 0f64;
        assert_eq!(
            hegel_generate_float(
                ctx,
                ptr::null_mut(),
                64,
                0.0,
                1.0,
                false,
                false,
                false,
                false,
                f64::from_bits(1),
                &mut out,
            ),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 20));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            assert_eq!(
                hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    0.0,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from_bits(1),
                    ptr::null_mut(),
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("out parameter is null"));
            assert_eq!(
                hegel_generate_float(
                    ctx,
                    tc,
                    16,
                    0.0,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from_bits(1),
                    &mut out,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("width"));
            assert_eq!(
                hegel_generate_float(
                    ctx, tc, 64, 0.0, 1.0, false, false, false, false, 0.0, &mut out,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("smallest_nonzero_magnitude"));
            assert_eq!(
                hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    f64::NAN,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from_bits(1),
                    &mut out,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("NaN"));
            assert_eq!(
                hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    2.0,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from_bits(1),
                    &mut out,
                ),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("min_value"));
            assert_eq!(
                hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    1.0,
                    1.0,
                    false,
                    false,
                    true,
                    false,
                    f64::from_bits(1),
                    &mut out,
                ),
                HEGEL_E_INVALID_ARG
            );

            let overran = 'draws: {
                let check = |rc: hegel_c::hegel_result_t| match rc {
                    HEGEL_OK => false,
                    HEGEL_E_STOP_TEST => true,
                    other => panic!("unexpected rc: {other:?}"),
                };
                if check(hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    0.0,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from_bits(1),
                    &mut out,
                )) {
                    break 'draws true;
                }
                assert!((0.0..=1.0).contains(&out));

                if check(hegel_generate_float(
                    ctx,
                    tc,
                    32,
                    0.0,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                    f64::from(f32::from_bits(1)),
                    &mut out,
                )) {
                    break 'draws true;
                }
                assert!((0.0..=1.0).contains(&out));
                assert_eq!(out, f64::from(out as f32));

                if check(hegel_generate_float(
                    ctx,
                    tc,
                    64,
                    0.0,
                    1.0,
                    false,
                    false,
                    true,
                    true,
                    f64::from_bits(1),
                    &mut out,
                )) {
                    break 'draws true;
                }
                assert!(out > 0.0 && out < 1.0);

                if check(hegel_generate_float(
                    ctx,
                    tc,
                    32,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    true,
                    false,
                    false,
                    false,
                    f64::from(f32::from_bits(1)),
                    &mut out,
                )) {
                    break 'draws true;
                }
                assert!(out.is_nan() || (f64::from(f32::MIN)..=f64::from(f32::MAX)).contains(&out));
                false
            };
            if overran {
                ok(hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_OVERRUN,
                    ptr::null(),
                ));
                ok(hegel_c::hegel_test_case_free(ctx, tc));
                continue;
            }
            complete_valid(ctx, tc);
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn boolean_draws_validate_and_respect_degenerate_probabilities() {
    let ctx = hegel_context_new();
    unsafe {
        let mut out = false;
        assert_eq!(
            hegel_generate_boolean(ctx, ptr::null_mut(), 0.5, false, false, &mut out),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 5));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 0.5, false, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_generate_boolean(ctx, tc, -0.5, false, false, &mut out),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 0.0, true, true, &mut out),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 1.0, false, true, &mut out),
            HEGEL_E_INVALID_ARG
        );
        ok(hegel_generate_boolean(ctx, tc, 0.0, false, false, &mut out));
        assert!(!out);
        ok(hegel_generate_boolean(ctx, tc, 1.0, false, false, &mut out));
        assert!(out);
        ok(hegel_generate_boolean(ctx, tc, 0.5, true, true, &mut out));
        assert!(out);
        ok(hegel_generate_boolean(ctx, tc, 0.5, false, false, &mut out));
        complete_valid(ctx, tc);
        drain(ctx, run);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn bytes_draws_transfer_ownership_and_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let mut result = hegel_generate_bytes_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            hegel_generate_bytes(ctx, ptr::null_mut(), 0, 3, &mut result),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 5));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert_eq!(
            hegel_generate_bytes(ctx, tc, 0, 3, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        assert_eq!(
            hegel_generate_bytes(ctx, tc, 4, 3, &mut result),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("min_size"));

        ok(hegel_generate_bytes(ctx, tc, 3, 3, &mut result));
        assert_eq!(result.len, 3);
        assert!(!result.data.is_null());
        std::slice::from_raw_parts(result.data, result.len).to_vec();
        ok(hegel_generate_bytes_result_free(ctx, &mut result));
        assert!(result.data.is_null());
        assert_eq!(result.len, 0);
        ok(hegel_generate_bytes_result_free(ctx, &mut result));
        ok(hegel_generate_bytes_result_free(ctx, ptr::null_mut()));

        ok(hegel_generate_bytes(ctx, tc, 0, 0, &mut result));
        assert_eq!(result.len, 0);
        ok(hegel_generate_bytes_result_free(ctx, &mut result));

        complete_valid(ctx, tc);
        drain(ctx, run);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

/// Build a default full-Unicode text generator with the given size bounds.
unsafe fn text_generator(
    ctx: *mut HegelContext,
    min_size: u64,
    max_size: u64,
) -> *mut HegelStringGenerator {
    let mut g: *mut HegelStringGenerator = ptr::null_mut();
    assert_eq!(
        unsafe {
            hegel_string_generator_text(
                ctx,
                min_size,
                max_size,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            )
        },
        HEGEL_OK
    );
    assert!(!g.is_null());
    g
}

unsafe fn draw_string(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    g: *const HegelStringGenerator,
) -> String {
    let mut result = hegel_generate_string_result_t {
        data: ptr::null_mut(),
        len: 0,
    };
    assert_eq!(
        unsafe { hegel_generate_string(ctx, tc, g, &mut result) },
        HEGEL_OK
    );
    assert!(!result.data.is_null());
    let s = String::from_utf8(
        unsafe { std::slice::from_raw_parts(result.data.cast::<u8>(), result.len) }.to_vec(),
    )
    .unwrap();
    unsafe { ok(hegel_generate_string_result_free(ctx, &mut result)) };
    assert!(result.data.is_null());
    s
}

#[test]
fn text_generator_constructor_validates_and_draws() {
    let ctx = hegel_context_new();
    unsafe {
        let mut g: *mut HegelStringGenerator = ptr::null_mut();
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null_mut(),
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));

        assert_eq!(
            hegel_string_generator_text(
                ctx,
                5,
                4,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("min_size"));

        let ebcdic = CString::new("ebcdic").unwrap();
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ebcdic.as_ptr(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("invalid codec"));

        let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                bad_utf8.as_ptr(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("codec is not valid UTF-8"));

        let bad_include: [u8; 1] = [0xFF];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                bad_include.as_ptr(),
                1,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("include_characters is not valid UTF-8"));

        let empty_cats: [*const c_char; 0] = [];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                empty_cats.as_ptr(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("No valid characters"));

        let null_cat: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                null_cat.as_ptr(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("categories[0] is null"));

        let bad_cat: [*const c_char; 1] = [bad_utf8.as_ptr()];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                bad_cat.as_ptr(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("categories[0] is not valid UTF-8"));

        let null_excl_cat: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                null_excl_cat.as_ptr(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("exclude_categories[0] is null"));

        let bad_exclude: [u8; 1] = [0xFF];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                0,
                10,
                ptr::null(),
                0,
                u32::MAX,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                bad_exclude.as_ptr(),
                1,
                &mut g,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("exclude_characters is not valid UTF-8"));

        let ascii = CString::new("ascii").unwrap();
        let nd = CString::new("Nd").unwrap();
        let cats: [*const c_char; 1] = [nd.as_ptr()];
        assert_eq!(
            hegel_string_generator_text(
                ctx,
                1,
                8,
                ascii.as_ptr(),
                0,
                u32::MAX,
                cats.as_ptr(),
                1,
                ptr::null(),
                0,
                b"a".as_ptr(),
                1,
                b"0".as_ptr(),
                1,
                &mut g,
            ),
            HEGEL_OK
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 20));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let drawn = draw_string(ctx, tc, g);
            let n = drawn.chars().count();
            assert!((1..=8).contains(&n), "bad length: {drawn:?}");
            for c in drawn.chars() {
                assert!(
                    c == 'a' || (c.is_ascii_digit() && c != '0'),
                    "bad char {c:?} in {drawn:?}"
                );
            }
            complete_valid(ctx, tc);
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_string_generator_free(ctx, g));
        ok(hegel_string_generator_free(ctx, ptr::null_mut()));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn regex_email_url_domain_generators_draw_valid_values() {
    let ctx = hegel_context_new();
    unsafe {
        let mut regex_g: *mut HegelStringGenerator = ptr::null_mut();
        assert_eq!(
            hegel_string_generator_regex(ctx, ptr::null(), false, ptr::null(), &mut regex_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("pattern is null"));
        let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
        assert_eq!(
            hegel_string_generator_regex(ctx, bad_utf8.as_ptr(), false, ptr::null(), &mut regex_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("pattern is not valid UTF-8"));
        let unclosed = CString::new("(unclosed").unwrap();
        assert_eq!(
            hegel_string_generator_regex(ctx, unclosed.as_ptr(), false, ptr::null(), &mut regex_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("invalid regex pattern"));
        let pattern = CString::new("[ab]{2,4}").unwrap();
        assert_eq!(
            hegel_string_generator_regex(
                ctx,
                pattern.as_ptr(),
                false,
                ptr::null(),
                ptr::null_mut()
            ),
            HEGEL_E_INVALID_ARG
        );

        let mut email_g: *mut HegelStringGenerator = ptr::null_mut();
        assert_eq!(
            hegel_string_generator_email(ctx, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(hegel_string_generator_email(ctx, &mut email_g), HEGEL_OK);

        let mut url_g: *mut HegelStringGenerator = ptr::null_mut();
        assert_eq!(
            hegel_string_generator_url(ctx, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(hegel_string_generator_url(ctx, &mut url_g), HEGEL_OK);

        let mut domain_g: *mut HegelStringGenerator = ptr::null_mut();
        assert_eq!(
            hegel_string_generator_domain(ctx, 255, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_string_generator_domain(ctx, 3, &mut domain_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("no eligible TLDs"));
        assert_eq!(
            hegel_string_generator_domain(ctx, 256, &mut domain_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("255"));
        assert_eq!(
            hegel_string_generator_domain(ctx, 255, &mut domain_g),
            HEGEL_OK
        );

        assert_eq!(
            hegel_string_generator_regex(ctx, pattern.as_ptr(), true, email_g, &mut regex_g),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("must be a text string generator"));
        let alphabet = text_generator(ctx, 0, 10);
        assert_eq!(
            hegel_string_generator_regex(ctx, pattern.as_ptr(), true, alphabet, &mut regex_g),
            HEGEL_OK
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 10));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let re = draw_string(ctx, tc, regex_g);
            assert!(
                (2..=4).contains(&re.len()) && re.chars().all(|c| c == 'a' || c == 'b'),
                "regex draw {re:?} does not fullmatch [ab]{{2,4}}"
            );

            let url = draw_string(ctx, tc, url_g);
            assert!(url.starts_with("http://") || url.starts_with("https://"));

            let domain = draw_string(ctx, tc, domain_g);
            assert!(domain.contains('.'));

            let mut result = hegel_generate_string_result_t {
                data: ptr::null_mut(),
                len: 0,
            };
            match hegel_generate_string(ctx, tc, email_g, &mut result) {
                HEGEL_OK => {
                    let email = String::from_utf8(
                        std::slice::from_raw_parts(result.data.cast::<u8>(), result.len).to_vec(),
                    )
                    .unwrap();
                    assert!(email.contains('@'), "no @ in {email:?}");
                    ok(hegel_generate_string_result_free(ctx, &mut result));
                    ok(hegel_mark_complete(
                        ctx,
                        tc,
                        hegel_status_t::HEGEL_STATUS_VALID,
                        ptr::null(),
                    ));
                }
                HEGEL_E_ASSUME => {
                    ok(hegel_mark_complete(
                        ctx,
                        tc,
                        hegel_status_t::HEGEL_STATUS_INVALID,
                        ptr::null(),
                    ));
                }
                other => panic!("unexpected rc from email draw: {other:?}"),
            }
            ok(hegel_c::hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));

        ok(hegel_string_generator_free(ctx, regex_g));
        ok(hegel_string_generator_free(ctx, email_g));
        ok(hegel_string_generator_free(ctx, url_g));
        ok(hegel_string_generator_free(ctx, domain_g));
        ok(hegel_string_generator_free(ctx, alphabet));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn generate_string_validates_handles_and_reports_stop_test() {
    let ctx = hegel_context_new();
    unsafe {
        let g = text_generator(ctx, 0, 10);
        let mut result = hegel_generate_string_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            hegel_generate_string(ctx, ptr::null_mut(), g, &mut result),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 1));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert_eq!(
            hegel_generate_string(ctx, tc, ptr::null(), &mut result),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_generate_string(ctx, tc, g, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));

        let mut bytes_result = hegel_generate_bytes_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        loop {
            match hegel_generate_bytes(ctx, tc, 1000, 10000, &mut bytes_result) {
                HEGEL_OK => ok(hegel_generate_bytes_result_free(ctx, &mut bytes_result)),
                HEGEL_E_STOP_TEST => break,
                other => panic!("unexpected rc: {other:?}"),
            }
        }
        assert_eq!(
            hegel_generate_string(ctx, tc, g, &mut result),
            HEGEL_E_STOP_TEST
        );

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_OVERRUN,
            ptr::null(),
        ));
        ok(hegel_c::hegel_test_case_free(ctx, tc));
        drain(ctx, run);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_string_generator_free(ctx, g));

        ok(hegel_generate_string_result_free(ctx, ptr::null_mut()));
        ok(hegel_generate_string_result_free(ctx, &mut result));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn structured_draws_produce_valid_values_and_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let mut date = hegel_c::hegel_date_t {
            year: 0,
            month: 0,
            day: 0,
        };
        assert_eq!(
            hegel_generate_date(ctx, ptr::null_mut(), &mut date),
            HEGEL_E_INVALID_HANDLE
        );
        let mut null_time = hegel_c::hegel_time_t {
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        };
        assert_eq!(
            hegel_generate_time(ctx, ptr::null_mut(), &mut null_time),
            HEGEL_E_INVALID_HANDLE
        );
        let mut null_dt = hegel_c::hegel_datetime_t {
            date: hegel_c::hegel_date_t {
                year: 0,
                month: 0,
                day: 0,
            },
            time: null_time,
        };
        assert_eq!(
            hegel_generate_datetime(ctx, ptr::null_mut(), &mut null_dt),
            HEGEL_E_INVALID_HANDLE
        );
        let mut null_uuid = [0u8; 16];
        assert_eq!(
            hegel_generate_uuid(ctx, ptr::null_mut(), 0, false, null_uuid.as_mut_ptr()),
            HEGEL_E_INVALID_HANDLE
        );
        let mut null_ip = [0u8; 16];
        assert_eq!(
            hegel_generate_ipv4(ctx, ptr::null_mut(), null_ip.as_mut_ptr()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_generate_ipv6(ctx, ptr::null_mut(), null_ip.as_mut_ptr()),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 20));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            assert_eq!(
                hegel_generate_date(ctx, tc, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            assert_eq!(
                hegel_generate_time(ctx, tc, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            assert_eq!(
                hegel_generate_datetime(ctx, tc, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            assert_eq!(
                hegel_generate_uuid(ctx, tc, 4, true, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            let mut uuid = [0u8; 16];
            assert_eq!(
                hegel_generate_uuid(ctx, tc, 16, true, uuid.as_mut_ptr()),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("hex nibble"));
            let mut ip = [0u8; 16];
            assert_eq!(
                hegel_generate_ipv4(ctx, tc, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );
            assert_eq!(
                hegel_generate_ipv6(ctx, tc, ptr::null_mut()),
                HEGEL_E_INVALID_ARG
            );

            let overran = 'draws: {
                let check = |rc: hegel_c::hegel_result_t| match rc {
                    HEGEL_OK => false,
                    HEGEL_E_STOP_TEST => true,
                    other => panic!("unexpected rc: {other:?}"),
                };
                if check(hegel_generate_date(ctx, tc, &mut date)) {
                    break 'draws true;
                }
                assert!((1..=9999).contains(&date.year));
                assert!((1..=12).contains(&date.month));
                assert!((1..=31).contains(&date.day));

                let mut time = hegel_c::hegel_time_t {
                    hour: 0,
                    minute: 0,
                    second: 0,
                    microsecond: 0,
                };
                if check(hegel_generate_time(ctx, tc, &mut time)) {
                    break 'draws true;
                }
                assert!(time.hour <= 23 && time.minute <= 59 && time.second <= 59);
                assert!(time.microsecond <= 999_999);

                let mut dt = hegel_c::hegel_datetime_t {
                    date: hegel_c::hegel_date_t {
                        year: 0,
                        month: 0,
                        day: 0,
                    },
                    time: hegel_c::hegel_time_t {
                        hour: 0,
                        minute: 0,
                        second: 0,
                        microsecond: 0,
                    },
                };
                if check(hegel_generate_datetime(ctx, tc, &mut dt)) {
                    break 'draws true;
                }
                assert!((1..=9999).contains(&dt.date.year));
                assert!(dt.time.hour <= 23);

                if check(hegel_generate_uuid(ctx, tc, 4, true, uuid.as_mut_ptr())) {
                    break 'draws true;
                }
                assert_eq!(uuid[6] >> 4, 4, "version nibble");
                assert!(matches!(uuid[8] >> 4, 0x8..=0xb), "variant nibble");
                if check(hegel_generate_uuid(ctx, tc, 0, false, uuid.as_mut_ptr())) {
                    break 'draws true;
                }
                assert_ne!(uuid, [0u8; 16], "nil UUID must never be produced");

                if check(hegel_generate_ipv4(ctx, tc, ip.as_mut_ptr())) {
                    break 'draws true;
                }
                if check(hegel_generate_ipv6(ctx, tc, ip.as_mut_ptr())) {
                    break 'draws true;
                }
                false
            };

            if overran {
                ok(hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_OVERRUN,
                    ptr::null(),
                ));
                ok(hegel_c::hegel_test_case_free(ctx, tc));
            } else {
                complete_valid(ctx, tc);
            }
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn structured_draws_after_overrun_report_stop_test() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 1));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        let mut result = hegel_generate_bytes_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        loop {
            match hegel_generate_bytes(ctx, tc, 1000, 10000, &mut result) {
                HEGEL_OK => ok(hegel_generate_bytes_result_free(ctx, &mut result)),
                HEGEL_E_STOP_TEST => break,
                other => panic!("unexpected rc: {other:?}"),
            }
        }
        let mut date = hegel_c::hegel_date_t {
            year: 0,
            month: 0,
            day: 0,
        };
        assert_eq!(hegel_generate_date(ctx, tc, &mut date), HEGEL_E_STOP_TEST);
        let mut time = hegel_c::hegel_time_t {
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        };
        assert_eq!(hegel_generate_time(ctx, tc, &mut time), HEGEL_E_STOP_TEST);
        let mut dt = hegel_c::hegel_datetime_t {
            date: hegel_c::hegel_date_t {
                year: 0,
                month: 0,
                day: 0,
            },
            time: hegel_c::hegel_time_t {
                hour: 0,
                minute: 0,
                second: 0,
                microsecond: 0,
            },
        };
        assert_eq!(hegel_generate_datetime(ctx, tc, &mut dt), HEGEL_E_STOP_TEST);
        let mut uuid = [0u8; 16];
        assert_eq!(
            hegel_generate_uuid(ctx, tc, 0, false, uuid.as_mut_ptr()),
            HEGEL_E_STOP_TEST
        );
        let mut ip = [0u8; 16];
        assert_eq!(
            hegel_generate_ipv4(ctx, tc, ip.as_mut_ptr()),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(
            hegel_generate_ipv6(ctx, tc, ip.as_mut_ptr()),
            HEGEL_E_STOP_TEST
        );

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_OVERRUN,
            ptr::null(),
        ));
        ok(hegel_c::hegel_test_case_free(ctx, tc));
        drain(ctx, run);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}

#[test]
fn typed_draws_after_overrun_report_stop_test() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        ok(hegel_settings_set_test_cases(ctx, s, 1));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        let mut result = hegel_generate_bytes_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        loop {
            match hegel_generate_bytes(ctx, tc, 1000, 10000, &mut result) {
                HEGEL_OK => ok(hegel_generate_bytes_result_free(ctx, &mut result)),
                HEGEL_E_STOP_TEST => break,
                other => panic!("unexpected rc: {other:?}"),
            }
        }
        let mut i = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 1, &mut i),
            HEGEL_E_STOP_TEST
        );
        let one = [1u8];
        let mut out_buf = [0u8; 4];
        let mut out_len = 0usize;
        assert_eq!(
            hegel_generate_integer_big(
                ctx,
                tc,
                one.as_ptr(),
                1,
                one.as_ptr(),
                1,
                out_buf.as_mut_ptr(),
                4,
                &mut out_len,
            ),
            HEGEL_E_STOP_TEST
        );
        let mut f = 0f64;
        assert_eq!(
            hegel_generate_float(
                ctx,
                tc,
                64,
                0.0,
                1.0,
                false,
                false,
                false,
                false,
                f64::from_bits(1),
                &mut f,
            ),
            HEGEL_E_STOP_TEST
        );
        let mut b = false;
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 0.5, false, false, &mut b),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(
            hegel_generate_bytes(ctx, tc, 0, 3, &mut result),
            HEGEL_E_STOP_TEST
        );

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_OVERRUN,
            ptr::null(),
        ));
        ok(hegel_c::hegel_test_case_free(ctx, tc));
        drain(ctx, run);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
    }
    unsafe { ok(hegel_context_free(ctx)) };
}
