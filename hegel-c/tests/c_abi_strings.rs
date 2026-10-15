mod common;

use common::{last_error, ok};
use hegel_c::hegel_result_t::*;
use hegel_c::*;
use std::ptr;

unsafe fn bytes<'a>(view: hegel_string_t) -> &'a [u8] {
    assert!(!view.data.is_null());
    assert_eq!(unsafe { *view.data.add(view.len) }, 0);
    if view.len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(view.data.cast(), view.len) }
    }
}

#[test]
fn strings_preserve_lengths_nuls_and_optional_values() {
    unsafe {
        let ctx = hegel_context_new();
        let mut settings = ptr::null_mut();
        ok(hegel_settings_new_for_profile(
            ctx,
            "base".into(),
            &mut settings,
        ));
        let input = b"a\0bignored";
        let view = hegel_string_t {
            data: input.as_ptr().cast(),
            len: 3,
        };
        ok(hegel_settings_set_database(
            ctx,
            settings,
            test_optional(&view),
        ));
        let mut output = ptr::null();
        ok(hegel_settings_get_database(ctx, settings, &mut output));
        assert_eq!(bytes(*output), b"a\0b");
        let mut full = 0;
        let mut prefix = 0;
        ok(hegel_label_from_name(ctx, view, &mut full));
        ok(hegel_label_from_name(ctx, "a".into(), &mut prefix));
        assert_ne!(full, prefix);
        ok(hegel_settings_set_database(
            ctx,
            settings,
            &hegel_string_t::default(),
        ));
        ok(hegel_settings_get_database(ctx, settings, &mut output));
        assert!(!output.is_null());
        assert_eq!(bytes(*output), b"");
        assert_eq!((*output).len, 0);
        ok(hegel_settings_set_database(
            ctx,
            settings,
            test_optional(&hegel_string_t::default()),
        ));
        ok(hegel_settings_get_database(ctx, settings, &mut output));
        assert!(output.is_null());
        let mut version = hegel_string_t::default();
        ok(hegel_version(ctx, &mut version));
        assert_eq!(bytes(version), env!("CARGO_PKG_VERSION").as_bytes());
        ok(hegel_settings_free(ctx, settings));
        ok(hegel_context_free(ctx));
        let absent = hegel_context_last_error(ptr::null());
        assert_eq!(bytes(absent), b"");
        assert_eq!(absent.len, 0);
    }
}

#[test]
fn strings_reject_invalid_layouts_and_utf8_before_reading() {
    unsafe {
        let ctx = hegel_context_new();
        let mut settings = ptr::null_mut();
        ok(hegel_settings_new_for_profile(
            ctx,
            "base".into(),
            &mut settings,
        ));
        let invalid_utf8 = [0xff];
        for value in [
            hegel_string_t {
                data: ptr::null(),
                len: 1,
            },
            hegel_string_t {
                data: ptr::dangling(),
                len: isize::MAX as usize + 1,
            },
            hegel_string_t {
                data: invalid_utf8.as_ptr().cast(),
                len: 1,
            },
        ] {
            assert_eq!(
                hegel_settings_set_database_key(ctx, settings, test_optional(&value)),
                HEGEL_E_INVALID_ARG
            );
            assert!(!last_error(ctx).is_empty());
            let mut generator = ptr::null_mut();
            assert_eq!(
                hegel_string_generator_regex(ctx, value, true, ptr::null(), &mut generator),
                HEGEL_E_INVALID_ARG
            );
            assert!(generator.is_null());
        }
        let mut label = 0;
        assert_eq!(
            hegel_label_from_name(
                ctx,
                hegel_string_t {
                    data: ptr::null(),
                    len: 1
                },
                &mut label
            ),
            HEGEL_E_INVALID_ARG
        );
        ok(hegel_label_from_name(ctx, "".into(), &mut label));
        let mut empty_label = 0;
        ok(hegel_label_from_name(
            ctx,
            hegel_string_t::default(),
            &mut empty_label,
        ));
        assert_eq!(label, empty_label);
        ok(hegel_label_from_name(
            ctx,
            hegel_string_t {
                data: ptr::dangling(),
                len: 0,
            },
            &mut empty_label,
        ));
        assert_eq!(label, empty_label);
        ok(hegel_settings_free(ctx, settings));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn string_arrays_use_each_elements_length() {
    unsafe {
        let ctx = hegel_context_new();
        let category = b"Luignored";
        let categories = [hegel_string_t {
            data: category.as_ptr().cast(),
            len: 2,
        }];
        let mut generator = ptr::null_mut();
        ok(hegel_string_generator_text(
            ctx,
            0,
            1,
            test_optional(&hegel_string_t::default()),
            0,
            u32::MAX,
            categories.as_ptr(),
            1,
            ptr::null(),
            0,
            hegel_string_t::default(),
            hegel_string_t::default(),
            &mut generator,
        ));
        ok(hegel_string_generator_free(ctx, generator));
        let absent = [hegel_string_t::default()];
        for (array, count) in [
            (absent.as_ptr(), 1),
            (ptr::null(), 1),
            (
                ptr::dangling(),
                isize::MAX as usize / size_of::<hegel_string_t>() + 1,
            ),
        ] {
            assert_eq!(
                hegel_string_generator_text(
                    ctx,
                    0,
                    1,
                    test_optional(&hegel_string_t::default()),
                    0,
                    u32::MAX,
                    array,
                    count,
                    ptr::null(),
                    0,
                    hegel_string_t::default(),
                    hegel_string_t::default(),
                    &mut generator
                ),
                HEGEL_E_INVALID_ARG
            );
        }
        ok(hegel_context_free(ctx));
    }
}

fn test_optional(value: &hegel_c::hegel_string_t) -> *const hegel_c::hegel_string_t {
    if value.data.is_null() && value.len == 0 {
        std::ptr::null()
    } else {
        value
    }
}
